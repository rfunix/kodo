//! Phase 52: Borrow checking tests and Phase 54: Send/Sync bounds for spawn blocks.

use super::*;

// ── Phase 52: Borrow Checking Tests ──────────────────────────────────

#[test]
fn mut_param_tracked_as_mut_borrowed() {
    let func = make_function(
        "take_mut",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            ownership: kodo_ast::Ownership::Mut,
            span: Span::new(0, 10),
        }],
        TypeExpr::Named("String".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::Ident("x".to_string(), Span::new(57, 58))),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "returning mut borrow should escape scope");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0241", "expected E0241, got {}", err.code());
}

#[test]
fn use_after_move_double_assign_detected() {
    let span = Span::new(0, 100);
    let func = make_function(
        "bad",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Let {
                span: Span::new(10, 30),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hi".to_string(), Span::new(20, 24)),
            },
            Stmt::Let {
                span: Span::new(30, 50),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(40, 41)),
            },
            Stmt::Let {
                span: Span::new(50, 70),
                mutable: false,
                name: "z".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(60, 61)),
            },
            Stmt::Return {
                span: Span::new(70, 80),
                value: Some(Expr::IntLit(0, span)),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should detect use after move");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0240", "expected E0240, got {}", err.code());
}

#[test]
fn move_while_borrowed_in_same_call() {
    let span = Span::new(0, 100);
    let callee = make_function(
        "two_args",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                ownership: kodo_ast::Ownership::Ref,
                span: Span::new(0, 10),
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                ownership: kodo_ast::Ownership::Owned,
                span: Span::new(12, 22),
            },
        ],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::IntLit(0, span)),
        }],
    );
    let caller = make_function(
        "bad",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Let {
                span: Span::new(10, 30),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hi".to_string(), Span::new(20, 24)),
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("two_args".to_string(), Span::new(30, 38))),
                args: vec![
                    Expr::Ident("x".to_string(), Span::new(39, 40)),
                    Expr::Ident("x".to_string(), Span::new(42, 43)),
                ],
                span: Span::new(30, 44),
            }),
            Stmt::Return {
                span: Span::new(70, 80),
                value: Some(Expr::IntLit(0, span)),
            },
        ],
    );
    let module = make_module(vec![callee, caller]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should detect move while borrowed");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0242", "expected E0242, got {}", err.code());
}

#[test]
fn ref_borrow_while_mut_borrowed_in_same_call() {
    let span = Span::new(0, 100);
    let callee = make_function(
        "two_args",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                ownership: kodo_ast::Ownership::Mut,
                span: Span::new(0, 10),
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                ownership: kodo_ast::Ownership::Ref,
                span: Span::new(12, 22),
            },
        ],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::IntLit(0, span)),
        }],
    );
    let caller = make_function(
        "bad",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Let {
                span: Span::new(10, 30),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hi".to_string(), Span::new(20, 24)),
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("two_args".to_string(), Span::new(30, 38))),
                args: vec![
                    Expr::Ident("x".to_string(), Span::new(39, 40)),
                    Expr::Ident("x".to_string(), Span::new(42, 43)),
                ],
                span: Span::new(30, 44),
            }),
            Stmt::Return {
                span: Span::new(70, 80),
                value: Some(Expr::IntLit(0, span)),
            },
        ],
    );
    let module = make_module(vec![callee, caller]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "should detect ref borrow while mut borrowed"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0246", "expected E0246, got {}", err.code());
}

#[test]
fn double_mut_borrow_in_same_call() {
    let span = Span::new(0, 100);
    let callee = make_function(
        "two_args",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                ownership: kodo_ast::Ownership::Mut,
                span: Span::new(0, 10),
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                ownership: kodo_ast::Ownership::Mut,
                span: Span::new(12, 22),
            },
        ],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::IntLit(0, span)),
        }],
    );
    let caller = make_function(
        "bad",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Let {
                span: Span::new(10, 30),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hi".to_string(), Span::new(20, 24)),
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("two_args".to_string(), Span::new(30, 38))),
                args: vec![
                    Expr::Ident("x".to_string(), Span::new(39, 40)),
                    Expr::Ident("x".to_string(), Span::new(42, 43)),
                ],
                span: Span::new(30, 44),
            }),
            Stmt::Return {
                span: Span::new(70, 80),
                value: Some(Expr::IntLit(0, span)),
            },
        ],
    );
    let module = make_module(vec![callee, caller]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should detect double mut borrow");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0247", "expected E0247, got {}", err.code());
}

#[test]
fn mut_borrow_while_ref_borrowed_in_same_call() {
    let span = Span::new(0, 100);
    let callee = make_function(
        "two_args",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                ownership: kodo_ast::Ownership::Ref,
                span: Span::new(0, 10),
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                ownership: kodo_ast::Ownership::Mut,
                span: Span::new(12, 22),
            },
        ],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::IntLit(0, span)),
        }],
    );
    let caller = make_function(
        "bad",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Let {
                span: Span::new(10, 30),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hi".to_string(), Span::new(20, 24)),
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("two_args".to_string(), Span::new(30, 38))),
                args: vec![
                    Expr::Ident("x".to_string(), Span::new(39, 40)),
                    Expr::Ident("x".to_string(), Span::new(42, 43)),
                ],
                span: Span::new(30, 44),
            }),
            Stmt::Return {
                span: Span::new(70, 80),
                value: Some(Expr::IntLit(0, span)),
            },
        ],
    );
    let module = make_module(vec![callee, caller]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "should detect mut borrow while ref borrowed"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0245", "expected E0245, got {}", err.code());
}

#[test]
fn multiple_ref_borrows_ok() {
    let span = Span::new(0, 100);
    let take_ref = make_function(
        "take_ref",
        vec![Param {
            name: "s".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            ownership: kodo_ast::Ownership::Ref,
            span: Span::new(0, 10),
        }],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::IntLit(0, span)),
        }],
    );
    let caller = make_function(
        "ok",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Let {
                span: Span::new(10, 30),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hi".to_string(), Span::new(20, 24)),
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("take_ref".to_string(), Span::new(30, 38))),
                args: vec![Expr::Ident("x".to_string(), Span::new(39, 40))],
                span: Span::new(30, 41),
            }),
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("take_ref".to_string(), Span::new(50, 58))),
                args: vec![Expr::Ident("x".to_string(), Span::new(59, 60))],
                span: Span::new(50, 61),
            }),
            Stmt::Return {
                span: Span::new(70, 80),
                value: Some(Expr::IntLit(0, span)),
            },
        ],
    );
    let module = make_module(vec![take_ref, caller]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "multiple ref borrows should be allowed: {result:?}"
    );
}

#[test]
fn assign_through_ref_detected() {
    let func = make_function(
        "bad",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            ownership: kodo_ast::Ownership::Ref,
            span: Span::new(0, 10),
        }],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Assign {
                span: Span::new(30, 40),
                name: "x".to_string(),
                value: Expr::IntLit(42, Span::new(34, 36)),
            },
            Stmt::Return {
                span: Span::new(50, 60),
                value: Some(Expr::Ident("x".to_string(), Span::new(57, 58))),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should detect assign through ref");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0248", "expected E0248, got {}", err.code());
}

#[test]
fn copy_types_not_moved() {
    let span = Span::new(0, 100);
    let func = make_function(
        "ok",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Let {
                span: Span::new(10, 30),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(42, Span::new(20, 22)),
            },
            Stmt::Let {
                span: Span::new(30, 50),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(40, 41)),
            },
            Stmt::Let {
                span: Span::new(50, 70),
                mutable: false,
                name: "z".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(60, 61)),
            },
            Stmt::Return {
                span: Span::new(70, 80),
                value: Some(Expr::Ident("z".to_string(), span)),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "copy types should not be moved: {result:?}");
}

#[test]
fn owned_param_can_be_used() {
    let func = make_function(
        "ok",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            ownership: kodo_ast::Ownership::Owned,
            span: Span::new(0, 10),
        }],
        TypeExpr::Named("String".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::Ident("x".to_string(), Span::new(57, 58))),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "owned param should be usable: {result:?}");
}

// --- Phase 54: Send/Sync bounds for spawn blocks ---

#[test]
fn spawn_capture_owned_value_is_ok() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Spawn {
            span: Span::new(0, 50),
            body: Block {
                span: Span::new(5, 45),
                stmts: vec![Stmt::Expr(Expr::IntLit(42, Span::new(10, 12)))],
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "spawn with owned value should pass: {result:?}"
    );
}

#[test]
fn spawn_capture_ref_borrow_is_non_send() {
    let span = Span::new(0, 100);
    let func = Function {
        id: NodeId(1),
        span,
        name: "test_fn".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            ownership: kodo_ast::Ownership::Ref,
            span: Span::new(0, 10),
        }],
        return_type: TypeExpr::Unit,
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(10, 90),
            stmts: vec![Stmt::Spawn {
                span: Span::new(20, 80),
                body: Block {
                    span: Span::new(25, 75),
                    stmts: vec![Stmt::Expr(Expr::Ident("x".to_string(), Span::new(30, 31)))],
                },
            }],
        },
    };
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "spawn capturing ref borrow should fail");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("cannot be sent between threads"),
        "error should mention Send safety: {err}"
    );
    assert_eq!(err.code(), "E0280");
}

#[test]
fn spawn_capture_non_send_error_has_correct_suggestion() {
    let err = TypeError::SpawnCaptureNonSend {
        name: "x".to_string(),
        type_name: "ref borrow".to_string(),
        span: Span::new(0, 10),
    };
    let diag: &dyn kodo_ast::Diagnostic = &err;
    let suggestion = diag.suggestion();
    assert!(suggestion.is_some(), "should have a suggestion");
    assert!(
        suggestion.unwrap().contains("owned values"),
        "suggestion should mention owned values"
    );
}

#[test]
fn spawn_with_binary_op_on_owned_values_is_ok() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 10),
                mutable: false,
                name: "a".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(1, Span::new(5, 6)),
            },
            Stmt::Let {
                span: Span::new(11, 20),
                mutable: false,
                name: "b".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(2, Span::new(15, 16)),
            },
            Stmt::Spawn {
                span: Span::new(21, 60),
                body: Block {
                    span: Span::new(25, 55),
                    stmts: vec![Stmt::Expr(Expr::BinaryOp {
                        left: Box::new(Expr::Ident("a".to_string(), Span::new(30, 31))),
                        op: BinOp::Add,
                        right: Box::new(Expr::Ident("b".to_string(), Span::new(34, 35))),
                        span: Span::new(30, 35),
                    })],
                },
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "spawn with owned values in binary op should pass: {result:?}"
    );
}

#[test]
fn parallel_with_spawn_blocks_is_ok() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Parallel {
            span: Span::new(0, 80),
            body: vec![
                Stmt::Spawn {
                    span: Span::new(10, 30),
                    body: Block {
                        span: Span::new(15, 25),
                        stmts: vec![Stmt::Expr(Expr::IntLit(1, Span::new(17, 18)))],
                    },
                },
                Stmt::Spawn {
                    span: Span::new(35, 55),
                    body: Block {
                        span: Span::new(40, 50),
                        stmts: vec![Stmt::Expr(Expr::IntLit(2, Span::new(42, 43)))],
                    },
                },
            ],
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "parallel with spawn blocks should pass: {result:?}"
    );
}

#[test]
fn repair_plan_mismatch_result_wrapping() {
    use kodo_ast::Diagnostic;
    let err = TypeError::Mismatch {
        expected: "Result<Int, String>".to_string(),
        found: "Int".to_string(),
        span: Span::new(10, 20),
    };
    let plan = err.repair_plan();
    assert!(
        plan.is_some(),
        "should produce a repair plan for Result mismatch"
    );
    let steps = plan.unwrap();
    assert_eq!(steps.len(), 2, "should have 2 steps");
    assert!(steps[0].0.contains("Result::Ok"));
    assert!(!steps[0].1.is_empty());
    assert!(steps[0].1[0].replacement.contains("Result::Ok(Int)"));
    assert!(steps[1].0.contains("verify"));
}

#[test]
fn repair_plan_mismatch_non_result_returns_none() {
    use kodo_ast::Diagnostic;
    let err = TypeError::Mismatch {
        expected: "Int".to_string(),
        found: "String".to_string(),
        span: Span::new(10, 20),
    };
    let plan = err.repair_plan();
    assert!(
        plan.is_none(),
        "non-Result mismatch should not produce a repair plan"
    );
}

#[test]
fn repair_plan_mismatch_result_to_result_returns_none() {
    use kodo_ast::Diagnostic;
    let err = TypeError::Mismatch {
        expected: "Result<Int, String>".to_string(),
        found: "Result<String, String>".to_string(),
        span: Span::new(10, 20),
    };
    let plan = err.repair_plan();
    assert!(
        plan.is_none(),
        "Result-to-Result mismatch should not produce a repair plan"
    );
}

#[test]
fn repair_plan_undefined_variable() {
    use kodo_ast::Diagnostic;
    let err = TypeError::Undefined {
        name: "foo".to_string(),
        span: Span::new(5, 8),
        similar: None,
    };
    let plan = err.repair_plan();
    assert!(
        plan.is_some(),
        "should produce a repair plan for undefined variable"
    );
    let steps = plan.unwrap();
    assert_eq!(steps.len(), 1, "should have 1 step");
    assert!(steps[0].0.contains("foo"));
    assert!(steps[0].1[0].replacement.contains("let foo"));
}

#[test]
fn repair_plan_other_errors_return_none() {
    use kodo_ast::Diagnostic;
    let err = TypeError::MissingMeta;
    let plan = err.repair_plan();
    assert!(
        plan.is_none(),
        "MissingMeta should not produce a repair plan"
    );
}
