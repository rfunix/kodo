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

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use kodo_ast::{BinOp, Block, Expr, MatchArm, Module, Pattern, Stmt};

/// Desugars an entire module in-place.
///
/// This function walks all functions in the module and transforms
/// syntactic sugar into simpler AST forms.
pub fn desugar_module(module: &mut Module) {
    for func in &mut module.functions {
        desugar_block(&mut func.body);
    }
    for impl_block in &mut module.impl_blocks {
        for method in &mut impl_block.methods {
            desugar_block(&mut method.body);
        }
    }
    for actor_decl in &mut module.actor_decls {
        for handler in &mut actor_decl.handlers {
            desugar_block(&mut handler.body);
        }
    }
}

/// Desugars all statements in a block.
#[allow(clippy::too_many_lines)]
fn desugar_block(block: &mut Block) {
    let mut new_stmts = Vec::new();
    for stmt in std::mem::take(&mut block.stmts) {
        match stmt {
            Stmt::For {
                span,
                name,
                start,
                end,
                inclusive,
                body,
            } => {
                // Desugar: for i in start..end { body }
                // Into:    let mut i = start; while i < end { body; i = i + 1 }
                let mut desugared_body = body;
                desugar_block(&mut desugared_body);

                // Also desugar expressions in start/end
                let start = desugar_expr(start);
                let end = desugar_expr(end);

                // let mut i = start
                let let_stmt = Stmt::Let {
                    span,
                    mutable: true,
                    name: name.clone(),
                    ty: None,
                    value: start,
                };

                // i < end (or i <= end for inclusive)
                let op = if inclusive { BinOp::Le } else { BinOp::Lt };
                let condition = Expr::BinaryOp {
                    left: Box::new(Expr::Ident(name.clone(), span)),
                    op,
                    right: Box::new(end),
                    span,
                };

                // i = i + 1 (appended to body)
                let increment = Stmt::Assign {
                    span,
                    name: name.clone(),
                    value: Expr::BinaryOp {
                        left: Box::new(Expr::Ident(name.clone(), span)),
                        op: BinOp::Add,
                        right: Box::new(Expr::IntLit(1, span)),
                        span,
                    },
                };
                desugared_body.stmts.push(increment);

                let while_stmt = Stmt::While {
                    span,
                    condition,
                    body: desugared_body,
                };

                new_stmts.push(let_stmt);
                new_stmts.push(while_stmt);
            }
            Stmt::While {
                span,
                condition,
                mut body,
            } => {
                desugar_block(&mut body);
                let condition = desugar_expr(condition);
                new_stmts.push(Stmt::While {
                    span,
                    condition,
                    body,
                });
            }
            Stmt::Let {
                span,
                mutable,
                name,
                ty,
                value,
            } => {
                let value = desugar_expr(value);
                new_stmts.push(Stmt::Let {
                    span,
                    mutable,
                    name,
                    ty,
                    value,
                });
            }
            Stmt::Expr(expr) => {
                new_stmts.push(Stmt::Expr(desugar_expr(expr)));
            }
            Stmt::Return { span, value } => {
                let value = value.map(desugar_expr);
                new_stmts.push(Stmt::Return { span, value });
            }
            Stmt::Assign { span, name, value } => {
                let value = desugar_expr(value);
                new_stmts.push(Stmt::Assign { span, name, value });
            }
            Stmt::IfLet {
                span,
                pattern,
                value,
                mut body,
                else_body,
            } => {
                // Desugar: if let Pattern = expr { body } else { else_body }
                // Into:    match expr { Pattern => { body }, _ => { else_body } }
                desugar_block(&mut body);
                let value = desugar_expr(value);

                let mut else_block = else_body.unwrap_or(Block {
                    span,
                    stmts: vec![],
                });
                desugar_block(&mut else_block);

                // Build a block expression for each arm body
                let then_expr = Expr::Block(body);
                let else_expr = Expr::Block(else_block);

                let match_expr = Expr::Match {
                    expr: Box::new(value),
                    arms: vec![
                        MatchArm {
                            pattern,
                            body: then_expr,
                            span,
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard(span),
                            body: else_expr,
                            span,
                        },
                    ],
                    span,
                };
                new_stmts.push(Stmt::Expr(match_expr));
            }
            Stmt::Spawn { span, mut body } => {
                // V1: spawn executes inline — just desugar the body.
                desugar_block(&mut body);
                new_stmts.push(Stmt::Spawn { span, body });
            }
        }
    }
    block.stmts = new_stmts;
}

/// Recursively desugars an expression, transforming sugar operators
/// (`??`, `?`, `?.`) into match expressions.
#[allow(clippy::too_many_lines)]
fn desugar_expr(expr: Expr) -> Expr {
    match expr {
        // NullCoalesce: `expr ?? default` =>
        // match expr { Option::Some(__val) => __val, Option::None => default }
        Expr::NullCoalesce { left, right, span } => {
            let left = desugar_expr(*left);
            let right = desugar_expr(*right);
            Expr::Match {
                expr: Box::new(left),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Variant {
                            enum_name: Some("Option".to_string()),
                            variant: "Some".to_string(),
                            bindings: vec!["__coalesce_val".to_string()],
                            span,
                        },
                        body: Expr::Ident("__coalesce_val".to_string(), span),
                        span,
                    },
                    MatchArm {
                        pattern: Pattern::Variant {
                            enum_name: Some("Option".to_string()),
                            variant: "None".to_string(),
                            bindings: vec![],
                            span,
                        },
                        body: right,
                        span,
                    },
                ],
                span,
            }
        }

        // Try: `expr?` =>
        // match expr { Result::Ok(__val) => __val, Result::Err(__e) => return Result::Err(__e) }
        Expr::Try { operand, span } => {
            let operand = desugar_expr(*operand);
            let return_err = Expr::Block(Block {
                span,
                stmts: vec![Stmt::Return {
                    span,
                    value: Some(Expr::EnumVariantExpr {
                        enum_name: "Result".to_string(),
                        variant: "Err".to_string(),
                        args: vec![Expr::Ident("__try_err".to_string(), span)],
                        span,
                    }),
                }],
            });
            Expr::Match {
                expr: Box::new(operand),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Variant {
                            enum_name: Some("Result".to_string()),
                            variant: "Ok".to_string(),
                            bindings: vec!["__try_val".to_string()],
                            span,
                        },
                        body: Expr::Ident("__try_val".to_string(), span),
                        span,
                    },
                    MatchArm {
                        pattern: Pattern::Variant {
                            enum_name: Some("Result".to_string()),
                            variant: "Err".to_string(),
                            bindings: vec!["__try_err".to_string()],
                            span,
                        },
                        body: return_err,
                        span,
                    },
                ],
                span,
            }
        }

        // OptionalChain: `expr?.field` =>
        // match expr { Option::Some(__val) => Option::Some(__val.field), Option::None => Option::None }
        Expr::OptionalChain {
            object,
            field,
            span,
        } => {
            let object = desugar_expr(*object);
            Expr::Match {
                expr: Box::new(object),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Variant {
                            enum_name: Some("Option".to_string()),
                            variant: "Some".to_string(),
                            bindings: vec!["__chain_val".to_string()],
                            span,
                        },
                        body: Expr::EnumVariantExpr {
                            enum_name: "Option".to_string(),
                            variant: "Some".to_string(),
                            args: vec![Expr::FieldAccess {
                                object: Box::new(Expr::Ident("__chain_val".to_string(), span)),
                                field,
                                span,
                            }],
                            span,
                        },
                        span,
                    },
                    MatchArm {
                        pattern: Pattern::Variant {
                            enum_name: Some("Option".to_string()),
                            variant: "None".to_string(),
                            bindings: vec![],
                            span,
                        },
                        body: Expr::EnumVariantExpr {
                            enum_name: "Option".to_string(),
                            variant: "None".to_string(),
                            args: vec![],
                            span,
                        },
                        span,
                    },
                ],
                span,
            }
        }

        // Recursively desugar sub-expressions
        Expr::BinaryOp {
            left,
            op,
            right,
            span,
        } => Expr::BinaryOp {
            left: Box::new(desugar_expr(*left)),
            op,
            right: Box::new(desugar_expr(*right)),
            span,
        },
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op,
            operand: Box::new(desugar_expr(*operand)),
            span,
        },
        Expr::Call { callee, args, span } => Expr::Call {
            callee: Box::new(desugar_expr(*callee)),
            args: args.into_iter().map(desugar_expr).collect(),
            span,
        },
        Expr::If {
            condition,
            mut then_branch,
            else_branch,
            span,
        } => {
            let condition = desugar_expr(*condition);
            desugar_block(&mut then_branch);
            let else_branch = else_branch.map(|mut b| {
                desugar_block(&mut b);
                b
            });
            Expr::If {
                condition: Box::new(condition),
                then_branch,
                else_branch,
                span,
            }
        }
        Expr::FieldAccess {
            object,
            field,
            span,
        } => Expr::FieldAccess {
            object: Box::new(desugar_expr(*object)),
            field,
            span,
        },
        Expr::StructLit { name, fields, span } => Expr::StructLit {
            name,
            fields: fields
                .into_iter()
                .map(|f| kodo_ast::FieldInit {
                    name: f.name,
                    value: desugar_expr(f.value),
                    span: f.span,
                })
                .collect(),
            span,
        },
        Expr::EnumVariantExpr {
            enum_name,
            variant,
            args,
            span,
        } => Expr::EnumVariantExpr {
            enum_name,
            variant,
            args: args.into_iter().map(desugar_expr).collect(),
            span,
        },
        Expr::Match { expr, arms, span } => Expr::Match {
            expr: Box::new(desugar_expr(*expr)),
            arms: arms
                .into_iter()
                .map(|arm| MatchArm {
                    pattern: arm.pattern,
                    body: desugar_expr(arm.body),
                    span: arm.span,
                })
                .collect(),
            span,
        },
        Expr::Range {
            start,
            end,
            inclusive,
            span,
        } => Expr::Range {
            start: Box::new(desugar_expr(*start)),
            end: Box::new(desugar_expr(*end)),
            inclusive,
            span,
        },
        Expr::Block(mut block) => {
            desugar_block(&mut block);
            Expr::Block(block)
        }
        // Closure desugaring: just desugar the body
        Expr::Closure {
            params,
            return_type,
            body,
            span,
        } => Expr::Closure {
            params,
            return_type,
            body: Box::new(desugar_expr(*body)),
            span,
        },
        // Is expression: `expr is VariantName` =>
        // match expr { VariantName(_) => true, _ => false }
        Expr::Is {
            operand,
            type_name,
            span,
        } => {
            let operand = desugar_expr(*operand);
            Expr::Match {
                expr: Box::new(operand),
                arms: vec![
                    MatchArm {
                        pattern: Pattern::Variant {
                            enum_name: None,
                            variant: type_name,
                            bindings: vec![],
                            span,
                        },
                        body: Expr::BoolLit(true, span),
                        span,
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard(span),
                        body: Expr::BoolLit(false, span),
                        span,
                    },
                ],
                span,
            }
        }
        // Await: v1 just desugars operand (no actual suspension).
        Expr::Await { operand, span } => Expr::Await {
            operand: Box::new(desugar_expr(*operand)),
            span,
        },
        // Leaf expressions — no sub-expressions to desugar
        e @ (Expr::IntLit(_, _)
        | Expr::FloatLit(_, _)
        | Expr::StringLit(_, _)
        | Expr::BoolLit(_, _)
        | Expr::Ident(_, _)) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{NodeIdGen, Span};

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
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![kodo_ast::Function {
                id: id_gen.next_id(),
                span: Span::new(0, 100),
                name: "main".to_string(),
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

        // Check the while body has the increment
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
        // Check the condition uses Le for inclusive
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

        // The while should still be there
        assert_eq!(module.functions[0].body.stmts.len(), 1);
        if let Stmt::While { body, .. } = &module.functions[0].body.stmts[0] {
            // The inner for should have been desugared into let + while
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
            assert_eq!(
                arms.len(),
                2,
                "coalesce should desugar into match with 2 arms"
            );
            // First arm: Option::Some(__coalesce_val) => __coalesce_val
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "Some"
            ));
            // Second arm: Option::None => default
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "None"
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
            assert_eq!(arms.len(), 2, "try should desugar into match with 2 arms");
            // First arm: Result::Ok(__try_val) => __try_val
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "Ok"
            ));
            // Second arm: Result::Err(__try_err) => return Result::Err(__try_err)
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "Err"
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
            assert_eq!(
                arms.len(),
                2,
                "optional chain should desugar into match with 2 arms"
            );
            // First arm: Option::Some(__chain_val) => Option::Some(__chain_val.x)
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "Some"
            ));
            if let Expr::EnumVariantExpr { variant, args, .. } = &arms[0].body {
                assert_eq!(variant, "Some");
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0], Expr::FieldAccess { field, .. } if field == "x"));
            } else {
                panic!("expected EnumVariantExpr in Some arm body");
            }
            // Second arm: Option::None => Option::None
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "None"
            ));
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
        assert_eq!(
            stmts.len(),
            1,
            "if let should desugar into one match expression statement"
        );
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2, "match should have 2 arms");
            // First arm: Option::Some(v) => { body }
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant { variant, .. } if variant == "Some"
            ));
            // Second arm: _ => { else_body }
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
            assert_eq!(arms.len(), 2, "is should desugar into match with 2 arms");
            // First arm: Some => true
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant { variant, .. } if variant == "Some"
            ));
            assert!(matches!(&arms[0].body, Expr::BoolLit(true, _)));
            // Second arm: _ => false
            assert!(matches!(&arms[1].pattern, Pattern::Wildcard(_)));
            assert!(matches!(&arms[1].body, Expr::BoolLit(false, _)));
        } else {
            panic!("expected Match expression");
        }
    }
}
