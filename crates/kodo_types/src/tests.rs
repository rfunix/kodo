//! Tests for the Kōdo type checker.

use super::*;
use kodo_ast::{
    Annotation, AnnotationArg, BinOp, Block, Expr, Function, Meta, MetaEntry, Module, NodeId,
    Param, Span, Stmt, TypeExpr, UnaryOp,
};

#[test]
fn type_display() {
    assert_eq!(Type::Int.to_string(), "Int");
    assert_eq!(Type::Unit.to_string(), "()");
    assert_eq!(
        Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Bool)).to_string(),
        "(Int, Int) -> Bool"
    );
    assert_eq!(
        Type::Generic("List".to_string(), vec![Type::Int]).to_string(),
        "List<Int>"
    );
}

#[test]
fn type_env_lookup() {
    let mut env = TypeEnv::new();
    env.insert("x".to_string(), Type::Int);
    env.insert("y".to_string(), Type::Bool);
    assert_eq!(env.lookup("x"), Some(&Type::Int));
    assert_eq!(env.lookup("y"), Some(&Type::Bool));
    assert_eq!(env.lookup("z"), None);
}

#[test]
fn type_env_shadowing() {
    let mut env = TypeEnv::new();
    env.insert("x".to_string(), Type::Int);
    env.insert("x".to_string(), Type::Bool);
    assert_eq!(env.lookup("x"), Some(&Type::Bool));
}

#[test]
fn check_eq_same_types() {
    let result = TypeEnv::check_eq(&Type::Int, &Type::Int, Span::new(0, 1));
    assert!(result.is_ok());
}

#[test]
fn check_eq_different_types() {
    let result = TypeEnv::check_eq(&Type::Int, &Type::Bool, Span::new(0, 1));
    assert!(result.is_err());
}

#[test]
fn resolve_primitive_types() {
    let span = Span::new(0, 3);
    let result = resolve_type(&kodo_ast::TypeExpr::Named("Int".to_string()), span);
    assert!(result.is_ok());
    assert_eq!(result.unwrap_or(Type::Unknown), Type::Int);
}

// --- TypeChecker tests ---

/// Helper to build a minimal module with one function.
fn make_module(functions: Vec<Function>) -> Module {
    Module {
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "unit test module".to_string(),
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
        functions,
    }
}

/// Creates a `GenericParam` with no bounds for test convenience.
fn gp(name: &str) -> kodo_ast::GenericParam {
    kodo_ast::GenericParam {
        name: name.to_string(),
        bounds: vec![],
        span: Span::new(0, 0),
    }
}

/// Helper to build a function with the given body statements.
fn make_function(
    name: &str,
    params: Vec<Param>,
    return_type: TypeExpr,
    stmts: Vec<Stmt>,
) -> Function {
    Function {
        id: NodeId(1),
        span: Span::new(0, 100),
        name: name.to_string(),
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params,
        return_type,
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 100),
            stmts,
        },
    }
}

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

/// Helper to build a module with a specific trust policy.
fn make_module_with_policy(functions: Vec<Function>, policy: Option<&str>) -> Module {
    let mut entries = vec![MetaEntry {
        key: "purpose".to_string(),
        value: "test".to_string(),
        span: Span::new(10, 40),
    }];
    if let Some(p) = policy {
        entries.push(MetaEntry {
            key: "trust_policy".to_string(),
            value: p.to_string(),
            span: Span::new(10, 40),
        });
    }
    Module {
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries,
        }),
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        functions,
    }
}

/// Helper to build a function with annotations.
fn make_function_with_annotations(name: &str, annotations: Vec<Annotation>) -> Function {
    Function {
        id: NodeId(1),
        span: Span::new(0, 100),
        name: name.to_string(),
        is_async: false,
        generic_params: vec![],
        annotations,
        params: vec![],
        return_type: TypeExpr::Unit,
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 100),
            stmts: vec![],
        },
    }
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

// ===== Generics (Phase 2) Tests =====

/// Helper to build a module with type and enum declarations.
fn make_module_with_decls(
    type_decls: Vec<kodo_ast::TypeDecl>,
    enum_decls: Vec<kodo_ast::EnumDecl>,
    functions: Vec<Function>,
) -> Module {
    Module {
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "unit test module".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls,
        enum_decls,
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        functions,
    }
}

#[test]
fn mono_name_single_arg() {
    let name = TypeChecker::mono_name("Option", &[Type::Int]);
    assert_eq!(name, "Option__Int");
}

#[test]
fn mono_name_multiple_args() {
    let name = TypeChecker::mono_name("Pair", &[Type::Int, Type::Bool]);
    assert_eq!(name, "Pair__Int_Bool");
}

#[test]
fn mono_name_string_arg() {
    let name = TypeChecker::mono_name("Box", &[Type::String]);
    assert_eq!(name, "Box__String");
}

#[test]
fn compatible_enum_types_same_name() {
    assert!(TypeChecker::compatible_enum_types(
        &Type::Enum("Option__Int".to_string()),
        &Type::Enum("Option__Int".to_string())
    ));
}

#[test]
fn compatible_enum_types_unresolved_param() {
    assert!(TypeChecker::compatible_enum_types(
        &Type::Enum("Option__Int".to_string()),
        &Type::Enum("Option__?".to_string())
    ));
}

#[test]
fn compatible_enum_types_different_base() {
    assert!(!TypeChecker::compatible_enum_types(
        &Type::Enum("Option__Int".to_string()),
        &Type::Enum("Result__Int".to_string())
    ));
}

#[test]
fn compatible_enum_types_non_enum() {
    assert!(!TypeChecker::compatible_enum_types(
        &Type::Int,
        &Type::Enum("Option__Int".to_string())
    ));
}

#[test]
fn monomorphize_option_int_registers_in_enum_registry() {
    let enum_decl = kodo_ast::EnumDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Option".to_string(),
        generic_params: vec![gp("T")],
        variants: vec![
            kodo_ast::EnumVariant {
                name: "Some".to_string(),
                fields: vec![TypeExpr::Named("T".to_string())],
                span: Span::new(0, 20),
            },
            kodo_ast::EnumVariant {
                name: "None".to_string(),
                fields: vec![],
                span: Span::new(21, 30),
            },
        ],
    };
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 30),
            mutable: false,
            name: "x".to_string(),
            ty: Some(TypeExpr::Generic(
                "Option".to_string(),
                vec![TypeExpr::Named("Int".to_string())],
            )),
            value: Expr::EnumVariantExpr {
                enum_name: "Option".to_string(),
                variant: "Some".to_string(),
                args: vec![Expr::IntLit(42, Span::new(25, 27))],
                span: Span::new(15, 28),
            },
        }],
    );
    let module = make_module_with_decls(vec![], vec![enum_decl], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "check_module failed: {result:?}");
    assert!(
        checker.enum_registry().contains_key("Option__Int"),
        "Option__Int should be in enum_registry"
    );
    let variants = checker.enum_registry().get("Option__Int").unwrap();
    let some_variant = variants.iter().find(|(n, _)| n == "Some").unwrap();
    assert_eq!(some_variant.1, vec![Type::Int]);
    let none_variant = variants.iter().find(|(n, _)| n == "None").unwrap();
    assert!(none_variant.1.is_empty());
}

#[test]
fn wrong_type_arg_count_error_e0221() {
    let enum_decl = kodo_ast::EnumDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Option".to_string(),
        generic_params: vec![gp("T")],
        variants: vec![
            kodo_ast::EnumVariant {
                name: "Some".to_string(),
                fields: vec![TypeExpr::Named("T".to_string())],
                span: Span::new(0, 20),
            },
            kodo_ast::EnumVariant {
                name: "None".to_string(),
                fields: vec![],
                span: Span::new(21, 30),
            },
        ],
    };
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 30),
            mutable: false,
            name: "x".to_string(),
            ty: Some(TypeExpr::Generic(
                "Option".to_string(),
                vec![
                    TypeExpr::Named("Int".to_string()),
                    TypeExpr::Named("Bool".to_string()),
                ],
            )),
            value: Expr::IntLit(0, Span::new(25, 26)),
        }],
    );
    let module = make_module_with_decls(vec![], vec![enum_decl], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0221");
    assert!(
        err.to_string().contains("type argument"),
        "error should mention type arguments: {err}"
    );
}

#[test]
fn missing_type_args_error_e0223() {
    let enum_decl = kodo_ast::EnumDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Option".to_string(),
        generic_params: vec![gp("T")],
        variants: vec![
            kodo_ast::EnumVariant {
                name: "Some".to_string(),
                fields: vec![TypeExpr::Named("T".to_string())],
                span: Span::new(0, 20),
            },
            kodo_ast::EnumVariant {
                name: "None".to_string(),
                fields: vec![],
                span: Span::new(21, 30),
            },
        ],
    };
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 30),
            mutable: false,
            name: "x".to_string(),
            ty: Some(TypeExpr::Named("Option".to_string())),
            value: Expr::IntLit(0, Span::new(25, 26)),
        }],
    );
    let module = make_module_with_decls(vec![], vec![enum_decl], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0223");
    assert!(
        err.to_string().contains("requires type arguments"),
        "error should mention requires type arguments: {err}"
    );
}

#[test]
fn generic_enum_some_and_none_typecheck() {
    let enum_decl = kodo_ast::EnumDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Option".to_string(),
        generic_params: vec![gp("T")],
        variants: vec![
            kodo_ast::EnumVariant {
                name: "Some".to_string(),
                fields: vec![TypeExpr::Named("T".to_string())],
                span: Span::new(0, 20),
            },
            kodo_ast::EnumVariant {
                name: "None".to_string(),
                fields: vec![],
                span: Span::new(21, 30),
            },
        ],
    };
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 30),
                mutable: false,
                name: "a".to_string(),
                ty: Some(TypeExpr::Generic(
                    "Option".to_string(),
                    vec![TypeExpr::Named("Int".to_string())],
                )),
                value: Expr::EnumVariantExpr {
                    enum_name: "Option".to_string(),
                    variant: "Some".to_string(),
                    args: vec![Expr::IntLit(42, Span::new(25, 27))],
                    span: Span::new(15, 28),
                },
            },
            Stmt::Let {
                span: Span::new(31, 60),
                mutable: false,
                name: "b".to_string(),
                ty: Some(TypeExpr::Generic(
                    "Option".to_string(),
                    vec![TypeExpr::Named("Int".to_string())],
                )),
                value: Expr::EnumVariantExpr {
                    enum_name: "Option".to_string(),
                    variant: "None".to_string(),
                    args: vec![],
                    span: Span::new(45, 58),
                },
            },
        ],
    );
    let module = make_module_with_decls(vec![], vec![enum_decl], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "should typecheck Option::Some(42) and Option::None: {result:?}"
    );
}

#[test]
fn generic_enum_type_mismatch_in_some_fails() {
    let enum_decl = kodo_ast::EnumDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Option".to_string(),
        generic_params: vec![gp("T")],
        variants: vec![
            kodo_ast::EnumVariant {
                name: "Some".to_string(),
                fields: vec![TypeExpr::Named("T".to_string())],
                span: Span::new(0, 20),
            },
            kodo_ast::EnumVariant {
                name: "None".to_string(),
                fields: vec![],
                span: Span::new(21, 30),
            },
        ],
    };
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 30),
            mutable: false,
            name: "x".to_string(),
            ty: Some(TypeExpr::Generic(
                "Option".to_string(),
                vec![TypeExpr::Named("Int".to_string())],
            )),
            value: Expr::EnumVariantExpr {
                enum_name: "Option".to_string(),
                variant: "Some".to_string(),
                args: vec![Expr::BoolLit(true, Span::new(25, 29))],
                span: Span::new(15, 30),
            },
        }],
    );
    let module = make_module_with_decls(vec![], vec![enum_decl], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should reject Bool in Option<Int>::Some");
}

#[test]
fn generic_struct_monomorphizes_correctly() {
    let struct_decl = kodo_ast::TypeDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Wrapper".to_string(),
        generic_params: vec![gp("T")],
        fields: vec![kodo_ast::FieldDef {
            name: "value".to_string(),
            ty: TypeExpr::Named("T".to_string()),
            span: Span::new(0, 20),
        }],
    };
    let func = make_function(
        "main",
        vec![Param {
            name: "w".to_string(),
            ty: TypeExpr::Generic(
                "Wrapper".to_string(),
                vec![TypeExpr::Named("Int".to_string())],
            ),
            span: Span::new(0, 20),
            ownership: kodo_ast::Ownership::Owned,
        }],
        TypeExpr::Unit,
        vec![],
    );
    let module = make_module_with_decls(vec![struct_decl], vec![], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "check_module failed: {result:?}");
    assert!(checker.struct_registry().contains_key("Wrapper__Int"));
    let fields = checker.struct_registry().get("Wrapper__Int").unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].0, "value");
    assert_eq!(fields[0].1, Type::Int);
}

#[test]
fn wrong_type_arg_count_for_generic_struct() {
    let struct_decl = kodo_ast::TypeDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Wrapper".to_string(),
        generic_params: vec![gp("T")],
        fields: vec![kodo_ast::FieldDef {
            name: "value".to_string(),
            ty: TypeExpr::Named("T".to_string()),
            span: Span::new(0, 20),
        }],
    };
    let func = make_function(
        "main",
        vec![Param {
            name: "w".to_string(),
            ty: TypeExpr::Generic(
                "Wrapper".to_string(),
                vec![
                    TypeExpr::Named("Int".to_string()),
                    TypeExpr::Named("Bool".to_string()),
                ],
            ),
            span: Span::new(0, 20),
            ownership: kodo_ast::Ownership::Owned,
        }],
        TypeExpr::Unit,
        vec![],
    );
    let module = make_module_with_decls(vec![struct_decl], vec![], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0221");
}

#[test]
fn type_display_generic() {
    let ty = Type::Generic("Option".to_string(), vec![Type::Int]);
    assert_eq!(ty.to_string(), "Option<Int>");
    let ty = Type::Generic("Pair".to_string(), vec![Type::Int, Type::Bool]);
    assert_eq!(ty.to_string(), "Pair<Int, Bool>");
}

#[test]
fn for_loop_valid_passes() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::For {
            span: Span::new(0, 30),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(9, 10)),
            end: Expr::IntLit(10, Span::new(12, 14)),
            inclusive: false,
            body: Block {
                span: Span::new(15, 30),
                stmts: vec![],
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn for_loop_non_int_start_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::For {
            span: Span::new(0, 30),
            name: "i".to_string(),
            start: Expr::BoolLit(true, Span::new(9, 13)),
            end: Expr::IntLit(10, Span::new(15, 17)),
            inclusive: false,
            body: Block {
                span: Span::new(18, 30),
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
fn for_loop_non_int_end_fails() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::For {
            span: Span::new(0, 30),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(9, 10)),
            end: Expr::BoolLit(false, Span::new(12, 17)),
            inclusive: false,
            body: Block {
                span: Span::new(18, 30),
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
fn for_loop_body_can_use_loop_var() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::For {
            span: Span::new(0, 40),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(9, 10)),
            end: Expr::IntLit(10, Span::new(12, 14)),
            inclusive: false,
            body: Block {
                span: Span::new(15, 40),
                stmts: vec![Stmt::Expr(Expr::BinaryOp {
                    left: Box::new(Expr::Ident("i".to_string(), Span::new(20, 21))),
                    op: BinOp::Add,
                    right: Box::new(Expr::IntLit(1, Span::new(24, 25))),
                    span: Span::new(20, 25),
                })],
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn closure_type_inference() {
    let closure = Expr::Closure {
        params: vec![kodo_ast::ClosureParam {
            name: "x".to_string(),
            ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
            span: Span::new(0, 5),
        }],
        return_type: None,
        body: Box::new(Expr::BinaryOp {
            left: Box::new(Expr::Ident("x".to_string(), Span::new(7, 8))),
            op: BinOp::Add,
            right: Box::new(Expr::IntLit(1, Span::new(11, 12))),
            span: Span::new(7, 12),
        }),
        span: Span::new(0, 12),
    };
    let mut checker = TypeChecker::new();
    let ty = checker.infer_expr(&closure);
    assert!(ty.is_ok());
    let ty = ty.unwrap_or_else(|_| panic!("type error"));
    assert_eq!(ty, Type::Function(vec![Type::Int], Box::new(Type::Int)));
}

#[test]
fn closure_param_missing_type_annotation() {
    let closure = Expr::Closure {
        params: vec![kodo_ast::ClosureParam {
            name: "x".to_string(),
            ty: None,
            span: Span::new(1, 2),
        }],
        return_type: None,
        body: Box::new(Expr::Ident("x".to_string(), Span::new(4, 5))),
        span: Span::new(0, 5),
    };
    let mut checker = TypeChecker::new();
    let result = checker.infer_expr(&closure);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0227");
}

#[test]
fn check_trait_and_impl_basic() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 200),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test traits".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Point".to_string(),
            generic_params: vec![],
            fields: vec![
                kodo_ast::FieldDef {
                    name: "x".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                    span: Span::new(60, 65),
                },
                kodo_ast::FieldDef {
                    name: "y".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                    span: Span::new(66, 71),
                },
            ],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(80, 120),
            name: "Summable".to_string(),
            associated_types: vec![],
            methods: vec![kodo_ast::TraitMethod {
                name: "sum".to_string(),
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                    span: Span::new(90, 94),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                has_self: true,
                body: None,
                span: Span::new(85, 115),
            }],
        }],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(3),
            span: Span::new(120, 180),
            trait_name: Some("Summable".to_string()),
            type_name: "Point".to_string(),
            type_bindings: vec![],
            methods: vec![Function {
                id: NodeId(4),
                span: Span::new(130, 175),
                name: "sum".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Point".to_string()),
                    span: Span::new(135, 139),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: kodo_ast::Block {
                    span: Span::new(145, 175),
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: Span::new(150, 170),
                        value: Some(Expr::BinaryOp {
                            left: Box::new(Expr::FieldAccess {
                                object: Box::new(Expr::Ident(
                                    "self".to_string(),
                                    Span::new(157, 161),
                                )),
                                field: "x".to_string(),
                                span: Span::new(157, 163),
                            }),
                            op: kodo_ast::BinOp::Add,
                            right: Box::new(Expr::FieldAccess {
                                object: Box::new(Expr::Ident(
                                    "self".to_string(),
                                    Span::new(166, 170),
                                )),
                                field: "y".to_string(),
                                span: Span::new(166, 172),
                            }),
                            span: Span::new(157, 172),
                        }),
                    }],
                },
            }],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![Function {
            id: NodeId(5),
            span: Span::new(180, 200),
            name: "main".to_string(),
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![],
            body: kodo_ast::Block {
                span: Span::new(185, 200),
                stmts: vec![kodo_ast::Stmt::Return {
                    span: Span::new(190, 198),
                    value: Some(Expr::IntLit(0, Span::new(197, 198))),
                }],
            },
        }],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "trait + impl should type check: {result:?}");
    let lookup = checker.method_lookup();
    assert!(
        lookup.contains_key(&("Point".to_string(), "sum".to_string())),
        "method lookup should contain Point.sum"
    );
}

#[test]
fn check_unknown_trait_error() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(1),
            span: Span::new(50, 80),
            trait_name: Some("NonExistent".to_string()),
            type_name: "Int".to_string(),
            type_bindings: vec![],
            methods: vec![],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0230");
}

#[test]
fn check_missing_trait_method_error() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 65),
            name: "Point".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "x".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(55, 60),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(65, 80),
            name: "Describable".to_string(),
            associated_types: vec![],
            methods: vec![kodo_ast::TraitMethod {
                name: "describe".to_string(),
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                    span: Span::new(70, 74),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                has_self: true,
                body: None,
                span: Span::new(68, 78),
            }],
        }],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(3),
            span: Span::new(80, 95),
            trait_name: Some("Describable".to_string()),
            type_name: "Point".to_string(),
            type_bindings: vec![],
            methods: vec![],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0231");
}

#[test]
fn check_inherent_impl_registers_methods() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 250),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Point".to_string(),
            generic_params: vec![],
            fields: vec![
                kodo_ast::FieldDef {
                    name: "x".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                    span: Span::new(55, 60),
                },
                kodo_ast::FieldDef {
                    name: "y".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                    span: Span::new(65, 70),
                },
            ],
        }],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(2),
            span: Span::new(80, 160),
            trait_name: None,
            type_name: "Point".to_string(),
            type_bindings: vec![],
            methods: vec![Function {
                id: NodeId(3),
                span: Span::new(85, 155),
                name: "sum".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Point".to_string()),
                    span: Span::new(90, 94),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: kodo_ast::Block {
                    span: Span::new(100, 155),
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: Span::new(105, 150),
                        value: Some(Expr::BinaryOp {
                            left: Box::new(Expr::FieldAccess {
                                object: Box::new(Expr::Ident(
                                    "self".to_string(),
                                    Span::new(112, 116),
                                )),
                                field: "x".to_string(),
                                span: Span::new(112, 118),
                            }),
                            op: kodo_ast::BinOp::Add,
                            right: Box::new(Expr::FieldAccess {
                                object: Box::new(Expr::Ident(
                                    "self".to_string(),
                                    Span::new(121, 125),
                                )),
                                field: "y".to_string(),
                                span: Span::new(121, 127),
                            }),
                            span: Span::new(112, 127),
                        }),
                    }],
                },
            }],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![Function {
            id: NodeId(4),
            span: Span::new(160, 200),
            name: "main".to_string(),
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![],
            body: kodo_ast::Block {
                span: Span::new(165, 200),
                stmts: vec![kodo_ast::Stmt::Return {
                    span: Span::new(170, 198),
                    value: Some(Expr::IntLit(0, Span::new(177, 178))),
                }],
            },
        }],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "inherent impl should type check: {result:?}"
    );

    // Verify method lookup was registered
    let lookup = checker.method_lookup();
    assert!(
        lookup.contains_key(&("Point".to_string(), "sum".to_string())),
        "method lookup should contain Point.sum from inherent impl"
    );
}

#[test]
fn check_inherent_impl_no_trait_required() {
    // Inherent impl should not require a trait to be defined.
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 200),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Point".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "x".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(55, 60),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![], // No traits defined
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(2),
            span: Span::new(80, 140),
            trait_name: None, // Inherent impl
            type_name: "Point".to_string(),
            type_bindings: vec![],
            methods: vec![Function {
                id: NodeId(3),
                span: Span::new(85, 135),
                name: "get_x".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Point".to_string()),
                    span: Span::new(90, 94),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: kodo_ast::Block {
                    span: Span::new(100, 135),
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: Span::new(105, 130),
                        value: Some(Expr::FieldAccess {
                            object: Box::new(Expr::Ident("self".to_string(), Span::new(112, 116))),
                            field: "x".to_string(),
                            span: Span::new(112, 118),
                        }),
                    }],
                },
            }],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![Function {
            id: NodeId(4),
            span: Span::new(140, 180),
            name: "main".to_string(),
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![],
            body: kodo_ast::Block {
                span: Span::new(145, 180),
                stmts: vec![kodo_ast::Stmt::Return {
                    span: Span::new(150, 178),
                    value: Some(Expr::IntLit(0, Span::new(157, 158))),
                }],
            },
        }],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "inherent impl without traits should type check: {result:?}"
    );

    let lookup = checker.method_lookup();
    assert!(
        lookup.contains_key(&("Point".to_string(), "get_x".to_string())),
        "method lookup should contain Point.get_x from inherent impl"
    );
}

#[test]
fn check_inherent_and_trait_impl_same_type() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 300),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Point".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "x".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(55, 60),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(80, 120),
            name: "Summable".to_string(),
            associated_types: vec![],
            methods: vec![kodo_ast::TraitMethod {
                name: "sum".to_string(),
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                    span: Span::new(90, 94),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                has_self: true,
                body: None,
                span: Span::new(85, 115),
            }],
        }],
        impl_blocks: vec![
            // Inherent impl
            kodo_ast::ImplBlock {
                id: NodeId(3),
                span: Span::new(120, 170),
                trait_name: None,
                type_name: "Point".to_string(),
                type_bindings: vec![],
                methods: vec![Function {
                    id: NodeId(4),
                    span: Span::new(125, 165),
                    name: "get_x".to_string(),
                    is_async: false,
                    generic_params: vec![],
                    annotations: vec![],
                    params: vec![kodo_ast::Param {
                        name: "self".to_string(),
                        ty: kodo_ast::TypeExpr::Named("Point".to_string()),
                        span: Span::new(130, 134),
                        ownership: kodo_ast::Ownership::Owned,
                    }],
                    return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                    requires: vec![],
                    ensures: vec![],
                    body: kodo_ast::Block {
                        span: Span::new(140, 165),
                        stmts: vec![kodo_ast::Stmt::Return {
                            span: Span::new(145, 160),
                            value: Some(Expr::FieldAccess {
                                object: Box::new(Expr::Ident(
                                    "self".to_string(),
                                    Span::new(152, 156),
                                )),
                                field: "x".to_string(),
                                span: Span::new(152, 158),
                            }),
                        }],
                    },
                }],
            },
            // Trait impl
            kodo_ast::ImplBlock {
                id: NodeId(5),
                span: Span::new(170, 230),
                trait_name: Some("Summable".to_string()),
                type_name: "Point".to_string(),
                type_bindings: vec![],
                methods: vec![Function {
                    id: NodeId(6),
                    span: Span::new(175, 225),
                    name: "sum".to_string(),
                    is_async: false,
                    generic_params: vec![],
                    annotations: vec![],
                    params: vec![kodo_ast::Param {
                        name: "self".to_string(),
                        ty: kodo_ast::TypeExpr::Named("Point".to_string()),
                        span: Span::new(180, 184),
                        ownership: kodo_ast::Ownership::Owned,
                    }],
                    return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                    requires: vec![],
                    ensures: vec![],
                    body: kodo_ast::Block {
                        span: Span::new(190, 225),
                        stmts: vec![kodo_ast::Stmt::Return {
                            span: Span::new(195, 220),
                            value: Some(Expr::FieldAccess {
                                object: Box::new(Expr::Ident(
                                    "self".to_string(),
                                    Span::new(202, 206),
                                )),
                                field: "x".to_string(),
                                span: Span::new(202, 208),
                            }),
                        }],
                    },
                }],
            },
        ],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![Function {
            id: NodeId(7),
            span: Span::new(230, 270),
            name: "main".to_string(),
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![],
            body: kodo_ast::Block {
                span: Span::new(235, 270),
                stmts: vec![kodo_ast::Stmt::Return {
                    span: Span::new(240, 268),
                    value: Some(Expr::IntLit(0, Span::new(247, 248))),
                }],
            },
        }],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "inherent + trait impl on same type should type check: {result:?}"
    );

    let lookup = checker.method_lookup();
    assert!(
        lookup.contains_key(&("Point".to_string(), "get_x".to_string())),
        "should contain inherent method Point.get_x"
    );
    assert!(
        lookup.contains_key(&("Point".to_string(), "sum".to_string())),
        "should contain trait method Point.sum"
    );
}

#[test]
fn await_outside_async_is_error() {
    let module = make_module(vec![Function {
        id: NodeId(0),
        span: Span::new(0, 10),
        name: "sync_fn".to_string(),
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![],
        body: kodo_ast::Block {
            span: Span::new(0, 10),
            stmts: vec![kodo_ast::Stmt::Return {
                span: Span::new(0, 10),
                value: Some(Expr::Await {
                    operand: Box::new(Expr::IntLit(42, Span::new(0, 2))),
                    span: Span::new(0, 8),
                }),
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "await outside async should be an error");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0250");
}

#[test]
fn await_inside_async_is_ok() {
    let module = make_module(vec![Function {
        id: NodeId(0),
        span: Span::new(0, 10),
        name: "async_fn".to_string(),
        is_async: true,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![],
        body: kodo_ast::Block {
            span: Span::new(0, 10),
            stmts: vec![kodo_ast::Stmt::Return {
                span: Span::new(0, 10),
                value: Some(Expr::Await {
                    operand: Box::new(Expr::IntLit(42, Span::new(0, 2))),
                    span: Span::new(0, 8),
                }),
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "await inside async should be ok: {result:?}"
    );
}

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
        functions: vec![func_weak, func_main],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should fail due to confidence threshold");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0261");
}

// ===== Ownership Enforcement Tests =====

#[test]
fn use_after_move_detected() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 20),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hello".to_string(), Span::new(15, 22)),
            },
            Stmt::Let {
                span: Span::new(25, 40),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(35, 36)),
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("println".to_string(), Span::new(45, 52))),
                args: vec![Expr::Ident("x".to_string(), Span::new(53, 54))],
                span: Span::new(45, 55),
            }),
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should detect use-after-move");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0240", "expected E0240, got {}", err.code());
}

#[test]
fn ownership_no_error_without_reuse() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 20),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hello".to_string(), Span::new(15, 22)),
            },
            Stmt::Let {
                span: Span::new(25, 40),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(35, 36)),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "should not error when moved var is not reused: {result:?}"
    );
}

#[test]
fn ownership_primitives_can_be_reused() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 20),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(42, Span::new(15, 17)),
            },
            Stmt::Let {
                span: Span::new(25, 40),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(35, 36)),
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("print_int".to_string(), Span::new(45, 54))),
                args: vec![Expr::Ident("x".to_string(), Span::new(55, 56))],
                span: Span::new(45, 57),
            }),
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "Copy types (Int) should not be moved");
}

#[test]
fn levenshtein_suggests_similar_name() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 20),
                mutable: false,
                name: "counter".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(0, Span::new(15, 16)),
            },
            Stmt::Expr(Expr::Ident("conter".to_string(), Span::new(25, 31))),
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    if let TypeError::Undefined { similar, .. } = &err {
        assert_eq!(similar.as_deref(), Some("counter"));
    } else {
        panic!("expected TypeError::Undefined, got {err:?}");
    }
}

#[test]
fn is_copy_returns_true_for_primitives() {
    assert!(Type::Int.is_copy());
    assert!(Type::Bool.is_copy());
    assert!(Type::Float64.is_copy());
    assert!(Type::Byte.is_copy());
    assert!(Type::Unit.is_copy());
    assert!(!Type::String.is_copy());
    assert!(!Type::Struct("Foo".to_string()).is_copy());
}

#[test]
fn struct_type_is_moved_in_let() {
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 20),
                mutable: false,
                name: "a".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hello".to_string(), Span::new(10, 17)),
            },
            Stmt::Let {
                span: Span::new(25, 45),
                mutable: false,
                name: "b".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::Ident("a".to_string(), Span::new(35, 36)),
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("println".to_string(), Span::new(50, 57))),
                args: vec![Expr::Ident("a".to_string(), Span::new(58, 59))],
                span: Span::new(50, 60),
            }),
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), "E0240");
}

#[test]
fn fix_patch_for_missing_meta() {
    use kodo_ast::Diagnostic;
    let err = TypeError::MissingMeta;
    let patch = err.fix_patch();
    assert!(patch.is_some());
    let patch = patch.unwrap();
    assert!(patch.replacement.contains("meta"));
    assert!(patch.replacement.contains("purpose"));
}

#[test]
fn fix_patch_for_low_confidence() {
    use kodo_ast::Diagnostic;
    let err = TypeError::LowConfidenceWithoutReview {
        name: "process".to_string(),
        confidence: "0.5".to_string(),
        span: Span::new(10, 20),
    };
    let patch = err.fix_patch();
    assert!(patch.is_some());
    let patch = patch.unwrap();
    assert!(patch.replacement.contains("@reviewed_by"));
}

#[test]
fn borrow_escapes_scope_detected() {
    let func = make_function(
        "bad",
        vec![Param {
            name: "s".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            ownership: kodo_ast::Ownership::Ref,
            span: Span::new(0, 10),
        }],
        TypeExpr::Named("String".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::Ident("s".to_string(), Span::new(57, 58))),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should detect borrow escaping scope");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0241", "expected E0241, got {}", err.code());
}

#[test]
fn return_owned_value_ok() {
    let func = make_function(
        "good",
        vec![Param {
            name: "s".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            ownership: kodo_ast::Ownership::Owned,
            span: Span::new(0, 10),
        }],
        TypeExpr::Named("String".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 60),
            value: Some(Expr::Ident("s".to_string(), Span::new(57, 58))),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "returning an owned value should succeed: {result:?}"
    );
}

#[test]
fn builtin_string_methods_registered() {
    let checker = TypeChecker::new();
    let lookup = checker.method_lookup();
    let key = ("String".to_string(), "length".to_string());
    let (mangled, params, ret) = lookup
        .get(&key)
        .cloned()
        .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
    assert_eq!(mangled, "String_length");
    assert_eq!(params, vec![Type::String]);
    assert_eq!(ret, Type::Int);
    let key = ("String".to_string(), "contains".to_string());
    let (mangled, params, ret) = lookup
        .get(&key)
        .cloned()
        .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
    assert_eq!(mangled, "String_contains");
    assert_eq!(params, vec![Type::String, Type::String]);
    assert_eq!(ret, Type::Bool);
    assert!(lookup.contains_key(&("String".to_string(), "starts_with".to_string())));
    assert!(lookup.contains_key(&("String".to_string(), "ends_with".to_string())));
    let key = ("String".to_string(), "trim".to_string());
    let (_, _, ret) = lookup
        .get(&key)
        .cloned()
        .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
    assert_eq!(ret, Type::String);
    assert!(lookup.contains_key(&("String".to_string(), "to_upper".to_string())));
    assert!(lookup.contains_key(&("String".to_string(), "to_lower".to_string())));
    let key = ("String".to_string(), "substring".to_string());
    let (_, params, ret) = lookup
        .get(&key)
        .cloned()
        .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
    assert_eq!(params, vec![Type::String, Type::Int, Type::Int]);
    assert_eq!(ret, Type::String);
}

#[test]
fn builtin_int_methods_registered() {
    let checker = TypeChecker::new();
    let lookup = checker.method_lookup();
    let key = ("Int".to_string(), "to_string".to_string());
    let (mangled, params, ret) = lookup
        .get(&key)
        .cloned()
        .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
    assert_eq!(mangled, "Int_to_string");
    assert_eq!(params, vec![Type::Int]);
    assert_eq!(ret, Type::String);
    let key = ("Int".to_string(), "to_float64".to_string());
    let (_, _, ret) = lookup
        .get(&key)
        .cloned()
        .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
    assert_eq!(ret, Type::Float64);
}

#[test]
fn builtin_float64_methods_registered() {
    let checker = TypeChecker::new();
    let lookup = checker.method_lookup();
    let key = ("Float64".to_string(), "to_string".to_string());
    let (mangled, _, ret) = lookup
        .get(&key)
        .cloned()
        .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
    assert_eq!(mangled, "Float64_to_string");
    assert_eq!(ret, Type::String);
    let key = ("Float64".to_string(), "to_int".to_string());
    let (_, _, ret) = lookup
        .get(&key)
        .cloned()
        .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
    assert_eq!(ret, Type::Int);
}

#[test]
fn string_method_call_typechecks() {
    let func = make_function(
        "test_string_length",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 80),
            value: Some(Expr::Call {
                callee: Box::new(Expr::FieldAccess {
                    object: Box::new(Expr::StringLit("hello".to_string(), Span::new(55, 62))),
                    field: "length".to_string(),
                    span: Span::new(55, 69),
                }),
                args: vec![],
                span: Span::new(55, 71),
            }),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "String.length() should type-check: {result:?}"
    );
}

#[test]
fn string_contains_method_typechecks() {
    let func = make_function(
        "test_contains",
        vec![],
        TypeExpr::Named("Bool".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 100),
            value: Some(Expr::Call {
                callee: Box::new(Expr::FieldAccess {
                    object: Box::new(Expr::StringLit(
                        "hello world".to_string(),
                        Span::new(55, 68),
                    )),
                    field: "contains".to_string(),
                    span: Span::new(55, 77),
                }),
                args: vec![Expr::StringLit("world".to_string(), Span::new(78, 85))],
                span: Span::new(55, 86),
            }),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "String.contains() should type-check: {result:?}"
    );
}

#[test]
fn int_to_string_method_typechecks() {
    let func = make_function(
        "test_int_to_string",
        vec![],
        TypeExpr::Named("String".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 80),
            value: Some(Expr::Call {
                callee: Box::new(Expr::FieldAccess {
                    object: Box::new(Expr::IntLit(42, Span::new(55, 57))),
                    field: "to_string".to_string(),
                    span: Span::new(55, 67),
                }),
                args: vec![],
                span: Span::new(55, 69),
            }),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "Int.to_string() should type-check: {result:?}"
    );
}

#[test]
fn method_not_found_suggests_similar() {
    let func = make_function(
        "test_typo",
        vec![],
        TypeExpr::Named("Int".to_string()),
        vec![Stmt::Return {
            span: Span::new(50, 80),
            value: Some(Expr::Call {
                callee: Box::new(Expr::FieldAccess {
                    object: Box::new(Expr::StringLit("hello".to_string(), Span::new(55, 62))),
                    field: "lenght".to_string(),
                    span: Span::new(55, 69),
                }),
                args: vec![],
                span: Span::new(55, 71),
            }),
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    if let Err(TypeError::MethodNotFound { similar, .. }) = result {
        assert_eq!(
            similar,
            Some("length".to_string()),
            "should suggest 'length' for typo 'lenght'"
        );
    } else {
        panic!("expected MethodNotFound error");
    }
}

#[test]
fn find_similar_in_finds_closest() {
    let candidates = vec!["length", "contains", "starts_with", "ends_with"];
    assert_eq!(
        crate::types::find_similar_in("lenght", candidates.into_iter()),
        Some("length".to_string())
    );
    assert_eq!(
        crate::types::find_similar_in("contans", vec!["contains", "length"].into_iter()),
        Some("contains".to_string())
    );
    assert_eq!(
        crate::types::find_similar_in("xyz", vec!["contains", "length"].into_iter()),
        None
    );
}

#[test]
fn list_functions_registered() {
    let checker = TypeChecker::new();
    assert!(
        checker.env.lookup("list_new").is_some(),
        "list_new should be registered"
    );
    assert!(
        checker.env.lookup("list_push").is_some(),
        "list_push should be registered"
    );
    assert!(
        checker.env.lookup("list_get").is_some(),
        "list_get should be registered"
    );
    assert!(
        checker.env.lookup("list_length").is_some(),
        "list_length should be registered"
    );
    assert!(
        checker.env.lookup("list_contains").is_some(),
        "list_contains should be registered"
    );
}

#[test]
fn map_functions_registered() {
    let checker = TypeChecker::new();
    assert!(
        checker.env.lookup("map_new").is_some(),
        "map_new should be registered"
    );
    assert!(
        checker.env.lookup("map_insert").is_some(),
        "map_insert should be registered"
    );
    assert!(
        checker.env.lookup("map_get").is_some(),
        "map_get should be registered"
    );
}

#[test]
fn string_split_method_registered() {
    let checker = TypeChecker::new();
    let lookup = checker.method_lookup();
    let split = lookup.get(&("String".to_string(), "split".to_string()));
    assert!(split.is_some(), "String.split should be registered");
    let (mangled, params, ret) = split.unwrap();
    assert_eq!(mangled, "String_split");
    assert_eq!(params.len(), 2);
    assert!(matches!(ret, Type::Generic(name, _) if name == "List"));
}

#[test]
fn qualified_call_with_imported_module() {
    let source = r#"module helper {
    meta {
        purpose: "helper module"
        version: "1.0.0"
    }

    fn double(x: Int) -> Int {
        return x + x
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let _ = checker.check_module(&module);
    checker.register_imported_module("helper".to_string());
    let double_ty = checker.env.lookup("double");
    assert!(
        double_ty.is_some(),
        "double should be in env after check_module"
    );
}

#[test]
fn generic_types_are_copy() {
    assert!(Type::Generic("List".to_string(), vec![Type::Int]).is_copy());
    assert!(Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]).is_copy());
}

#[test]
fn definition_spans_populated_after_check() {
    let source = r#"module test {
    meta {
        purpose: "test"
        version: "1.0.0"
    }

    fn my_func(x: Int) -> Int {
        return x
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let _ = checker.check_module(&module);
    let spans = checker.definition_spans();
    assert!(
        spans.contains_key("my_func"),
        "should have definition span for my_func"
    );
}

// ===== Phase 37: Trait Bound Tests =====

#[test]
fn trait_bound_satisfied_generic_fn() {
    let source = r#"module test {
    meta {
        purpose: "test trait bounds"
        version: "1.0.0"
    }

    trait Printable {
        fn display(self) -> String
    }

    struct MyType {
        value: Int,
    }

    impl Printable for MyType {
        fn display(self) -> String {
            return "hello"
        }
    }

    fn show<T: Printable>(item: T) -> Int {
        return 42
    }

    fn main() -> Int {
        let x: MyType = MyType { value: 1 }
        return show(x)
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "should pass: MyType implements Printable");
}

#[test]
fn trait_bound_not_satisfied_generic_fn() {
    let source = r#"module test {
    meta {
        purpose: "test trait bounds"
        version: "1.0.0"
    }

    trait Printable {
        fn display(self) -> String
    }

    struct MyType {
        value: Int,
    }

    fn show<T: Printable>(item: T) -> Int {
        return 42
    }

    fn main() -> Int {
        let x: MyType = MyType { value: 1 }
        return show(x)
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "should fail: MyType does not implement Printable"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0232");
}

#[test]
fn trait_bound_multiple_bounds_all_satisfied() {
    let source = r#"module test {
    meta {
        purpose: "test multiple trait bounds"
        version: "1.0.0"
    }

    trait Printable {
        fn display(self) -> String
    }

    trait Comparable {
        fn compare(self) -> Int
    }

    struct MyType {
        value: Int,
    }

    impl Printable for MyType {
        fn display(self) -> String {
            return "hello"
        }
    }

    impl Comparable for MyType {
        fn compare(self) -> Int {
            return 0
        }
    }

    fn process<T: Printable + Comparable>(item: T) -> Int {
        return 42
    }

    fn main() -> Int {
        let x: MyType = MyType { value: 1 }
        return process(x)
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "should pass: MyType implements both Printable and Comparable"
    );
}

#[test]
fn trait_bound_multiple_bounds_one_missing() {
    let source = r#"module test {
    meta {
        purpose: "test multiple trait bounds"
        version: "1.0.0"
    }

    trait Printable {
        fn display(self) -> String
    }

    trait Comparable {
        fn compare(self) -> Int
    }

    struct MyType {
        value: Int,
    }

    impl Printable for MyType {
        fn display(self) -> String {
            return "hello"
        }
    }

    fn process<T: Printable + Comparable>(item: T) -> Int {
        return 42
    }

    fn main() -> Int {
        let x: MyType = MyType { value: 1 }
        return process(x)
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should fail: MyType missing Comparable");
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0232");
}

#[test]
fn trait_bound_on_enum_generic_not_satisfied() {
    let source = r#"module test {
    meta {
        purpose: "test enum generic bounds"
        version: "1.0.0"
    }

    trait Sortable {
        fn sort_key(self) -> Int
    }

    enum Wrapper<T: Sortable> {
        Val(T),
        Empty,
    }

    struct Item {
        val: Int,
    }

    fn main() -> Int {
        let w: Wrapper<Item> = Wrapper::Val(Item { val: 1 })
        return 0
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "should fail: Item does not implement Sortable"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0232");
}

#[test]
fn trait_bound_on_enum_generic_satisfied() {
    let source = r#"module test {
    meta {
        purpose: "test enum generic bounds"
        version: "1.0.0"
    }

    trait Sortable {
        fn sort_key(self) -> Int
    }

    enum Wrapper<T: Sortable> {
        Val(T),
        Empty,
    }

    struct Item {
        val: Int,
    }

    impl Sortable for Item {
        fn sort_key(self) -> Int {
            return 0
        }
    }

    fn main() -> Int {
        let w: Wrapper<Item> = Wrapper::Val(Item { val: 1 })
        return 0
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "should pass: Item implements Sortable");
}

#[test]
fn trait_bound_on_enum_generic() {
    let source = r#"module test {
    meta {
        purpose: "test enum generic bounds"
        version: "1.0.0"
    }

    trait Printable {
        fn display(self) -> String
    }

    enum Container<T: Printable> {
        Some(T),
        None,
    }

    struct Msg {
        text: String,
    }

    impl Printable for Msg {
        fn display(self) -> String {
            return "msg"
        }
    }

    fn main() -> Int {
        let c: Container<Msg> = Container::Some(Msg { text: "hi" })
        return 0
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "should pass: Msg implements Printable");
}

#[test]
fn trait_bound_no_bounds_still_works() {
    let source = r#"module test {
    meta {
        purpose: "test no bounds"
        version: "1.0.0"
    }

    fn identity<T>(x: T) -> T {
        return x
    }

    fn main() -> Int {
        return identity(42)
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "should pass: no bounds to check");
}

#[test]
fn trait_bound_error_message_quality() {
    let source = r#"module test {
    meta {
        purpose: "test error message"
        version: "1.0.0"
    }

    trait Hashable {
        fn hash(self) -> Int
    }

    fn lookup<T: Hashable>(key: T) -> Int {
        return 0
    }

    fn main() -> Int {
        return lookup(42)
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0232");
    let msg = err.to_string();
    assert!(
        msg.contains("Hashable"),
        "error should mention the trait name: {msg}"
    );
    assert!(
        msg.contains("T"),
        "error should mention the param name: {msg}"
    );
}

#[test]
fn trait_impl_set_populated() {
    let source = r#"module test {
    meta {
        purpose: "test"
        version: "1.0.0"
    }

    trait MyTrait {
        fn method(self) -> Int
    }

    struct MyType {
        x: Int,
    }

    impl MyTrait for MyType {
        fn method(self) -> Int {
            return 0
        }
    }

    fn main() -> Int {
        return 0
    }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let _ = checker.check_module(&module);
    assert!(
        checker.type_implements_trait("MyType", "MyTrait"),
        "MyType should implement MyTrait after check_module"
    );
    assert!(
        !checker.type_implements_trait("MyType", "NonExistent"),
        "MyType should not implement NonExistent"
    );
}

#[test]
fn trait_bound_generic_param_struct() {
    // Verify that GenericParam correctly carries bounds from parser
    let source = r#"module test {
        struct Container<T: Ord + Display, U: Clone> {
            first: T,
            second: U,
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let decl = &module.type_decls[0];
    assert_eq!(decl.generic_params.len(), 2);
    assert_eq!(decl.generic_params[0].name, "T");
    assert_eq!(decl.generic_params[0].bounds, vec!["Ord", "Display"]);
    assert_eq!(decl.generic_params[1].name, "U");
    assert_eq!(decl.generic_params[1].bounds, vec!["Clone"]);
}

#[test]
fn for_in_list_int_passes() {
    let func = make_function(
        "main",
        vec![Param {
            name: "items".to_string(),
            ty: TypeExpr::Generic("List".to_string(), vec![TypeExpr::Named("Int".to_string())]),
            span: Span::new(0, 20),
            ownership: kodo_ast::Ownership::Owned,
        }],
        TypeExpr::Unit,
        vec![Stmt::ForIn {
            span: Span::new(0, 50),
            name: "x".to_string(),
            iterable: Expr::Ident("items".to_string(), Span::new(10, 15)),
            body: Block {
                span: Span::new(16, 50),
                stmts: vec![],
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn for_in_non_list_fails() {
    let func = make_function(
        "main",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span: Span::new(0, 10),
            ownership: kodo_ast::Ownership::Owned,
        }],
        TypeExpr::Unit,
        vec![Stmt::ForIn {
            span: Span::new(0, 40),
            name: "item".to_string(),
            iterable: Expr::Ident("x".to_string(), Span::new(15, 16)),
            body: Block {
                span: Span::new(17, 40),
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
fn for_in_list_string_binds_string() {
    // for x in items { print(x) }  where items: List<String>
    // x should be typed as String, and using it where Int is needed should fail.
    let func = make_function(
        "main",
        vec![Param {
            name: "items".to_string(),
            ty: TypeExpr::Generic(
                "List".to_string(),
                vec![TypeExpr::Named("String".to_string())],
            ),
            span: Span::new(0, 20),
            ownership: kodo_ast::Ownership::Owned,
        }],
        TypeExpr::Unit,
        vec![Stmt::ForIn {
            span: Span::new(0, 60),
            name: "x".to_string(),
            iterable: Expr::Ident("items".to_string(), Span::new(10, 15)),
            body: Block {
                span: Span::new(16, 60),
                stmts: vec![Stmt::Let {
                    span: Span::new(20, 40),
                    mutable: false,
                    name: "y".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::Ident("x".to_string(), Span::new(30, 31)),
                }],
            },
        }],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "should fail: x is String, not Int");
}

#[test]
fn for_in_loop_variable_scoped() {
    // x is only in scope within the for-in body, not after.
    let func = make_function(
        "main",
        vec![Param {
            name: "items".to_string(),
            ty: TypeExpr::Generic("List".to_string(), vec![TypeExpr::Named("Int".to_string())]),
            span: Span::new(0, 20),
            ownership: kodo_ast::Ownership::Owned,
        }],
        TypeExpr::Unit,
        vec![
            Stmt::ForIn {
                span: Span::new(0, 40),
                name: "x".to_string(),
                iterable: Expr::Ident("items".to_string(), Span::new(10, 15)),
                body: Block {
                    span: Span::new(16, 40),
                    stmts: vec![],
                },
            },
            // Using x after the loop should fail.
            Stmt::Let {
                span: Span::new(41, 55),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(50, 51)),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err(), "x should be out of scope after for-in");
}

// ========== Tuple type tests ==========

#[test]
fn tuple_type_display() {
    assert_eq!(
        Type::Tuple(vec![Type::Int, Type::String]).to_string(),
        "(Int, String)"
    );
}

#[test]
fn tuple_type_equality() {
    let a = Type::Tuple(vec![Type::Int, Type::Bool]);
    let b = Type::Tuple(vec![Type::Int, Type::Bool]);
    assert_eq!(a, b);
}

#[test]
fn tuple_type_inequality() {
    let a = Type::Tuple(vec![Type::Int, Type::Bool]);
    let b = Type::Tuple(vec![Type::Int, Type::String]);
    assert_ne!(a, b);
}

#[test]
fn tuple_type_check_eq_same() {
    let ty = Type::Tuple(vec![Type::Int, Type::Bool]);
    let result = TypeEnv::check_eq(&ty, &ty, Span::new(0, 1));
    assert!(result.is_ok());
}

#[test]
fn tuple_type_check_eq_different() {
    let a = Type::Tuple(vec![Type::Int, Type::Bool]);
    let b = Type::Tuple(vec![Type::Int, Type::String]);
    let result = TypeEnv::check_eq(&a, &b, Span::new(0, 1));
    assert!(result.is_err());
}

#[test]
fn tuple_type_not_numeric() {
    let ty = Type::Tuple(vec![Type::Int]);
    assert!(!ty.is_numeric());
}

#[test]
fn tuple_type_not_copy() {
    let ty = Type::Tuple(vec![Type::Int, Type::String]);
    assert!(!ty.is_copy());
}

#[test]
fn tuple_index_out_of_bounds_error_has_code() {
    let err = TypeError::TupleIndexOutOfBounds {
        index: 3,
        length: 2,
        span: Span::new(0, 5),
    };
    assert_eq!(err.code(), "E0253");
    assert!(err.span().is_some());
}

// --- Phase 43: Associated types and default methods ---

#[test]
fn missing_associated_type_error() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 200),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test associated types".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "MyList".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "len".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(60, 70),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(80, 120),
            name: "Container".to_string(),
            associated_types: vec![kodo_ast::AssociatedType {
                name: "Item".to_string(),
                bounds: vec![],
                span: Span::new(90, 100),
            }],
            methods: vec![kodo_ast::TraitMethod {
                name: "get".to_string(),
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                    span: Span::new(105, 109),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                has_self: true,
                body: None,
                span: Span::new(100, 115),
            }],
        }],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(3),
            span: Span::new(120, 180),
            trait_name: Some("Container".to_string()),
            type_name: "MyList".to_string(),
            type_bindings: vec![], // Missing the required `type Item = ...`
            methods: vec![Function {
                id: NodeId(4),
                span: Span::new(130, 175),
                name: "get".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("MyList".to_string()),
                    span: Span::new(135, 139),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: kodo_ast::Block {
                    span: Span::new(145, 175),
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: Span::new(150, 170),
                        value: Some(Expr::IntLit(0, Span::new(157, 158))),
                    }],
                },
            }],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![make_function(
            "main",
            vec![],
            kodo_ast::TypeExpr::Named("Int".to_string()),
            vec![kodo_ast::Stmt::Return {
                span: Span::new(190, 198),
                value: Some(Expr::IntLit(0, Span::new(197, 198))),
            }],
        )],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0233");
}

#[test]
fn unexpected_associated_type_error() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 200),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test unexpected associated type".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "MyList".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "len".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(60, 70),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(80, 120),
            name: "Simple".to_string(),
            associated_types: vec![], // No associated types in trait
            methods: vec![kodo_ast::TraitMethod {
                name: "get".to_string(),
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                    span: Span::new(105, 109),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                has_self: true,
                body: None,
                span: Span::new(100, 115),
            }],
        }],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(3),
            span: Span::new(120, 180),
            trait_name: Some("Simple".to_string()),
            type_name: "MyList".to_string(),
            type_bindings: vec![(
                "Bogus".to_string(),
                kodo_ast::TypeExpr::Named("Int".to_string()),
            )],
            methods: vec![Function {
                id: NodeId(4),
                span: Span::new(130, 175),
                name: "get".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("MyList".to_string()),
                    span: Span::new(135, 139),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: kodo_ast::Block {
                    span: Span::new(145, 175),
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: Span::new(150, 170),
                        value: Some(Expr::IntLit(0, Span::new(157, 158))),
                    }],
                },
            }],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![make_function(
            "main",
            vec![],
            kodo_ast::TypeExpr::Named("Int".to_string()),
            vec![kodo_ast::Stmt::Return {
                span: Span::new(190, 198),
                value: Some(Expr::IntLit(0, Span::new(197, 198))),
            }],
        )],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0234");
}

#[test]
fn default_method_not_required_in_impl() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 200),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test default methods".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Point".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "x".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(60, 70),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(80, 130),
            name: "Greetable".to_string(),
            associated_types: vec![],
            methods: vec![
                kodo_ast::TraitMethod {
                    name: "required_method".to_string(),
                    params: vec![kodo_ast::Param {
                        name: "self".to_string(),
                        ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                        span: Span::new(90, 94),
                        ownership: kodo_ast::Ownership::Owned,
                    }],
                    return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                    has_self: true,
                    body: None,
                    span: Span::new(85, 100),
                },
                kodo_ast::TraitMethod {
                    name: "default_method".to_string(),
                    params: vec![kodo_ast::Param {
                        name: "self".to_string(),
                        ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                        span: Span::new(105, 109),
                        ownership: kodo_ast::Ownership::Owned,
                    }],
                    return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                    has_self: true,
                    body: Some(kodo_ast::Block {
                        span: Span::new(115, 125),
                        stmts: vec![kodo_ast::Stmt::Return {
                            span: Span::new(117, 123),
                            value: Some(Expr::IntLit(42, Span::new(120, 122))),
                        }],
                    }),
                    span: Span::new(103, 127),
                },
            ],
        }],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(3),
            span: Span::new(130, 180),
            trait_name: Some("Greetable".to_string()),
            type_name: "Point".to_string(),
            type_bindings: vec![],
            // Only implement the required method, skip default_method
            methods: vec![Function {
                id: NodeId(4),
                span: Span::new(140, 175),
                name: "required_method".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Point".to_string()),
                    span: Span::new(145, 149),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: kodo_ast::Block {
                    span: Span::new(155, 175),
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: Span::new(160, 170),
                        value: Some(Expr::IntLit(1, Span::new(167, 168))),
                    }],
                },
            }],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![make_function(
            "main",
            vec![],
            kodo_ast::TypeExpr::Named("Int".to_string()),
            vec![kodo_ast::Stmt::Return {
                span: Span::new(190, 198),
                value: Some(Expr::IntLit(0, Span::new(197, 198))),
            }],
        )],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    // Should succeed — default_method has a default, so not required in impl
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn associated_type_provided_passes() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 200),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test passing associated types".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "MyList".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "len".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(60, 70),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(80, 120),
            name: "Container".to_string(),
            associated_types: vec![kodo_ast::AssociatedType {
                name: "Item".to_string(),
                bounds: vec![],
                span: Span::new(90, 100),
            }],
            methods: vec![kodo_ast::TraitMethod {
                name: "get".to_string(),
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                    span: Span::new(105, 109),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                has_self: true,
                body: None,
                span: Span::new(100, 115),
            }],
        }],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(3),
            span: Span::new(120, 180),
            trait_name: Some("Container".to_string()),
            type_name: "MyList".to_string(),
            type_bindings: vec![(
                "Item".to_string(),
                kodo_ast::TypeExpr::Named("Int".to_string()),
            )],
            methods: vec![Function {
                id: NodeId(4),
                span: Span::new(130, 175),
                name: "get".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("MyList".to_string()),
                    span: Span::new(135, 139),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: kodo_ast::Block {
                    span: Span::new(145, 175),
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: Span::new(150, 170),
                        value: Some(Expr::IntLit(0, Span::new(157, 158))),
                    }],
                },
            }],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![make_function(
            "main",
            vec![],
            kodo_ast::TypeExpr::Named("Int".to_string()),
            vec![kodo_ast::Stmt::Return {
                span: Span::new(190, 198),
                value: Some(Expr::IntLit(0, Span::new(197, 198))),
            }],
        )],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn missing_associated_type_error_code() {
    let err = TypeError::MissingAssociatedType {
        assoc_type: "Item".to_string(),
        trait_name: "Container".to_string(),
        span: Span::new(0, 5),
    };
    assert_eq!(err.code(), "E0233");
    assert!(err.span().is_some());
}

#[test]
fn unexpected_associated_type_error_code() {
    let err = TypeError::UnexpectedAssociatedType {
        assoc_type: "Bogus".to_string(),
        trait_name: "Simple".to_string(),
        span: Span::new(0, 5),
    };
    assert_eq!(err.code(), "E0234");
    assert!(err.span().is_some());
}

#[test]
fn default_method_collecting_not_required() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 200),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test default methods collecting".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Foo".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "x".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(60, 70),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(80, 130),
            name: "WithDefault".to_string(),
            associated_types: vec![],
            methods: vec![kodo_ast::TraitMethod {
                name: "default_fn".to_string(),
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Self".to_string()),
                    span: Span::new(90, 94),
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                has_self: true,
                body: Some(kodo_ast::Block {
                    span: Span::new(100, 120),
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: Span::new(105, 115),
                        value: Some(Expr::IntLit(99, Span::new(112, 114))),
                    }],
                }),
                span: Span::new(85, 125),
            }],
        }],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(3),
            span: Span::new(130, 150),
            trait_name: Some("WithDefault".to_string()),
            type_name: "Foo".to_string(),
            type_bindings: vec![],
            methods: vec![], // No methods needed — all are default
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![make_function(
            "main",
            vec![],
            kodo_ast::TypeExpr::Named("Int".to_string()),
            vec![kodo_ast::Stmt::Return {
                span: Span::new(190, 198),
                value: Some(Expr::IntLit(0, Span::new(197, 198))),
            }],
        )],
    };
    let mut checker = TypeChecker::new();
    let errors = checker.check_module_collecting(&module);
    assert!(errors.is_empty(), "expected no errors, got {:?}", errors);
}

#[test]
fn missing_associated_type_collecting() {
    let module = Module {
        id: NodeId(0),
        span: Span::new(0, 200),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test missing assoc type collecting".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Foo".to_string(),
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "x".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(60, 70),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![kodo_ast::TraitDecl {
            id: NodeId(2),
            span: Span::new(80, 120),
            name: "HasAssoc".to_string(),
            associated_types: vec![kodo_ast::AssociatedType {
                name: "Output".to_string(),
                bounds: vec![],
                span: Span::new(90, 100),
            }],
            methods: vec![],
        }],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(3),
            span: Span::new(120, 150),
            trait_name: Some("HasAssoc".to_string()),
            type_name: "Foo".to_string(),
            type_bindings: vec![], // Missing Output
            methods: vec![],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        functions: vec![make_function(
            "main",
            vec![],
            kodo_ast::TypeExpr::Named("Int".to_string()),
            vec![kodo_ast::Stmt::Return {
                span: Span::new(190, 198),
                value: Some(Expr::IntLit(0, Span::new(197, 198))),
            }],
        )],
    };
    let mut checker = TypeChecker::new();
    let errors = checker.check_module_collecting(&module);
    assert!(
        errors.iter().any(|e| e.code() == "E0233"),
        "expected E0233, got {:?}",
        errors
    );
}
