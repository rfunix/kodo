//! Phase 17 annotation policy tests and confidence propagation tests.

use super::*;

// ===== Phase 17: Annotation Policy Tests =====

#[test]
fn low_confidence_without_review_emits_e0260() {
    let func = make_function_with_annotations(
        "risky_fn",
        vec![Annotation {
            name: "confidence".to_string(),
            args: vec![AnnotationArg::Positional(Expr::FloatLit(
                0.5,
                Span::new(0, 10),
            ))],
            span: Span::new(0, 20),
        }],
    );
    let module = make_module_with_policy(vec![func], None);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "should reject low confidence without review"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0260");
}

#[test]
fn low_confidence_with_human_review_is_ok() {
    let func = make_function_with_annotations(
        "reviewed_fn",
        vec![
            Annotation {
                name: "confidence".to_string(),
                args: vec![AnnotationArg::Positional(Expr::FloatLit(
                    0.5,
                    Span::new(0, 10),
                ))],
                span: Span::new(0, 20),
            },
            Annotation {
                name: "reviewed_by".to_string(),
                args: vec![AnnotationArg::Named(
                    "human".to_string(),
                    Expr::StringLit("rafael".to_string(), Span::new(0, 10)),
                )],
                span: Span::new(0, 20),
            },
        ],
    );
    let module = make_module_with_policy(vec![func], None);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "low confidence with @reviewed_by(human: ...) should pass: {result:?}"
    );
}

#[test]
fn security_sensitive_without_contracts_emits_e0262() {
    let func = make_function_with_annotations(
        "unsafe_fn",
        vec![Annotation {
            name: "security_sensitive".to_string(),
            args: vec![],
            span: Span::new(0, 20),
        }],
    );
    let module = make_module_with_policy(vec![func], None);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "should reject @security_sensitive without contracts"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0262");
}

#[test]
fn security_sensitive_with_requires_is_ok() {
    let mut func = make_function_with_annotations(
        "safe_fn",
        vec![Annotation {
            name: "security_sensitive".to_string(),
            args: vec![],
            span: Span::new(0, 20),
        }],
    );
    func.requires = vec![Expr::BoolLit(true, Span::new(0, 4))];
    let module = make_module_with_policy(vec![func], None);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "@security_sensitive with requires should pass: {result:?}"
    );
}

#[test]
fn security_sensitive_with_ensures_is_ok() {
    let mut func = make_function_with_annotations(
        "safe_fn",
        vec![Annotation {
            name: "security_sensitive".to_string(),
            args: vec![],
            span: Span::new(0, 20),
        }],
    );
    func.ensures = vec![Expr::BoolLit(true, Span::new(0, 4))];
    let module = make_module_with_policy(vec![func], None);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "@security_sensitive with ensures should pass: {result:?}"
    );
}

#[test]
fn confidence_at_threshold_is_ok() {
    let func = make_function_with_annotations(
        "threshold_fn",
        vec![Annotation {
            name: "confidence".to_string(),
            args: vec![AnnotationArg::Positional(Expr::FloatLit(
                0.8,
                Span::new(0, 10),
            ))],
            span: Span::new(0, 20),
        }],
    );
    let module = make_module_with_policy(vec![func], None);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "@confidence(0.8) is at the threshold and should pass: {result:?}"
    );
}

#[test]
fn high_confidence_without_review_is_ok() {
    let func = make_function_with_annotations(
        "confident_fn",
        vec![Annotation {
            name: "confidence".to_string(),
            args: vec![AnnotationArg::Positional(Expr::FloatLit(
                0.95,
                Span::new(0, 10),
            ))],
            span: Span::new(0, 20),
        }],
    );
    let module = make_module_with_policy(vec![func], None);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "@confidence(0.95) should not require review: {result:?}"
    );
}

// ===== Confidence Propagation Tests =====

#[test]
fn confidence_propagation_simple() {
    let func_b = Function {
        id: NodeId(1),
        span: Span::new(0, 100),
        name: "b_func".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![
            Annotation {
                name: "confidence".to_string(),
                args: vec![AnnotationArg::Positional(Expr::FloatLit(
                    0.5,
                    Span::new(0, 3),
                ))],
                span: Span::new(0, 10),
            },
            Annotation {
                name: "reviewed_by".to_string(),
                args: vec![AnnotationArg::Named(
                    "human".to_string(),
                    Expr::StringLit("alice".to_string(), Span::new(0, 5)),
                )],
                span: Span::new(0, 10),
            },
        ],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 100),
            stmts: vec![Stmt::Return {
                span: Span::new(0, 10),
                value: Some(Expr::IntLit(0, Span::new(0, 1))),
            }],
        },
    };
    let func_a = Function {
        id: NodeId(2),
        span: Span::new(0, 50),
        name: "a_func".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![Annotation {
            name: "confidence".to_string(),
            args: vec![AnnotationArg::Positional(Expr::FloatLit(
                0.95,
                Span::new(0, 4),
            ))],
            span: Span::new(0, 10),
        }],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 50),
            stmts: vec![Stmt::Return {
                span: Span::new(0, 20),
                value: Some(Expr::Call {
                    callee: Box::new(Expr::Ident("b_func".to_string(), Span::new(0, 6))),
                    args: vec![],
                    span: Span::new(0, 8),
                }),
            }],
        },
    };
    let module = make_module(vec![func_b, func_a]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "should compile: {result:?}");
    let computed = checker.compute_confidence("a_func", &mut std::collections::HashSet::new());
    assert!(
        (computed - 0.5).abs() < 0.01,
        "a_func confidence should be 0.5 (min of 0.95 and 0.5), got {computed}"
    );
}

#[test]
fn confidence_threshold_violation() {
    let func_weak = Function {
        id: NodeId(1),
        span: Span::new(0, 100),
        name: "weak_fn".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![
            Annotation {
                name: "confidence".to_string(),
                args: vec![AnnotationArg::Positional(Expr::FloatLit(
                    0.5,
                    Span::new(0, 3),
                ))],
                span: Span::new(0, 10),
            },
            Annotation {
                name: "reviewed_by".to_string(),
                args: vec![AnnotationArg::Named(
                    "human".to_string(),
                    Expr::StringLit("alice".to_string(), Span::new(0, 5)),
                )],
                span: Span::new(0, 10),
            },
        ],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 100),
            stmts: vec![Stmt::Return {
                span: Span::new(0, 10),
                value: Some(Expr::IntLit(0, Span::new(0, 1))),
            }],
        },
    };
    let func_main = Function {
        id: NodeId(3),
        span: Span::new(0, 50),
        name: "main".to_string(),
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
                span: Span::new(0, 20),
                value: Some(Expr::Call {
                    callee: Box::new(Expr::Ident("weak_fn".to_string(), Span::new(0, 7))),
                    args: vec![],
                    span: Span::new(0, 9),
                }),
            }],
        },
    };
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![
                MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                },
                MetaEntry {
                    key: "min_confidence".to_string(),
                    value: "0.9".to_string(),
                    span: Span::new(0, 20),
                },
            ],
        }),
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![func_weak, func_main],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should fail due to confidence threshold");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0261");
}
