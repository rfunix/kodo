//! Tests for ForAll statement type checking and property testing annotation acceptance.

use super::*;

/// A `forall` statement with typed bindings should type-check without errors.
/// The bindings must be available inside the body block.
#[test]
fn forall_bindings_are_typed() {
    // forall x: Int, y: String { }
    let module = make_module_with_body(vec![Stmt::ForAll {
        span: Span::new(0, 50),
        bindings: vec![
            ("x".to_string(), TypeExpr::Named("Int".to_string())),
            ("y".to_string(), TypeExpr::Named("String".to_string())),
        ],
        body: Block {
            span: Span::new(20, 50),
            stmts: vec![],
        },
    }]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_ok(),
        "forall with Int and String bindings and empty body should type-check"
    );
}

/// The bindings introduced by `forall` must be visible inside the body block.
#[test]
fn forall_binding_visible_in_body() {
    // forall x: Int { let z: Int = x }
    let module = make_module_with_body(vec![Stmt::ForAll {
        span: Span::new(0, 60),
        bindings: vec![("x".to_string(), TypeExpr::Named("Int".to_string()))],
        body: Block {
            span: Span::new(15, 60),
            stmts: vec![Stmt::Let {
                span: Span::new(16, 30),
                mutable: false,
                name: "z".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(28, 29)),
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_ok(),
        "forall binding x should be visible inside the body"
    );
}

/// A type mismatch inside a `forall` body should still be caught.
#[test]
fn forall_body_type_mismatch_is_caught() {
    // forall x: Int { let z: Bool = x }  — should fail
    let module = make_module_with_body(vec![Stmt::ForAll {
        span: Span::new(0, 60),
        bindings: vec![("x".to_string(), TypeExpr::Named("Int".to_string()))],
        body: Block {
            span: Span::new(15, 60),
            stmts: vec![Stmt::Let {
                span: Span::new(16, 30),
                mutable: false,
                name: "z".to_string(),
                ty: Some(TypeExpr::Named("Bool".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(28, 29)),
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_err(),
        "type mismatch inside forall body should produce a type error"
    );
}

/// `forall` bindings must not leak out of their scope.
#[test]
fn forall_bindings_do_not_escape_scope() {
    // forall x: Int { }; let z: Int = x  — should fail: x is not in scope
    let module = make_module_with_body(vec![
        Stmt::ForAll {
            span: Span::new(0, 30),
            bindings: vec![("x".to_string(), TypeExpr::Named("Int".to_string()))],
            body: Block {
                span: Span::new(15, 30),
                stmts: vec![],
            },
        },
        Stmt::Let {
            span: Span::new(32, 48),
            mutable: false,
            name: "z".to_string(),
            ty: Some(TypeExpr::Named("Int".to_string())),
            value: Expr::Ident("x".to_string(), Span::new(46, 47)),
        },
    ]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_err(),
        "forall binding x should not be visible after the forall statement"
    );
}

/// `@skip`, `@todo`, `@timeout`, and `@property` annotations should be silently
/// accepted by the type checker without producing errors.
#[test]
fn testing_annotations_are_accepted_without_errors() {
    let annotations = vec!["skip", "todo", "timeout", "property"];
    for name in annotations {
        let func = make_function_with_annotations(
            "annotated_fn",
            vec![Annotation {
                name: name.to_string(),
                args: vec![],
                span: Span::new(0, 20),
            }],
        );
        let module = make_module(vec![func]);
        let mut checker = TypeChecker::new();
        let result = checker.check_module(&module);
        assert!(
            result.is_ok(),
            "@{name} annotation should be silently accepted by the type checker, got: {result:?}"
        );
    }
}

/// Property testing builtins should be registered in the type environment.
#[test]
fn property_testing_builtins_are_registered() {
    let builtins = vec![
        "kodo_prop_start",
        "kodo_prop_gen_int",
        "kodo_prop_gen_bool",
        "kodo_prop_gen_float",
        "kodo_prop_gen_string",
        "kodo_test_set_timeout",
        "kodo_test_clear_timeout",
        "kodo_test_isolate_start",
        "kodo_test_isolate_end",
    ];
    // Calling any of these from inside a function body must type-check.
    // We verify registration by checking that calls to them do not produce
    // "undefined function" errors.
    for builtin in builtins {
        let module = make_module_with_body(vec![Stmt::Expr(Expr::Call {
            callee: Box::new(Expr::Ident(builtin.to_string(), Span::new(0, 20))),
            args: vec![],
            span: Span::new(0, 22),
        })]);
        let mut checker = TypeChecker::new();
        let result = checker.check_module(&module);
        // The call may fail due to arity mismatch but must NOT produce an
        // "undefined" error, meaning the builtin is registered.
        if let Err(ref e) = result {
            let msg = e.to_string();
            assert!(
                !msg.contains("undefined") && !msg.contains("not in scope"),
                "builtin `{builtin}` should be registered (not undefined), got: {msg}"
            );
        }
    }
}
