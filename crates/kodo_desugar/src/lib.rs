//! # `kodo_desugar` — AST Desugaring Pass
//!
//! Transforms syntactic sugar in the AST into simpler forms before
//! MIR lowering. This simplifies all subsequent compiler passes.
//!
//! Currently desugars:
//! - `for i in start..end { body }` into `let mut i = start; while i < end { body; i = i + 1 }`
//! - `for i in start..=end { body }` into `let mut i = start; while i <= end { body; i = i + 1 }`
//! - `expr ?? default` into `match expr { Option::Some(val) => val, Option::None => default }`
//! - `expr?` into `match expr { Result::Ok(val) => val, Result::Err(e) => return Result::Err(e) }`
//! - `expr?.field` into `match expr { Option::Some(val) => Option::Some(val.field), Option::None => Option::None }`
//!
//! ## Modules
//!
//! - `expr` — Recursive expression desugaring
//! - `stmt` — Statement dispatch and block desugaring
//! - `for_loop` — For and for-in loop transforms
//! - `operators` — Sugar operator transforms (`??`, `?`, `?.`, `is`)

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

pub(crate) mod expr;
pub(crate) mod for_loop;
pub(crate) mod operators;
pub(crate) mod stmt;

use kodo_ast::Module;

// Re-export the internal functions so they can be used across modules.
pub(crate) use expr::desugar_expr;
pub(crate) use stmt::desugar_block;

/// Desugars an entire module in-place.
///
/// This function walks all functions in the module and transforms
/// syntactic sugar into simpler AST forms.  Module-level `invariant`
/// conditions are injected as `requires` clauses into every function.
pub fn desugar_module(module: &mut Module) {
    // Collect invariant conditions to inject into every function.
    let invariant_conditions: Vec<kodo_ast::Expr> = module
        .invariants
        .iter()
        .map(|inv| inv.condition.clone())
        .collect();

    for func in &mut module.functions {
        inject_invariants(&invariant_conditions, func);
        desugar_block(&mut func.body);
    }
    for impl_block in &mut module.impl_blocks {
        for method in &mut impl_block.methods {
            inject_invariants(&invariant_conditions, method);
            desugar_block(&mut method.body);
        }
    }
    for actor_decl in &mut module.actor_decls {
        for handler in &mut actor_decl.handlers {
            inject_invariants(&invariant_conditions, handler);
            desugar_block(&mut handler.body);
        }
    }
}

/// Injects module-level invariant conditions as `requires` clauses
/// into a function.  Each invariant expression is prepended to the
/// function's existing `requires` list so that MIR lowering emits
/// the corresponding `kodo_contract_fail` calls.
fn inject_invariants(invariants: &[kodo_ast::Expr], func: &mut kodo_ast::Function) {
    if invariants.is_empty() {
        return;
    }
    let mut combined = invariants.to_vec();
    combined.append(&mut func.requires);
    func.requires = combined;
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{BinOp, Block, Expr, NodeIdGen, Pattern, Span, Stmt, StringPart};

    fn make_test_module(stmts: Vec<Stmt>) -> Module {
        let mut id_gen = NodeIdGen::new();
        Module {
            id: id_gen.next_id(),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: id_gen.next_id(),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            invariants: vec![],
            functions: vec![kodo_ast::Function {
                id: id_gen.next_id(),
                span: Span::new(0, 100),
                name: "main".to_string(),
                visibility: kodo_ast::Visibility::Private,
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 100),
                    stmts,
                },
            }],
        }
    }

    #[test]
    fn desugar_for_loop_exclusive() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2, "for should desugar into let + while");
        assert!(matches!(stmts[0], Stmt::Let { mutable: true, .. }));
        assert!(matches!(stmts[1], Stmt::While { .. }));

        if let Stmt::While { body, .. } = &stmts[1] {
            assert_eq!(body.stmts.len(), 1, "while body should have increment");
            assert!(matches!(body.stmts[0], Stmt::Assign { .. }));
        }
    }

    #[test]
    fn desugar_for_loop_inclusive() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: true,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { op, .. } = condition {
                assert_eq!(*op, BinOp::Le);
            } else {
                panic!("expected BinaryOp condition");
            }
        }
    }

    #[test]
    fn desugar_preserves_non_for_stmts() {
        let let_stmt = Stmt::Let {
            span: Span::new(0, 20),
            mutable: false,
            name: "x".to_string(),
            ty: None,
            value: Expr::IntLit(42, Span::new(10, 12)),
        };
        let mut module = make_test_module(vec![let_stmt]);
        desugar_module(&mut module);

        assert_eq!(module.functions[0].body.stmts.len(), 1);
        assert!(matches!(
            module.functions[0].body.stmts[0],
            Stmt::Let { .. }
        ));
    }

    #[test]
    fn desugar_idempotent() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);
        let count_after_first = module.functions[0].body.stmts.len();
        desugar_module(&mut module);
        let count_after_second = module.functions[0].body.stmts.len();
        assert_eq!(
            count_after_first, count_after_second,
            "desugaring should be idempotent"
        );
    }

    #[test]
    fn desugar_nested_for_in_while() {
        let inner_for = Stmt::For {
            span: Span::new(30, 45),
            name: "j".to_string(),
            start: Expr::IntLit(0, Span::new(35, 36)),
            end: Expr::IntLit(5, Span::new(39, 40)),
            inclusive: false,
            body: Block {
                span: Span::new(41, 45),
                stmts: vec![],
            },
        };
        let while_stmt = Stmt::While {
            span: Span::new(0, 50),
            condition: Expr::BoolLit(true, Span::new(6, 10)),
            body: Block {
                span: Span::new(12, 50),
                stmts: vec![inner_for],
            },
        };
        let mut module = make_test_module(vec![while_stmt]);
        desugar_module(&mut module);

        assert_eq!(module.functions[0].body.stmts.len(), 1);
        if let Stmt::While { body, .. } = &module.functions[0].body.stmts[0] {
            assert_eq!(body.stmts.len(), 2);
            assert!(matches!(body.stmts[0], Stmt::Let { mutable: true, .. }));
            assert!(matches!(body.stmts[1], Stmt::While { .. }));
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_null_coalesce() {
        let span = Span::new(0, 20);
        let coalesce = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("opt".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        };
        let stmt = Stmt::Expr(coalesce);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant { variant, .. } if variant == "Some"
            ));
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant { variant, .. } if variant == "None"
            ));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_try_operator() {
        let span = Span::new(0, 10);
        let try_expr = Expr::Try {
            operand: Box::new(Expr::Ident("result".to_string(), span)),
            span,
        };
        let stmt = Stmt::Expr(try_expr);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant { variant, .. } if variant == "Ok"
            ));
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant { variant, .. } if variant == "Err"
            ));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_optional_chain() {
        let span = Span::new(0, 15);
        let chain = Expr::OptionalChain {
            object: Box::new(Expr::Ident("opt_point".to_string(), span)),
            field: "x".to_string(),
            span,
        };
        let stmt = Stmt::Expr(chain);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            if let Expr::EnumVariantExpr { variant, args, .. } = &arms[0].body {
                assert_eq!(variant, "Some");
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0], Expr::FieldAccess { field, .. } if field == "x"));
            } else {
                panic!("expected EnumVariantExpr in Some arm body");
            }
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_if_let_to_match() {
        let span = Span::new(0, 50);
        let if_let_stmt = Stmt::IfLet {
            span,
            pattern: Pattern::Variant {
                enum_name: Some("Option".to_string()),
                variant: "Some".to_string(),
                bindings: vec!["v".to_string()],
                span,
            },
            value: Expr::Ident("opt".to_string(), span),
            body: Block {
                span,
                stmts: vec![Stmt::Return {
                    span,
                    value: Some(Expr::Ident("v".to_string(), span)),
                }],
            },
            else_body: Some(Block {
                span,
                stmts: vec![Stmt::Return {
                    span,
                    value: Some(Expr::IntLit(0, span)),
                }],
            }),
        };
        let mut module = make_test_module(vec![if_let_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant { variant, .. } if variant == "Some"
            ));
            assert!(matches!(&arms[1].pattern, Pattern::Wildcard(_)));
        } else {
            panic!("expected Match expression, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn desugar_is_expression() {
        let span = Span::new(0, 20);
        let is_expr = Expr::Is {
            operand: Box::new(Expr::Ident("opt".to_string(), span)),
            type_name: "Some".to_string(),
            span,
        };
        let stmt = Stmt::Expr(is_expr);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            assert!(matches!(&arms[0].body, Expr::BoolLit(true, _)));
            assert!(matches!(&arms[1].body, Expr::BoolLit(false, _)));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_for_loop_inclusive_uses_le() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: true,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![Stmt::Expr(Expr::Ident("i".to_string(), Span::new(25, 26)))],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        if let Stmt::While {
            condition, body, ..
        } = &stmts[1]
        {
            if let Expr::BinaryOp { op, .. } = condition {
                assert_eq!(*op, BinOp::Le, "inclusive range should use <= operator");
            } else {
                panic!("expected BinaryOp condition");
            }
            assert_eq!(body.stmts.len(), 2);
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_for_loop_exclusive_uses_lt() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![Stmt::Expr(Expr::Ident("i".to_string(), Span::new(25, 26)))],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { op, .. } = condition {
                assert_eq!(*op, BinOp::Lt, "exclusive range should use < operator");
            } else {
                panic!("expected BinaryOp condition");
            }
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_nested_for_loops() {
        let inner_for = Stmt::For {
            span: Span::new(30, 50),
            name: "j".to_string(),
            start: Expr::IntLit(0, Span::new(35, 36)),
            end: Expr::IntLit(3, Span::new(39, 40)),
            inclusive: false,
            body: Block {
                span: Span::new(41, 49),
                stmts: vec![],
            },
        };
        let outer_for = Stmt::For {
            span: Span::new(0, 55),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(5, Span::new(14, 15)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 55),
                stmts: vec![inner_for],
            },
        };
        let mut module = make_test_module(vec![outer_for]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        if let Stmt::While { body, .. } = &stmts[1] {
            assert_eq!(body.stmts.len(), 3);
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_null_coalesce_chain() {
        let span = Span::new(0, 30);
        let inner = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("a".to_string(), span)),
            right: Box::new(Expr::Ident("b".to_string(), span)),
            span,
        };
        let outer = Expr::NullCoalesce {
            left: Box::new(inner),
            right: Box::new(Expr::Ident("c".to_string(), span)),
            span,
        };
        let stmt = Stmt::Expr(outer);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, expr, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            assert!(matches!(expr.as_ref(), Expr::Match { .. }));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_optional_chain_nested() {
        let span = Span::new(0, 25);
        let inner_chain = Expr::OptionalChain {
            object: Box::new(Expr::Ident("obj".to_string(), span)),
            field: "field1".to_string(),
            span,
        };
        let outer_chain = Expr::OptionalChain {
            object: Box::new(inner_chain),
            field: "field2".to_string(),
            span,
        };
        let stmt = Stmt::Expr(outer_chain);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { expr, arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            assert!(matches!(expr.as_ref(), Expr::Match { .. }));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_empty_block_unchanged() {
        let mut module = make_test_module(vec![]);
        desugar_module(&mut module);
        assert!(module.functions[0].body.stmts.is_empty());
    }

    #[test]
    fn desugar_mixed_sugar_in_one_function() {
        let span = Span::new(0, 80);
        let for_stmt = Stmt::For {
            span,
            name: "i".to_string(),
            start: Expr::IntLit(0, span),
            end: Expr::IntLit(5, span),
            inclusive: false,
            body: Block {
                span,
                stmts: vec![],
            },
        };
        let coalesce_stmt = Stmt::Expr(Expr::NullCoalesce {
            left: Box::new(Expr::Ident("x".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        });
        let mut module = make_test_module(vec![for_stmt, coalesce_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 3, "should have let + while + match");
    }

    #[test]
    fn desugar_for_loop_variable_names_distinct() {
        let span = Span::new(0, 80);
        let for1 = Stmt::For {
            span,
            name: "i".to_string(),
            start: Expr::IntLit(0, span),
            end: Expr::IntLit(5, span),
            inclusive: false,
            body: Block {
                span,
                stmts: vec![],
            },
        };
        let for2 = Stmt::For {
            span,
            name: "j".to_string(),
            start: Expr::IntLit(0, span),
            end: Expr::IntLit(3, span),
            inclusive: false,
            body: Block {
                span,
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![for1, for2]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 4);
        if let Stmt::Let { name, .. } = &stmts[0] {
            assert_eq!(name, "i");
        }
        if let Stmt::Let { name, .. } = &stmts[2] {
            assert_eq!(name, "j");
        }
    }

    #[test]
    fn desugar_module_with_multiple_functions() {
        let span = Span::new(0, 100);
        let mut id_gen = NodeIdGen::new();
        let mut module = Module {
            id: id_gen.next_id(),
            span,
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: id_gen.next_id(),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            invariants: vec![],
            functions: vec![
                kodo_ast::Function {
                    id: id_gen.next_id(),
                    span,
                    name: "func_a".to_string(),
                    visibility: kodo_ast::Visibility::Private,
                    is_async: false,
                    generic_params: vec![],
                    annotations: vec![],
                    params: vec![],
                    return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                    requires: vec![],
                    ensures: vec![],
                    body: Block {
                        span,
                        stmts: vec![Stmt::For {
                            span,
                            name: "i".to_string(),
                            start: Expr::IntLit(0, span),
                            end: Expr::IntLit(3, span),
                            inclusive: false,
                            body: Block {
                                span,
                                stmts: vec![],
                            },
                        }],
                    },
                },
                kodo_ast::Function {
                    id: id_gen.next_id(),
                    span,
                    name: "func_b".to_string(),
                    visibility: kodo_ast::Visibility::Private,
                    is_async: false,
                    generic_params: vec![],
                    annotations: vec![],
                    params: vec![],
                    return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                    requires: vec![],
                    ensures: vec![],
                    body: Block {
                        span,
                        stmts: vec![Stmt::Expr(Expr::NullCoalesce {
                            left: Box::new(Expr::Ident("x".to_string(), span)),
                            right: Box::new(Expr::IntLit(0, span)),
                            span,
                        })],
                    },
                },
            ],
        };

        desugar_module(&mut module);
        assert_eq!(module.functions[0].body.stmts.len(), 2);
        assert_eq!(module.functions[1].body.stmts.len(), 1);
    }

    #[test]
    fn desugar_spawn_body_desugared() {
        let span = Span::new(0, 50);
        let spawn_stmt = Stmt::Spawn {
            span,
            body: Block {
                span: Span::new(5, 45),
                stmts: vec![Stmt::For {
                    span,
                    name: "i".to_string(),
                    start: Expr::IntLit(0, span),
                    end: Expr::IntLit(5, span),
                    inclusive: false,
                    body: Block {
                        span,
                        stmts: vec![],
                    },
                }],
            },
        };
        let mut module = make_test_module(vec![spawn_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Spawn { body, .. } = &stmts[0] {
            assert_eq!(body.stmts.len(), 2, "for in spawn should desugar");
        } else {
            panic!("expected Spawn statement");
        }
    }

    #[test]
    fn desugar_parallel_body_desugared() {
        let span = Span::new(0, 80);
        let parallel_stmt = Stmt::Parallel {
            span,
            body: vec![
                Stmt::Spawn {
                    span,
                    body: Block {
                        span,
                        stmts: vec![Stmt::For {
                            span,
                            name: "i".to_string(),
                            start: Expr::IntLit(0, span),
                            end: Expr::IntLit(3, span),
                            inclusive: false,
                            body: Block {
                                span,
                                stmts: vec![],
                            },
                        }],
                    },
                },
                Stmt::Spawn {
                    span,
                    body: Block {
                        span,
                        stmts: vec![Stmt::Expr(Expr::IntLit(42, span))],
                    },
                },
            ],
        };
        let mut module = make_test_module(vec![parallel_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Parallel { body, .. } = &stmts[0] {
            assert_eq!(body.len(), 2);
            if let Stmt::Spawn { body, .. } = &body[0] {
                assert_eq!(body.stmts.len(), 2, "for in spawn should desugar");
            }
        } else {
            panic!("expected Parallel statement");
        }
    }

    #[test]
    fn desugar_closure_body_desugared() {
        let span = Span::new(0, 30);
        let closure = Expr::Closure {
            params: vec![kodo_ast::ClosureParam {
                name: "x".to_string(),
                ty: None,
                span,
            }],
            return_type: None,
            body: Box::new(Expr::NullCoalesce {
                left: Box::new(Expr::Ident("x".to_string(), span)),
                right: Box::new(Expr::IntLit(0, span)),
                span,
            }),
            span,
        };
        let stmt = Stmt::Expr(closure);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Expr(Expr::Closure { body, .. }) = &stmts[0] {
            assert!(matches!(body.as_ref(), Expr::Match { .. }));
        } else {
            panic!("expected Closure expression");
        }
    }

    #[test]
    fn desugar_for_in_produces_while_loop() {
        let for_in = Stmt::ForIn {
            span: Span::new(0, 50),
            name: "x".to_string(),
            iterable: Expr::Ident("items".to_string(), Span::new(10, 15)),
            body: Block {
                span: Span::new(16, 50),
                stmts: vec![Stmt::Expr(Expr::Ident("x".to_string(), Span::new(20, 21)))],
            },
        };

        let mut block = Block {
            span: Span::new(0, 50),
            stmts: vec![for_in],
        };
        desugar_block(&mut block);

        // Iterator-based desugaring: let __iter_x = list_iter(...), while, free
        assert_eq!(block.stmts.len(), 3);
        assert!(matches!(&block.stmts[0], Stmt::Let { name, .. } if name == "__iter_x"));
        assert!(matches!(&block.stmts[1], Stmt::While { .. }));
        assert!(matches!(&block.stmts[2], Stmt::Expr(Expr::Call { .. })));
    }

    #[test]
    fn desugar_string_interp_in_module() {
        let span = Span::new(0, 30);
        let mut id_gen = NodeIdGen::new();
        let mut module = Module {
            id: id_gen.next_id(),
            span,
            name: "test".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            invariants: vec![],
            functions: vec![kodo_ast::Function {
                id: id_gen.next_id(),
                span,
                name: "main".to_string(),
                visibility: kodo_ast::Visibility::Private,
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: kodo_ast::TypeExpr::Named("String".to_string()),
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span,
                    stmts: vec![Stmt::Expr(Expr::StringInterp {
                        parts: vec![
                            StringPart::Literal("count: ".to_string()),
                            StringPart::Expr(Box::new(Expr::IntLit(42, span))),
                        ],
                        span,
                    })],
                },
            }],
        };
        desugar_module(&mut module);
        let body = &module.functions[0].body.stmts;
        assert_eq!(body.len(), 1);
        // StringInterp is now preserved through desugar (handled in MIR lowering).
        assert!(matches!(&body[0], Stmt::Expr(Expr::StringInterp { .. })));
    }

    // Additional modularization verification tests

    #[test]
    fn desugar_for_in_while_body_structure() {
        let for_in = Stmt::ForIn {
            span: Span::new(0, 50),
            name: "item".to_string(),
            iterable: Expr::Ident("data".to_string(), Span::new(10, 14)),
            body: Block {
                span: Span::new(15, 50),
                stmts: vec![Stmt::Expr(Expr::IntLit(42, Span::new(20, 22)))],
            },
        };

        let mut block = Block {
            span: Span::new(0, 50),
            stmts: vec![for_in],
        };
        desugar_block(&mut block);

        // Iterator-based: stmts[0] = let __iter_item, stmts[1] = while, stmts[2] = free
        if let Stmt::While { body, .. } = &block.stmts[1] {
            // while body: [let item = list_iterator_value(...), <original body>]
            assert!(matches!(&body.stmts[0], Stmt::Let { name, .. } if name == "item"));
            assert!(matches!(&body.stmts[1], Stmt::Expr(Expr::IntLit(42, _))));
            assert_eq!(body.stmts.len(), 2); // no more increment stmt
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_binary_op_with_nested_sugar() {
        let span = Span::new(0, 40);
        let left = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("a".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        };
        let right = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("b".to_string(), span)),
            right: Box::new(Expr::IntLit(1, span)),
            span,
        };
        let binop = Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::Add,
            right: Box::new(right),
            span,
        };
        let stmt = Stmt::Expr(binop);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        if let Stmt::Expr(Expr::BinaryOp { left, right, .. }) = &module.functions[0].body.stmts[0] {
            assert!(matches!(left.as_ref(), Expr::Match { .. }));
            assert!(matches!(right.as_ref(), Expr::Match { .. }));
        } else {
            panic!("expected BinaryOp expression");
        }
    }

    #[test]
    fn desugar_call_with_sugar_args() {
        let span = Span::new(0, 40);
        let call = Expr::Call {
            callee: Box::new(Expr::Ident("foo".to_string(), span)),
            args: vec![
                Expr::NullCoalesce {
                    left: Box::new(Expr::Ident("a".to_string(), span)),
                    right: Box::new(Expr::IntLit(0, span)),
                    span,
                },
                Expr::Try {
                    operand: Box::new(Expr::Ident("b".to_string(), span)),
                    span,
                },
            ],
            span,
        };
        let stmt = Stmt::Expr(call);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        if let Stmt::Expr(Expr::Call { args, .. }) = &module.functions[0].body.stmts[0] {
            assert!(matches!(&args[0], Expr::Match { .. }));
            assert!(matches!(&args[1], Expr::Match { .. }));
        } else {
            panic!("expected Call expression");
        }
    }

    // ── Phase 49: Invariant injection ────────────────────────────────

    #[test]
    fn desugar_invariant_injected_as_requires() {
        let mut id_gen = NodeIdGen::new();
        let span = Span::new(0, 100);
        let mut module = Module {
            id: id_gen.next_id(),
            span,
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: id_gen.next_id(),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            invariants: vec![kodo_ast::InvariantDecl {
                span,
                condition: Expr::BoolLit(true, span),
            }],
            functions: vec![kodo_ast::Function {
                id: id_gen.next_id(),
                span,
                name: "f".to_string(),
                visibility: kodo_ast::Visibility::Private,
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span,
                    stmts: vec![Stmt::Return {
                        span,
                        value: Some(Expr::IntLit(1, span)),
                    }],
                },
            }],
        };

        assert!(module.functions[0].requires.is_empty());
        desugar_module(&mut module);
        assert_eq!(
            module.functions[0].requires.len(),
            1,
            "invariant should be injected as requires"
        );
        assert!(matches!(
            module.functions[0].requires[0],
            Expr::BoolLit(true, _)
        ));
    }

    #[test]
    fn desugar_invariant_prepended_before_existing_requires() {
        let mut id_gen = NodeIdGen::new();
        let span = Span::new(0, 100);
        let mut module = Module {
            id: id_gen.next_id(),
            span,
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: id_gen.next_id(),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            invariants: vec![kodo_ast::InvariantDecl {
                span,
                condition: Expr::BoolLit(true, span),
            }],
            functions: vec![kodo_ast::Function {
                id: id_gen.next_id(),
                span,
                name: "f".to_string(),
                visibility: kodo_ast::Visibility::Private,
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![Expr::BoolLit(false, span)],
                ensures: vec![],
                body: Block {
                    span,
                    stmts: vec![Stmt::Return {
                        span,
                        value: Some(Expr::IntLit(1, span)),
                    }],
                },
            }],
        };

        desugar_module(&mut module);
        assert_eq!(
            module.functions[0].requires.len(),
            2,
            "should have invariant + original requires"
        );
        // Invariant comes first.
        assert!(matches!(
            module.functions[0].requires[0],
            Expr::BoolLit(true, _)
        ));
        // Original requires preserved.
        assert!(matches!(
            module.functions[0].requires[1],
            Expr::BoolLit(false, _)
        ));
    }

    #[test]
    fn desugar_no_invariant_no_change() {
        let mut module = make_test_module(vec![Stmt::Return {
            span: Span::new(0, 10),
            value: Some(Expr::IntLit(1, Span::new(0, 10))),
        }]);
        assert!(module.invariants.is_empty());
        let original_requires_len = module.functions[0].requires.len();
        desugar_module(&mut module);
        assert_eq!(module.functions[0].requires.len(), original_requires_len);
    }
}
