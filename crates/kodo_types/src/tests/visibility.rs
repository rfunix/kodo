//! Visibility enforcement tests (E0270).

use super::*;

#[test]
fn private_function_call_from_another_module_errors() {
    let func_secret = Function {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "secret".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 50),
            stmts: vec![Stmt::Return {
                span: Span::new(10, 20),
                value: Some(Expr::IntLit(42, Span::new(15, 17))),
            }],
        },
    };
    let module_a = Module {
        test_decls: vec![],
        describe_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "module_a".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "provider module".to_string(),
                span: Span::new(10, 40),
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
        functions: vec![func_secret],
    };

    let func_main = make_function(
        "main",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(60, 80),
            value: Some(Expr::Call {
                callee: Box::new(Expr::Ident("secret".to_string(), Span::new(67, 73))),
                args: vec![],
                span: Span::new(67, 75),
            }),
        }],
    );
    let module_b = make_module(vec![func_main]);

    let mut checker = TypeChecker::new();
    checker.register_module_visibility(&module_a);

    let errors = checker.check_module_collecting(&module_b);
    assert!(
        !errors.is_empty(),
        "calling private fn from another module should produce an error"
    );
    let has_private_access = errors.iter().any(|e| {
        matches!(
            e,
            TypeError::PrivateAccess {
                name,
                defining_module,
                ..
            } if name == "secret" && defining_module == "module_a"
        )
    });
    assert!(
        has_private_access,
        "expected PrivateAccess error for `secret`, got: {errors:?}"
    );
}

#[test]
fn public_function_call_from_another_module_succeeds() {
    let func_greet = Function {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "greet".to_string(),
        visibility: Visibility::Public,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 50),
            stmts: vec![Stmt::Return {
                span: Span::new(10, 20),
                value: Some(Expr::IntLit(1, Span::new(15, 16))),
            }],
        },
    };
    let module_a = Module {
        test_decls: vec![],
        describe_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "module_a".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "provider module".to_string(),
                span: Span::new(10, 40),
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
        functions: vec![func_greet],
    };

    let func_main = make_function(
        "main",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(60, 80),
            value: Some(Expr::Call {
                callee: Box::new(Expr::Ident("greet".to_string(), Span::new(67, 72))),
                args: vec![],
                span: Span::new(67, 74),
            }),
        }],
    );
    let module_b = make_module(vec![func_main]);

    let mut checker = TypeChecker::new();
    checker.register_module_visibility(&module_a);

    let errors = checker.check_module_collecting(&module_b);
    let has_private_access = errors
        .iter()
        .any(|e| matches!(e, TypeError::PrivateAccess { .. }));
    assert!(
        !has_private_access,
        "calling public fn should NOT produce PrivateAccess error, got: {errors:?}"
    );
}

#[test]
fn private_access_error_has_correct_code() {
    let err = TypeError::PrivateAccess {
        name: "secret".to_string(),
        defining_module: "utils".to_string(),
        span: Span::new(10, 16),
    };
    assert_eq!(err.code(), "E0270");
}
