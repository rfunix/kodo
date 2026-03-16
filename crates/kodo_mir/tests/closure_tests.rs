//! Integration tests for closure lambda lifting in MIR lowering.

use kodo_ast::{
    BinOp, Block, ClosureParam, Expr, Function, Module, NodeId, Param, Span, Stmt, TypeExpr,
    Visibility,
};
use kodo_mir::lowering::{lower_function, lower_module};
use kodo_mir::Instruction;
use kodo_types::Type;

fn span() -> Span {
    Span { start: 0, end: 0 }
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

fn make_module(functions: Vec<Function>) -> Module {
    Module {
        test_decls: vec![],
        id: NodeId(0),
        name: "test".to_string(),
        span: span(),
        meta: None,
        imports: vec![],
        functions,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        intent_decls: vec![],
        invariants: vec![],
        actor_decls: vec![],
    }
}

#[test]
fn closure_generates_lifted_function_in_module() {
    let module = make_module(vec![make_fn(
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
                        params: vec![ClosureParam {
                            name: "x".to_string(),
                            ty: Some(TypeExpr::Named("Int".to_string())),
                            span: span(),
                        }],
                        return_type: None,
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
                    args: vec![Expr::IntLit(10, span())],
                    span: span(),
                }),
            ],
        },
        TypeExpr::Unit,
    )]);
    let mir_functions = lower_module(&module).unwrap();
    assert!(
        mir_functions.len() >= 2,
        "expected at least 2 MIR functions (main + closure), got {}",
        mir_functions.len()
    );
    let closure_fn = mir_functions
        .iter()
        .find(|f| f.name.starts_with("__closure_"));
    assert!(
        closure_fn.is_some(),
        "expected a lambda-lifted closure function"
    );
    let closure_fn = closure_fn.unwrap();
    assert_eq!(closure_fn.param_count, 1);
    closure_fn.validate().unwrap();
}

#[test]
fn closure_with_multiple_captures_in_module() {
    let module = make_module(vec![make_fn(
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
                    value: Expr::IntLit(1, span()),
                },
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "b".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(2, span()),
                },
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "f".to_string(),
                    ty: None,
                    value: Expr::Closure {
                        params: vec![ClosureParam {
                            name: "x".to_string(),
                            ty: Some(TypeExpr::Named("Int".to_string())),
                            span: span(),
                        }],
                        return_type: None,
                        body: Box::new(Expr::BinaryOp {
                            left: Box::new(Expr::BinaryOp {
                                left: Box::new(Expr::Ident("x".to_string(), span())),
                                op: BinOp::Add,
                                right: Box::new(Expr::Ident("a".to_string(), span())),
                                span: span(),
                            }),
                            op: BinOp::Add,
                            right: Box::new(Expr::Ident("b".to_string(), span())),
                            span: span(),
                        }),
                        span: span(),
                    },
                },
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("f".to_string(), span())),
                    args: vec![Expr::IntLit(3, span())],
                    span: span(),
                }),
            ],
        },
        TypeExpr::Unit,
    )]);
    let mir_functions = lower_module(&module).unwrap();
    let closure_fn = mir_functions
        .iter()
        .find(|f| f.name.starts_with("__closure_"))
        .expect("expected a lambda-lifted closure function");
    assert_eq!(
        closure_fn.param_count, 3,
        "closure should have 3 params (2 captures + 1 param), got {}",
        closure_fn.param_count
    );
}

#[test]
fn closure_returning_bool() {
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
                        params: vec![ClosureParam {
                            name: "x".to_string(),
                            ty: Some(TypeExpr::Named("Int".to_string())),
                            span: span(),
                        }],
                        return_type: None,
                        body: Box::new(Expr::BinaryOp {
                            left: Box::new(Expr::Ident("x".to_string(), span())),
                            op: BinOp::Gt,
                            right: Box::new(Expr::IntLit(0, span())),
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
}

#[test]
fn closure_empty_params_no_captures() {
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
                        params: vec![],
                        return_type: None,
                        body: Box::new(Expr::IntLit(42, span())),
                        span: span(),
                    },
                },
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("f".to_string(), span())),
                    args: vec![],
                    span: span(),
                }),
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    let has_call = mir.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(i, Instruction::Call { callee, args, .. }
                if callee.starts_with("__closure_") && args.is_empty())
        })
    });
    assert!(has_call, "expected call to __closure_ with 0 args");
}

#[test]
fn closure_body_with_if_expression() {
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
                        params: vec![ClosureParam {
                            name: "x".to_string(),
                            ty: Some(TypeExpr::Named("Int".to_string())),
                            span: span(),
                        }],
                        return_type: None,
                        body: Box::new(Expr::If {
                            condition: Box::new(Expr::BinaryOp {
                                left: Box::new(Expr::Ident("x".to_string(), span())),
                                op: BinOp::Gt,
                                right: Box::new(Expr::IntLit(0, span())),
                                span: span(),
                            }),
                            then_branch: Block {
                                span: span(),
                                stmts: vec![Stmt::Expr(Expr::Ident("x".to_string(), span()))],
                            },
                            else_branch: Some(Block {
                                span: span(),
                                stmts: vec![Stmt::Expr(Expr::BinaryOp {
                                    left: Box::new(Expr::IntLit(0, span())),
                                    op: BinOp::Sub,
                                    right: Box::new(Expr::Ident("x".to_string(), span())),
                                    span: span(),
                                })],
                            }),
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
}

#[test]
fn closure_two_params_bool_return() {
    let func = make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "eq".to_string(),
                    ty: None,
                    value: Expr::Closure {
                        params: vec![
                            ClosureParam {
                                name: "a".to_string(),
                                ty: Some(TypeExpr::Named("Int".to_string())),
                                span: span(),
                            },
                            ClosureParam {
                                name: "b".to_string(),
                                ty: Some(TypeExpr::Named("Int".to_string())),
                                span: span(),
                            },
                        ],
                        return_type: None,
                        body: Box::new(Expr::BinaryOp {
                            left: Box::new(Expr::Ident("a".to_string(), span())),
                            op: BinOp::Eq,
                            right: Box::new(Expr::Ident("b".to_string(), span())),
                            span: span(),
                        }),
                        span: span(),
                    },
                },
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("eq".to_string(), span())),
                    args: vec![Expr::IntLit(1, span()), Expr::IntLit(1, span())],
                    span: span(),
                }),
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    let has_2arg_call = mir.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(i, Instruction::Call { callee, args, .. }
                if callee.starts_with("__closure_") && args.len() == 2)
        })
    });
    assert!(has_2arg_call, "expected call to __closure_ with 2 args");
}

#[test]
fn closure_capture_preserves_variable_type() {
    let module = make_module(vec![make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "flag".to_string(),
                    ty: Some(TypeExpr::Named("Bool".to_string())),
                    value: Expr::BoolLit(true, span()),
                },
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "f".to_string(),
                    ty: None,
                    value: Expr::Closure {
                        params: vec![ClosureParam {
                            name: "x".to_string(),
                            ty: Some(TypeExpr::Named("Int".to_string())),
                            span: span(),
                        }],
                        return_type: None,
                        body: Box::new(Expr::Ident("flag".to_string(), span())),
                        span: span(),
                    },
                },
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("f".to_string(), span())),
                    args: vec![Expr::IntLit(0, span())],
                    span: span(),
                }),
            ],
        },
        TypeExpr::Unit,
    )]);
    let mir_functions = lower_module(&module).unwrap();
    let closure_fn = mir_functions
        .iter()
        .find(|f| f.name.starts_with("__closure_"))
        .expect("expected a lambda-lifted closure function");
    assert_eq!(closure_fn.param_count, 2);
}

#[test]
fn closure_return_type_inferred_as_int() {
    let module = make_module(vec![make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "f".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![ClosureParam {
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        span: span(),
                    }],
                    return_type: None,
                    body: Box::new(Expr::BinaryOp {
                        left: Box::new(Expr::Ident("x".to_string(), span())),
                        op: BinOp::Add,
                        right: Box::new(Expr::IntLit(1, span())),
                        span: span(),
                    }),
                    span: span(),
                },
            }],
        },
        TypeExpr::Unit,
    )]);
    let mir_functions = lower_module(&module).unwrap();
    let closure_fn = mir_functions
        .iter()
        .find(|f| f.name.starts_with("__closure_"))
        .expect("expected a lambda-lifted closure function");
    assert_eq!(
        closure_fn.return_type,
        Type::Int,
        "closure returning x + 1 should have return_type Int"
    );
}

#[test]
fn closure_return_type_inferred_as_bool() {
    let module = make_module(vec![make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "f".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![ClosureParam {
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        span: span(),
                    }],
                    return_type: None,
                    body: Box::new(Expr::BinaryOp {
                        left: Box::new(Expr::Ident("x".to_string(), span())),
                        op: BinOp::Gt,
                        right: Box::new(Expr::IntLit(0, span())),
                        span: span(),
                    }),
                    span: span(),
                },
            }],
        },
        TypeExpr::Unit,
    )]);
    let mir_functions = lower_module(&module).unwrap();
    let closure_fn = mir_functions
        .iter()
        .find(|f| f.name.starts_with("__closure_"))
        .expect("expected a lambda-lifted closure function");
    assert_eq!(
        closure_fn.return_type,
        Type::Bool,
        "closure returning x > 0 should have return_type Bool"
    );
}

#[test]
fn closure_explicit_return_type_annotation() {
    let module = make_module(vec![make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "f".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![ClosureParam {
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        span: span(),
                    }],
                    return_type: Some(TypeExpr::Named("Int".to_string())),
                    body: Box::new(Expr::BinaryOp {
                        left: Box::new(Expr::Ident("x".to_string(), span())),
                        op: BinOp::Mul,
                        right: Box::new(Expr::IntLit(2, span())),
                        span: span(),
                    }),
                    span: span(),
                },
            }],
        },
        TypeExpr::Unit,
    )]);
    let mir_functions = lower_module(&module).unwrap();
    let closure_fn = mir_functions
        .iter()
        .find(|f| f.name.starts_with("__closure_"))
        .expect("expected a lambda-lifted closure function");
    assert_eq!(
        closure_fn.return_type,
        Type::Int,
        "closure with explicit -> Int should have return_type Int"
    );
}

#[test]
fn closure_return_type_bool_constant() {
    let module = make_module(vec![make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "always_true".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![],
                    return_type: None,
                    body: Box::new(Expr::BoolLit(true, span())),
                    span: span(),
                },
            }],
        },
        TypeExpr::Unit,
    )]);
    let mir_functions = lower_module(&module).unwrap();
    let closure_fn = mir_functions
        .iter()
        .find(|f| f.name.starts_with("__closure_"))
        .expect("expected a lambda-lifted closure function");
    assert_eq!(
        closure_fn.return_type,
        Type::Bool,
        "closure returning true should have return_type Bool"
    );
}
