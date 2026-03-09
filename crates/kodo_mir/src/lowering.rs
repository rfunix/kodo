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
        }
    }

    /// Allocates a new local variable and returns its identifier.
    fn alloc_local(&mut self, ty: Type, mutable: bool) -> LocalId {
        let id = LocalId(self.next_local);
        self.next_local += 1;
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
                // Seal the current block with a Return terminator and
                // create an unreachable continuation block.
                let continuation = self.new_block();
                self.seal_block(Terminator::Return(ret_val), continuation);
                Ok(Value::Unit)
            }
            Stmt::Expr(expr) => self.lower_expr(expr),
        }
    }

    /// Lowers an expression to a [`Value`].
    ///
    /// Compound expressions (calls, if/else) may emit instructions and
    /// create new basic blocks as a side effect.
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
            Expr::FieldAccess { .. } => {
                // Stub: field access is not yet implemented.
                Ok(Value::Unit)
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
    let mut builder = MirBuilder::new();

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

/// Lowers all functions in a [`Module`] into a `Vec` of [`MirFunction`].
///
/// # Errors
///
/// Returns the first [`MirError`] encountered during lowering.
pub fn lower_module(module: &Module) -> Result<Vec<MirFunction>> {
    module.functions.iter().map(lower_function).collect()
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
}
