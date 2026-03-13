//! Property-based tests for MIR lowering — verifies that lowering
//! never panics on valid AST inputs and produces structurally valid MIR.

use kodo_ast::{
    BinOp, Block, Expr, Function, NodeId, Ownership, Param, Span, Stmt, TypeExpr, Visibility,
};
use kodo_mir::lowering::lower_function;
use proptest::prelude::*;

fn span() -> Span {
    Span::new(0, 0)
}

fn make_fn(name: &str, params: Vec<Param>, body: Block, ret: TypeExpr) -> Function {
    Function {
        id: NodeId(0),
        span: span(),
        name: name.to_string(),
        visibility: Visibility::Private,
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

/// Strategy for generating valid integer literals.
fn int_expr_strategy() -> impl Strategy<Value = Expr> {
    (-1000i64..1000).prop_map(|n| Expr::IntLit(n, span()))
}

/// Strategy for generating valid binary operations on int literals.
fn binop_expr_strategy() -> impl Strategy<Value = Expr> {
    let ops = prop::sample::select(vec![
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::Eq,
        BinOp::Ne,
        BinOp::Lt,
        BinOp::Le,
        BinOp::Gt,
        BinOp::Ge,
    ]);
    (int_expr_strategy(), ops, int_expr_strategy()).prop_map(|(left, op, right)| Expr::BinaryOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
        span: span(),
    })
}

proptest! {
    /// Lowering a function returning an int literal never panics.
    #[test]
    fn lowering_int_return_never_panics(n in -10000i64..10000) {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::IntLit(n, span())),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
        let mir = result.unwrap();
        prop_assert!(mir.validate().is_ok());
    }

    /// Lowering a function returning a bool never panics.
    #[test]
    fn lowering_bool_return_never_panics(b in prop::bool::ANY) {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::BoolLit(b, span())),
                }],
            },
            TypeExpr::Named("Bool".to_string()),
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
    }

    /// Lowering a function returning a float never panics.
    #[test]
    fn lowering_float_return_never_panics(f in -1e6f64..1e6) {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::FloatLit(f, span())),
                }],
            },
            TypeExpr::Named("Float64".to_string()),
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
    }

    /// Lowering a function returning a string never panics.
    #[test]
    fn lowering_string_return_never_panics(s in "[a-zA-Z0-9 ]{0,50}") {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::StringLit(s, span())),
                }],
            },
            TypeExpr::Named("String".to_string()),
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
    }

    /// Lowering a binary operation never panics.
    #[test]
    fn lowering_binop_never_panics(expr in binop_expr_strategy()) {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(expr),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
    }

    /// Lowering a function with let bindings never panics.
    #[test]
    fn lowering_let_binding_never_panics(
        val in -1000i64..1000,
        name in "[a-z]{1,8}"
    ) {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Let {
                    span: span(),
                    mutable: false,
                    name,
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(val, span()),
                }],
            },
            TypeExpr::Unit,
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
    }

    /// Lowering a function with multiple let statements never panics.
    #[test]
    fn lowering_multiple_lets_never_panics(count in 1usize..10) {
        let stmts: Vec<Stmt> = (0..count)
            .map(|i| Stmt::Let {
                span: span(),
                mutable: false,
                name: format!("v{i}"),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(i as i64, span()),
            })
            .collect();
        let func = make_fn(
            "test",
            vec![],
            Block { span: span(), stmts },
            TypeExpr::Unit,
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
    }

    /// Lowering a function with params that are referenced never panics.
    #[test]
    fn lowering_param_reference_never_panics(n in 1usize..5) {
        let params: Vec<Param> = (0..n)
            .map(|i| Param {
                name: format!("p{i}"),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            })
            .collect();
        // Return the first param.
        let func = make_fn(
            "test",
            params,
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Ident("p0".to_string(), span())),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
        let mir = result.unwrap();
        prop_assert_eq!(mir.param_count, n);
    }

    /// Lowering an empty function never panics.
    #[test]
    fn lowering_empty_function_never_panics(name in "[a-z]{1,10}") {
        let func = make_fn(
            &name,
            vec![],
            Block {
                span: span(),
                stmts: vec![],
            },
            TypeExpr::Unit,
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
        let mir = result.unwrap();
        prop_assert!(mir.validate().is_ok());
        prop_assert_eq!(mir.name, name);
    }

    /// Lowering an if/else expression never panics.
    #[test]
    fn lowering_if_else_never_panics(
        cond in prop::bool::ANY,
        then_val in -100i64..100,
        else_val in -100i64..100
    ) {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Expr(Expr::If {
                    condition: Box::new(Expr::BoolLit(cond, span())),
                    then_branch: Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::IntLit(then_val, span())),
                        }],
                    },
                    else_branch: Some(Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::IntLit(else_val, span())),
                        }],
                    }),
                    span: span(),
                })],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
        let mir = result.unwrap();
        prop_assert!(mir.validate().is_ok());
        // if/else creates multiple blocks
        prop_assert!(mir.blocks.len() >= 4);
    }

    /// Lowering a for loop never panics.
    #[test]
    fn lowering_for_loop_never_panics(
        start in 0i64..10,
        end in 0i64..20,
        inclusive in prop::bool::ANY
    ) {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::For {
                    span: span(),
                    name: "i".to_string(),
                    start: Expr::IntLit(start, span()),
                    end: Expr::IntLit(end, span()),
                    inclusive,
                    body: Block {
                        span: span(),
                        stmts: vec![],
                    },
                }],
            },
            TypeExpr::Unit,
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
        let mir = result.unwrap();
        prop_assert!(mir.validate().is_ok());
    }

    /// Lowering a while loop with constant condition never panics.
    #[test]
    fn lowering_while_never_panics(cond in prop::bool::ANY) {
        let func = make_fn(
            "test",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::While {
                    span: span(),
                    condition: Expr::BoolLit(cond, span()),
                    body: Block {
                        span: span(),
                        stmts: vec![Stmt::Break { span: span() }],
                    },
                }],
            },
            TypeExpr::Unit,
        );
        let result = lower_function(&func);
        prop_assert!(result.is_ok());
        let mir = result.unwrap();
        prop_assert!(mir.validate().is_ok());
    }
}
