//! Phase 47: Iterator protocol builtins and for-in over Map tests.

use super::*;

#[test]
fn iterator_builtins_registered() {
    let checker = TypeChecker::new();

    // list_iter should be in method_lookup for List
    let list_iter = checker
        .method_lookup
        .get(&("List".to_string(), "iter".to_string()));
    assert!(list_iter.is_some(), "List.iter should be registered");

    // list_iter free function in env
    let list_iter_fn = checker.env.lookup("list_iter");
    assert!(list_iter_fn.is_some(), "list_iter should be in environment");

    // list_iterator_advance in env
    let advance_fn = checker.env.lookup("list_iterator_advance");
    assert!(
        advance_fn.is_some(),
        "list_iterator_advance should be in environment"
    );

    // list_iterator_value in env
    let value_fn = checker.env.lookup("list_iterator_value");
    assert!(
        value_fn.is_some(),
        "list_iterator_value should be in environment"
    );

    // list_iterator_free in env
    let free_fn = checker.env.lookup("list_iterator_free");
    assert!(
        free_fn.is_some(),
        "list_iterator_free should be in environment"
    );
}

#[test]
fn iterator_advance_returns_int() {
    let checker = TypeChecker::new();

    let advance_ty = checker.env.lookup("list_iterator_advance").unwrap();
    match advance_ty {
        Type::Function(params, ret) => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0], Type::Int);
            assert_eq!(**ret, Type::Int);
        }
        _ => panic!("expected Function type for list_iterator_advance"),
    }
}

#[test]
fn iterator_value_returns_int() {
    let checker = TypeChecker::new();

    let value_ty = checker.env.lookup("list_iterator_value").unwrap();
    match value_ty {
        Type::Function(params, ret) => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0], Type::Int);
            assert_eq!(**ret, Type::Int);
        }
        _ => panic!("expected Function type for list_iterator_value"),
    }
}

#[test]
fn iterator_free_returns_unit() {
    let checker = TypeChecker::new();

    let free_ty = checker.env.lookup("list_iterator_free").unwrap();
    match free_ty {
        Type::Function(params, ret) => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0], Type::Int);
            assert_eq!(**ret, Type::Unit);
        }
        _ => panic!("expected Function type for list_iterator_free"),
    }
}

#[test]
fn for_in_type_checks_list() {
    let span = Span::new(0, 100);
    let module = Module {
        test_decls: vec![],
        describe_decls: vec![],
        id: NodeId(0),
        span,
        name: "for_in_test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            entries: vec![
                MetaEntry {
                    key: "version".to_string(),
                    value: "1.0.0".to_string(),
                    span,
                },
                MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span,
                },
            ],
            span,
        }),
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(1),
            span,
            name: "test_fn".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![kodo_ast::Param {
                name: "items".to_string(),
                ty: kodo_ast::TypeExpr::Generic(
                    "List".to_string(),
                    vec![kodo_ast::TypeExpr::Named("Int".to_string())],
                ),
                span,
                ownership: kodo_ast::Ownership::Owned,
            }],
            return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![],
            body: Block {
                span,
                stmts: vec![
                    Stmt::Let {
                        span,
                        mutable: true,
                        name: "total".to_string(),
                        ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(0, span),
                    },
                    Stmt::ForIn {
                        span,
                        name: "x".to_string(),
                        iterable: Expr::Ident("items".to_string(), span),
                        body: Block {
                            span,
                            stmts: vec![Stmt::Assign {
                                span,
                                name: "total".to_string(),
                                value: Expr::BinaryOp {
                                    left: Box::new(Expr::Ident("total".to_string(), span)),
                                    op: BinOp::Add,
                                    right: Box::new(Expr::Ident("x".to_string(), span)),
                                    span,
                                },
                            }],
                        },
                    },
                    Stmt::Return {
                        span,
                        value: Some(Expr::Ident("total".to_string(), span)),
                    },
                ],
            },
        }],
    };

    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "for-in over List<Int> should type-check: {result:?}"
    );
}

#[test]
fn for_in_rejects_non_list() {
    let span = Span::new(0, 100);
    let module = Module {
        test_decls: vec![],
        describe_decls: vec![],
        id: NodeId(0),
        span,
        name: "for_in_err_test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(98),
            entries: vec![
                MetaEntry {
                    key: "version".to_string(),
                    value: "1.0.0".to_string(),
                    span,
                },
                MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span,
                },
            ],
            span,
        }),
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(1),
            span,
            name: "bad_fn".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![kodo_ast::Param {
                name: "x".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span,
                ownership: kodo_ast::Ownership::Owned,
            }],
            return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![],
            body: Block {
                span,
                stmts: vec![Stmt::ForIn {
                    span,
                    name: "item".to_string(),
                    iterable: Expr::Ident("x".to_string(), span),
                    body: Block {
                        span,
                        stmts: vec![],
                    },
                }],
            },
        }],
    };

    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "for-in over Int should fail type-check");
}

// --- for-in over Map type check tests ---

#[test]
fn for_in_over_map_yields_key_type() {
    let stmts = vec![
        // let m: Map<Int, String> = Map_new()
        Stmt::Let {
            span: Span::new(0, 30),
            mutable: false,
            name: "m".to_string(),
            ty: Some(TypeExpr::Generic(
                "Map".to_string(),
                vec![
                    TypeExpr::Named("Int".to_string()),
                    TypeExpr::Named("String".to_string()),
                ],
            )),
            value: Expr::Call {
                callee: Box::new(Expr::Ident("Map_new".to_string(), Span::new(20, 27))),
                args: vec![],
                span: Span::new(20, 29),
            },
        },
        // for k in m { let _: Int = k }
        Stmt::ForIn {
            span: Span::new(30, 80),
            name: "k".to_string(),
            iterable: Expr::Ident("m".to_string(), Span::new(40, 41)),
            body: Block {
                span: Span::new(45, 80),
                stmts: vec![Stmt::Let {
                    span: Span::new(50, 70),
                    mutable: false,
                    name: "_check".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::Ident("k".to_string(), Span::new(60, 61)),
                }],
            },
        },
    ];

    let func = make_function("main", vec![], TypeExpr::Unit, stmts);
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let errors = checker.check_module_collecting(&module);
    // k should be Int (key type of Map<Int, String>), so assigning to Int should succeed.
    let has_mismatch = errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. }));
    assert!(
        !has_mismatch,
        "for-in over Map<Int, String> should yield Int keys, got: {errors:?}"
    );
}
