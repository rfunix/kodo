//! Phase 49: Module invariants tests.

use super::*;

#[test]
fn invariant_bool_condition_passes() {
    let span = Span::new(0, 10);
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span,
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span,
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test invariant".to_string(),
                span,
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
        functions: vec![make_function(
            "f",
            vec![],
            TypeExpr::Named("Int".to_string()),
            vec![Stmt::Return {
                span,
                value: Some(Expr::IntLit(1, span)),
            }],
        )],
    };

    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "Bool invariant should pass: {result:?}");
}

#[test]
fn invariant_non_bool_condition_fails() {
    let span = Span::new(0, 10);
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span,
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span,
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test invariant".to_string(),
                span,
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
            condition: Expr::IntLit(42, span),
        }],
        functions: vec![make_function(
            "f",
            vec![],
            TypeExpr::Named("Int".to_string()),
            vec![Stmt::Return {
                span,
                value: Some(Expr::IntLit(1, span)),
            }],
        )],
    };

    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "Int invariant should fail type-check");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0310");
}

#[test]
fn invariant_comparison_expr_passes() {
    let span = Span::new(0, 10);
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span,
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span,
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test invariant".to_string(),
                span,
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
            condition: Expr::BinaryOp {
                op: BinOp::Gt,
                left: Box::new(Expr::IntLit(10, span)),
                right: Box::new(Expr::IntLit(5, span)),
                span,
            },
        }],
        functions: vec![make_function(
            "f",
            vec![],
            TypeExpr::Named("Int".to_string()),
            vec![Stmt::Return {
                span,
                value: Some(Expr::IntLit(1, span)),
            }],
        )],
    };

    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "comparison invariant should pass: {result:?}"
    );
}

#[test]
fn invariant_collecting_reports_error() {
    let span = Span::new(0, 10);
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span,
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span,
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test invariant".to_string(),
                span,
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
            condition: Expr::IntLit(42, span),
        }],
        functions: vec![make_function(
            "f",
            vec![],
            TypeExpr::Named("Int".to_string()),
            vec![Stmt::Return {
                span,
                value: Some(Expr::IntLit(1, span)),
            }],
        )],
    };

    let mut checker = TypeChecker::new();
    let errors = checker.check_module_collecting(&module);
    assert!(
        errors.iter().any(|e| e.code() == "E0310"),
        "collecting should include E0310, got: {errors:?}"
    );
}

#[test]
fn invariant_multiple_conditions() {
    let span = Span::new(0, 10);
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span,
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span,
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test invariant".to_string(),
                span,
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![
            kodo_ast::InvariantDecl {
                span,
                condition: Expr::BoolLit(true, span),
            },
            kodo_ast::InvariantDecl {
                span,
                condition: Expr::BinaryOp {
                    op: BinOp::Eq,
                    left: Box::new(Expr::IntLit(1, span)),
                    right: Box::new(Expr::IntLit(1, span)),
                    span,
                },
            },
        ],
        functions: vec![make_function(
            "f",
            vec![],
            TypeExpr::Named("Int".to_string()),
            vec![Stmt::Return {
                span,
                value: Some(Expr::IntLit(1, span)),
            }],
        )],
    };

    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "multiple Bool invariants should pass: {result:?}"
    );
}
