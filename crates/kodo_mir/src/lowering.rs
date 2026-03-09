//! # AST-to-MIR Lowering
//!
//! This module translates a typed AST into the MIR control-flow graph
//! representation. Each AST function becomes a [`MirFunction`] with basic
//! blocks, instructions, and terminators.
//!
//! This is the initial lowering pass — it produces correct but unoptimised
//! MIR. SSA construction and optimisations are left to later passes.
//!
//! ## Academic References
//!
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 7 — Translation
//!   to intermediate trees and canonical form.
//! - **\[EC\]** *Engineering a Compiler* Ch. 5 — Intermediate representations
//!   and the translation from AST to CFG-based IR.

use std::collections::HashMap;

use kodo_ast::{Block, Expr, Function, Module, Stmt, UnaryOp};
use kodo_types::{resolve_type, Type};

use crate::{
    BasicBlock, BlockId, Instruction, Local, LocalId, MirError, MirFunction, Result, Terminator,
    Value,
};

/// Builds MIR for a single function by maintaining locals, blocks, and a
/// name-to-local mapping as the AST is traversed.
///
/// The builder accumulates basic blocks in a `Vec` and tracks which block
/// is currently being populated. New locals and blocks are allocated with
/// monotonically increasing identifiers.
#[derive(Debug)]
pub struct MirBuilder {
    /// All local variable declarations.
    locals: Vec<Local>,
    /// Completed basic blocks.
    blocks: Vec<BasicBlock>,
    /// Instructions accumulated for the current block.
    current_instructions: Vec<Instruction>,
    /// Identifier of the block currently being built.
    current_block: BlockId,
    /// Counter for allocating fresh [`LocalId`] values.
    next_local: u32,
    /// Counter for allocating fresh [`BlockId`] values.
    next_block: u32,
    /// Maps variable names to their [`LocalId`].
    name_map: HashMap<String, LocalId>,
    /// Maps local IDs to their types (for struct field access resolution).
    local_types: HashMap<LocalId, kodo_types::Type>,
    /// Registry of struct types: name to field list in declaration order.
    struct_registry: HashMap<String, Vec<(String, kodo_types::Type)>>,
    /// Registry of enum types: name to variant definitions.
    enum_registry: HashMap<String, Vec<(String, Vec<kodo_types::Type>)>>,
    /// Ensures expressions to inject before each return.
    ensures: Vec<kodo_ast::Expr>,
    /// The name of the function being built (for error messages).
    fn_name: String,
}

impl MirBuilder {
    /// Creates a new builder with an initial entry block.
    fn new() -> Self {
        Self {
            locals: Vec::new(),
            blocks: Vec::new(),
            current_instructions: Vec::new(),
            current_block: BlockId(0),
            next_local: 0,
            next_block: 1, // 0 is the entry block
            name_map: HashMap::new(),
            local_types: HashMap::new(),
            struct_registry: HashMap::new(),
            enum_registry: HashMap::new(),
            ensures: Vec::new(),
            fn_name: String::new(),
        }
    }

    /// Allocates a new local variable and returns its identifier.
    fn alloc_local(&mut self, ty: Type, mutable: bool) -> LocalId {
        let id = LocalId(self.next_local);
        self.next_local += 1;
        self.local_types.insert(id, ty.clone());
        self.locals.push(Local { id, ty, mutable });
        id
    }

    /// Creates a new basic block and returns its identifier.
    ///
    /// The block is not yet added to the function — it becomes current
    /// only when the builder switches to it.
    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        id
    }

    /// Emits an instruction into the current basic block.
    fn emit(&mut self, instruction: Instruction) {
        self.current_instructions.push(instruction);
    }

    /// Finalises the current basic block with the given terminator and
    /// switches the builder to `next_block`.
    fn seal_block(&mut self, terminator: Terminator, next_block: BlockId) {
        let block = BasicBlock {
            id: self.current_block,
            instructions: std::mem::take(&mut self.current_instructions),
            terminator,
        };
        self.blocks.push(block);
        self.current_block = next_block;
    }

    /// Finalises the current block with the given terminator without
    /// starting a new block. Used for the last block in a function.
    fn seal_block_final(&mut self, terminator: Terminator) {
        let block = BasicBlock {
            id: self.current_block,
            instructions: std::mem::take(&mut self.current_instructions),
            terminator,
        };
        self.blocks.push(block);
    }

    /// Lowers a block of statements, returning the value of the last
    /// expression statement (or `Value::Unit` if the block is empty or
    /// ends with a non-expression statement).
    fn lower_block(&mut self, block: &Block) -> Result<Value> {
        let mut last_value = Value::Unit;
        for stmt in &block.stmts {
            last_value = self.lower_stmt(stmt)?;
        }
        Ok(last_value)
    }

    /// Lowers a single statement and returns the resulting value.
    ///
    /// - `Let` bindings allocate a local and assign the initialiser.
    /// - `Return` emits a return terminator (subsequent statements in
    ///   the same block become unreachable, but we handle that simply
    ///   by letting the caller continue — the block is already sealed).
    /// - `Expr` statements lower the expression and discard the result.
    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<Value> {
        match stmt {
            Stmt::Let {
                mutable,
                name,
                ty,
                value,
                ..
            } => {
                let resolved_ty = if let Some(type_expr) = ty {
                    resolve_type(type_expr, kodo_ast::Span::new(0, 0))
                        .map_err(|e| MirError::TypeResolution(e.to_string()))?
                } else {
                    Type::Unknown
                };
                let local_id = self.alloc_local(resolved_ty, *mutable);
                self.name_map.insert(name.clone(), local_id);
                let val = self.lower_expr(value)?;
                self.emit(Instruction::Assign(local_id, val));
                Ok(Value::Unit)
            }
            Stmt::Return { value, .. } => {
                let ret_val = match value {
                    Some(expr) => self.lower_expr(expr)?,
                    None => Value::Unit,
                };
                // Inject ensures checks before returning.
                self.inject_ensures_checks(&ret_val)?;
                // Seal the current block with a Return terminator and
                // create an unreachable continuation block.
                let continuation = self.new_block();
                self.seal_block(Terminator::Return(ret_val), continuation);
                Ok(Value::Unit)
            }
            Stmt::Expr(expr) => self.lower_expr(expr),
            Stmt::While {
                condition, body, ..
            } => {
                let loop_header = self.new_block();
                let loop_body = self.new_block();
                let loop_exit = self.new_block();

                // Jump from current block to the loop header.
                self.seal_block(Terminator::Goto(loop_header), loop_header);

                // In the header: evaluate condition and branch.
                let cond = self.lower_expr(condition)?;
                self.seal_block(
                    Terminator::Branch {
                        condition: cond,
                        true_block: loop_body,
                        false_block: loop_exit,
                    },
                    loop_body,
                );

                // In the body: lower statements and jump back to header.
                self.lower_block(body)?;
                self.seal_block(Terminator::Goto(loop_header), loop_exit);

                Ok(Value::Unit)
            }
            Stmt::Assign { name, value, .. } => {
                let local_id = self
                    .name_map
                    .get(name)
                    .copied()
                    .ok_or_else(|| MirError::UndefinedVariable(name.clone()))?;
                let val = self.lower_expr(value)?;
                self.emit(Instruction::Assign(local_id, val));
                Ok(Value::Unit)
            }
        }
    }

    /// Injects ensures contract checks for the given return value.
    ///
    /// For each ensures expression, registers the return value as `"result"`
    /// in the name map (so the ensures expression can reference it), evaluates
    /// the condition, and generates a branch to a fail block if the condition
    /// is false.
    fn inject_ensures_checks(&mut self, ret_val: &Value) -> Result<()> {
        if self.ensures.is_empty() {
            return Ok(());
        }

        // Clone ensures to avoid borrow issues.
        let ensures_exprs = self.ensures.clone();

        // Store the return value in a local so ensures expressions can reference it.
        let result_local = self.alloc_local(Type::Unknown, false);
        self.emit(Instruction::Assign(result_local, ret_val.clone()));
        let prev_result = self.name_map.insert("result".to_string(), result_local);

        for (i, ens_expr) in ensures_exprs.iter().enumerate() {
            let cond = self.lower_expr(ens_expr)?;
            let fail_block = self.new_block();
            let continue_block = self.new_block();
            self.seal_block(
                Terminator::Branch {
                    condition: cond,
                    true_block: continue_block,
                    false_block: fail_block,
                },
                fail_block,
            );
            // In the fail block, call kodo_contract_fail with the message.
            let msg = format!(
                "ensures clause {} failed in function `{}`",
                i + 1,
                self.fn_name
            );
            let dest = self.alloc_local(Type::Unit, false);
            self.emit(Instruction::Call {
                dest,
                callee: "kodo_contract_fail".to_string(),
                args: vec![Value::StringConst(msg)],
            });
            self.seal_block(Terminator::Unreachable, continue_block);
        }

        // Restore previous "result" binding.
        if let Some(prev) = prev_result {
            self.name_map.insert("result".to_string(), prev);
        } else {
            self.name_map.remove("result");
        }

        Ok(())
    }

    /// Lowers an expression to a [`Value`].
    ///
    /// Compound expressions (calls, if/else) may emit instructions and
    /// create new basic blocks as a side effect.
    #[allow(clippy::too_many_lines)]
    fn lower_expr(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::IntLit(n, _) => Ok(Value::IntConst(*n)),
            Expr::BoolLit(b, _) => Ok(Value::BoolConst(*b)),
            Expr::StringLit(s, _) => Ok(Value::StringConst(s.clone())),
            Expr::Ident(name, _) => {
                let local_id = self
                    .name_map
                    .get(name)
                    .copied()
                    .ok_or_else(|| MirError::UndefinedVariable(name.clone()))?;
                Ok(Value::Local(local_id))
            }
            Expr::BinaryOp {
                left, op, right, ..
            } => {
                let lhs = self.lower_expr(left)?;
                let rhs = self.lower_expr(right)?;
                Ok(Value::BinOp(*op, Box::new(lhs), Box::new(rhs)))
            }
            Expr::UnaryOp { op, operand, .. } => {
                let inner = self.lower_expr(operand)?;
                match op {
                    UnaryOp::Not => Ok(Value::Not(Box::new(inner))),
                    UnaryOp::Neg => Ok(Value::Neg(Box::new(inner))),
                }
            }
            Expr::Call { callee, args, .. } => {
                let callee_name = match callee.as_ref() {
                    Expr::Ident(name, _) => name.clone(),
                    _ => return Err(MirError::NonIdentCallee),
                };
                let mut arg_values = Vec::with_capacity(args.len());
                for arg in args {
                    arg_values.push(self.lower_expr(arg)?);
                }
                let dest = self.alloc_local(Type::Unknown, false);
                self.emit(Instruction::Call {
                    dest,
                    callee: callee_name,
                    args: arg_values,
                });
                Ok(Value::Local(dest))
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond = self.lower_expr(condition)?;

                let then_block = self.new_block();
                let else_block = self.new_block();
                let merge_block = self.new_block();

                // Seal the current block with a Branch terminator.
                self.seal_block(
                    Terminator::Branch {
                        condition: cond,
                        true_block: then_block,
                        false_block: else_block,
                    },
                    then_block,
                );

                // Lower the then branch.
                let then_val = self.lower_block(then_branch)?;
                let then_result = self.alloc_local(Type::Unknown, false);
                self.emit(Instruction::Assign(then_result, then_val));
                self.seal_block(Terminator::Goto(merge_block), else_block);

                // Lower the else branch (or produce Unit).
                let else_val = if let Some(else_blk) = else_branch {
                    self.lower_block(else_blk)?
                } else {
                    Value::Unit
                };
                let else_result = self.alloc_local(Type::Unknown, false);
                self.emit(Instruction::Assign(else_result, else_val));
                self.seal_block(Terminator::Goto(merge_block), merge_block);

                // In the merge block, we return the then-branch result
                // local. A proper phi-node / SSA pass would unify both
                // values later; for now this is a simplification.
                Ok(Value::Local(then_result))
            }
            Expr::Block(block) => self.lower_block(block),
            Expr::FieldAccess { object, field, .. } => {
                let obj_val = self.lower_expr(object)?;
                // Resolve struct name from the object's type.
                let struct_name = match object.as_ref() {
                    Expr::Ident(name, _) => {
                        let local_id = self
                            .name_map
                            .get(name)
                            .copied()
                            .ok_or_else(|| MirError::UndefinedVariable(name.clone()))?;
                        match self.local_types.get(&local_id) {
                            Some(Type::Struct(s)) => s.clone(),
                            _ => return Ok(Value::Unit),
                        }
                    }
                    _ => return Ok(Value::Unit),
                };
                let local_id = self.alloc_local(Type::Unknown, false);
                self.emit(Instruction::Assign(
                    local_id,
                    Value::FieldGet {
                        object: Box::new(obj_val),
                        field: field.clone(),
                        struct_name,
                    },
                ));
                Ok(Value::Local(local_id))
            }
            Expr::StructLit { name, fields, .. } => {
                // Reorder fields to match declaration order.
                let decl_fields = self.struct_registry.get(name).cloned().unwrap_or_default();
                let mut ordered_fields = Vec::with_capacity(fields.len());
                for (decl_name, _) in &decl_fields {
                    if let Some(init) = fields.iter().find(|f| &f.name == decl_name) {
                        let val = self.lower_expr(&init.value)?;
                        ordered_fields.push((decl_name.clone(), val));
                    }
                }
                let local_id = self.alloc_local(Type::Struct(name.clone()), false);
                self.emit(Instruction::Assign(
                    local_id,
                    Value::StructLit {
                        name: name.clone(),
                        fields: ordered_fields,
                    },
                ));
                Ok(Value::Local(local_id))
            }
            Expr::EnumVariantExpr {
                enum_name,
                variant,
                args,
                ..
            } => {
                // Find discriminant index for this variant.
                let variants = self
                    .enum_registry
                    .get(enum_name)
                    .cloned()
                    .unwrap_or_default();
                let discriminant = variants.iter().position(|(n, _)| n == variant).unwrap_or(0);
                let mut arg_values = Vec::with_capacity(args.len());
                for arg in args {
                    arg_values.push(self.lower_expr(arg)?);
                }
                let local_id = self.alloc_local(Type::Enum(enum_name.clone()), false);
                #[allow(clippy::cast_possible_truncation)]
                let disc_u8 = discriminant as u8;
                self.emit(Instruction::Assign(
                    local_id,
                    Value::EnumVariant {
                        enum_name: enum_name.clone(),
                        variant: variant.clone(),
                        discriminant: disc_u8,
                        args: arg_values,
                    },
                ));
                Ok(Value::Local(local_id))
            }
            Expr::Match { expr, arms, .. } => {
                // Lower the matched expression.
                let matched_val = self.lower_expr(expr)?;
                let matched_local = self.alloc_local(Type::Unknown, false);
                self.emit(Instruction::Assign(matched_local, matched_val));

                let merge_block = self.new_block();
                let result_local = self.alloc_local(Type::Unknown, true);

                // Generate a chain of branches testing discriminant.
                for (i, arm) in arms.iter().enumerate() {
                    let is_last = i + 1 == arms.len();
                    match &arm.pattern {
                        kodo_ast::Pattern::Variant {
                            enum_name,
                            variant,
                            bindings,
                            ..
                        } => {
                            // Resolve discriminant for this variant.
                            let enum_name_resolved = enum_name
                                .as_ref()
                                .and_then(|en| self.enum_registry.get(en))
                                .or_else(|| {
                                    if let Some(Type::Enum(en)) =
                                        self.local_types.get(&matched_local)
                                    {
                                        self.enum_registry.get(en)
                                    } else {
                                        None
                                    }
                                });
                            let disc_idx = enum_name_resolved
                                .and_then(|vs| vs.iter().position(|(n, _)| n == variant))
                                .unwrap_or(0);

                            // Branch: compare discriminant.
                            let arm_block = self.new_block();
                            let next_block = if is_last {
                                merge_block
                            } else {
                                self.new_block()
                            };

                            #[allow(clippy::cast_possible_wrap)]
                            let cond = Value::BinOp(
                                kodo_ast::BinOp::Eq,
                                Box::new(Value::EnumDiscriminant(Box::new(Value::Local(
                                    matched_local,
                                )))),
                                Box::new(Value::IntConst(disc_idx as i64)),
                            );
                            self.seal_block(
                                Terminator::Branch {
                                    condition: cond,
                                    true_block: arm_block,
                                    false_block: next_block,
                                },
                                arm_block,
                            );

                            // Bind pattern variables to payload fields.
                            for (idx, binding) in bindings.iter().enumerate() {
                                let bind_local = self.alloc_local(Type::Unknown, false);
                                self.name_map.insert(binding.clone(), bind_local);
                                #[allow(clippy::cast_possible_truncation)]
                                let field_idx = idx as u32;
                                self.emit(Instruction::Assign(
                                    bind_local,
                                    Value::EnumPayload {
                                        value: Box::new(Value::Local(matched_local)),
                                        field_index: field_idx,
                                    },
                                ));
                            }

                            // Lower arm body.
                            let arm_val = self.lower_expr(&arm.body)?;
                            self.emit(Instruction::Assign(result_local, arm_val));
                            self.seal_block(Terminator::Goto(merge_block), next_block);
                        }
                        kodo_ast::Pattern::Wildcard(_) => {
                            // Wildcard catches everything remaining.
                            let arm_val = self.lower_expr(&arm.body)?;
                            self.emit(Instruction::Assign(result_local, arm_val));
                            self.seal_block(Terminator::Goto(merge_block), merge_block);
                        }
                        kodo_ast::Pattern::Literal(lit_expr) => {
                            // Compare matched value against literal.
                            let lit_val = self.lower_expr(lit_expr)?;
                            let arm_block = self.new_block();
                            let next_block = if is_last {
                                merge_block
                            } else {
                                self.new_block()
                            };
                            let cond = Value::BinOp(
                                kodo_ast::BinOp::Eq,
                                Box::new(Value::Local(matched_local)),
                                Box::new(lit_val),
                            );
                            self.seal_block(
                                Terminator::Branch {
                                    condition: cond,
                                    true_block: arm_block,
                                    false_block: next_block,
                                },
                                arm_block,
                            );
                            let arm_val = self.lower_expr(&arm.body)?;
                            self.emit(Instruction::Assign(result_local, arm_val));
                            self.seal_block(Terminator::Goto(merge_block), next_block);
                        }
                    }
                }

                Ok(Value::Local(result_local))
            }
        }
    }
}

/// Lowers a single AST [`Function`] into a [`MirFunction`].
///
/// Creates an entry block, allocates locals for parameters, lowers the
/// function body, and — if no explicit `return` statement terminates the
/// last block — appends a `Return(Unit)` terminator.
///
/// # Errors
///
/// Returns [`MirError`] if a variable is undefined, a type cannot be
/// resolved, or a non-identifier callee is encountered in a call.
pub fn lower_function(function: &Function) -> Result<MirFunction> {
    lower_function_with_registries(function, &HashMap::new(), &HashMap::new())
}

/// Lowers a single AST [`Function`] into a [`MirFunction`] with type registries.
fn lower_function_with_registries(
    function: &Function,
    struct_registry: &HashMap<String, Vec<(String, Type)>>,
    enum_registry: &HashMap<String, Vec<(String, Vec<Type>)>>,
) -> Result<MirFunction> {
    let mut builder = MirBuilder::new();
    builder.struct_registry.clone_from(struct_registry);
    builder.enum_registry.clone_from(enum_registry);
    builder.ensures.clone_from(&function.ensures);
    builder.fn_name.clone_from(&function.name);

    // Allocate locals for parameters and populate the name map.
    for param in &function.params {
        let ty = resolve_type(&param.ty, param.span)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        let local_id = builder.alloc_local(ty, false);
        builder.name_map.insert(param.name.clone(), local_id);
    }
    let param_count = function.params.len();

    // Inject `requires` contract checks before the function body.
    for (i, req_expr) in function.requires.iter().enumerate() {
        let cond = builder.lower_expr(req_expr)?;
        let fail_block = builder.new_block();
        let continue_block = builder.new_block();
        builder.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: continue_block,
                false_block: fail_block,
            },
            fail_block,
        );
        // In the fail block, call kodo_contract_fail with the message.
        let msg = format!(
            "requires clause {} failed in function `{}`",
            i + 1,
            function.name
        );
        let dest = builder.alloc_local(Type::Unit, false);
        builder.emit(Instruction::Call {
            dest,
            callee: "kodo_contract_fail".to_string(),
            args: vec![Value::StringConst(msg)],
        });
        builder.seal_block(Terminator::Unreachable, continue_block);
    }

    // Lower the function body.
    builder.lower_block(&function.body)?;

    // Inject ensures checks before the implicit Return(Unit).
    builder.inject_ensures_checks(&Value::Unit)?;

    // If the current block still has no terminator (i.e. it was not
    // sealed by a Return statement), seal it with Return(Unit).
    builder.seal_block_final(Terminator::Return(Value::Unit));

    // Resolve the return type.
    let return_type = resolve_type(&function.return_type, function.span)
        .map_err(|e| MirError::TypeResolution(e.to_string()))?;

    Ok(MirFunction {
        name: function.name.clone(),
        return_type,
        param_count,
        locals: builder.locals,
        blocks: builder.blocks,
        entry: BlockId(0),
    })
}

/// Generates a validator function for a function with `requires` contracts.
///
/// The validator has the same parameters as the original function but returns
/// `Bool`. It evaluates all preconditions combined with `&&` and returns
/// the result — no abort, no side effects.
fn generate_validator(function: &Function) -> Result<MirFunction> {
    let validator_name = format!("validate_{}", function.name);
    let mut builder = MirBuilder::new();
    builder.fn_name.clone_from(&validator_name);

    // Allocate locals for parameters (same as original function).
    for param in &function.params {
        let ty = resolve_type(&param.ty, param.span)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        let local_id = builder.alloc_local(ty, false);
        builder.name_map.insert(param.name.clone(), local_id);
    }
    let param_count = function.params.len();

    // Evaluate all requires expressions and combine with &&.
    let mut combined: Option<Value> = None;
    for req_expr in &function.requires {
        let cond = builder.lower_expr(req_expr)?;
        combined = Some(match combined {
            Some(prev) => Value::BinOp(kodo_ast::BinOp::And, Box::new(prev), Box::new(cond)),
            None => cond,
        });
    }

    // Return the combined result (or true if somehow empty — shouldn't happen).
    let result = combined.unwrap_or(Value::BoolConst(true));
    builder.seal_block_final(Terminator::Return(result));

    Ok(MirFunction {
        name: validator_name,
        return_type: Type::Bool,
        param_count,
        locals: builder.locals,
        blocks: builder.blocks,
        entry: BlockId(0),
    })
}

/// Lowers all functions in a [`Module`] into a `Vec` of [`MirFunction`].
///
/// For each function with `requires` contracts, an additional validator
/// function (`validate_{name}`) is generated that evaluates the preconditions
/// without side effects and returns `Bool`.
///
/// # Errors
///
/// Returns the first [`MirError`] encountered during lowering.
pub fn lower_module(module: &Module) -> Result<Vec<MirFunction>> {
    // Build struct registry from type declarations.
    let mut struct_registry: HashMap<String, Vec<(String, Type)>> = HashMap::new();
    for type_decl in &module.type_decls {
        let mut fields = Vec::new();
        for field in &type_decl.fields {
            let ty = resolve_type(&field.ty, field.span)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fields.push((field.name.clone(), ty));
        }
        struct_registry.insert(type_decl.name.clone(), fields);
    }

    // Build enum registry from enum declarations.
    let mut enum_registry: HashMap<String, Vec<(String, Vec<Type>)>> = HashMap::new();
    for enum_decl in &module.enum_decls {
        let mut variants = Vec::new();
        for variant in &enum_decl.variants {
            let field_types: std::result::Result<Vec<_>, _> = variant
                .fields
                .iter()
                .map(|f| {
                    resolve_type(f, variant.span)
                        .map_err(|e| MirError::TypeResolution(e.to_string()))
                })
                .collect();
            variants.push((variant.name.clone(), field_types?));
        }
        enum_registry.insert(enum_decl.name.clone(), variants);
    }

    let mut mir_functions: Vec<MirFunction> = module
        .functions
        .iter()
        .map(|f| lower_function_with_registries(f, &struct_registry, &enum_registry))
        .collect::<Result<Vec<_>>>()?;

    // Generate validator functions for contracts.
    for func in &module.functions {
        if func.requires.is_empty() {
            continue;
        }
        let validator = generate_validator(func)?;
        mir_functions.push(validator);
    }

    Ok(mir_functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{BinOp, Block, Expr, Function, Module, NodeId, Param, Span, Stmt, TypeExpr};

    /// Helper to create a dummy span.
    fn span() -> Span {
        Span::new(0, 0)
    }

    /// Helper to build a simple function with a body block.
    fn make_fn(name: &str, params: Vec<Param>, body: Block, ret: TypeExpr) -> Function {
        Function {
            id: NodeId(0),
            span: span(),
            name: name.to_string(),
            annotations: vec![],
            params,
            return_type: ret,
            requires: vec![],
            ensures: vec![],
            body,
        }
    }

    #[test]
    fn lower_empty_function() {
        let func = make_fn(
            "empty",
            vec![],
            Block {
                span: span(),
                stmts: vec![],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        assert_eq!(mir.name, "empty");
        assert_eq!(mir.return_type, Type::Unit);
        assert_eq!(mir.blocks.len(), 1);
        // The only block should have a Return(Unit) terminator.
        assert!(matches!(
            mir.blocks[0].terminator,
            Terminator::Return(Value::Unit)
        ));
    }

    #[test]
    fn lower_let_and_return() {
        // fn example() -> Int { let x: Int = 42; return x }
        let func = make_fn(
            "example",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(42, span()),
                    },
                    Stmt::Return {
                        span: span(),
                        value: Some(Expr::Ident("x".to_string(), span())),
                    },
                ],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        assert_eq!(mir.name, "example");
        assert_eq!(mir.return_type, Type::Int);
        // Should have local _0 for x.
        assert!(!mir.locals.is_empty());
        // The entry block should have an Assign + a Return terminator.
        let entry = &mir.blocks[0];
        assert_eq!(entry.instructions.len(), 1);
        assert!(matches!(entry.terminator, Terminator::Return(_)));
    }

    #[test]
    fn lower_binary_expression() {
        // fn add() -> Int { return 1 + 2 }
        let func = make_fn(
            "add",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::BinaryOp {
                        left: Box::new(Expr::IntLit(1, span())),
                        op: BinOp::Add,
                        right: Box::new(Expr::IntLit(2, span())),
                        span: span(),
                    }),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // The return terminator should contain a BinOp value.
        match &mir.blocks[0].terminator {
            Terminator::Return(Value::BinOp(BinOp::Add, lhs, rhs)) => {
                assert!(matches!(lhs.as_ref(), Value::IntConst(1)));
                assert!(matches!(rhs.as_ref(), Value::IntConst(2)));
            }
            other => panic!("expected Return(BinOp(Add, ...)), got {other:?}"),
        }
    }

    #[test]
    fn lower_if_else_creates_cfg() {
        // fn branch(x: Bool) -> Int {
        //     if x { return 1 } else { return 2 }
        // }
        let func = make_fn(
            "branch",
            vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Bool".to_string()),
                span: span(),
            }],
            Block {
                span: span(),
                stmts: vec![Stmt::Expr(Expr::If {
                    condition: Box::new(Expr::Ident("x".to_string(), span())),
                    then_branch: Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::IntLit(1, span())),
                        }],
                    },
                    else_branch: Some(Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::IntLit(2, span())),
                        }],
                    }),
                    span: span(),
                })],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();

        // There should be multiple blocks: entry, then, else, merge,
        // plus continuation blocks from return statements.
        assert!(
            mir.blocks.len() >= 4,
            "expected at least 4 blocks, got {}",
            mir.blocks.len()
        );

        // The entry block should have a Branch terminator.
        assert!(matches!(
            mir.blocks[0].terminator,
            Terminator::Branch { .. }
        ));
    }

    #[test]
    fn lower_function_call() {
        // fn caller() -> Int { return add(1, 2) }
        let func = make_fn(
            "caller",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Call {
                        callee: Box::new(Expr::Ident("add".to_string(), span())),
                        args: vec![Expr::IntLit(1, span()), Expr::IntLit(2, span())],
                        span: span(),
                    }),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // The entry block should have a Call instruction.
        assert_eq!(mir.blocks[0].instructions.len(), 1);
        assert!(matches!(
            mir.blocks[0].instructions[0],
            Instruction::Call { .. }
        ));
    }

    #[test]
    fn lower_module_multiple_functions() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_module".to_string(),
            meta: None,
            type_decls: vec![],
            enum_decls: vec![],
            functions: vec![
                make_fn(
                    "first",
                    vec![],
                    Block {
                        span: span(),
                        stmts: vec![],
                    },
                    TypeExpr::Unit,
                ),
                make_fn(
                    "second",
                    vec![],
                    Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::IntLit(99, span())),
                        }],
                    },
                    TypeExpr::Named("Int".to_string()),
                ),
            ],
        };
        let fns = lower_module(&module).unwrap();
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "first");
        assert_eq!(fns[1].name, "second");
        for f in &fns {
            f.validate().unwrap();
        }
    }

    #[test]
    fn lower_unary_not_and_neg() {
        // fn negate() { let a = !true; let b = -42 }
        let func = make_fn(
            "negate",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "a".to_string(),
                        ty: Some(TypeExpr::Named("Bool".to_string())),
                        value: Expr::UnaryOp {
                            op: UnaryOp::Not,
                            operand: Box::new(Expr::BoolLit(true, span())),
                            span: span(),
                        },
                    },
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "b".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::UnaryOp {
                            op: UnaryOp::Neg,
                            operand: Box::new(Expr::IntLit(42, span())),
                            span: span(),
                        },
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Two assign instructions: Not(BoolConst(true)) and Neg(IntConst(42)).
        assert_eq!(mir.blocks[0].instructions.len(), 2);
        match &mir.blocks[0].instructions[0] {
            Instruction::Assign(_, Value::Not(inner)) => {
                assert!(matches!(inner.as_ref(), Value::BoolConst(true)));
            }
            other => panic!("expected Assign(_, Not(BoolConst(true))), got {other:?}"),
        }
        match &mir.blocks[0].instructions[1] {
            Instruction::Assign(_, Value::Neg(inner)) => {
                assert!(matches!(inner.as_ref(), Value::IntConst(42)));
            }
            other => panic!("expected Assign(_, Neg(IntConst(42))), got {other:?}"),
        }
    }

    #[test]
    fn lower_undefined_variable_errors() {
        let func = make_fn(
            "bad",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Ident("nonexistent".to_string(), span())),
                }],
            },
            TypeExpr::Unit,
        );
        let result = lower_function(&func);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, MirError::UndefinedVariable(ref name) if name == "nonexistent"),
            "expected UndefinedVariable, got {err:?}"
        );
    }

    #[test]
    fn lower_params_are_accessible() {
        // fn id(x: Int) -> Int { return x }
        let func = make_fn(
            "id",
            vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
            }],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Ident("x".to_string(), span())),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Local _0 should be the parameter.
        assert_eq!(mir.locals[0].ty, Type::Int);
        // Return should reference Local(_0).
        assert!(matches!(
            mir.blocks[0].terminator,
            Terminator::Return(Value::Local(LocalId(0)))
        ));
    }

    #[test]
    fn lower_while_creates_loop_cfg() {
        // fn counter() { let mut i: Int = 3; while i > 0 { i = i - 1 } }
        let func = make_fn(
            "counter",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: true,
                        name: "i".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(3, span()),
                    },
                    Stmt::While {
                        span: span(),
                        condition: Expr::BinaryOp {
                            left: Box::new(Expr::Ident("i".to_string(), span())),
                            op: BinOp::Gt,
                            right: Box::new(Expr::IntLit(0, span())),
                            span: span(),
                        },
                        body: Block {
                            span: span(),
                            stmts: vec![Stmt::Assign {
                                span: span(),
                                name: "i".to_string(),
                                value: Expr::BinaryOp {
                                    left: Box::new(Expr::Ident("i".to_string(), span())),
                                    op: BinOp::Sub,
                                    right: Box::new(Expr::IntLit(1, span())),
                                    span: span(),
                                },
                            }],
                        },
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have: entry, loop_header, loop_body, loop_exit + final
        assert!(
            mir.blocks.len() >= 4,
            "expected at least 4 blocks for while loop, got {}",
            mir.blocks.len()
        );
        // First block should have a Goto to the header
        assert!(
            matches!(mir.blocks[0].terminator, Terminator::Goto(_)),
            "entry should goto loop header"
        );
    }

    #[test]
    fn lower_while_false_exits_immediately() {
        // fn skip() { while false { } }
        let func = make_fn(
            "skip",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::While {
                    span: span(),
                    condition: Expr::BoolLit(false, span()),
                    body: Block {
                        span: span(),
                        stmts: vec![],
                    },
                }],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // The loop header should have a Branch with false condition
        // leading to exit.
        let header = &mir.blocks[1]; // block after entry's Goto
        assert!(
            matches!(header.terminator, Terminator::Branch { .. }),
            "loop header should have Branch terminator"
        );
    }

    #[test]
    fn lower_assignment() {
        // fn reassign() { let mut x: Int = 1; x = 42 }
        let func = make_fn(
            "reassign",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: true,
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(1, span()),
                    },
                    Stmt::Assign {
                        span: span(),
                        name: "x".to_string(),
                        value: Expr::IntLit(42, span()),
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have 2 Assign instructions in the entry block
        assert_eq!(mir.blocks[0].instructions.len(), 2);
    }

    #[test]
    fn lower_function_with_ensures_injects_check() {
        // fn positive() -> Int ensures { result > 0 } { return 42 }
        let func = Function {
            id: NodeId(0),
            span: span(),
            name: "positive".to_string(),
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![Expr::BinaryOp {
                left: Box::new(Expr::Ident("result".to_string(), span())),
                op: BinOp::Gt,
                right: Box::new(Expr::IntLit(0, span())),
                span: span(),
            }],
            body: Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::IntLit(42, span())),
                }],
            },
        };
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have more blocks due to ensures checks
        assert!(
            mir.blocks.len() >= 3,
            "expected at least 3 blocks with ensures, got {}",
            mir.blocks.len()
        );
    }

    #[test]
    fn lower_function_with_ensures_result_reference() {
        // ensures { result == 0 } with implicit return
        let func = Function {
            id: NodeId(0),
            span: span(),
            name: "zero".to_string(),
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![Expr::BinaryOp {
                left: Box::new(Expr::Ident("result".to_string(), span())),
                op: BinOp::Eq,
                right: Box::new(Expr::IntLit(0, span())),
                span: span(),
            }],
            body: Block {
                span: span(),
                stmts: vec![],
            },
        };
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have blocks for ensures check even on implicit return
        assert!(mir.blocks.len() >= 3);
    }

    #[test]
    fn validator_generated_for_function_with_requires() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_mod".to_string(),
            meta: None,
            type_decls: vec![],
            enum_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                annotations: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                }],
                return_type: TypeExpr::Named("Int".to_string()),
                requires: vec![Expr::BinaryOp {
                    left: Box::new(Expr::Ident("x".to_string(), span())),
                    op: BinOp::Gt,
                    right: Box::new(Expr::IntLit(0, span())),
                    span: span(),
                }],
                ensures: vec![],
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        };
        let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
        assert_eq!(
            fns.len(),
            2,
            "expected original + validator, got {}",
            fns.len()
        );
        assert_eq!(fns[0].name, "checked");
        assert_eq!(fns[1].name, "validate_checked");
    }

    #[test]
    fn validator_not_generated_without_requires() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_mod".to_string(),
            meta: None,
            type_decls: vec![],
            enum_decls: vec![],
            functions: vec![make_fn(
                "plain",
                vec![],
                Block {
                    span: span(),
                    stmts: vec![],
                },
                TypeExpr::Unit,
            )],
        };
        let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
        assert_eq!(fns.len(), 1, "expected only original, got {}", fns.len());
    }

    #[test]
    fn validator_has_same_params() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_mod".to_string(),
            meta: None,
            type_decls: vec![],
            enum_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                annotations: vec![],
                params: vec![
                    Param {
                        name: "x".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                    },
                    Param {
                        name: "y".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                    },
                ],
                return_type: TypeExpr::Named("Int".to_string()),
                requires: vec![Expr::BinaryOp {
                    left: Box::new(Expr::Ident("x".to_string(), span())),
                    op: BinOp::Gt,
                    right: Box::new(Expr::IntLit(0, span())),
                    span: span(),
                }],
                ensures: vec![],
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        };
        let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
        let original = &fns[0];
        let validator = &fns[1];
        assert_eq!(
            original.param_count, validator.param_count,
            "validator should have same param count as original"
        );
    }

    #[test]
    fn validator_returns_bool() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_mod".to_string(),
            meta: None,
            type_decls: vec![],
            enum_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                annotations: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                }],
                return_type: TypeExpr::Named("Int".to_string()),
                requires: vec![Expr::BinaryOp {
                    left: Box::new(Expr::Ident("x".to_string(), span())),
                    op: BinOp::Gt,
                    right: Box::new(Expr::IntLit(0, span())),
                    span: span(),
                }],
                ensures: vec![],
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        };
        let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
        let validator = &fns[1];
        assert_eq!(
            validator.return_type,
            Type::Bool,
            "validator should return Bool"
        );
    }

    #[test]
    fn lower_function_multiple_ensures() {
        let func = Function {
            id: NodeId(0),
            span: span(),
            name: "multi".to_string(),
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![
                Expr::BinaryOp {
                    left: Box::new(Expr::Ident("result".to_string(), span())),
                    op: BinOp::Gt,
                    right: Box::new(Expr::IntLit(0, span())),
                    span: span(),
                },
                Expr::BinaryOp {
                    left: Box::new(Expr::Ident("result".to_string(), span())),
                    op: BinOp::Lt,
                    right: Box::new(Expr::IntLit(100, span())),
                    span: span(),
                },
            ],
            body: Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::IntLit(42, span())),
                }],
            },
        };
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // With 2 ensures, we should have even more blocks
        assert!(
            mir.blocks.len() >= 5,
            "expected at least 5 blocks with 2 ensures, got {}",
            mir.blocks.len()
        );
    }
}
