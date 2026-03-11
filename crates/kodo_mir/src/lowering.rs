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

use std::collections::{HashMap, HashSet};

use kodo_ast::{Block, Expr, Function, Module, Stmt, UnaryOp};
use kodo_types::{resolve_type, resolve_type_with_enums, Type};

/// Size of a single actor field in bytes (i64 = 8 bytes).
const ACTOR_FIELD_SIZE: i64 = 8;

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
    /// Registry of function return types: name to return type.
    fn_return_types: HashMap<String, kodo_types::Type>,
    /// Ensures expressions to inject before each return.
    ensures: Vec<kodo_ast::Expr>,
    /// The name of the function being built (for error messages).
    fn_name: String,
    /// Counter for generating unique closure function names.
    closure_counter: u32,
    /// Lambda-lifted closure functions generated during lowering.
    generated_closures: Vec<MirFunction>,
    /// Maps variable names bound to closures to their generated function
    /// name and list of captured variable names.
    closure_registry: HashMap<String, (String, Vec<String>)>,
    /// Number of function parameters (first N locals are params).
    param_count: usize,
    /// Names of actor types — used to distinguish actors from structs during
    /// lowering so that actor-specific runtime calls are emitted instead of
    /// regular struct operations.
    actor_names: HashSet<String>,
    /// Registry of type aliases: name to (base type, optional refinement constraint).
    ///
    /// When a `let` binding has a type annotation that matches a refined alias,
    /// the MIR builder emits a runtime contract check after the assignment.
    type_alias_registry: HashMap<String, (kodo_types::Type, Option<kodo_ast::Expr>)>,
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
            param_count: 0,
            name_map: HashMap::new(),
            local_types: HashMap::new(),
            struct_registry: HashMap::new(),
            enum_registry: HashMap::new(),
            fn_return_types: HashMap::new(),
            ensures: Vec::new(),
            fn_name: String::new(),
            closure_counter: 0,
            generated_closures: Vec::new(),
            closure_registry: HashMap::new(),
            actor_names: HashSet::new(),
            type_alias_registry: HashMap::new(),
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

    /// Infers the type of a [`Value`] from the builder's local type map.
    ///
    /// Constants map to their corresponding primitive types, locals are
    /// looked up in the `local_types` map, and composite values (struct
    /// literals, enum variants) resolve to their named types.
    fn infer_value_type(&self, value: &Value) -> Type {
        match value {
            Value::IntConst(_) | Value::EnumDiscriminant(_) => Type::Int,
            Value::FloatConst(_) => Type::Float64,
            Value::BoolConst(_) | Value::Not(_) => Type::Bool,
            Value::StringConst(_) => Type::String,
            Value::Unit => Type::Unit,
            Value::Local(lid) => self.local_types.get(lid).cloned().unwrap_or(Type::Unknown),
            Value::BinOp(op, lhs, _rhs) => {
                use kodo_ast::BinOp;
                match op {
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or => Type::Bool,
                    _ => self.infer_value_type(lhs),
                }
            }
            Value::Neg(inner) => self.infer_value_type(inner),
            Value::StructLit { name, .. } => Type::Struct(name.clone()),
            Value::EnumVariant { enum_name, .. } => Type::Enum(enum_name.clone()),
            Value::EnumPayload { .. } | Value::FieldGet { .. } | Value::FuncRef(_) => Type::Unknown,
        }
    }

    /// Returns `true` if `callee_name` is a mangled actor handler name
    /// (i.e. `"ActorName_HandlerName"` where `ActorName` is a known actor).
    fn is_actor_handler(&self, callee_name: &str) -> bool {
        self.actor_names.iter().any(|actor| {
            callee_name.starts_with(actor.as_str())
                && callee_name.as_bytes().get(actor.len()) == Some(&b'_')
        })
    }

    /// Returns `true` if the given type is heap-allocated and requires
    /// reference counting (String, Struct, or generic containers like List/Map).
    fn is_heap_type(ty: &Type) -> bool {
        matches!(ty, Type::String | Type::Struct(_) | Type::Generic(_, _))
    }

    /// Emits [`Instruction::DecRef`] for all heap-allocated locals in the
    /// function body, excluding parameters and the return value local.
    ///
    /// Called before emitting a `Return` terminator to ensure heap locals
    /// are cleaned up.
    fn emit_decref_for_heap_locals(&mut self, param_count: usize, return_local: Option<LocalId>) {
        let heap_locals: Vec<LocalId> = self
            .locals
            .iter()
            .filter(|local| {
                // Skip parameters — they are owned by the caller.
                if (local.id.0 as usize) < param_count {
                    return false;
                }
                // Skip the local being returned — ownership transfers.
                if return_local == Some(local.id) {
                    return false;
                }
                Self::is_heap_type(&local.ty)
            })
            .map(|local| local.id)
            .collect();

        for local_id in heap_locals {
            self.emit(Instruction::DecRef(local_id));
        }
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

    /// Collects free variables in an expression that are defined in the
    /// enclosing scope but not in the given set of local parameter names.
    fn collect_free_vars(
        expr: &Expr,
        params: &std::collections::HashSet<String>,
        free: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match expr {
            Expr::Ident(name, _) => {
                if !params.contains(name) && seen.insert(name.clone()) {
                    free.push(name.clone());
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_free_vars(left, params, free, seen);
                Self::collect_free_vars(right, params, free, seen);
            }
            Expr::UnaryOp { operand, .. }
            | Expr::Is { operand, .. }
            | Expr::Await { operand, .. } => {
                Self::collect_free_vars(operand, params, free, seen);
            }
            Expr::Call { callee, args, .. } => {
                Self::collect_free_vars(callee, params, free, seen);
                for arg in args {
                    Self::collect_free_vars(arg, params, free, seen);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::collect_free_vars(condition, params, free, seen);
                for stmt in &then_branch.stmts {
                    Self::collect_free_vars_in_stmt(stmt, params, free, seen);
                }
                if let Some(else_blk) = else_branch {
                    for stmt in &else_blk.stmts {
                        Self::collect_free_vars_in_stmt(stmt, params, free, seen);
                    }
                }
            }
            Expr::Block(block) => {
                for stmt in &block.stmts {
                    Self::collect_free_vars_in_stmt(stmt, params, free, seen);
                }
            }
            Expr::FieldAccess { object, .. } => {
                Self::collect_free_vars(object, params, free, seen);
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    Self::collect_free_vars(&f.value, params, free, seen);
                }
            }
            Expr::EnumVariantExpr { args, .. } => {
                for arg in args {
                    Self::collect_free_vars(arg, params, free, seen);
                }
            }
            Expr::Match { expr, arms, .. } => {
                Self::collect_free_vars(expr, params, free, seen);
                for arm in arms {
                    Self::collect_free_vars(&arm.body, params, free, seen);
                }
            }
            Expr::Closure { body, .. } => {
                Self::collect_free_vars(body, params, free, seen);
            }
            // Literals and other expressions have no free variables.
            Expr::IntLit(..)
            | Expr::FloatLit(..)
            | Expr::StringLit(..)
            | Expr::BoolLit(..)
            | Expr::Range { .. }
            | Expr::Try { .. }
            | Expr::OptionalChain { .. }
            | Expr::NullCoalesce { .. } => {}
        }
    }

    /// Collects free variables in a statement.
    fn collect_free_vars_in_stmt(
        stmt: &Stmt,
        params: &std::collections::HashSet<String>,
        free: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } => {
                Self::collect_free_vars(value, params, free, seen);
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    Self::collect_free_vars(expr, params, free, seen);
                }
            }
            Stmt::Expr(expr) => {
                Self::collect_free_vars(expr, params, free, seen);
            }
            Stmt::While {
                condition, body, ..
            } => {
                Self::collect_free_vars(condition, params, free, seen);
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
            Stmt::For {
                start, end, body, ..
            } => {
                Self::collect_free_vars(start, params, free, seen);
                Self::collect_free_vars(end, params, free, seen);
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
            // IfLet is desugared before MIR lowering.
            Stmt::IfLet {
                value,
                body,
                else_body,
                ..
            } => {
                Self::collect_free_vars(value, params, free, seen);
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
                if let Some(eb) = else_body {
                    for s in &eb.stmts {
                        Self::collect_free_vars_in_stmt(s, params, free, seen);
                    }
                }
            }
            // Spawn is desugared before MIR lowering.
            Stmt::Spawn { body, .. } => {
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
            Stmt::Parallel { body, .. } => {
                for s in body {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
        }
    }

    /// Lambda-lifts a closure into a top-level [`MirFunction`].
    ///
    /// Generates a unique function name, prepends captured variables as
    /// extra parameters, and lowers the closure body.
    fn lift_closure(
        &mut self,
        closure_params: &[kodo_ast::ClosureParam],
        return_type: Option<&kodo_ast::TypeExpr>,
        body: &Expr,
    ) -> Result<(String, Vec<String>)> {
        let closure_name = format!("__closure_{}", self.closure_counter);
        self.closure_counter += 1;

        // Collect free variables (captures).
        let param_names: std::collections::HashSet<String> =
            closure_params.iter().map(|p| p.name.clone()).collect();
        let mut free_vars = Vec::new();
        let mut seen = std::collections::HashSet::new();
        Self::collect_free_vars(body, &param_names, &mut free_vars, &mut seen);

        // Only keep free vars that are actually defined in the enclosing scope.
        let captures: Vec<String> = free_vars
            .into_iter()
            .filter(|name| self.name_map.contains_key(name))
            .collect();

        // Build enum names set for type resolution.
        let enum_names: std::collections::HashSet<String> =
            self.enum_registry.keys().cloned().collect();

        // Create the MirFunction for the closure.
        let mut closure_builder = MirBuilder::new();
        closure_builder
            .struct_registry
            .clone_from(&self.struct_registry);
        closure_builder
            .enum_registry
            .clone_from(&self.enum_registry);
        closure_builder
            .fn_return_types
            .clone_from(&self.fn_return_types);
        closure_builder.fn_name.clone_from(&closure_name);

        // Allocate locals for captured variables (extra params).
        for cap_name in &captures {
            let cap_ty = self
                .name_map
                .get(cap_name)
                .and_then(|lid| self.local_types.get(lid))
                .cloned()
                .unwrap_or(Type::Unknown);
            let local_id = closure_builder.alloc_local(cap_ty, false);
            closure_builder.name_map.insert(cap_name.clone(), local_id);
        }

        // Allocate locals for closure parameters.
        for param in closure_params {
            let ty = if let Some(type_expr) = &param.ty {
                resolve_type_with_enums(type_expr, param.span, &enum_names)
                    .map_err(|e| MirError::TypeResolution(e.to_string()))?
            } else {
                Type::Unknown
            };
            let local_id = closure_builder.alloc_local(ty, false);
            closure_builder
                .name_map
                .insert(param.name.clone(), local_id);
        }

        let param_count = captures.len() + closure_params.len();

        // Lower the closure body.
        let body_val = closure_builder.lower_expr(body)?;

        // Determine the return type: prefer an explicit annotation, then
        // fall back to inferring from the body value.
        let inferred_ret = if let Some(ret_expr) = return_type {
            let enum_names: std::collections::HashSet<String> =
                self.enum_registry.keys().cloned().collect();
            resolve_type_with_enums(ret_expr, kodo_ast::Span::new(0, 0), &enum_names)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?
        } else {
            closure_builder.infer_value_type(&body_val)
        };

        closure_builder.seal_block_final(Terminator::Return(body_val));

        let mir_func = MirFunction {
            name: closure_name.clone(),
            return_type: inferred_ret.clone(),
            param_count,
            locals: closure_builder.locals,
            blocks: closure_builder.blocks,
            entry: BlockId(0),
        };

        // Collect any nested closures generated during lowering.
        self.generated_closures
            .extend(closure_builder.generated_closures);
        self.generated_closures.push(mir_func);

        // Register the closure in fn_return_types so calls resolve.
        self.fn_return_types
            .insert(closure_name.clone(), inferred_ret);

        Ok((closure_name, captures))
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
    #[allow(clippy::too_many_lines)]
    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<Value> {
        match stmt {
            Stmt::Let {
                mutable,
                name,
                ty,
                value,
                ..
            } => {
                // If the annotation is a type alias, resolve to the base type.
                let resolved_ty = if let Some(type_expr) = ty {
                    if let kodo_ast::TypeExpr::Named(alias_name) = type_expr {
                        if let Some((base_ty, _)) = self.type_alias_registry.get(alias_name) {
                            base_ty.clone()
                        } else {
                            resolve_type(type_expr, kodo_ast::Span::new(0, 0))
                                .map_err(|e| MirError::TypeResolution(e.to_string()))?
                        }
                    } else {
                        resolve_type(type_expr, kodo_ast::Span::new(0, 0))
                            .map_err(|e| MirError::TypeResolution(e.to_string()))?
                    }
                } else {
                    Type::Unknown
                };
                // Check if the value is a closure — if so, we need to register
                // the variable name in the closure registry after lowering.
                let is_closure = matches!(value, Expr::Closure { .. });
                let local_id = self.alloc_local(resolved_ty, *mutable);
                self.name_map.insert(name.clone(), local_id);
                let val = self.lower_expr(value)?;
                self.emit(Instruction::Assign(local_id, val));

                // If the value was a closure, the lift_closure method stored
                // the closure info under the generated name. Find it and also
                // register under the user-visible variable name.
                if is_closure {
                    // The most recently generated closure name is the one we want.
                    if let Some(last_closure) = self.generated_closures.last() {
                        let closure_name = last_closure.name.clone();
                        if let Some(entry) = self.closure_registry.get(&closure_name).cloned() {
                            self.closure_registry.insert(name.clone(), entry);
                        }
                    }
                }

                // Emit a refinement check if the type annotation is a refined alias.
                if let Some(kodo_ast::TypeExpr::Named(alias_name)) = ty {
                    if let Some((_base_ty, Some(constraint))) =
                        self.type_alias_registry.get(alias_name).cloned()
                    {
                        self.inject_refinement_check(name, &constraint, alias_name)?;
                    }
                }

                Ok(Value::Unit)
            }
            Stmt::Return { value, .. } => {
                let ret_val = match value {
                    Some(expr) => self.lower_expr(expr)?,
                    None => Value::Unit,
                };
                // Inject ensures checks before returning.
                self.inject_ensures_checks(&ret_val)?;
                // Emit DecRef for heap-allocated locals before returning.
                let return_local = if let Value::Local(lid) = &ret_val {
                    Some(*lid)
                } else {
                    None
                };
                let pc = self.param_count;
                self.emit_decref_for_heap_locals(pc, return_local);
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
            Stmt::For {
                name,
                start,
                end,
                inclusive,
                body,
                ..
            } => self.lower_for_loop(name, start, end, *inclusive, body),
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
            // IfLet is desugared before MIR lowering.
            Stmt::IfLet { .. } => Ok(Value::Unit),
            // Spawn: lambda-lift body into a task function and schedule it.
            // When the body captures variables, we pack them into an
            // environment buffer and use `kodo_spawn_task_with_env`.
            Stmt::Spawn { body, .. } => {
                // Check for free variables in the spawn body.
                let params = std::collections::HashSet::new();
                let mut free_vars = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, &params, &mut free_vars, &mut seen);
                }
                let captures: Vec<String> = free_vars
                    .into_iter()
                    .filter(|name| self.name_map.contains_key(name))
                    .collect();

                let spawn_name = format!("__spawn_{}", self.closure_counter);
                self.closure_counter += 1;

                let mut spawn_builder = MirBuilder::new();
                spawn_builder
                    .struct_registry
                    .clone_from(&self.struct_registry);
                spawn_builder.enum_registry.clone_from(&self.enum_registry);
                spawn_builder
                    .fn_return_types
                    .clone_from(&self.fn_return_types);
                spawn_builder.fn_name.clone_from(&spawn_name);

                if captures.is_empty() {
                    // No captures — lambda-lift into a zero-arg function.
                    let body_val = spawn_builder.lower_block(body)?;
                    let _ = body_val;
                    spawn_builder.seal_block_final(Terminator::Return(Value::Unit));

                    let mir_func = MirFunction {
                        name: spawn_name.clone(),
                        return_type: Type::Unit,
                        param_count: 0,
                        locals: spawn_builder.locals,
                        blocks: spawn_builder.blocks,
                        entry: BlockId(0),
                    };

                    self.generated_closures
                        .extend(spawn_builder.generated_closures);
                    self.generated_closures.push(mir_func);
                    self.fn_return_types.insert(spawn_name.clone(), Type::Unit);

                    // Emit: kodo_spawn_task(FuncRef(spawn_name))
                    let dest = self.alloc_local(Type::Unit, false);
                    self.emit(Instruction::Call {
                        dest,
                        callee: "kodo_spawn_task".to_string(),
                        args: vec![Value::FuncRef(spawn_name)],
                    });
                } else {
                    // Has captures — lambda-lift into a function that takes a
                    // single env-pointer argument and unpacks the captures.
                    // The spawned function receives one i64 param (env pointer).
                    let env_param = spawn_builder.alloc_local(Type::Int, false);
                    spawn_builder
                        .name_map
                        .insert("__env_ptr".to_string(), env_param);

                    // For each capture, emit an unpack instruction that loads
                    // the value from the env buffer at the correct offset.
                    // Each capture occupies 8 bytes (one i64 word).
                    for (idx, cap_name) in captures.iter().enumerate() {
                        let cap_ty = self
                            .name_map
                            .get(cap_name)
                            .and_then(|lid| self.local_types.get(lid))
                            .cloned()
                            .unwrap_or(Type::Int);
                        let cap_local = spawn_builder.alloc_local(cap_ty.clone(), false);
                        spawn_builder.name_map.insert(cap_name.clone(), cap_local);
                        spawn_builder.local_types.insert(cap_local, cap_ty);
                        // Emit: cap_local = env_ptr + offset (via BinOp)
                        // We load the value: cap_local = *(env_ptr + idx*8)
                        // Represented as: cap_local = __env_load(env_ptr, offset)
                        // Using a Call to a synthetic builtin that codegen
                        // will translate as a memory load.
                        #[allow(clippy::cast_possible_wrap)]
                        let offset = (idx as i64) * 8;
                        spawn_builder.emit(Instruction::Call {
                            dest: cap_local,
                            callee: "__env_load".to_string(),
                            args: vec![Value::Local(env_param), Value::IntConst(offset)],
                        });
                    }

                    let param_count = 1; // just the env pointer

                    let body_val = spawn_builder.lower_block(body)?;
                    let _ = body_val;
                    spawn_builder.seal_block_final(Terminator::Return(Value::Unit));

                    let mir_func = MirFunction {
                        name: spawn_name.clone(),
                        return_type: Type::Unit,
                        param_count,
                        locals: spawn_builder.locals,
                        blocks: spawn_builder.blocks,
                        entry: BlockId(0),
                    };

                    self.generated_closures
                        .extend(spawn_builder.generated_closures);
                    self.generated_closures.push(mir_func);
                    self.fn_return_types.insert(spawn_name.clone(), Type::Unit);

                    // In the caller: pack captures into an env buffer on the
                    // stack, then call kodo_spawn_task_with_env.
                    // Emit: env_local = __env_pack(capture1, capture2, ...)
                    let env_local = self.alloc_local(Type::Int, false);
                    let mut pack_args = Vec::with_capacity(captures.len());
                    for cap_name in &captures {
                        let cap_lid = self
                            .name_map
                            .get(cap_name)
                            .copied()
                            .ok_or_else(|| MirError::UndefinedVariable(cap_name.clone()))?;
                        pack_args.push(Value::Local(cap_lid));
                    }
                    self.emit(Instruction::Call {
                        dest: env_local,
                        callee: "__env_pack".to_string(),
                        args: pack_args,
                    });

                    // Emit: kodo_spawn_task_with_env(FuncRef, env_ptr, env_size)
                    #[allow(clippy::cast_possible_wrap)]
                    let env_size = (captures.len() as i64) * 8;
                    let dest = self.alloc_local(Type::Unit, false);
                    self.emit(Instruction::Call {
                        dest,
                        callee: "kodo_spawn_task_with_env".to_string(),
                        args: vec![
                            Value::FuncRef(spawn_name),
                            Value::Local(env_local),
                            Value::IntConst(env_size),
                        ],
                    });
                }

                Ok(Value::Unit)
            }
            Stmt::Parallel { body, .. } => {
                // Structured concurrency: spawn each task asynchronously,
                // collect handles, then await all before the block exits.
                let mut handles = Vec::new();
                for stmt in body {
                    if let Stmt::Spawn {
                        body: spawn_body, ..
                    } = stmt
                    {
                        let handle = self.lower_parallel_spawn(spawn_body)?;
                        handles.push(handle);
                    }
                }
                // Emit kodo_await for each handle to guarantee structured
                // concurrency: all tasks complete before leaving the block.
                for handle in handles {
                    let await_dest = self.alloc_local(Type::Unit, false);
                    self.emit(Instruction::Call {
                        dest: await_dest,
                        callee: "kodo_await".to_string(),
                        args: vec![Value::Local(handle)],
                    });
                }
                Ok(Value::Unit)
            }
        }
    }

    /// Lambda-lifts a spawn body inside a parallel block and emits a
    /// `kodo_spawn_async` call that returns a future handle.
    ///
    /// Returns the [`LocalId`] holding the handle so the caller can emit
    /// a corresponding `kodo_await` after all spawns.
    #[allow(clippy::too_many_lines)]
    fn lower_parallel_spawn(&mut self, body: &Block) -> Result<LocalId> {
        let params = std::collections::HashSet::new();
        let mut free_vars = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for s in &body.stmts {
            Self::collect_free_vars_in_stmt(s, &params, &mut free_vars, &mut seen);
        }
        let captures: Vec<String> = free_vars
            .into_iter()
            .filter(|name| self.name_map.contains_key(name))
            .collect();
        let spawn_name = format!("__parallel_spawn_{}", self.closure_counter);
        self.closure_counter += 1;
        let mut spawn_builder = MirBuilder::new();
        spawn_builder
            .struct_registry
            .clone_from(&self.struct_registry);
        spawn_builder.enum_registry.clone_from(&self.enum_registry);
        spawn_builder
            .fn_return_types
            .clone_from(&self.fn_return_types);
        spawn_builder.fn_name.clone_from(&spawn_name);
        if captures.is_empty() {
            let body_val = spawn_builder.lower_block(body)?;
            let _ = body_val;
            spawn_builder.seal_block_final(Terminator::Return(Value::Unit));
            let mir_func = MirFunction {
                name: spawn_name.clone(),
                return_type: Type::Unit,
                param_count: 0,
                locals: spawn_builder.locals,
                blocks: spawn_builder.blocks,
                entry: BlockId(0),
            };
            self.generated_closures
                .extend(spawn_builder.generated_closures);
            self.generated_closures.push(mir_func);
            self.fn_return_types.insert(spawn_name.clone(), Type::Unit);
            // kodo_spawn_async returns a handle (i64).
            let handle = self.alloc_local(Type::Int, false);
            self.emit(Instruction::Call {
                dest: handle,
                callee: "kodo_spawn_async".to_string(),
                args: vec![
                    Value::FuncRef(spawn_name),
                    Value::IntConst(0),
                    Value::IntConst(0),
                ],
            });
            Ok(handle)
        } else {
            let env_param = spawn_builder.alloc_local(Type::Int, false);
            spawn_builder
                .name_map
                .insert("__env_ptr".to_string(), env_param);
            for (idx, cap_name) in captures.iter().enumerate() {
                let cap_ty = self
                    .name_map
                    .get(cap_name)
                    .and_then(|lid| self.local_types.get(lid))
                    .cloned()
                    .unwrap_or(Type::Int);
                let cap_local = spawn_builder.alloc_local(cap_ty.clone(), false);
                spawn_builder.name_map.insert(cap_name.clone(), cap_local);
                spawn_builder.local_types.insert(cap_local, cap_ty);
                #[allow(clippy::cast_possible_wrap)]
                let offset = (idx as i64) * 8;
                spawn_builder.emit(Instruction::Call {
                    dest: cap_local,
                    callee: "__env_load".to_string(),
                    args: vec![Value::Local(env_param), Value::IntConst(offset)],
                });
            }
            let param_count = 1;
            let body_val = spawn_builder.lower_block(body)?;
            let _ = body_val;
            spawn_builder.seal_block_final(Terminator::Return(Value::Unit));
            let mir_func = MirFunction {
                name: spawn_name.clone(),
                return_type: Type::Unit,
                param_count,
                locals: spawn_builder.locals,
                blocks: spawn_builder.blocks,
                entry: BlockId(0),
            };
            self.generated_closures
                .extend(spawn_builder.generated_closures);
            self.generated_closures.push(mir_func);
            self.fn_return_types.insert(spawn_name.clone(), Type::Unit);
            let env_local = self.alloc_local(Type::Int, false);
            let mut pack_args = Vec::with_capacity(captures.len());
            for cap_name in &captures {
                let cap_lid = self
                    .name_map
                    .get(cap_name)
                    .copied()
                    .ok_or_else(|| MirError::UndefinedVariable(cap_name.clone()))?;
                pack_args.push(Value::Local(cap_lid));
            }
            self.emit(Instruction::Call {
                dest: env_local,
                callee: "__env_pack".to_string(),
                args: pack_args,
            });
            #[allow(clippy::cast_possible_wrap)]
            let env_size = (captures.len() as i64) * 8;
            let handle = self.alloc_local(Type::Int, false);
            self.emit(Instruction::Call {
                dest: handle,
                callee: "kodo_spawn_async".to_string(),
                args: vec![
                    Value::FuncRef(spawn_name),
                    Value::Local(env_local),
                    Value::IntConst(env_size),
                ],
            });
            Ok(handle)
        }
    }

    /// Lowers a `for` loop into MIR by desugaring into a while-style loop.
    ///
    /// The translation is:
    /// ```text
    /// let mut <name> = <start>
    /// while <name> < <end> { <body>; <name> = <name> + 1 }
    /// ```
    /// For inclusive ranges, `<=` is used instead of `<`.
    fn lower_for_loop(
        &mut self,
        name: &str,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        body: &Block,
    ) -> Result<Value> {
        let start_val = self.lower_expr(start)?;
        let loop_var = self.alloc_local(Type::Int, true);
        self.name_map.insert(name.to_string(), loop_var);
        self.emit(Instruction::Assign(loop_var, start_val));

        let loop_header = self.new_block();
        let loop_body = self.new_block();
        let loop_exit = self.new_block();

        // Jump to loop header.
        self.seal_block(Terminator::Goto(loop_header), loop_header);

        // In header: compare loop_var < end (or <= for inclusive).
        let end_val = self.lower_expr(end)?;
        let cmp_op = if inclusive {
            kodo_ast::BinOp::Le
        } else {
            kodo_ast::BinOp::Lt
        };
        let cond = Value::BinOp(cmp_op, Box::new(Value::Local(loop_var)), Box::new(end_val));

        self.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: loop_body,
                false_block: loop_exit,
            },
            loop_body,
        );

        // In body: lower statements, then increment loop var.
        self.lower_block(body)?;
        let inc_val = Value::BinOp(
            kodo_ast::BinOp::Add,
            Box::new(Value::Local(loop_var)),
            Box::new(Value::IntConst(1)),
        );
        self.emit(Instruction::Assign(loop_var, inc_val));
        self.seal_block(Terminator::Goto(loop_header), loop_exit);

        Ok(Value::Unit)
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

    /// Emits a runtime refinement check for a variable bound to a refined type alias.
    ///
    /// Substitutes `self` in the constraint expression with the variable name,
    /// lowers the resulting condition, and emits a branch to a fail block that
    /// calls `kodo_contract_fail` if the constraint is violated.
    fn inject_refinement_check(
        &mut self,
        var_name: &str,
        constraint: &kodo_ast::Expr,
        alias_name: &str,
    ) -> Result<()> {
        let substituted = Self::substitute_self_in_expr(constraint, var_name);
        let cond = self.lower_expr(&substituted)?;
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
        let msg = format!(
            "refinement constraint failed: `{var_name}` does not satisfy type `{alias_name}`"
        );
        let dest = self.alloc_local(Type::Unit, false);
        self.emit(Instruction::Call {
            dest,
            callee: "kodo_contract_fail".to_string(),
            args: vec![Value::StringConst(msg)],
        });
        self.seal_block(Terminator::Unreachable, continue_block);
        Ok(())
    }

    /// Replaces `Ident("self")` with `Ident(var_name)` recursively in an expression.
    ///
    /// Used by refinement type checks to bind the constraint's `self` keyword
    /// to the actual variable being constrained.
    fn substitute_self_in_expr(expr: &kodo_ast::Expr, var_name: &str) -> kodo_ast::Expr {
        match expr {
            Expr::Ident(name, span) if name == "self" => Expr::Ident(var_name.to_string(), *span),
            Expr::BinaryOp {
                left,
                op,
                right,
                span,
            } => Expr::BinaryOp {
                left: Box::new(Self::substitute_self_in_expr(left, var_name)),
                op: *op,
                right: Box::new(Self::substitute_self_in_expr(right, var_name)),
                span: *span,
            },
            Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
                op: *op,
                operand: Box::new(Self::substitute_self_in_expr(operand, var_name)),
                span: *span,
            },
            Expr::FieldAccess {
                object,
                field,
                span,
            } => Expr::FieldAccess {
                object: Box::new(Self::substitute_self_in_expr(object, var_name)),
                field: field.clone(),
                span: *span,
            },
            other => other.clone(),
        }
    }

    /// Lowers an expression to a [`Value`].
    ///
    /// Compound expressions (calls, if/else) may emit instructions and
    /// create new basic blocks as a side effect.
    #[allow(clippy::too_many_lines)]
    fn lower_expr(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::IntLit(n, _) => Ok(Value::IntConst(*n)),
            #[allow(clippy::cast_possible_wrap)]
            Expr::FloatLit(f, _) => Ok(Value::FloatConst(*f)),
            Expr::BoolLit(b, _) => Ok(Value::BoolConst(*b)),
            Expr::StringLit(s, _) => Ok(Value::StringConst(s.clone())),
            Expr::Ident(name, _) => {
                if let Some(local_id) = self.name_map.get(name).copied() {
                    Ok(Value::Local(local_id))
                } else if self.fn_return_types.contains_key(name) {
                    // The identifier refers to a function — produce a function pointer.
                    Ok(Value::FuncRef(name.clone()))
                } else {
                    Err(MirError::UndefinedVariable(name.clone()))
                }
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

                // Check if the callee is a closure — prepend captures.
                if let Some((closure_func, captures)) =
                    self.closure_registry.get(&callee_name).cloned()
                {
                    for cap_name in &captures {
                        let cap_local = self
                            .name_map
                            .get(cap_name)
                            .copied()
                            .ok_or_else(|| MirError::UndefinedVariable(cap_name.clone()))?;
                        arg_values.push(Value::Local(cap_local));
                    }
                    for arg in args {
                        arg_values.push(self.lower_expr(arg)?);
                    }
                    let ret_ty = self
                        .fn_return_types
                        .get(&closure_func)
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    let dest = self.alloc_local(ret_ty, false);
                    self.emit(Instruction::Call {
                        dest,
                        callee: closure_func,
                        args: arg_values,
                    });
                    return Ok(Value::Local(dest));
                }

                // Check if the callee is a local variable with a function type
                // (i.e. a function pointer / higher-order function parameter).
                if let Some(local_id) = self.name_map.get(&callee_name).copied() {
                    if let Some(Type::Function(param_types, ret_type)) =
                        self.local_types.get(&local_id).cloned()
                    {
                        for arg in args {
                            arg_values.push(self.lower_expr(arg)?);
                        }
                        let dest = self.alloc_local(*ret_type.clone(), false);
                        self.emit(Instruction::IndirectCall {
                            dest,
                            callee: Value::Local(local_id),
                            args: arg_values,
                            return_type: *ret_type,
                            param_types,
                        });
                        return Ok(Value::Local(dest));
                    }
                }

                // Check if the callee is a mangled actor handler name
                // (e.g. "Counter_increment"). If so, emit kodo_actor_send
                // with the actor pointer, a function reference, and the
                // first non-self argument.
                if self.is_actor_handler(&callee_name) {
                    // After method call rewriting, args are [self, ...rest].
                    // self (args[0]) is the actor pointer.
                    let actor_val = self.lower_expr(&args[0])?;
                    let handler_arg = if args.len() > 1 {
                        self.lower_expr(&args[1])?
                    } else {
                        Value::IntConst(0)
                    };
                    let dest = self.alloc_local(Type::Unit, false);
                    self.emit(Instruction::Call {
                        dest,
                        callee: "kodo_actor_send".to_string(),
                        args: vec![actor_val, Value::FuncRef(callee_name.clone()), handler_arg],
                    });
                    return Ok(Value::Local(dest));
                }

                for arg in args {
                    arg_values.push(self.lower_expr(arg)?);
                }
                // Resolve generic function calls: if callee_name is not in
                // fn_return_types, try to find a monomorphized version.
                let resolved_callee = if self.fn_return_types.contains_key(&callee_name) {
                    callee_name
                } else {
                    let prefix = format!("{callee_name}__");
                    self.fn_return_types
                        .keys()
                        .find(|k| k.starts_with(&prefix))
                        .cloned()
                        .unwrap_or(callee_name)
                };
                // Look up return type from fn_return_types so composite types
                // get proper stack slot allocation in codegen.
                let ret_ty = self
                    .fn_return_types
                    .get(&resolved_callee)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                let dest = self.alloc_local(ret_ty, false);
                self.emit(Instruction::Call {
                    dest,
                    callee: resolved_callee,
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
                if self.actor_names.contains(&struct_name) {
                    // Actor field access: emit `kodo_actor_get_field(actor_ptr, offset)`.
                    let decl_fields = self
                        .struct_registry
                        .get(&struct_name)
                        .cloned()
                        .unwrap_or_default();
                    let field_index = decl_fields
                        .iter()
                        .position(|(n, _)| n == field)
                        .unwrap_or(0);
                    #[allow(clippy::cast_possible_wrap)]
                    let offset = (field_index as i64) * ACTOR_FIELD_SIZE;
                    let dest = self.alloc_local(Type::Int, false);
                    self.emit(Instruction::Call {
                        dest,
                        callee: "kodo_actor_get_field".to_string(),
                        args: vec![obj_val, Value::IntConst(offset)],
                    });
                    Ok(Value::Local(dest))
                } else {
                    // Regular struct field access.
                    let field_ty = self
                        .struct_registry
                        .get(&struct_name)
                        .and_then(|fields| fields.iter().find(|(n, _)| n == field))
                        .map_or(Type::Unknown, |(_, ty)| ty.clone());
                    let local_id = self.alloc_local(field_ty, false);
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
            }
            Expr::StructLit { name, fields, .. } => {
                if self.actor_names.contains(name) {
                    // Actor instantiation: allocate state via runtime call and
                    // set each field using `kodo_actor_set_field`.
                    let decl_fields = self.struct_registry.get(name).cloned().unwrap_or_default();
                    #[allow(clippy::cast_possible_wrap)]
                    let state_size = (decl_fields.len() as i64) * ACTOR_FIELD_SIZE;
                    let actor_ptr = self.alloc_local(Type::Int, false);
                    self.emit(Instruction::Call {
                        dest: actor_ptr,
                        callee: "kodo_actor_new".to_string(),
                        args: vec![Value::IntConst(state_size)],
                    });
                    // Set each field in declaration order.
                    for (idx, (decl_name, _)) in decl_fields.iter().enumerate() {
                        if let Some(init) = fields.iter().find(|f| &f.name == decl_name) {
                            let val = self.lower_expr(&init.value)?;
                            #[allow(clippy::cast_possible_wrap)]
                            let offset = (idx as i64) * ACTOR_FIELD_SIZE;
                            let void_dest = self.alloc_local(Type::Unit, false);
                            self.emit(Instruction::Call {
                                dest: void_dest,
                                callee: "kodo_actor_set_field".to_string(),
                                args: vec![Value::Local(actor_ptr), Value::IntConst(offset), val],
                            });
                        }
                    }
                    // Store the actor pointer with the actor's struct type so
                    // later field access / handler calls can identify it.
                    self.local_types
                        .insert(actor_ptr, Type::Struct(name.clone()));
                    Ok(Value::Local(actor_ptr))
                } else {
                    // Regular struct literal.
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
            }
            Expr::EnumVariantExpr {
                enum_name,
                variant,
                args,
                ..
            } => {
                // Resolve the actual enum name — for generic enums, look up a
                // monomorphized instance (e.g. "Option" → "Option__Int").
                let resolved_name = if self.enum_registry.contains_key(enum_name) {
                    enum_name.clone()
                } else {
                    // Find a monomorphized instance with matching prefix and variant.
                    let prefix = format!("{enum_name}__");
                    self.enum_registry
                        .keys()
                        .find(|k| {
                            k.starts_with(&prefix)
                                && self
                                    .enum_registry
                                    .get(*k)
                                    .is_some_and(|vs| vs.iter().any(|(n, _)| n == variant))
                        })
                        .cloned()
                        .unwrap_or_else(|| enum_name.clone())
                };

                // Find discriminant index for this variant.
                let variants = self
                    .enum_registry
                    .get(&resolved_name)
                    .cloned()
                    .unwrap_or_default();
                let discriminant = variants.iter().position(|(n, _)| n == variant).unwrap_or(0);
                let mut arg_values = Vec::with_capacity(args.len());
                for arg in args {
                    arg_values.push(self.lower_expr(arg)?);
                }
                let local_id = self.alloc_local(Type::Enum(resolved_name.clone()), false);
                #[allow(clippy::cast_possible_truncation)]
                let disc_u8 = discriminant as u8;
                self.emit(Instruction::Assign(
                    local_id,
                    Value::EnumVariant {
                        enum_name: resolved_name,
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
                            // For generic enums, fall back to the matched local's
                            // type which already carries the monomorphized name.
                            let enum_name_resolved = enum_name
                                .as_ref()
                                .and_then(|en| {
                                    self.enum_registry.get(en).or_else(|| {
                                        // Try monomorphized prefix match.
                                        let prefix = format!("{en}__");
                                        self.enum_registry
                                            .keys()
                                            .find(|k| k.starts_with(&prefix))
                                            .and_then(|k| self.enum_registry.get(k))
                                    })
                                })
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

            // Range and sugar expressions are not valid standalone expressions
            // in MIR. Ranges are only used in for loops (desugared before MIR),
            // sugar operators (?/?.//??) are desugared to match expressions
            // before MIR.
            Expr::Range { .. }
            | Expr::Try { .. }
            | Expr::OptionalChain { .. }
            | Expr::NullCoalesce { .. }
            | Expr::Is { .. } => Ok(Value::Unit),

            // Lambda-lift closures into top-level functions.
            Expr::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                let (closure_name, captures) =
                    self.lift_closure(params, return_type.as_ref(), body)?;

                // Register this local's variable name → closure mapping.
                // The caller (Let statement) will pick up the closure_registry
                // entry using the variable name.
                self.closure_registry
                    .insert(closure_name.clone(), (closure_name.clone(), captures));

                // Return a FuncRef so the closure can be used as a value
                // (e.g., assigned to a variable, passed as argument).
                Ok(Value::FuncRef(closure_name))
            }

            // `Await` in v1: no real suspension — lower the inner expression.
            Expr::Await { operand, .. } => self.lower_expr(operand),
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
    lower_function_with_registries(function, &HashMap::new(), &HashMap::new(), &HashMap::new())
}

/// Lowers a single AST [`Function`] into a [`MirFunction`] with type registries.
fn lower_function_with_registries(
    function: &Function,
    struct_registry: &HashMap<String, Vec<(String, Type)>>,
    enum_registry: &HashMap<String, Vec<(String, Vec<Type>)>>,
    fn_return_types: &HashMap<String, Type>,
) -> Result<MirFunction> {
    let (func, _closures) = lower_function_with_closures(
        function,
        struct_registry,
        enum_registry,
        fn_return_types,
        &HashSet::new(),
        &HashMap::new(),
    )?;
    Ok(func)
}

/// Lowers a single AST [`Function`] into a [`MirFunction`] and any
/// lambda-lifted closure functions.
fn lower_function_with_closures(
    function: &Function,
    struct_registry: &HashMap<String, Vec<(String, Type)>>,
    enum_registry: &HashMap<String, Vec<(String, Vec<Type>)>>,
    fn_return_types: &HashMap<String, Type>,
    actor_names: &HashSet<String>,
    type_alias_registry: &HashMap<String, (Type, Option<kodo_ast::Expr>)>,
) -> Result<(MirFunction, Vec<MirFunction>)> {
    let mut builder = MirBuilder::new();
    builder.actor_names.clone_from(actor_names);
    builder.struct_registry.clone_from(struct_registry);
    builder.enum_registry.clone_from(enum_registry);
    builder.fn_return_types.clone_from(fn_return_types);
    builder.type_alias_registry.clone_from(type_alias_registry);
    builder.ensures.clone_from(&function.ensures);
    builder.fn_name.clone_from(&function.name);

    // Build enum names set for type resolution.
    let enum_names: std::collections::HashSet<String> = enum_registry.keys().cloned().collect();

    // Allocate locals for parameters and populate the name map.
    for param in &function.params {
        let ty = resolve_type_with_enums(&param.ty, param.span, &enum_names)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        let local_id = builder.alloc_local(ty, false);
        builder.name_map.insert(param.name.clone(), local_id);
    }
    let param_count = function.params.len();
    builder.param_count = param_count;

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

    // Emit DecRef for heap-allocated locals before the implicit return.
    builder.emit_decref_for_heap_locals(param_count, None);

    // If the current block still has no terminator (i.e. it was not
    // sealed by a Return statement), seal it with Return(Unit).
    builder.seal_block_final(Terminator::Return(Value::Unit));

    // Resolve the return type.
    let return_type = resolve_type_with_enums(&function.return_type, function.span, &enum_names)
        .map_err(|e| MirError::TypeResolution(e.to_string()))?;

    let generated_closures = builder.generated_closures;

    Ok((
        MirFunction {
            name: function.name.clone(),
            return_type,
            param_count,
            locals: builder.locals,
            blocks: builder.blocks,
            entry: BlockId(0),
        },
        generated_closures,
    ))
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

/// Registers return types for builtin functions so that MIR locals receiving
/// their results get the correct type (e.g. `Type::String` for `Int_to_string`).
fn register_builtin_return_types(fn_return_types: &mut HashMap<String, Type>) {
    // Builtins that return String.
    for name in &[
        "Int_to_string",
        "Float64_to_string",
        "String_trim",
        "String_to_upper",
        "String_to_lower",
        "String_substring",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::String);
    }
    // Builtins that return Int.
    for name in &[
        "String_length",
        "String_contains",
        "String_starts_with",
        "String_ends_with",
        "abs",
        "min",
        "max",
        "clamp",
        "list_length",
        "list_contains",
        "map_length",
        "map_contains_key",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    // Time builtins.
    for name in &["time_now", "time_now_ms", "time_elapsed_ms"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    fn_return_types
        .entry("time_format".to_string())
        .or_insert(Type::String);
    fn_return_types
        .entry("env_get".to_string())
        .or_insert(Type::String);

    // Actor runtime builtins.
    fn_return_types
        .entry("kodo_actor_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_actor_get_field".to_string())
        .or_insert(Type::Int);

    // Channel builtins.
    fn_return_types
        .entry("channel_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("channel_recv".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("channel_recv_bool".to_string())
        .or_insert(Type::Bool);
    fn_return_types
        .entry("channel_recv_string".to_string())
        .or_insert(Type::String);
}

/// Lowers all functions in a [`Module`] into a `Vec` of [`MirFunction`],
/// using pre-built registries from the type checker (including monomorphized generics).
///
/// # Errors
///
/// Returns the first [`MirError`] encountered during lowering.
pub fn lower_module_with_type_info<S: std::hash::BuildHasher>(
    module: &Module,
    struct_registry: &HashMap<String, Vec<(String, Type)>, S>,
    enum_registry: &HashMap<String, Vec<(String, Vec<Type>)>, S>,
    enum_names: &std::collections::HashSet<String, S>,
    type_alias_registry: &HashMap<String, (Type, Option<kodo_ast::Expr>), S>,
) -> Result<Vec<MirFunction>> {
    // Copy to standard HashMaps for internal use.
    let struct_reg: HashMap<String, Vec<(String, Type)>> = struct_registry
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let enum_reg: HashMap<String, Vec<(String, Vec<Type>)>> = enum_registry
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let enum_ns: std::collections::HashSet<String> = enum_names.iter().cloned().collect();
    let alias_reg: HashMap<String, (Type, Option<kodo_ast::Expr>)> = type_alias_registry
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Build function return type registry.
    let mut fn_return_types: HashMap<String, Type> = HashMap::new();
    for func in &module.functions {
        if !func.generic_params.is_empty() {
            continue;
        }
        let ret_ty = resolve_type_with_enums(&func.return_type, func.span, &enum_ns)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        fn_return_types.insert(func.name.clone(), ret_ty);
    }
    register_builtin_return_types(&mut fn_return_types);

    // Collect actor names so the builder can distinguish actors from structs.
    let actor_names: HashSet<String> = module.actor_decls.iter().map(|a| a.name.clone()).collect();

    let mut mir_functions: Vec<MirFunction> = Vec::new();
    for f in module
        .functions
        .iter()
        .filter(|f| f.generic_params.is_empty())
    {
        let (mut func, mut closures) = lower_function_with_closures(
            f,
            &struct_reg,
            &enum_reg,
            &fn_return_types,
            &actor_names,
            &alias_reg,
        )?;
        crate::optimize::optimize_function(&mut func);
        for c in &mut closures {
            crate::optimize::optimize_function(c);
        }
        mir_functions.push(func);
        mir_functions.extend(closures);
    }

    // Lower actor handler functions with mangled names.
    for actor_decl in &module.actor_decls {
        for handler in &actor_decl.handlers {
            let mangled_name = format!("{}_{}", actor_decl.name, handler.name);
            let mut renamed = handler.clone();
            renamed.name.clone_from(&mangled_name);
            let ret_ty = resolve_type_with_enums(&renamed.return_type, renamed.span, &enum_ns)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fn_return_types.insert(mangled_name, ret_ty);
            let (mut func, mut closures) = lower_function_with_closures(
                &renamed,
                &struct_reg,
                &enum_reg,
                &fn_return_types,
                &actor_names,
                &alias_reg,
            )?;
            crate::optimize::optimize_function(&mut func);
            for c in &mut closures {
                crate::optimize::optimize_function(c);
            }
            mir_functions.push(func);
            mir_functions.extend(closures);
        }
    }

    // Generate validator functions for contracts.
    for func in &module.functions {
        if func.requires.is_empty() || !func.generic_params.is_empty() {
            continue;
        }
        let mut validator = generate_validator(func)?;
        crate::optimize::optimize_function(&mut validator);
        mir_functions.push(validator);
    }

    Ok(mir_functions)
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
#[allow(clippy::too_many_lines)]
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

    // Register actor fields in the struct registry (mirrors what the type
    // checker does) so that field access and struct literal lowering can
    // resolve field names and offsets.
    for actor_decl in &module.actor_decls {
        let mut fields = Vec::new();
        for field in &actor_decl.fields {
            let ty = resolve_type(&field.ty, field.span)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fields.push((field.name.clone(), ty));
        }
        struct_registry.insert(actor_decl.name.clone(), fields);
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

    // Build function return type registry.
    let enum_names: std::collections::HashSet<String> = enum_registry.keys().cloned().collect();
    let mut fn_return_types: HashMap<String, Type> = HashMap::new();
    for func in &module.functions {
        if !func.generic_params.is_empty() {
            continue;
        }
        let ret_ty = resolve_type_with_enums(&func.return_type, func.span, &enum_names)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        fn_return_types.insert(func.name.clone(), ret_ty);
    }
    register_builtin_return_types(&mut fn_return_types);

    // Collect actor names so the builder can distinguish actors from structs.
    let actor_names: HashSet<String> = module.actor_decls.iter().map(|a| a.name.clone()).collect();

    // Build type alias registry from module type aliases.
    let mut alias_reg: HashMap<String, (Type, Option<kodo_ast::Expr>)> = HashMap::new();
    for alias in &module.type_aliases {
        let base_ty = resolve_type(&alias.base_type, alias.span)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        alias_reg.insert(alias.name.clone(), (base_ty, alias.constraint.clone()));
    }

    let mut mir_functions: Vec<MirFunction> = Vec::new();
    for f in module
        .functions
        .iter()
        .filter(|f| f.generic_params.is_empty())
    {
        let (mut func, mut closures) = lower_function_with_closures(
            f,
            &struct_registry,
            &enum_registry,
            &fn_return_types,
            &actor_names,
            &alias_reg,
        )?;
        crate::optimize::optimize_function(&mut func);
        for c in &mut closures {
            crate::optimize::optimize_function(c);
        }
        mir_functions.push(func);
        mir_functions.extend(closures);
    }

    // Lower actor handler functions with mangled names.
    for actor_decl in &module.actor_decls {
        for handler in &actor_decl.handlers {
            let mangled_name = format!("{}_{}", actor_decl.name, handler.name);
            let mut renamed = handler.clone();
            renamed.name.clone_from(&mangled_name);
            let ret_ty = resolve_type_with_enums(&renamed.return_type, renamed.span, &enum_names)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fn_return_types.insert(mangled_name, ret_ty);
            let (mut func, mut closures) = lower_function_with_closures(
                &renamed,
                &struct_registry,
                &enum_registry,
                &fn_return_types,
                &actor_names,
                &alias_reg,
            )?;
            crate::optimize::optimize_function(&mut func);
            for c in &mut closures {
                crate::optimize::optimize_function(c);
            }
            mir_functions.push(func);
            mir_functions.extend(closures);
        }
    }

    // Generate validator functions for contracts.
    for func in &module.functions {
        if func.requires.is_empty() || !func.generic_params.is_empty() {
            continue;
        }
        let mut validator = generate_validator(func)?;
        crate::optimize::optimize_function(&mut validator);
        mir_functions.push(validator);
    }

    Ok(mir_functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{
        ActorDecl, BinOp, Block, Expr, FieldDef, FieldInit, Function, Meta, MetaEntry, Module,
        NodeId, Ownership, Param, Span, Stmt, TypeDecl, TypeExpr,
    };

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
            is_async: false,
            generic_params: vec![],
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
                ownership: Ownership::Owned,
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
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
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
                ownership: Ownership::Owned,
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
            is_async: false,
            generic_params: vec![],
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
            is_async: false,
            generic_params: vec![],
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
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                    ownership: Ownership::Owned,
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
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
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
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![
                    Param {
                        name: "x".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                        ownership: Ownership::Owned,
                    },
                    Param {
                        name: "y".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                        ownership: Ownership::Owned,
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
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                    ownership: Ownership::Owned,
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
            is_async: false,
            generic_params: vec![],
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

    #[test]
    fn lower_for_creates_loop_cfg() {
        // fn sum() { let mut s: Int = 0; for i in 0..5 { s = s + i } }
        let func = make_fn(
            "sum",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: true,
                        name: "s".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(0, span()),
                    },
                    Stmt::For {
                        span: span(),
                        name: "i".to_string(),
                        start: Expr::IntLit(0, span()),
                        end: Expr::IntLit(5, span()),
                        inclusive: false,
                        body: Block {
                            span: span(),
                            stmts: vec![Stmt::Assign {
                                span: span(),
                                name: "s".to_string(),
                                value: Expr::BinaryOp {
                                    left: Box::new(Expr::Ident("s".to_string(), span())),
                                    op: BinOp::Add,
                                    right: Box::new(Expr::Ident("i".to_string(), span())),
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
            "expected at least 4 blocks for for loop, got {}",
            mir.blocks.len()
        );
        // First block should have a Goto to the header
        assert!(
            matches!(mir.blocks[0].terminator, Terminator::Goto(_)),
            "entry should goto loop header"
        );
    }

    #[test]
    fn lower_for_inclusive_creates_loop_cfg() {
        // fn sum() { for i in 0..=3 { } }
        let func = make_fn(
            "sum_inc",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::For {
                    span: span(),
                    name: "i".to_string(),
                    start: Expr::IntLit(0, span()),
                    end: Expr::IntLit(3, span()),
                    inclusive: true,
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
        assert!(
            mir.blocks.len() >= 4,
            "expected at least 4 blocks for inclusive for loop, got {}",
            mir.blocks.len()
        );
    }

    #[test]
    fn lower_closure_without_captures() {
        // fn main() { let f = |x: Int| x * 2; f(21) }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "f".to_string(),
                        ty: None,
                        value: Expr::Closure {
                            params: vec![kodo_ast::ClosureParam {
                                name: "x".to_string(),
                                ty: Some(TypeExpr::Named("Int".to_string())),
                                span: span(),
                            }],
                            return_type: None,
                            body: Box::new(Expr::BinaryOp {
                                left: Box::new(Expr::Ident("x".to_string(), span())),
                                op: BinOp::Mul,
                                right: Box::new(Expr::IntLit(2, span())),
                                span: span(),
                            }),
                            span: span(),
                        },
                    },
                    Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("f".to_string(), span())),
                        args: vec![Expr::IntLit(21, span())],
                        span: span(),
                    }),
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have instructions for the closure call.
        assert!(mir.blocks[0].instructions.len() >= 2);
    }

    #[test]
    fn lower_closure_with_captures() {
        // fn main() { let a: Int = 10; let f = |x: Int| x + a; f(5) }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "a".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(10, span()),
                    },
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "f".to_string(),
                        ty: None,
                        value: Expr::Closure {
                            params: vec![kodo_ast::ClosureParam {
                                name: "x".to_string(),
                                ty: Some(TypeExpr::Named("Int".to_string())),
                                span: span(),
                            }],
                            return_type: None,
                            body: Box::new(Expr::BinaryOp {
                                left: Box::new(Expr::Ident("x".to_string(), span())),
                                op: BinOp::Add,
                                right: Box::new(Expr::Ident("a".to_string(), span())),
                                span: span(),
                            }),
                            span: span(),
                        },
                    },
                    Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("f".to_string(), span())),
                        args: vec![Expr::IntLit(5, span())],
                        span: span(),
                    }),
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Check that the call includes an extra captured argument.
        let has_call_with_2_args = mir.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, Instruction::Call { callee, args, .. }
                    if callee.starts_with("__closure_") && args.len() == 2)
            })
        });
        assert!(
            has_call_with_2_args,
            "expected a call to __closure_N with 2 args (capture + param)"
        );
    }

    #[test]
    fn lower_indirect_call_via_function_param() {
        // fn apply(f: (Int) -> Int, x: Int) -> Int { return f(x) }
        let func = make_fn(
            "apply",
            vec![
                kodo_ast::Param {
                    name: "f".to_string(),
                    ty: TypeExpr::Function(
                        vec![TypeExpr::Named("Int".to_string())],
                        Box::new(TypeExpr::Named("Int".to_string())),
                    ),
                    span: span(),
                    ownership: kodo_ast::Ownership::Owned,
                },
                kodo_ast::Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                    ownership: kodo_ast::Ownership::Owned,
                },
            ],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Call {
                        callee: Box::new(Expr::Ident("f".to_string(), span())),
                        args: vec![Expr::Ident("x".to_string(), span())],
                        span: span(),
                    }),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have an IndirectCall instruction for f(x).
        let has_indirect_call = mir.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::IndirectCall { .. }))
        });
        assert!(
            has_indirect_call,
            "expected an IndirectCall for calling function parameter"
        );
    }

    #[test]
    fn lower_closure_assigned_with_function_type() {
        // fn main() { let f: (Int) -> Int = |x: Int| -> Int { x + 1 }; f(41) }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "f".to_string(),
                        ty: Some(TypeExpr::Function(
                            vec![TypeExpr::Named("Int".to_string())],
                            Box::new(TypeExpr::Named("Int".to_string())),
                        )),
                        value: Expr::Closure {
                            params: vec![kodo_ast::ClosureParam {
                                name: "x".to_string(),
                                ty: Some(TypeExpr::Named("Int".to_string())),
                                span: span(),
                            }],
                            return_type: Some(TypeExpr::Named("Int".to_string())),
                            body: Box::new(Expr::BinaryOp {
                                left: Box::new(Expr::Ident("x".to_string(), span())),
                                op: BinOp::Add,
                                right: Box::new(Expr::IntLit(1, span())),
                                span: span(),
                            }),
                            span: span(),
                        },
                    },
                    Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("f".to_string(), span())),
                        args: vec![Expr::IntLit(41, span())],
                        span: span(),
                    }),
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // When the let has a Function type annotation, f should be called
        // via IndirectCall since local_types maps f to Type::Function.
        // Note: Closures registered in closure_registry are still called
        // directly. The indirect call path applies when the variable is typed
        // as a function but not in the closure registry.
        // With the current implementation, closure registry takes priority,
        // so this may use a direct Call instead.
        let has_any_call = mir.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(
                    i,
                    Instruction::Call { .. } | Instruction::IndirectCall { .. }
                )
            })
        });
        assert!(has_any_call, "expected a call instruction for f(41)");
    }

    #[test]
    fn test_builtin_return_types_registered() {
        let mut types = HashMap::new();
        register_builtin_return_types(&mut types);

        // String-returning builtins
        assert_eq!(types.get("Int_to_string"), Some(&Type::String));
        assert_eq!(types.get("Float64_to_string"), Some(&Type::String));
        assert_eq!(types.get("String_trim"), Some(&Type::String));
        assert_eq!(types.get("String_to_upper"), Some(&Type::String));
        assert_eq!(types.get("String_to_lower"), Some(&Type::String));
        assert_eq!(types.get("String_substring"), Some(&Type::String));

        // Int-returning builtins
        assert_eq!(types.get("String_length"), Some(&Type::Int));
        assert_eq!(types.get("abs"), Some(&Type::Int));
        assert_eq!(types.get("min"), Some(&Type::Int));
        assert_eq!(types.get("max"), Some(&Type::Int));
        assert_eq!(types.get("clamp"), Some(&Type::Int));
        assert_eq!(types.get("list_length"), Some(&Type::Int));
        assert_eq!(types.get("map_length"), Some(&Type::Int));
        assert_eq!(types.get("map_contains_key"), Some(&Type::Int));
    }

    #[test]
    fn test_field_access_type_resolution() {
        // Create a module with:
        // struct Point { x: Int, y: Int }
        // fn get_x(p: Point) -> Int { return p.x }
        let module = Module {
            id: NodeId(0),
            name: "test".to_string(),
            span: span(),
            meta: Some(Meta {
                id: NodeId(2),
                span: span(),
                entries: vec![
                    MetaEntry {
                        key: "purpose".to_string(),
                        value: "test".to_string(),
                        span: span(),
                    },
                    MetaEntry {
                        key: "version".to_string(),
                        value: "1.0.0".to_string(),
                        span: span(),
                    },
                ],
            }),
            imports: vec![],
            type_aliases: vec![],
            type_decls: vec![TypeDecl {
                id: NodeId(1),
                name: "Point".to_string(),
                span: span(),
                generic_params: vec![],
                fields: vec![
                    FieldDef {
                        name: "x".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                    },
                    FieldDef {
                        name: "y".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                    },
                ],
            }],
            enum_decls: vec![],
            functions: vec![make_fn(
                "get_x",
                vec![Param {
                    name: "p".to_string(),
                    ty: TypeExpr::Named("Point".to_string()),
                    ownership: Ownership::Owned,
                    span: span(),
                }],
                Block {
                    span: span(),
                    stmts: vec![Stmt::Return {
                        span: span(),
                        value: Some(Expr::FieldAccess {
                            object: Box::new(Expr::Ident("p".to_string(), span())),
                            field: "x".to_string(),
                            span: span(),
                        }),
                    }],
                },
                TypeExpr::Named("Int".to_string()),
            )],
            intent_decls: vec![],
            impl_blocks: vec![],
            trait_decls: vec![],
            actor_decls: vec![],
        };

        let struct_registry = HashMap::from([(
            "Point".to_string(),
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        )]);
        let enum_registry: HashMap<String, Vec<(String, Vec<Type>)>> = HashMap::new();
        let enum_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        let type_alias_registry: HashMap<String, (Type, Option<kodo_ast::Expr>)> = HashMap::new();
        let result = lower_module_with_type_info(
            &module,
            &struct_registry,
            &enum_registry,
            &enum_names,
            &type_alias_registry,
        );
        assert!(result.is_ok(), "field access lowering failed: {result:?}");

        let mir_functions = result.unwrap();
        // Find the get_x function
        let get_x = mir_functions
            .iter()
            .find(|f| f.name == "get_x")
            .expect("get_x not found");
        // Verify the return type is Int
        assert_eq!(get_x.return_type, Type::Int);
    }

    #[test]
    fn lower_spawn_without_captures() {
        // fn main() { spawn { print_int(42) } }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Spawn {
                    span: span(),
                    body: Block {
                        span: span(),
                        stmts: vec![Stmt::Expr(Expr::Call {
                            span: span(),
                            callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                            args: vec![Expr::IntLit(42, span())],
                        })],
                    },
                }],
            },
            TypeExpr::Unit,
        );
        let (mir, closures) = lower_function_with_closures(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
        .unwrap();
        mir.validate().unwrap();

        // Should generate a __spawn_ function and call kodo_spawn_task.
        assert!(!closures.is_empty(), "expected a generated spawn function");
        let has_spawn_task_call = mir.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_spawn_task"),
            )
        });
        assert!(has_spawn_task_call, "expected kodo_spawn_task call");
    }

    #[test]
    fn lower_spawn_with_captures() {
        // fn main() { let x: Int = 10; spawn { print_int(x) } }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        mutable: false,
                        value: Expr::IntLit(10, span()),
                    },
                    Stmt::Spawn {
                        span: span(),
                        body: Block {
                            span: span(),
                            stmts: vec![Stmt::Expr(Expr::Call {
                                span: span(),
                                callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                                args: vec![Expr::Ident("x".to_string(), span())],
                            })],
                        },
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let (mir, closures) = lower_function_with_closures(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
        .unwrap();
        mir.validate().unwrap();

        // Should generate a __spawn_ function that takes 1 param (env ptr).
        let spawn_fn = closures
            .iter()
            .find(|f| f.name.starts_with("__spawn_"))
            .expect("expected a __spawn_ function");
        assert_eq!(spawn_fn.param_count, 1, "spawn fn should take env pointer");

        // Main should call __env_pack and kodo_spawn_task_with_env.
        let has_env_pack = mir.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "__env_pack"))
        });
        assert!(has_env_pack, "expected __env_pack call in main");

        let has_spawn_with_env = mir.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, Instruction::Call { callee, .. } if callee == "kodo_spawn_task_with_env")
            })
        });
        assert!(has_spawn_with_env, "expected kodo_spawn_task_with_env call");

        // The spawn function should contain an __env_load call.
        let has_env_load = spawn_fn.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "__env_load"))
        });
        assert!(has_env_load, "spawn fn should unpack env with __env_load");
    }

    /// Helper to build a module with a Counter actor and a main function.
    fn make_actor_module(main_body: Block) -> Module {
        Module {
            id: NodeId(0),
            span: span(),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(Meta {
                id: NodeId(99),
                entries: vec![MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: span(),
                }],
                span: span(),
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![ActorDecl {
                id: NodeId(1),
                span: span(),
                name: "Counter".to_string(),
                fields: vec![FieldDef {
                    name: "count".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                }],
                handlers: vec![make_fn(
                    "increment",
                    vec![Param {
                        name: "self".to_string(),
                        ty: TypeExpr::Named("Counter".to_string()),
                        ownership: Ownership::Owned,
                        span: span(),
                    }],
                    Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::BinaryOp {
                                left: Box::new(Expr::FieldAccess {
                                    object: Box::new(Expr::Ident("self".to_string(), span())),
                                    field: "count".to_string(),
                                    span: span(),
                                }),
                                op: BinOp::Add,
                                right: Box::new(Expr::IntLit(1, span())),
                                span: span(),
                            }),
                        }],
                    },
                    TypeExpr::Named("Int".to_string()),
                )],
            }],
            intent_decls: vec![],
            functions: vec![make_fn("main", vec![], main_body, TypeExpr::Unit)],
        }
    }

    #[test]
    fn actor_instantiation_emits_actor_new_and_set_field() {
        // let c = Counter { count: 42 }
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "c".to_string(),
                ty: Some(TypeExpr::Named("Counter".to_string())),
                value: Expr::StructLit {
                    name: "Counter".to_string(),
                    fields: vec![FieldInit {
                        name: "count".to_string(),
                        value: Expr::IntLit(42, span()),
                        span: span(),
                    }],
                    span: span(),
                },
            }],
        });

        let mir_fns = lower_module(&module).unwrap();
        let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

        // Should have a kodo_actor_new call.
        let has_actor_new = main.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_new"),
            )
        });
        assert!(has_actor_new, "expected kodo_actor_new call");

        // Should have a kodo_actor_set_field call.
        let has_set_field = main.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_set_field"),
            )
        });
        assert!(has_set_field, "expected kodo_actor_set_field call");

        // Should NOT have a StructLit instruction (actors use runtime calls).
        let has_struct_lit = main.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, Instruction::Assign(_, Value::StructLit { name, .. }) if name == "Counter")
            })
        });
        assert!(!has_struct_lit, "actor should not produce StructLit MIR");
    }

    #[test]
    fn actor_field_access_emits_get_field() {
        // let c = Counter { count: 10 }
        // let v: Int = c.count
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "c".to_string(),
                    ty: Some(TypeExpr::Named("Counter".to_string())),
                    value: Expr::StructLit {
                        name: "Counter".to_string(),
                        fields: vec![FieldInit {
                            name: "count".to_string(),
                            value: Expr::IntLit(10, span()),
                            span: span(),
                        }],
                        span: span(),
                    },
                },
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "v".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::FieldAccess {
                        object: Box::new(Expr::Ident("c".to_string(), span())),
                        field: "count".to_string(),
                        span: span(),
                    },
                },
            ],
        });

        let mir_fns = lower_module(&module).unwrap();
        let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

        // Should have a kodo_actor_get_field call.
        let has_get_field = main.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_get_field"),
            )
        });
        assert!(has_get_field, "expected kodo_actor_get_field call");

        // Should NOT have a FieldGet instruction (actors use runtime calls).
        let has_field_get = main.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, Instruction::Assign(_, Value::FieldGet { struct_name, .. }) if struct_name == "Counter")
            })
        });
        assert!(
            !has_field_get,
            "actor field access should not produce FieldGet MIR"
        );
    }

    #[test]
    fn actor_handler_call_emits_send() {
        // let c = Counter { count: 0 }
        // Counter_increment(c) — already rewritten from c.increment()
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "c".to_string(),
                    ty: Some(TypeExpr::Named("Counter".to_string())),
                    value: Expr::StructLit {
                        name: "Counter".to_string(),
                        fields: vec![FieldInit {
                            name: "count".to_string(),
                            value: Expr::IntLit(0, span()),
                            span: span(),
                        }],
                        span: span(),
                    },
                },
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("Counter_increment".to_string(), span())),
                    args: vec![Expr::Ident("c".to_string(), span())],
                    span: span(),
                }),
            ],
        });

        let mir_fns = lower_module(&module).unwrap();
        let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

        // Should have a kodo_actor_send call.
        let has_send = main.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_send"),
            )
        });
        assert!(has_send, "expected kodo_actor_send call");

        // The args to kodo_actor_send should include a FuncRef to the handler.
        let has_func_ref = main.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| match i {
                Instruction::Call { callee, args, .. } if callee == "kodo_actor_send" => args
                    .iter()
                    .any(|a| matches!(a, Value::FuncRef(name) if name == "Counter_increment")),
                _ => false,
            })
        });
        assert!(
            has_func_ref,
            "kodo_actor_send should contain FuncRef(Counter_increment)"
        );
    }

    #[test]
    fn actor_handler_lowered_as_function() {
        // Verify that actor handlers are still lowered as standalone functions.
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![],
        });

        let mir_fns = lower_module(&module).unwrap();

        let handler = mir_fns.iter().find(|f| f.name == "Counter_increment");
        assert!(
            handler.is_some(),
            "expected Counter_increment function in MIR output"
        );
    }

    #[test]
    fn actor_handler_field_access_uses_get_field() {
        // The Counter_increment handler accesses self.count — verify it
        // emits kodo_actor_get_field instead of FieldGet.
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![],
        });

        let mir_fns = lower_module(&module).unwrap();
        let handler = mir_fns
            .iter()
            .find(|f| f.name == "Counter_increment")
            .unwrap();

        let has_get_field = handler.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_get_field"),
            )
        });
        assert!(
            has_get_field,
            "handler should use kodo_actor_get_field for self.count"
        );
    }

    #[test]
    fn is_actor_handler_helper() {
        let mut builder = MirBuilder::new();
        builder.actor_names.insert("Counter".to_string());
        builder.actor_names.insert("Logger".to_string());

        assert!(builder.is_actor_handler("Counter_increment"));
        assert!(builder.is_actor_handler("Logger_log"));
        assert!(!builder.is_actor_handler("Counter")); // no underscore suffix
        assert!(!builder.is_actor_handler("print_int")); // not an actor
        assert!(!builder.is_actor_handler("")); // empty string
    }

    #[test]
    fn refinement_check_emitted_for_refined_alias() {
        // Build a module with `type Port = Int requires { self > 0 }` and
        // `let port: Port = 8080`. Verify the MIR contains a contract check.
        let constraint = Expr::BinaryOp {
            left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
            op: kodo_ast::BinOp::Gt,
            right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
            span: Span::new(0, 10),
        };
        let module = Module {
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: NodeId(1),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![kodo_ast::TypeAlias {
                id: NodeId(2),
                span: Span::new(0, 30),
                name: "Port".to_string(),
                base_type: TypeExpr::Named("Int".to_string()),
                constraint: Some(constraint),
            }],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(3),
                span: Span::new(0, 80),
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: TypeExpr::Unit,
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 60),
                    stmts: vec![Stmt::Let {
                        span: Span::new(0, 20),
                        mutable: false,
                        name: "port".to_string(),
                        ty: Some(TypeExpr::Named("Port".to_string())),
                        value: Expr::IntLit(8080, Span::new(0, 4)),
                    }],
                },
            }],
        };

        let result = lower_module(&module);
        assert!(result.is_ok(), "refinement lowering failed: {result:?}");
        let fns = result.unwrap();
        let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

        // The MIR should have a kodo_contract_fail call for the refinement check.
        let has_contract_fail = main_fn.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
            )
        });
        assert!(
            has_contract_fail,
            "expected kodo_contract_fail call for refinement check"
        );

        // Should have at least 3 blocks: entry (with branch), fail block, continue block.
        assert!(
            main_fn.blocks.len() >= 3,
            "expected at least 3 blocks for refinement check, got {}",
            main_fn.blocks.len()
        );
    }

    #[test]
    fn no_refinement_check_for_unconstrained_alias() {
        // A type alias without a constraint should NOT emit any contract check.
        let module = Module {
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: NodeId(1),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![kodo_ast::TypeAlias {
                id: NodeId(2),
                span: Span::new(0, 30),
                name: "Name".to_string(),
                base_type: TypeExpr::Named("String".to_string()),
                constraint: None,
            }],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(3),
                span: Span::new(0, 80),
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: TypeExpr::Unit,
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 60),
                    stmts: vec![Stmt::Let {
                        span: Span::new(0, 20),
                        mutable: false,
                        name: "s".to_string(),
                        ty: Some(TypeExpr::Named("Name".to_string())),
                        value: Expr::StringLit("hello".to_string(), Span::new(0, 7)),
                    }],
                },
            }],
        };

        let result = lower_module(&module);
        assert!(result.is_ok(), "unconstrained alias lowering failed");
        let fns = result.unwrap();
        let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

        // No contract_fail call should exist for unconstrained aliases.
        let has_contract_fail = main_fn.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
            )
        });
        assert!(
            !has_contract_fail,
            "should NOT emit contract_fail for unconstrained alias"
        );
    }

    #[test]
    fn substitute_self_replaces_ident() {
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
            op: kodo_ast::BinOp::Gt,
            right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
            span: Span::new(0, 10),
        };
        let substituted = MirBuilder::substitute_self_in_expr(&expr, "port");
        match &substituted {
            Expr::BinaryOp { left, .. } => match left.as_ref() {
                Expr::Ident(name, _) => assert_eq!(name, "port"),
                other => panic!("expected Ident, got {other:?}"),
            },
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn substitute_self_preserves_non_self_idents() {
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Ident("other".to_string(), Span::new(0, 5))),
            op: kodo_ast::BinOp::Gt,
            right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
            span: Span::new(0, 10),
        };
        let substituted = MirBuilder::substitute_self_in_expr(&expr, "port");
        match &substituted {
            Expr::BinaryOp { left, .. } => match left.as_ref() {
                Expr::Ident(name, _) => assert_eq!(name, "other"),
                other => panic!("expected Ident, got {other:?}"),
            },
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn refinement_check_with_compound_constraint() {
        // Test `type Port = Int requires { self > 0 && self < 65535 }`
        let constraint = Expr::BinaryOp {
            left: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
                op: kodo_ast::BinOp::Gt,
                right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
                span: Span::new(0, 10),
            }),
            op: kodo_ast::BinOp::And,
            right: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
                op: kodo_ast::BinOp::Lt,
                right: Box::new(Expr::IntLit(65535, Span::new(0, 5))),
                span: Span::new(0, 15),
            }),
            span: Span::new(0, 20),
        };
        let module = Module {
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: NodeId(1),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![kodo_ast::TypeAlias {
                id: NodeId(2),
                span: Span::new(0, 30),
                name: "Port".to_string(),
                base_type: TypeExpr::Named("Int".to_string()),
                constraint: Some(constraint),
            }],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(3),
                span: Span::new(0, 80),
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: TypeExpr::Unit,
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 60),
                    stmts: vec![Stmt::Let {
                        span: Span::new(0, 20),
                        mutable: false,
                        name: "port".to_string(),
                        ty: Some(TypeExpr::Named("Port".to_string())),
                        value: Expr::IntLit(8080, Span::new(0, 4)),
                    }],
                },
            }],
        };

        let result = lower_module(&module);
        assert!(
            result.is_ok(),
            "compound constraint lowering failed: {result:?}"
        );
        let fns = result.unwrap();
        let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

        // Should have a contract_fail call.
        let has_contract_fail = main_fn.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
            )
        });
        assert!(
            has_contract_fail,
            "expected kodo_contract_fail for compound constraint"
        );

        // Verify the fail message references the alias name "Port".
        let fail_msg = main_fn.blocks.iter().find_map(|b| {
            b.instructions.iter().find_map(|i| {
                if let Instruction::Call { callee, args, .. } = i {
                    if callee == "kodo_contract_fail" {
                        if let Some(Value::StringConst(msg)) = args.first() {
                            return Some(msg.clone());
                        }
                    }
                }
                None
            })
        });
        assert!(fail_msg.is_some(), "expected a fail message");
        let msg = fail_msg.unwrap();
        assert!(
            msg.contains("Port"),
            "fail message should reference 'Port', got: {msg}"
        );
        assert!(
            msg.contains("port"),
            "fail message should reference 'port', got: {msg}"
        );
    }
}
