//! Basic type checker tests: types, bindings, operators, control flow, function calls.

use super::*;

#[test]
fn correct_let_binding_passes() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 10),
            mutable: false,
            name: "x".to_string(),
            ty: Some(TypeExpr::Named("Int".to_string())),
            value: Expr::IntLit(42, Span::new(5, 7)),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn let_type_mismatch_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 10),
            mutable: false,
            name: "x".to_string(),
            ty: Some(TypeExpr::Named("Int".to_string())),
            value: Expr::BoolLit(true, Span::new(5, 9)),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("type mismatch"));
}

#[test]
fn binary_op_arithmetic_correct() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::BinaryOp {
            left: Box::new(Expr::IntLit(1, Span::new(0, 1))),
            op: BinOp::Add,
            right: Box::new(Expr::IntLit(2, Span::new(4, 5))),
            span: Span::new(0, 5),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn binary_op_type_mismatch_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::BinaryOp {
            left: Box::new(Expr::IntLit(1, Span::new(0, 1))),
            op: BinOp::Add,
            right: Box::new(Expr::BoolLit(true, Span::new(4, 8))),
            span: Span::new(0, 8),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn binary_op_non_numeric_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::BinaryOp {
            left: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
            op: BinOp::Add,
            right: Box::new(Expr::BoolLit(false, Span::new(7, 12))),
            span: Span::new(0, 12),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("numeric type"));
}

#[test]
fn return_type_mismatch_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(0, 10),
            value: Some(Expr::BoolLit(true, Span::new(7, 11))),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("type mismatch"));
}

#[test]
fn return_type_correct_passes() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(0, 10),
            value: Some(Expr::IntLit(42, Span::new(7, 9))),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn undefined_variable_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::Ident("x".to_string(), Span::new(0, 1)))],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("undefined"));
}

#[test]
fn function_params_in_scope() {
    let func = make_function(
        "add",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: Span::new(0, 5),
                ownership: kodo_ast::Ownership::Owned,
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: Span::new(7, 12),
                ownership: kodo_ast::Ownership::Owned,
            },
        ],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(20, 30),
            value: Some(Expr::BinaryOp {
                left: Box::new(Expr::Ident("a".to_string(), Span::new(27, 28))),
                op: BinOp::Add,
                right: Box::new(Expr::Ident("b".to_string(), Span::new(31, 32))),
                span: Span::new(27, 32),
            }),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn logical_ops_require_bool() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::BinaryOp {
            left: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
            op: BinOp::And,
            right: Box::new(Expr::BoolLit(false, Span::new(8, 13))),
            span: Span::new(0, 13),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn logical_ops_reject_non_bool() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::BinaryOp {
            left: Box::new(Expr::IntLit(1, Span::new(0, 1))),
            op: BinOp::And,
            right: Box::new(Expr::IntLit(2, Span::new(5, 6))),
            span: Span::new(0, 6),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn unary_neg_requires_numeric() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand: Box::new(Expr::IntLit(42, Span::new(1, 3))),
            span: Span::new(0, 3),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn unary_neg_rejects_bool() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand: Box::new(Expr::BoolLit(true, Span::new(1, 5))),
            span: Span::new(0, 5),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn unary_not_requires_bool() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(Expr::BoolLit(true, Span::new(1, 5))),
            span: Span::new(0, 5),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn call_correct_passes() {
    let add_fn = make_function(
        "add",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: Span::new(0, 5),
                ownership: kodo_ast::Ownership::Owned,
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: Span::new(7, 12),
                ownership: kodo_ast::Ownership::Owned,
            },
        ],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(20, 30),
            value: Some(Expr::BinaryOp {
                left: Box::new(Expr::Ident("a".to_string(), Span::new(27, 28))),
                op: BinOp::Add,
                right: Box::new(Expr::Ident("b".to_string(), Span::new(31, 32))),
                span: Span::new(27, 32),
            }),
        }],
    );
    let main_fn = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::Call {
            callee: Box::new(Expr::Ident("add".to_string(), Span::new(0, 3))),
            args: vec![
                Expr::IntLit(1, Span::new(4, 5)),
                Expr::IntLit(2, Span::new(7, 8)),
            ],
            span: Span::new(0, 9),
        })],
    );
    let module = make_module(vec![add_fn, main_fn]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn call_arity_mismatch_fails() {
    let add_fn = make_function(
        "add",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: Span::new(0, 5),
                ownership: kodo_ast::Ownership::Owned,
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: Span::new(7, 12),
                ownership: kodo_ast::Ownership::Owned,
            },
        ],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(20, 30),
            value: Some(Expr::IntLit(0, Span::new(27, 28))),
        }],
    );
    let main_fn = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::Call {
            callee: Box::new(Expr::Ident("add".to_string(), Span::new(0, 3))),
            args: vec![Expr::IntLit(1, Span::new(4, 5))],
            span: Span::new(0, 6),
        })],
    );
    let module = make_module(vec![add_fn, main_fn]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("arguments"));
}

#[test]
fn if_condition_must_be_bool() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::If {
            condition: Box::new(Expr::IntLit(1, Span::new(3, 4))),
            then_branch: Block {
                span: Span::new(5, 10),
                stmts: vec![],
            },
            else_branch: None,
            span: Span::new(0, 10),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn if_branches_must_match() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::If {
            condition: Box::new(Expr::BoolLit(true, Span::new(3, 7))),
            then_branch: Block {
                span: Span::new(9, 20),
                stmts: vec![Stmt::Expr(Expr::IntLit(1, Span::new(10, 11)))],
            },
            else_branch: Some(Block {
                span: Span::new(22, 35),
                stmts: vec![Stmt::Expr(Expr::BoolLit(true, Span::new(23, 27)))],
            }),
            span: Span::new(0, 35),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn is_numeric_covers_all_numeric_types() {
    assert!(Type::Int.is_numeric());
    assert!(Type::Int8.is_numeric());
    assert!(Type::Int16.is_numeric());
    assert!(Type::Int32.is_numeric());
    assert!(Type::Int64.is_numeric());
    assert!(Type::Uint.is_numeric());
    assert!(Type::Uint8.is_numeric());
    assert!(Type::Float32.is_numeric());
    assert!(Type::Float64.is_numeric());
    assert!(!Type::Bool.is_numeric());
    assert!(!Type::String.is_numeric());
    assert!(!Type::Unit.is_numeric());
}

#[test]
fn scope_cleanup_after_function() {
    let func = make_function(
        "foo",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span: Span::new(0, 5),
            ownership: kodo_ast::Ownership::Owned,
        }],
        TypeExpr::Unit,
        vec![],
    );
    let func2 = make_function(
        "bar",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::Ident("x".to_string(), Span::new(0, 1)))],
    );
    let module = make_module(vec![func, func2]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("undefined"));
}

#[test]
fn let_without_annotation_infers_type() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![
            Stmt::Let {
                span: Span::new(0, 10),
                mutable: false,
                name: "x".to_string(),
                ty: None,
                value: Expr::IntLit(42, Span::new(5, 7)),
            },
            Stmt::Return {
                span: Span::new(12, 20),
                value: Some(Expr::Ident("x".to_string(), Span::new(19, 20))),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn field_access_returns_unknown() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 20),
            mutable: false,
            name: "x".to_string(),
            ty: None,
            value: Expr::FieldAccess {
                object: Box::new(Expr::Ident("obj".to_string(), Span::new(5, 8))),
                field: "field".to_string(),
                span: Span::new(5, 14),
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn comparison_ops_return_bool() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Named("Bool".to_string()),
        vec![Stmt::Return {
            span: Span::new(0, 15),
            value: Some(Expr::BinaryOp {
                left: Box::new(Expr::IntLit(1, Span::new(7, 8))),
                op: BinOp::Lt,
                right: Box::new(Expr::IntLit(2, Span::new(11, 12))),
                span: Span::new(7, 12),
            }),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn while_condition_must_be_bool() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::While {
            span: Span::new(0, 20),
            condition: Expr::IntLit(1, Span::new(6, 7)),
            body: Block {
                span: Span::new(8, 20),
                stmts: vec![],
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("type mismatch"));
}

#[test]
fn while_body_is_typechecked() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::While {
            span: Span::new(0, 30),
            condition: Expr::BoolLit(true, Span::new(6, 10)),
            body: Block {
                span: Span::new(11, 30),
                stmts: vec![Stmt::Expr(Expr::BinaryOp {
                    left: Box::new(Expr::IntLit(1, Span::new(12, 13))),
                    op: BinOp::Add,
                    right: Box::new(Expr::BoolLit(true, Span::new(16, 20))),
                    span: Span::new(12, 20),
                })],
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn while_valid_passes() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::While {
            span: Span::new(0, 20),
            condition: Expr::BoolLit(true, Span::new(6, 10)),
            body: Block {
                span: Span::new(11, 20),
                stmts: vec![],
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn assign_to_existing_variable_passes() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 15),
                mutable: true,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(1, Span::new(14, 15)),
            },
            Stmt::Assign {
                span: Span::new(16, 22),
                name: "x".to_string(),
                value: Expr::IntLit(42, Span::new(20, 22)),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn assign_to_undefined_variable_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Assign {
            span: Span::new(0, 10),
            name: "x".to_string(),
            value: Expr::IntLit(42, Span::new(4, 6)),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn assign_type_mismatch_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 15),
                mutable: true,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(1, Span::new(14, 15)),
            },
            Stmt::Assign {
                span: Span::new(16, 30),
                name: "x".to_string(),
                value: Expr::BoolLit(true, Span::new(20, 24)),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_err());
}

#[test]
fn trust_policy_rejects_missing_authored_by() {
    let func = make_function_with_annotations("foo", vec![]);
    let module = make_module_with_policy(vec![func], Some("high_security"));
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should reject missing @authored_by");
}

#[test]
fn trust_policy_rejects_missing_confidence() {
    let func = make_function_with_annotations(
        "foo",
        vec![Annotation {
            name: "authored_by".to_string(),
            args: vec![AnnotationArg::Named(
                "agent".to_string(),
                Expr::StringLit("claude".to_string(), Span::new(0, 10)),
            )],
            span: Span::new(0, 20),
        }],
    );
    let module = make_module_with_policy(vec![func], Some("high_security"));
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should reject missing @confidence");
}

#[test]
fn trust_policy_rejects_low_confidence() {
    let func = make_function_with_annotations(
        "foo",
        vec![
            Annotation {
                name: "authored_by".to_string(),
                args: vec![AnnotationArg::Named(
                    "agent".to_string(),
                    Expr::StringLit("claude".to_string(), Span::new(0, 10)),
                )],
                span: Span::new(0, 20),
            },
            Annotation {
                name: "confidence".to_string(),
                args: vec![AnnotationArg::Positional(Expr::IntLit(
                    50,
                    Span::new(0, 10),
                ))],
                span: Span::new(0, 20),
            },
        ],
    );
    let module = make_module_with_policy(vec![func], Some("high_security"));
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "should reject low confidence without @reviewed_by"
    );
}

#[test]
fn trust_policy_accepts_reviewed() {
    let func = make_function_with_annotations(
        "foo",
        vec![
            Annotation {
                name: "authored_by".to_string(),
                args: vec![AnnotationArg::Named(
                    "agent".to_string(),
                    Expr::StringLit("claude".to_string(), Span::new(0, 10)),
                )],
                span: Span::new(0, 20),
            },
            Annotation {
                name: "confidence".to_string(),
                args: vec![AnnotationArg::Positional(Expr::IntLit(
                    50,
                    Span::new(0, 10),
                ))],
                span: Span::new(0, 20),
            },
            Annotation {
                name: "reviewed_by".to_string(),
                args: vec![AnnotationArg::Positional(Expr::StringLit(
                    "human:alice".to_string(),
                    Span::new(0, 10),
                ))],
                span: Span::new(0, 20),
            },
        ],
    );
    let module = make_module_with_policy(vec![func], Some("high_security"));
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "should accept low confidence with @reviewed_by human: {result:?}"
    );
}

#[test]
fn no_policy_no_enforcement() {
    let func = make_function_with_annotations("foo", vec![]);
    let module = make_module_with_policy(vec![func], None);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "without trust_policy, no annotation enforcement: {result:?}"
    );
}

#[test]
fn type_error_span_method() {
    let err = TypeError::Mismatch {
        expected: "Int".to_string(),
        found: "Bool".to_string(),
        span: Span::new(5, 10),
    };
    assert_eq!(err.span(), Some(Span::new(5, 10)));
    let err = TypeError::Undefined {
        name: "x".to_string(),
        span: Span::new(3, 4),
        similar: None,
    };
    assert_eq!(err.span(), Some(Span::new(3, 4)));
}
