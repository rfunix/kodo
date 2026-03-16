//! Unit tests for the AST-to-MIR lowering pass.

use super::*;
use kodo_ast::{
    ActorDecl, BinOp, Block, Expr, FieldDef, FieldInit, Function, Meta, MetaEntry, Module, NodeId,
    Ownership, Param, Span, Stmt, TypeDecl, TypeExpr, Visibility,
};

/// Helper to create a dummy span.
fn span() -> Span {
    Span::new(0, 0)
}

/// Helper to build a simple function with a body block.
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

#[test]
fn lower_empty_function() {
    let func = make_fn(
        "empty",
        vec![],
        Block {
            span: span(),
            stmts: vec![],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    assert_eq!(mir.name, "empty");
    assert_eq!(mir.return_type, Type::Unit);
    assert_eq!(mir.blocks.len(), 1);
    // The only block should have a Return(Unit) terminator.
    assert!(matches!(
        mir.blocks[0].terminator,
        Terminator::Return(Value::Unit)
    ));
}

#[test]
fn lower_let_and_return() {
    // fn example() -> Int { let x: Int = 42; return x }
    let func = make_fn(
        "example",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "x".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(42, span()),
                },
                Stmt::Return {
                    span: span(),
                    value: Some(Expr::Ident("x".to_string(), span())),
                },
            ],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    assert_eq!(mir.name, "example");
    assert_eq!(mir.return_type, Type::Int);
    // Should have local _0 for x.
    assert!(!mir.locals.is_empty());
    // The entry block should have an Assign + a Return terminator.
    let entry = &mir.blocks[0];
    assert_eq!(entry.instructions.len(), 1);
    assert!(matches!(entry.terminator, Terminator::Return(_)));
}

#[test]
fn lower_binary_expression() {
    // fn add() -> Int { return 1 + 2 }
    let func = make_fn(
        "add",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::BinaryOp {
                    left: Box::new(Expr::IntLit(1, span())),
                    op: BinOp::Add,
                    right: Box::new(Expr::IntLit(2, span())),
                    span: span(),
                }),
            }],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // The return terminator should contain a BinOp value.
    match &mir.blocks[0].terminator {
        Terminator::Return(Value::BinOp(BinOp::Add, lhs, rhs)) => {
            assert!(matches!(lhs.as_ref(), Value::IntConst(1)));
            assert!(matches!(rhs.as_ref(), Value::IntConst(2)));
        }
        other => panic!("expected Return(BinOp(Add, ...)), got {other:?}"),
    }
}

#[test]
fn lower_if_else_creates_cfg() {
    // fn branch(x: Bool) -> Int {
    //     if x { return 1 } else { return 2 }
    // }
    let func = make_fn(
        "branch",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Bool".to_string()),
            span: span(),
            ownership: Ownership::Owned,
        }],
        Block {
            span: span(),
            stmts: vec![Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Ident("x".to_string(), span())),
                then_branch: Block {
                    span: span(),
                    stmts: vec![Stmt::Return {
                        span: span(),
                        value: Some(Expr::IntLit(1, span())),
                    }],
                },
                else_branch: Some(Block {
                    span: span(),
                    stmts: vec![Stmt::Return {
                        span: span(),
                        value: Some(Expr::IntLit(2, span())),
                    }],
                }),
                span: span(),
            })],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();

    // There should be multiple blocks: entry, then, else, merge,
    // plus continuation blocks from return statements.
    assert!(
        mir.blocks.len() >= 4,
        "expected at least 4 blocks, got {}",
        mir.blocks.len()
    );

    // The entry block should have a Branch terminator.
    assert!(matches!(
        mir.blocks[0].terminator,
        Terminator::Branch { .. }
    ));
}

#[test]
fn lower_function_call() {
    // fn caller() -> Int { return add(1, 2) }
    let func = make_fn(
        "caller",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::Call {
                    callee: Box::new(Expr::Ident("add".to_string(), span())),
                    args: vec![Expr::IntLit(1, span()), Expr::IntLit(2, span())],
                    span: span(),
                }),
            }],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // The entry block should have a Call instruction.
    assert_eq!(mir.blocks[0].instructions.len(), 1);
    assert!(matches!(
        mir.blocks[0].instructions[0],
        Instruction::Call { .. }
    ));
}

#[test]
fn lower_module_multiple_functions() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "test_module".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![
            make_fn(
                "first",
                vec![],
                Block {
                    span: span(),
                    stmts: vec![],
                },
                TypeExpr::Unit,
            ),
            make_fn(
                "second",
                vec![],
                Block {
                    span: span(),
                    stmts: vec![Stmt::Return {
                        span: span(),
                        value: Some(Expr::IntLit(99, span())),
                    }],
                },
                TypeExpr::Named("Int".to_string()),
            ),
        ],
    };
    let fns = lower_module(&module).unwrap();
    assert_eq!(fns.len(), 2);
    assert_eq!(fns[0].name, "first");
    assert_eq!(fns[1].name, "second");
    for f in &fns {
        f.validate().unwrap();
    }
}

#[test]
fn lower_unary_not_and_neg() {
    // fn negate() { let a = !true; let b = -42 }
    let func = make_fn(
        "negate",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "a".to_string(),
                    ty: Some(TypeExpr::Named("Bool".to_string())),
                    value: Expr::UnaryOp {
                        op: kodo_ast::UnaryOp::Not,
                        operand: Box::new(Expr::BoolLit(true, span())),
                        span: span(),
                    },
                },
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "b".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::UnaryOp {
                        op: kodo_ast::UnaryOp::Neg,
                        operand: Box::new(Expr::IntLit(42, span())),
                        span: span(),
                    },
                },
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Two assign instructions: Not(BoolConst(true)) and Neg(IntConst(42)).
    assert_eq!(mir.blocks[0].instructions.len(), 2);
    match &mir.blocks[0].instructions[0] {
        Instruction::Assign(_, Value::Not(inner)) => {
            assert!(matches!(inner.as_ref(), Value::BoolConst(true)));
        }
        other => panic!("expected Assign(_, Not(BoolConst(true))), got {other:?}"),
    }
    match &mir.blocks[0].instructions[1] {
        Instruction::Assign(_, Value::Neg(inner)) => {
            assert!(matches!(inner.as_ref(), Value::IntConst(42)));
        }
        other => panic!("expected Assign(_, Neg(IntConst(42))), got {other:?}"),
    }
}

#[test]
fn lower_undefined_variable_errors() {
    let func = make_fn(
        "bad",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::Ident("nonexistent".to_string(), span())),
            }],
        },
        TypeExpr::Unit,
    );
    let result = lower_function(&func);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, MirError::UndefinedVariable(ref name) if name == "nonexistent"),
        "expected UndefinedVariable, got {err:?}"
    );
}

#[test]
fn lower_params_are_accessible() {
    // fn id(x: Int) -> Int { return x }
    let func = make_fn(
        "id",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span: span(),
            ownership: Ownership::Owned,
        }],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::Ident("x".to_string(), span())),
            }],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Local _0 should be the parameter.
    assert_eq!(mir.locals[0].ty, Type::Int);
    // Return should reference Local(_0).
    assert!(matches!(
        mir.blocks[0].terminator,
        Terminator::Return(Value::Local(LocalId(0)))
    ));
}

#[test]
fn lower_while_creates_loop_cfg() {
    // fn counter() { let mut i: Int = 3; while i > 0 { i = i - 1 } }
    let func = make_fn(
        "counter",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: true,
                    name: "i".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(3, span()),
                },
                Stmt::While {
                    span: span(),
                    condition: Expr::BinaryOp {
                        left: Box::new(Expr::Ident("i".to_string(), span())),
                        op: BinOp::Gt,
                        right: Box::new(Expr::IntLit(0, span())),
                        span: span(),
                    },
                    body: Block {
                        span: span(),
                        stmts: vec![Stmt::Assign {
                            span: span(),
                            name: "i".to_string(),
                            value: Expr::BinaryOp {
                                left: Box::new(Expr::Ident("i".to_string(), span())),
                                op: BinOp::Sub,
                                right: Box::new(Expr::IntLit(1, span())),
                                span: span(),
                            },
                        }],
                    },
                },
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have: entry, loop_header, loop_body, loop_exit + final
    assert!(
        mir.blocks.len() >= 4,
        "expected at least 4 blocks for while loop, got {}",
        mir.blocks.len()
    );
    // First block should have a Goto to the header
    assert!(
        matches!(mir.blocks[0].terminator, Terminator::Goto(_)),
        "entry should goto loop header"
    );
}

#[test]
fn lower_while_false_exits_immediately() {
    // fn skip() { while false { } }
    let func = make_fn(
        "skip",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::While {
                span: span(),
                condition: Expr::BoolLit(false, span()),
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // The loop header should have a Branch with false condition
    // leading to exit.
    let header = &mir.blocks[1]; // block after entry's Goto
    assert!(
        matches!(header.terminator, Terminator::Branch { .. }),
        "loop header should have Branch terminator"
    );
}

#[test]
fn lower_assignment() {
    // fn reassign() { let mut x: Int = 1; x = 42 }
    let func = make_fn(
        "reassign",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: true,
                    name: "x".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(1, span()),
                },
                Stmt::Assign {
                    span: span(),
                    name: "x".to_string(),
                    value: Expr::IntLit(42, span()),
                },
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have 2 Assign instructions in the entry block
    assert_eq!(mir.blocks[0].instructions.len(), 2);
}

#[test]
fn lower_function_with_ensures_injects_check() {
    // fn positive() -> Int ensures { result > 0 } { return 42 }
    let func = Function {
        id: NodeId(0),
        span: span(),
        name: "positive".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![Expr::BinaryOp {
            left: Box::new(Expr::Ident("result".to_string(), span())),
            op: BinOp::Gt,
            right: Box::new(Expr::IntLit(0, span())),
            span: span(),
        }],
        body: Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::IntLit(42, span())),
            }],
        },
    };
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have more blocks due to ensures checks
    assert!(
        mir.blocks.len() >= 3,
        "expected at least 3 blocks with ensures, got {}",
        mir.blocks.len()
    );
}

#[test]
fn lower_function_with_ensures_result_reference() {
    // ensures { result == 0 } with implicit return
    let func = Function {
        id: NodeId(0),
        span: span(),
        name: "zero".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![Expr::BinaryOp {
            left: Box::new(Expr::Ident("result".to_string(), span())),
            op: BinOp::Eq,
            right: Box::new(Expr::IntLit(0, span())),
            span: span(),
        }],
        body: Block {
            span: span(),
            stmts: vec![],
        },
    };
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have blocks for ensures check even on implicit return
    assert!(mir.blocks.len() >= 3);
}

#[test]
fn validator_generated_for_function_with_requires() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "test_mod".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(0),
            span: span(),
            name: "checked".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            }],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![Expr::BinaryOp {
                left: Box::new(Expr::Ident("x".to_string(), span())),
                op: BinOp::Gt,
                right: Box::new(Expr::IntLit(0, span())),
                span: span(),
            }],
            ensures: vec![],
            body: Block {
                span: span(),
                stmts: vec![],
            },
        }],
    };
    let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
    assert_eq!(
        fns.len(),
        2,
        "expected original + validator, got {}",
        fns.len()
    );
    assert_eq!(fns[0].name, "checked");
    assert_eq!(fns[1].name, "validate_checked");
}

#[test]
fn validator_not_generated_without_requires() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "test_mod".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![make_fn(
            "plain",
            vec![],
            Block {
                span: span(),
                stmts: vec![],
            },
            TypeExpr::Unit,
        )],
    };
    let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
    assert_eq!(fns.len(), 1, "expected only original, got {}", fns.len());
}

#[test]
fn validator_has_same_params() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "test_mod".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(0),
            span: span(),
            name: "checked".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![
                Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                    ownership: Ownership::Owned,
                },
                Param {
                    name: "y".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                    ownership: Ownership::Owned,
                },
            ],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![Expr::BinaryOp {
                left: Box::new(Expr::Ident("x".to_string(), span())),
                op: BinOp::Gt,
                right: Box::new(Expr::IntLit(0, span())),
                span: span(),
            }],
            ensures: vec![],
            body: Block {
                span: span(),
                stmts: vec![],
            },
        }],
    };
    let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
    let original = &fns[0];
    let validator = &fns[1];
    assert_eq!(
        original.param_count, validator.param_count,
        "validator should have same param count as original"
    );
}

#[test]
fn validator_returns_bool() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "test_mod".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(0),
            span: span(),
            name: "checked".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            }],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![Expr::BinaryOp {
                left: Box::new(Expr::Ident("x".to_string(), span())),
                op: BinOp::Gt,
                right: Box::new(Expr::IntLit(0, span())),
                span: span(),
            }],
            ensures: vec![],
            body: Block {
                span: span(),
                stmts: vec![],
            },
        }],
    };
    let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
    let validator = &fns[1];
    assert_eq!(
        validator.return_type,
        Type::Bool,
        "validator should return Bool"
    );
}

#[test]
fn lower_function_multiple_ensures() {
    let func = Function {
        id: NodeId(0),
        span: span(),
        name: "multi".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![
            Expr::BinaryOp {
                left: Box::new(Expr::Ident("result".to_string(), span())),
                op: BinOp::Gt,
                right: Box::new(Expr::IntLit(0, span())),
                span: span(),
            },
            Expr::BinaryOp {
                left: Box::new(Expr::Ident("result".to_string(), span())),
                op: BinOp::Lt,
                right: Box::new(Expr::IntLit(100, span())),
                span: span(),
            },
        ],
        body: Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::IntLit(42, span())),
            }],
        },
    };
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // With 2 ensures, we should have even more blocks
    assert!(
        mir.blocks.len() >= 5,
        "expected at least 5 blocks with 2 ensures, got {}",
        mir.blocks.len()
    );
}

#[test]
fn lower_for_creates_loop_cfg() {
    // fn sum() { let mut s: Int = 0; for i in 0..5 { s = s + i } }
    let func = make_fn(
        "sum",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: true,
                    name: "s".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(0, span()),
                },
                Stmt::For {
                    span: span(),
                    name: "i".to_string(),
                    start: Expr::IntLit(0, span()),
                    end: Expr::IntLit(5, span()),
                    inclusive: false,
                    body: Block {
                        span: span(),
                        stmts: vec![Stmt::Assign {
                            span: span(),
                            name: "s".to_string(),
                            value: Expr::BinaryOp {
                                left: Box::new(Expr::Ident("s".to_string(), span())),
                                op: BinOp::Add,
                                right: Box::new(Expr::Ident("i".to_string(), span())),
                                span: span(),
                            },
                        }],
                    },
                },
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have: entry, loop_header, loop_body, loop_exit + final
    assert!(
        mir.blocks.len() >= 4,
        "expected at least 4 blocks for for loop, got {}",
        mir.blocks.len()
    );
    // First block should have a Goto to the header
    assert!(
        matches!(mir.blocks[0].terminator, Terminator::Goto(_)),
        "entry should goto loop header"
    );
}

#[test]
fn lower_for_inclusive_creates_loop_cfg() {
    // fn sum() { for i in 0..=3 { } }
    let func = make_fn(
        "sum_inc",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::For {
                span: span(),
                name: "i".to_string(),
                start: Expr::IntLit(0, span()),
                end: Expr::IntLit(3, span()),
                inclusive: true,
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    assert!(
        mir.blocks.len() >= 4,
        "expected at least 4 blocks for inclusive for loop, got {}",
        mir.blocks.len()
    );
}

#[test]
fn lower_closure_without_captures() {
    // fn main() { let f = |x: Int| x * 2; f(21) }
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
                        params: vec![kodo_ast::ClosureParam {
                            name: "x".to_string(),
                            ty: Some(TypeExpr::Named("Int".to_string())),
                            span: span(),
                        }],
                        return_type: None,
                        body: Box::new(Expr::BinaryOp {
                            left: Box::new(Expr::Ident("x".to_string(), span())),
                            op: BinOp::Mul,
                            right: Box::new(Expr::IntLit(2, span())),
                            span: span(),
                        }),
                        span: span(),
                    },
                },
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("f".to_string(), span())),
                    args: vec![Expr::IntLit(21, span())],
                    span: span(),
                }),
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have instructions for the closure call.
    assert!(mir.blocks[0].instructions.len() >= 2);
}

#[test]
fn lower_closure_with_captures() {
    // fn main() { let a: Int = 10; let f = |x: Int| x + a; f(5) }
    let func = make_fn(
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
                    value: Expr::IntLit(10, span()),
                },
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "f".to_string(),
                    ty: None,
                    value: Expr::Closure {
                        params: vec![kodo_ast::ClosureParam {
                            name: "x".to_string(),
                            ty: Some(TypeExpr::Named("Int".to_string())),
                            span: span(),
                        }],
                        return_type: None,
                        body: Box::new(Expr::BinaryOp {
                            left: Box::new(Expr::Ident("x".to_string(), span())),
                            op: BinOp::Add,
                            right: Box::new(Expr::Ident("a".to_string(), span())),
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
    // Check that the call includes an extra captured argument.
    let has_call_with_2_args = mir.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(i, Instruction::Call { callee, args, .. }
                if callee.starts_with("__closure_") && args.len() == 2)
        })
    });
    assert!(
        has_call_with_2_args,
        "expected a call to __closure_N with 2 args (capture + param)"
    );
}

#[test]
fn lower_indirect_call_via_function_param() {
    // fn apply(f: (Int) -> Int, x: Int) -> Int { return f(x) }
    let func = make_fn(
        "apply",
        vec![
            kodo_ast::Param {
                name: "f".to_string(),
                ty: TypeExpr::Function(
                    vec![TypeExpr::Named("Int".to_string())],
                    Box::new(TypeExpr::Named("Int".to_string())),
                ),
                span: span(),
                ownership: kodo_ast::Ownership::Owned,
            },
            kodo_ast::Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: kodo_ast::Ownership::Owned,
            },
        ],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::Call {
                    callee: Box::new(Expr::Ident("f".to_string(), span())),
                    args: vec![Expr::Ident("x".to_string(), span())],
                    span: span(),
                }),
            }],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have an IndirectCall instruction for f(x).
    let has_indirect_call = mir.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::IndirectCall { .. }))
    });
    assert!(
        has_indirect_call,
        "expected an IndirectCall for calling function parameter"
    );
}

#[test]
fn lower_closure_assigned_with_function_type() {
    // fn main() { let f: (Int) -> Int = |x: Int| -> Int { x + 1 }; f(41) }
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
                    ty: Some(TypeExpr::Function(
                        vec![TypeExpr::Named("Int".to_string())],
                        Box::new(TypeExpr::Named("Int".to_string())),
                    )),
                    value: Expr::Closure {
                        params: vec![kodo_ast::ClosureParam {
                            name: "x".to_string(),
                            ty: Some(TypeExpr::Named("Int".to_string())),
                            span: span(),
                        }],
                        return_type: Some(TypeExpr::Named("Int".to_string())),
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
                    args: vec![Expr::IntLit(41, span())],
                    span: span(),
                }),
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    let has_any_call = mir.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(
                i,
                Instruction::Call { .. } | Instruction::IndirectCall { .. }
            )
        })
    });
    assert!(has_any_call, "expected a call instruction for f(41)");
}

#[test]
fn test_builtin_return_types_registered() {
    let mut types = HashMap::new();
    registry::register_builtin_return_types(&mut types);

    // String-returning builtins
    assert_eq!(types.get("Int_to_string"), Some(&Type::String));
    assert_eq!(types.get("Float64_to_string"), Some(&Type::String));
    assert_eq!(types.get("Bool_to_string"), Some(&Type::String));
    assert_eq!(types.get("String_trim"), Some(&Type::String));
    assert_eq!(types.get("String_to_upper"), Some(&Type::String));
    assert_eq!(types.get("String_to_lower"), Some(&Type::String));
    assert_eq!(types.get("String_substring"), Some(&Type::String));

    // Int-returning builtins
    assert_eq!(types.get("String_length"), Some(&Type::Int));
    assert_eq!(types.get("abs"), Some(&Type::Int));
    assert_eq!(types.get("min"), Some(&Type::Int));
    assert_eq!(types.get("max"), Some(&Type::Int));
    assert_eq!(types.get("clamp"), Some(&Type::Int));
    assert_eq!(types.get("list_length"), Some(&Type::Int));
    assert_eq!(types.get("map_length"), Some(&Type::Int));
    assert_eq!(types.get("map_contains_key"), Some(&Type::Int));

    // Iterator builtins
    assert_eq!(types.get("String_chars"), Some(&Type::Int));
    assert_eq!(types.get("string_chars_advance"), Some(&Type::Int));
    assert_eq!(types.get("string_chars_value"), Some(&Type::Int));
    assert_eq!(types.get("Map_keys"), Some(&Type::Int));
    assert_eq!(types.get("map_keys_advance"), Some(&Type::Int));
    assert_eq!(types.get("map_keys_value"), Some(&Type::Int));
    assert_eq!(types.get("Map_values"), Some(&Type::Int));
    assert_eq!(types.get("map_values_advance"), Some(&Type::Int));
    assert_eq!(types.get("map_values_value"), Some(&Type::Int));
}

#[test]
fn test_field_access_type_resolution() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        name: "test".to_string(),
        span: span(),
        meta: Some(Meta {
            id: NodeId(2),
            span: span(),
            entries: vec![
                MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: span(),
                },
                MetaEntry {
                    key: "version".to_string(),
                    value: "1.0.0".to_string(),
                    span: span(),
                },
            ],
        }),
        imports: vec![],
        type_aliases: vec![],
        type_decls: vec![TypeDecl {
            id: NodeId(1),
            name: "Point".to_string(),
            visibility: Visibility::Private,
            span: span(),
            generic_params: vec![],
            fields: vec![
                FieldDef {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                },
                FieldDef {
                    name: "y".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                },
            ],
        }],
        enum_decls: vec![],
        functions: vec![make_fn(
            "get_x",
            vec![Param {
                name: "p".to_string(),
                ty: TypeExpr::Named("Point".to_string()),
                ownership: Ownership::Owned,
                span: span(),
            }],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::FieldAccess {
                        object: Box::new(Expr::Ident("p".to_string(), span())),
                        field: "x".to_string(),
                        span: span(),
                    }),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        )],
        intent_decls: vec![],
        invariants: vec![],
        impl_blocks: vec![],
        trait_decls: vec![],
        actor_decls: vec![],
    };

    let struct_registry = HashMap::from([(
        "Point".to_string(),
        vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
    )]);
    let enum_registry: HashMap<String, Vec<(String, Vec<Type>)>> = HashMap::new();
    let enum_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    let type_alias_registry: HashMap<String, (Type, Option<kodo_ast::Expr>)> = HashMap::new();
    let result = lower_module_with_type_info(
        &module,
        &struct_registry,
        &enum_registry,
        &enum_names,
        &type_alias_registry,
    );
    assert!(result.is_ok(), "field access lowering failed: {result:?}");

    let mir_functions = result.unwrap();
    // Find the get_x function
    let get_x = mir_functions
        .iter()
        .find(|f| f.name == "get_x")
        .expect("get_x not found");
    // Verify the return type is Int
    assert_eq!(get_x.return_type, Type::Int);
}

#[test]
fn lower_spawn_without_captures() {
    // fn main() { spawn { print_int(42) } }
    let func = make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Spawn {
                span: span(),
                body: Block {
                    span: span(),
                    stmts: vec![Stmt::Expr(Expr::Call {
                        span: span(),
                        callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                        args: vec![Expr::IntLit(42, span())],
                    })],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let (mir, closures) = lower_function_with_closures(
        &func,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashSet::new(),
        &HashMap::new(),
    )
    .unwrap();
    mir.validate().unwrap();

    // Should generate a __spawn_ function and call kodo_spawn_task.
    assert!(!closures.is_empty(), "expected a generated spawn function");
    let has_spawn_task_call = mir.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_spawn_task"))
    });
    assert!(has_spawn_task_call, "expected kodo_spawn_task call");
}

#[test]
fn lower_spawn_with_captures() {
    // fn main() { let x: Int = 10; spawn { print_int(x) } }
    let func = make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    name: "x".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    mutable: false,
                    value: Expr::IntLit(10, span()),
                },
                Stmt::Spawn {
                    span: span(),
                    body: Block {
                        span: span(),
                        stmts: vec![Stmt::Expr(Expr::Call {
                            span: span(),
                            callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                            args: vec![Expr::Ident("x".to_string(), span())],
                        })],
                    },
                },
            ],
        },
        TypeExpr::Unit,
    );
    let (mir, closures) = lower_function_with_closures(
        &func,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashSet::new(),
        &HashMap::new(),
    )
    .unwrap();
    mir.validate().unwrap();

    // Should generate a __spawn_ function that takes 1 param (env ptr).
    let spawn_fn = closures
        .iter()
        .find(|f| f.name.starts_with("__spawn_"))
        .expect("expected a __spawn_ function");
    assert_eq!(spawn_fn.param_count, 1, "spawn fn should take env pointer");

    // Main should call __env_pack and kodo_spawn_task_with_env.
    let has_env_pack = mir.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "__env_pack"))
    });
    assert!(has_env_pack, "expected __env_pack call in main");

    let has_spawn_with_env = mir.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(i, Instruction::Call { callee, .. } if callee == "kodo_spawn_task_with_env")
        })
    });
    assert!(has_spawn_with_env, "expected kodo_spawn_task_with_env call");

    // The spawn function should contain an __env_load call.
    let has_env_load = spawn_fn.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "__env_load"))
    });
    assert!(has_env_load, "spawn fn should unpack env with __env_load");
}

/// Helper to build a module with a Counter actor and a main function.
fn make_actor_module(main_body: Block) -> Module {
    Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: span(),
            }],
            span: span(),
        }),
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![ActorDecl {
            id: NodeId(1),
            span: span(),
            name: "Counter".to_string(),
            fields: vec![FieldDef {
                name: "count".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
            }],
            handlers: vec![make_fn(
                "increment",
                vec![Param {
                    name: "self".to_string(),
                    ty: TypeExpr::Named("Counter".to_string()),
                    ownership: Ownership::Owned,
                    span: span(),
                }],
                Block {
                    span: span(),
                    stmts: vec![Stmt::Return {
                        span: span(),
                        value: Some(Expr::BinaryOp {
                            left: Box::new(Expr::FieldAccess {
                                object: Box::new(Expr::Ident("self".to_string(), span())),
                                field: "count".to_string(),
                                span: span(),
                            }),
                            op: BinOp::Add,
                            right: Box::new(Expr::IntLit(1, span())),
                            span: span(),
                        }),
                    }],
                },
                TypeExpr::Named("Int".to_string()),
            )],
        }],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![make_fn("main", vec![], main_body, TypeExpr::Unit)],
    }
}

#[test]
fn actor_instantiation_emits_actor_new_and_set_field() {
    let module = make_actor_module(Block {
        span: span(),
        stmts: vec![Stmt::Let {
            span: span(),
            mutable: false,
            name: "c".to_string(),
            ty: Some(TypeExpr::Named("Counter".to_string())),
            value: Expr::StructLit {
                name: "Counter".to_string(),
                fields: vec![FieldInit {
                    name: "count".to_string(),
                    value: Expr::IntLit(42, span()),
                    span: span(),
                }],
                span: span(),
            },
        }],
    });

    let mir_fns = lower_module(&module).unwrap();
    let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

    let has_actor_new = main.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_new"))
    });
    assert!(has_actor_new, "expected kodo_actor_new call");

    let has_set_field = main.blocks.iter().any(|b| {
        b.instructions.iter().any(
            |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_set_field"),
        )
    });
    assert!(has_set_field, "expected kodo_actor_set_field call");

    let has_struct_lit = main.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(i, Instruction::Assign(_, Value::StructLit { name, .. }) if name == "Counter")
        })
    });
    assert!(!has_struct_lit, "actor should not produce StructLit MIR");
}

#[test]
fn actor_field_access_emits_get_field() {
    let module = make_actor_module(Block {
        span: span(),
        stmts: vec![
            Stmt::Let {
                span: span(),
                mutable: false,
                name: "c".to_string(),
                ty: Some(TypeExpr::Named("Counter".to_string())),
                value: Expr::StructLit {
                    name: "Counter".to_string(),
                    fields: vec![FieldInit {
                        name: "count".to_string(),
                        value: Expr::IntLit(10, span()),
                        span: span(),
                    }],
                    span: span(),
                },
            },
            Stmt::Let {
                span: span(),
                mutable: false,
                name: "v".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::FieldAccess {
                    object: Box::new(Expr::Ident("c".to_string(), span())),
                    field: "count".to_string(),
                    span: span(),
                },
            },
        ],
    });

    let mir_fns = lower_module(&module).unwrap();
    let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

    let has_get_field = main.blocks.iter().any(|b| {
        b.instructions.iter().any(
            |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_get_field"),
        )
    });
    assert!(has_get_field, "expected kodo_actor_get_field call");

    let has_field_get = main.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(i, Instruction::Assign(_, Value::FieldGet { struct_name, .. }) if struct_name == "Counter")
        })
    });
    assert!(
        !has_field_get,
        "actor field access should not produce FieldGet MIR"
    );
}

#[test]
fn actor_handler_call_emits_send() {
    let module = make_actor_module(Block {
        span: span(),
        stmts: vec![
            Stmt::Let {
                span: span(),
                mutable: false,
                name: "c".to_string(),
                ty: Some(TypeExpr::Named("Counter".to_string())),
                value: Expr::StructLit {
                    name: "Counter".to_string(),
                    fields: vec![FieldInit {
                        name: "count".to_string(),
                        value: Expr::IntLit(0, span()),
                        span: span(),
                    }],
                    span: span(),
                },
            },
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("Counter_increment".to_string(), span())),
                args: vec![Expr::Ident("c".to_string(), span())],
                span: span(),
            }),
        ],
    });

    let mir_fns = lower_module(&module).unwrap();
    let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

    let has_send = main.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_send"))
    });
    assert!(has_send, "expected kodo_actor_send call");

    let has_func_ref = main.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| match i {
            Instruction::Call { callee, args, .. } if callee == "kodo_actor_send" => args
                .iter()
                .any(|a| matches!(a, Value::FuncRef(name) if name == "Counter_increment")),
            _ => false,
        })
    });
    assert!(
        has_func_ref,
        "kodo_actor_send should contain FuncRef(Counter_increment)"
    );
}

#[test]
fn actor_handler_lowered_as_function() {
    let module = make_actor_module(Block {
        span: span(),
        stmts: vec![],
    });

    let mir_fns = lower_module(&module).unwrap();

    let handler = mir_fns.iter().find(|f| f.name == "Counter_increment");
    assert!(
        handler.is_some(),
        "expected Counter_increment function in MIR output"
    );
}

#[test]
fn actor_handler_field_access_uses_get_field() {
    let module = make_actor_module(Block {
        span: span(),
        stmts: vec![],
    });

    let mir_fns = lower_module(&module).unwrap();
    let handler = mir_fns
        .iter()
        .find(|f| f.name == "Counter_increment")
        .unwrap();

    let has_get_field = handler.blocks.iter().any(|b| {
        b.instructions.iter().any(
            |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_get_field"),
        )
    });
    assert!(
        has_get_field,
        "handler should use kodo_actor_get_field for self.count"
    );
}

#[test]
fn is_actor_handler_helper() {
    let mut builder = MirBuilder::new();
    builder.actor_names.insert("Counter".to_string());
    builder.actor_names.insert("Logger".to_string());

    assert!(builder.is_actor_handler("Counter_increment"));
    assert!(builder.is_actor_handler("Logger_log"));
    assert!(!builder.is_actor_handler("Counter")); // no underscore suffix
    assert!(!builder.is_actor_handler("print_int")); // not an actor
    assert!(!builder.is_actor_handler("")); // empty string
}

#[test]
fn refinement_check_emitted_for_refined_alias() {
    let constraint = Expr::BinaryOp {
        left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
        span: Span::new(0, 10),
    };
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(kodo_ast::Meta {
            id: NodeId(1),
            span: Span::new(0, 50),
            entries: vec![kodo_ast::MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(0, 20),
            }],
        }),
        type_aliases: vec![kodo_ast::TypeAlias {
            id: NodeId(2),
            span: Span::new(0, 30),
            name: "Port".to_string(),
            base_type: TypeExpr::Named("Int".to_string()),
            constraint: Some(constraint),
        }],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(3),
            span: Span::new(0, 80),
            name: "main".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: Block {
                span: Span::new(0, 60),
                stmts: vec![Stmt::Let {
                    span: Span::new(0, 20),
                    mutable: false,
                    name: "port".to_string(),
                    ty: Some(TypeExpr::Named("Port".to_string())),
                    value: Expr::IntLit(8080, Span::new(0, 4)),
                }],
            },
        }],
    };

    let result = lower_module(&module);
    assert!(result.is_ok(), "refinement lowering failed: {result:?}");
    let fns = result.unwrap();
    let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

    let has_contract_fail = main_fn.blocks.iter().any(|b| {
        b.instructions.iter().any(
            |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
        )
    });
    assert!(
        has_contract_fail,
        "expected kodo_contract_fail call for refinement check"
    );

    assert!(
        main_fn.blocks.len() >= 3,
        "expected at least 3 blocks for refinement check, got {}",
        main_fn.blocks.len()
    );
}

#[test]
fn no_refinement_check_for_unconstrained_alias() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(kodo_ast::Meta {
            id: NodeId(1),
            span: Span::new(0, 50),
            entries: vec![kodo_ast::MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(0, 20),
            }],
        }),
        type_aliases: vec![kodo_ast::TypeAlias {
            id: NodeId(2),
            span: Span::new(0, 30),
            name: "Name".to_string(),
            base_type: TypeExpr::Named("String".to_string()),
            constraint: None,
        }],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(3),
            span: Span::new(0, 80),
            name: "main".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: Block {
                span: Span::new(0, 60),
                stmts: vec![Stmt::Let {
                    span: Span::new(0, 20),
                    mutable: false,
                    name: "s".to_string(),
                    ty: Some(TypeExpr::Named("Name".to_string())),
                    value: Expr::StringLit("hello".to_string(), Span::new(0, 7)),
                }],
            },
        }],
    };

    let result = lower_module(&module);
    assert!(result.is_ok(), "unconstrained alias lowering failed");
    let fns = result.unwrap();
    let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

    let has_contract_fail = main_fn.blocks.iter().any(|b| {
        b.instructions.iter().any(
            |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
        )
    });
    assert!(
        !has_contract_fail,
        "should NOT emit contract_fail for unconstrained alias"
    );
}

#[test]
fn substitute_self_replaces_ident() {
    let expr = Expr::BinaryOp {
        left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
        span: Span::new(0, 10),
    };
    let substituted = MirBuilder::substitute_self_in_expr(&expr, "port");
    match &substituted {
        Expr::BinaryOp { left, .. } => match left.as_ref() {
            Expr::Ident(name, _) => assert_eq!(name, "port"),
            other => panic!("expected Ident, got {other:?}"),
        },
        other => panic!("expected BinaryOp, got {other:?}"),
    }
}

#[test]
fn substitute_self_preserves_non_self_idents() {
    let expr = Expr::BinaryOp {
        left: Box::new(Expr::Ident("other".to_string(), Span::new(0, 5))),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
        span: Span::new(0, 10),
    };
    let substituted = MirBuilder::substitute_self_in_expr(&expr, "port");
    match &substituted {
        Expr::BinaryOp { left, .. } => match left.as_ref() {
            Expr::Ident(name, _) => assert_eq!(name, "other"),
            other => panic!("expected Ident, got {other:?}"),
        },
        other => panic!("expected BinaryOp, got {other:?}"),
    }
}

#[test]
fn refinement_check_with_compound_constraint() {
    let constraint = Expr::BinaryOp {
        left: Box::new(Expr::BinaryOp {
            left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
            op: kodo_ast::BinOp::Gt,
            right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
            span: Span::new(0, 10),
        }),
        op: kodo_ast::BinOp::And,
        right: Box::new(Expr::BinaryOp {
            left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
            op: kodo_ast::BinOp::Lt,
            right: Box::new(Expr::IntLit(65535, Span::new(0, 5))),
            span: Span::new(0, 15),
        }),
        span: Span::new(0, 20),
    };
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(kodo_ast::Meta {
            id: NodeId(1),
            span: Span::new(0, 50),
            entries: vec![kodo_ast::MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(0, 20),
            }],
        }),
        type_aliases: vec![kodo_ast::TypeAlias {
            id: NodeId(2),
            span: Span::new(0, 30),
            name: "Port".to_string(),
            base_type: TypeExpr::Named("Int".to_string()),
            constraint: Some(constraint),
        }],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(3),
            span: Span::new(0, 80),
            name: "main".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: Block {
                span: Span::new(0, 60),
                stmts: vec![Stmt::Let {
                    span: Span::new(0, 20),
                    mutable: false,
                    name: "port".to_string(),
                    ty: Some(TypeExpr::Named("Port".to_string())),
                    value: Expr::IntLit(8080, Span::new(0, 4)),
                }],
            },
        }],
    };

    let result = lower_module(&module);
    assert!(
        result.is_ok(),
        "compound constraint lowering failed: {result:?}"
    );
    let fns = result.unwrap();
    let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

    let has_contract_fail = main_fn.blocks.iter().any(|b| {
        b.instructions.iter().any(
            |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
        )
    });
    assert!(
        has_contract_fail,
        "expected kodo_contract_fail for compound constraint"
    );

    let fail_msg = main_fn.blocks.iter().find_map(|b| {
        b.instructions.iter().find_map(|i| {
            if let Instruction::Call { callee, args, .. } = i {
                if callee == "kodo_contract_fail" {
                    if let Some(Value::StringConst(msg)) = args.first() {
                        return Some(msg.clone());
                    }
                }
            }
            None
        })
    });
    assert!(fail_msg.is_some(), "expected a fail message");
    let msg = fail_msg.unwrap();
    assert!(
        msg.contains("Port"),
        "fail message should reference 'Port', got: {msg}"
    );
    assert!(
        msg.contains("port"),
        "fail message should reference 'port', got: {msg}"
    );
}

// -------------------------------------------------------------------
// Reference counting lowering tests (Phase 39)
// -------------------------------------------------------------------

#[test]
fn is_heap_type_string_returns_true() {
    assert!(MirBuilder::is_heap_type(&Type::String));
}

#[test]
fn is_heap_type_struct_returns_true() {
    assert!(MirBuilder::is_heap_type(&Type::Struct("Point".to_string())));
}

#[test]
fn is_heap_type_int_returns_false() {
    assert!(!MirBuilder::is_heap_type(&Type::Int));
}

#[test]
fn is_heap_type_bool_returns_false() {
    assert!(!MirBuilder::is_heap_type(&Type::Bool));
}

#[test]
fn is_heap_type_float64_returns_false() {
    assert!(!MirBuilder::is_heap_type(&Type::Float64));
}

#[test]
fn is_heap_type_unit_returns_false() {
    assert!(!MirBuilder::is_heap_type(&Type::Unit));
}

#[test]
fn decref_emitted_for_string_local_before_return() {
    // fn main() -> Unit { let msg: String = "hello"; return }
    let func = make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "msg".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hello".to_string(), span()),
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();

    // There should be at least one DecRef instruction for the string local.
    let has_decref = mir.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::DecRef(_)))
    });
    assert!(
        has_decref,
        "expected DecRef for heap-allocated String local before return"
    );
}

#[test]
fn no_decref_for_int_local() {
    // fn main() -> Unit { let x: Int = 42 }
    let func = make_fn(
        "main",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(42, span()),
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();

    // There should be no DecRef for an Int local.
    let has_decref = mir.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::DecRef(_)))
    });
    assert!(
        !has_decref,
        "should NOT emit DecRef for primitive Int local"
    );
}

#[test]
fn is_heap_type_generic_returns_true() {
    assert!(MirBuilder::is_heap_type(&Type::Generic(
        "List".to_string(),
        vec![Type::Int]
    )));
}

#[test]
fn is_heap_type_byte_returns_false() {
    assert!(!MirBuilder::is_heap_type(&Type::Byte));
}

#[test]
fn lower_break_in_while() {
    let func = make_fn(
        "break_while",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::While {
                span: span(),
                condition: Expr::BoolLit(true, span()),
                body: Block {
                    span: span(),
                    stmts: vec![Stmt::Break { span: span() }],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    // The MIR should have multiple blocks including the loop exit.
    assert!(mir.blocks.len() >= 3);
    // At least one block should have a Goto terminator to the exit block.
    let goto_count = mir
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::Goto(_)))
        .count();
    assert!(
        goto_count >= 2,
        "expected at least 2 Goto terminators for break"
    );
}

#[test]
fn lower_continue_in_while() {
    let func = make_fn(
        "continue_while",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::While {
                span: span(),
                condition: Expr::BoolLit(true, span()),
                body: Block {
                    span: span(),
                    stmts: vec![Stmt::Continue { span: span() }],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    assert!(mir.blocks.len() >= 3);
    // Continue should generate a Goto back to the loop header.
    let goto_count = mir
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::Goto(_)))
        .count();
    assert!(
        goto_count >= 2,
        "expected at least 2 Goto terminators for continue"
    );
}

#[test]
fn lower_break_in_for_loop() {
    let func = make_fn(
        "break_for",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::For {
                span: span(),
                name: "i".to_string(),
                start: Expr::IntLit(0, span()),
                end: Expr::IntLit(10, span()),
                inclusive: false,
                body: Block {
                    span: span(),
                    stmts: vec![Stmt::Break { span: span() }],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    assert!(mir.blocks.len() >= 3);
}

#[test]
fn lower_nested_break_only_breaks_inner() {
    let func = make_fn(
        "nested_break",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::While {
                span: span(),
                condition: Expr::BoolLit(true, span()),
                body: Block {
                    span: span(),
                    stmts: vec![Stmt::While {
                        span: span(),
                        condition: Expr::BoolLit(true, span()),
                        body: Block {
                            span: span(),
                            stmts: vec![Stmt::Break { span: span() }],
                        },
                    }],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    // Nested loops should produce more blocks.
    assert!(mir.blocks.len() >= 5);
}

// -----------------------------------------------------------------
// Phase 51 — Additional lowering coverage tests
// -----------------------------------------------------------------

#[test]
fn lower_string_literal() {
    // fn greet() -> String { return "hello" }
    let func = make_fn(
        "greet",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::StringLit("hello".to_string(), span())),
            }],
        },
        TypeExpr::Named("String".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    assert!(matches!(
        mir.blocks[0].terminator,
        Terminator::Return(Value::StringConst(ref s)) if s == "hello"
    ));
}

#[test]
fn lower_float_literal() {
    // fn pi() -> Float64 { return 3.14 }
    let func = make_fn(
        "pi",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::FloatLit(3.14, span())),
            }],
        },
        TypeExpr::Named("Float64".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    match &mir.blocks[0].terminator {
        Terminator::Return(Value::FloatConst(f)) => {
            assert!((*f - 3.14).abs() < f64::EPSILON);
        }
        other => panic!("expected Return(FloatConst(3.14)), got {other:?}"),
    }
}

#[test]
fn lower_bool_literal() {
    // fn truthy() -> Bool { return true }
    let func = make_fn(
        "truthy",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::BoolLit(true, span())),
            }],
        },
        TypeExpr::Named("Bool".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    assert!(matches!(
        mir.blocks[0].terminator,
        Terminator::Return(Value::BoolConst(true))
    ));
}

#[test]
fn lower_multiple_params() {
    // fn add3(a: Int, b: Int, c: Int) -> Int { return a + b + c }
    let func = make_fn(
        "add3",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            },
            Param {
                name: "c".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            },
        ],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::BinaryOp {
                    left: Box::new(Expr::BinaryOp {
                        left: Box::new(Expr::Ident("a".to_string(), span())),
                        op: BinOp::Add,
                        right: Box::new(Expr::Ident("b".to_string(), span())),
                        span: span(),
                    }),
                    op: BinOp::Add,
                    right: Box::new(Expr::Ident("c".to_string(), span())),
                    span: span(),
                }),
            }],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    assert_eq!(mir.param_count, 3);
    assert_eq!(mir.locals.len(), 3);
    assert_eq!(mir.locals[0].ty, Type::Int);
    assert_eq!(mir.locals[1].ty, Type::Int);
    assert_eq!(mir.locals[2].ty, Type::Int);
}

#[test]
fn lower_if_without_else() {
    // fn maybe(x: Bool) { if x { print_int(1) } }
    let func = make_fn(
        "maybe",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Bool".to_string()),
            span: span(),
            ownership: Ownership::Owned,
        }],
        Block {
            span: span(),
            stmts: vec![Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Ident("x".to_string(), span())),
                then_branch: Block {
                    span: span(),
                    stmts: vec![Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                        args: vec![Expr::IntLit(1, span())],
                        span: span(),
                    })],
                },
                else_branch: None,
                span: span(),
            })],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have: entry (Branch), then, else (empty), merge
    assert!(mir.blocks.len() >= 4);
    assert!(matches!(
        mir.blocks[0].terminator,
        Terminator::Branch { .. }
    ));
}

#[test]
fn lower_nested_if_else() {
    // fn nested(a: Bool, b: Bool) -> Int {
    //   if a { if b { return 1 } else { return 2 } } else { return 3 }
    // }
    let func = make_fn(
        "nested",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("Bool".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("Bool".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            },
        ],
        Block {
            span: span(),
            stmts: vec![Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Ident("a".to_string(), span())),
                then_branch: Block {
                    span: span(),
                    stmts: vec![Stmt::Expr(Expr::If {
                        condition: Box::new(Expr::Ident("b".to_string(), span())),
                        then_branch: Block {
                            span: span(),
                            stmts: vec![Stmt::Return {
                                span: span(),
                                value: Some(Expr::IntLit(1, span())),
                            }],
                        },
                        else_branch: Some(Block {
                            span: span(),
                            stmts: vec![Stmt::Return {
                                span: span(),
                                value: Some(Expr::IntLit(2, span())),
                            }],
                        }),
                        span: span(),
                    })],
                },
                else_branch: Some(Block {
                    span: span(),
                    stmts: vec![Stmt::Return {
                        span: span(),
                        value: Some(Expr::IntLit(3, span())),
                    }],
                }),
                span: span(),
            })],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Nested if/else creates many blocks
    assert!(
        mir.blocks.len() >= 7,
        "nested if/else should produce >= 7 blocks, got {}",
        mir.blocks.len()
    );
}

#[test]
fn lower_comparison_operators() {
    // fn cmp(a: Int, b: Int) -> Bool { return a >= b }
    let func = make_fn(
        "cmp",
        vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            },
        ],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::BinaryOp {
                    left: Box::new(Expr::Ident("a".to_string(), span())),
                    op: BinOp::Ge,
                    right: Box::new(Expr::Ident("b".to_string(), span())),
                    span: span(),
                }),
            }],
        },
        TypeExpr::Named("Bool".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    match &mir.blocks[0].terminator {
        Terminator::Return(Value::BinOp(BinOp::Ge, _, _)) => {}
        other => panic!("expected Return(BinOp(Ge, ...)), got {other:?}"),
    }
}

#[test]
fn lower_mutable_let_and_reassign() {
    // fn mutate() { let mut x: Int = 0; x = 1; x = 2 }
    let func = make_fn(
        "mutate",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: true,
                    name: "x".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(0, span()),
                },
                Stmt::Assign {
                    span: span(),
                    name: "x".to_string(),
                    value: Expr::IntLit(1, span()),
                },
                Stmt::Assign {
                    span: span(),
                    name: "x".to_string(),
                    value: Expr::IntLit(2, span()),
                },
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // 3 assignments in one block
    assert_eq!(mir.blocks[0].instructions.len(), 3);
}

#[test]
fn lower_return_without_value() {
    // fn noop() { return }
    let func = make_fn(
        "noop",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: None,
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    assert!(matches!(
        mir.blocks[0].terminator,
        Terminator::Return(Value::Unit)
    ));
}

#[test]
fn lower_while_with_body_assignments() {
    // fn countdown() { let mut n: Int = 5; while n > 0 { n = n - 1 } return }
    let func = make_fn(
        "countdown",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: true,
                    name: "n".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(5, span()),
                },
                Stmt::While {
                    span: span(),
                    condition: Expr::BinaryOp {
                        left: Box::new(Expr::Ident("n".to_string(), span())),
                        op: BinOp::Gt,
                        right: Box::new(Expr::IntLit(0, span())),
                        span: span(),
                    },
                    body: Block {
                        span: span(),
                        stmts: vec![Stmt::Assign {
                            span: span(),
                            name: "n".to_string(),
                            value: Expr::BinaryOp {
                                left: Box::new(Expr::Ident("n".to_string(), span())),
                                op: BinOp::Sub,
                                right: Box::new(Expr::IntLit(1, span())),
                                span: span(),
                            },
                        }],
                    },
                },
                Stmt::Return {
                    span: span(),
                    value: None,
                },
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // While loop: entry -> header -> body -> exit -> return
    assert!(mir.blocks.len() >= 4);
    // Body block should have an Assign instruction (n = n - 1)
    let body_block = &mir.blocks[2]; // loop body is typically block 2
    assert!(
        !body_block.instructions.is_empty(),
        "loop body should have assignment"
    );
}

#[test]
fn lower_for_with_body_call() {
    // fn repeat() { for i in 0..3 { print_int(i) } }
    let func = make_fn(
        "repeat",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::For {
                span: span(),
                name: "i".to_string(),
                start: Expr::IntLit(0, span()),
                end: Expr::IntLit(3, span()),
                inclusive: false,
                body: Block {
                    span: span(),
                    stmts: vec![Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                        args: vec![Expr::Ident("i".to_string(), span())],
                        span: span(),
                    })],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Verify loop structure: entry Goto header
    assert!(matches!(mir.blocks[0].terminator, Terminator::Goto(_)));
    // Header should have Branch
    assert!(matches!(
        mir.blocks[1].terminator,
        Terminator::Branch { .. }
    ));
}

#[test]
fn lower_multiple_function_calls_in_sequence() {
    // fn multi() { print_int(1); print_int(2); print_int(3) }
    let func = make_fn(
        "multi",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                    args: vec![Expr::IntLit(1, span())],
                    span: span(),
                }),
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                    args: vec![Expr::IntLit(2, span())],
                    span: span(),
                }),
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                    args: vec![Expr::IntLit(3, span())],
                    span: span(),
                }),
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have 3 Call instructions
    let call_count = mir.blocks[0]
        .instructions
        .iter()
        .filter(|i| matches!(i, Instruction::Call { .. }))
        .count();
    assert_eq!(call_count, 3);
}

#[test]
fn lower_struct_literal_module() {
    // Module with struct type and a function that creates it
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "struct_test".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![TypeDecl {
            id: NodeId(0),
            name: "Point".to_string(),
            visibility: Visibility::Private,
            span: span(),
            generic_params: vec![],
            fields: vec![
                FieldDef {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                },
                FieldDef {
                    name: "y".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                },
            ],
        }],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![make_fn(
            "make_point",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "p".to_string(),
                    ty: None,
                    value: Expr::StructLit {
                        name: "Point".to_string(),
                        fields: vec![
                            FieldInit {
                                name: "x".to_string(),
                                value: Expr::IntLit(10, span()),
                                span: span(),
                            },
                            FieldInit {
                                name: "y".to_string(),
                                value: Expr::IntLit(20, span()),
                                span: span(),
                            },
                        ],
                        span: span(),
                    },
                }],
            },
            TypeExpr::Unit,
        )],
    };
    let fns = lower_module(&module).unwrap();
    assert_eq!(fns.len(), 1);
    let mir = &fns[0];
    mir.validate().unwrap();
    // Should have a StructLit assignment
    let has_struct_lit = mir.blocks[0].instructions.iter().any(
        |i| matches!(i, Instruction::Assign(_, Value::StructLit { name, .. }) if name == "Point"),
    );
    assert!(has_struct_lit, "expected StructLit assignment for Point");
}

#[test]
fn lower_struct_field_access_module() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "field_test".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![TypeDecl {
            id: NodeId(0),
            name: "Pair".to_string(),
            visibility: Visibility::Private,
            span: span(),
            generic_params: vec![],
            fields: vec![
                FieldDef {
                    name: "a".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                },
                FieldDef {
                    name: "b".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                },
            ],
        }],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![make_fn(
            "get_a",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "p".to_string(),
                        ty: Some(TypeExpr::Named("Pair".to_string())),
                        value: Expr::StructLit {
                            name: "Pair".to_string(),
                            fields: vec![
                                FieldInit {
                                    name: "a".to_string(),
                                    value: Expr::IntLit(42, span()),
                                    span: span(),
                                },
                                FieldInit {
                                    name: "b".to_string(),
                                    value: Expr::IntLit(99, span()),
                                    span: span(),
                                },
                            ],
                            span: span(),
                        },
                    },
                    Stmt::Return {
                        span: span(),
                        value: Some(Expr::FieldAccess {
                            object: Box::new(Expr::Ident("p".to_string(), span())),
                            field: "a".to_string(),
                            span: span(),
                        }),
                    },
                ],
            },
            TypeExpr::Named("Int".to_string()),
        )],
    };
    let fns = lower_module(&module).unwrap();
    let mir = &fns[0];
    mir.validate().unwrap();
    // Should have a FieldGet in instructions
    let has_field_get = mir.blocks.iter().any(|b| {
        b.instructions.iter().any(
            |i| matches!(i, Instruction::Assign(_, Value::FieldGet { field, .. }) if field == "a"),
        )
    });
    assert!(has_field_get, "expected FieldGet for field 'a'");
}

#[test]
fn lower_enum_variant_and_match_module() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "enum_test".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![kodo_ast::EnumDecl {
            id: NodeId(0),
            name: "Color".to_string(),
            span: span(),
            generic_params: vec![],
            variants: vec![
                kodo_ast::EnumVariant {
                    name: "Red".to_string(),
                    fields: vec![],
                    span: span(),
                },
                kodo_ast::EnumVariant {
                    name: "Green".to_string(),
                    fields: vec![],
                    span: span(),
                },
                kodo_ast::EnumVariant {
                    name: "Blue".to_string(),
                    fields: vec![],
                    span: span(),
                },
            ],
        }],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![make_fn(
            "make_red",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "c".to_string(),
                    ty: None,
                    value: Expr::EnumVariantExpr {
                        enum_name: "Color".to_string(),
                        variant: "Red".to_string(),
                        args: vec![],
                        span: span(),
                    },
                }],
            },
            TypeExpr::Unit,
        )],
    };
    let fns = lower_module(&module).unwrap();
    let mir = &fns[0];
    mir.validate().unwrap();
    let has_enum = mir.blocks[0].instructions.iter().any(|i| {
        matches!(i, Instruction::Assign(_, Value::EnumVariant { variant, discriminant, .. })
            if variant == "Red" && *discriminant == 0)
    });
    assert!(has_enum, "expected EnumVariant assignment for Red");
}

#[test]
fn lower_match_with_wildcard() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "match_test".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![kodo_ast::EnumDecl {
            id: NodeId(0),
            name: "Dir".to_string(),
            span: span(),
            generic_params: vec![],
            variants: vec![
                kodo_ast::EnumVariant {
                    name: "Up".to_string(),
                    fields: vec![],
                    span: span(),
                },
                kodo_ast::EnumVariant {
                    name: "Down".to_string(),
                    fields: vec![],
                    span: span(),
                },
            ],
        }],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![make_fn(
            "dir_to_int",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "d".to_string(),
                        ty: None,
                        value: Expr::EnumVariantExpr {
                            enum_name: "Dir".to_string(),
                            variant: "Up".to_string(),
                            args: vec![],
                            span: span(),
                        },
                    },
                    Stmt::Expr(Expr::Match {
                        expr: Box::new(Expr::Ident("d".to_string(), span())),
                        arms: vec![
                            kodo_ast::MatchArm {
                                pattern: kodo_ast::Pattern::Variant {
                                    enum_name: Some("Dir".to_string()),
                                    variant: "Up".to_string(),
                                    bindings: vec![],
                                    span: span(),
                                },
                                body: Expr::IntLit(1, span()),
                                span: span(),
                            },
                            kodo_ast::MatchArm {
                                pattern: kodo_ast::Pattern::Wildcard(span()),
                                body: Expr::IntLit(0, span()),
                                span: span(),
                            },
                        ],
                        span: span(),
                    }),
                ],
            },
            TypeExpr::Unit,
        )],
    };
    let fns = lower_module(&module).unwrap();
    let mir = &fns[0];
    mir.validate().unwrap();
    // Match creates multiple blocks for discriminant checking
    assert!(
        mir.blocks.len() >= 3,
        "match should create >= 3 blocks, got {}",
        mir.blocks.len()
    );
}

#[test]
fn lower_actor_with_handler_module() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "actor_test".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![ActorDecl {
            id: NodeId(0),
            name: "Counter".to_string(),
            span: span(),
            fields: vec![FieldDef {
                name: "count".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
            }],
            handlers: vec![make_fn(
                "increment",
                vec![],
                Block {
                    span: span(),
                    stmts: vec![],
                },
                TypeExpr::Unit,
            )],
        }],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![],
    };
    let fns = lower_module(&module).unwrap();
    // Handler should be lowered with mangled name Counter_increment
    assert!(
        fns.iter().any(|f| f.name == "Counter_increment"),
        "expected mangled handler name Counter_increment"
    );
}

#[test]
fn lower_decref_emitted_for_string_locals() {
    // fn with_strings() { let s: String = "hello"; return }
    let func = make_fn(
        "with_strings",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "s".to_string(),
                    ty: Some(TypeExpr::Named("String".to_string())),
                    value: Expr::StringLit("hello".to_string(), span()),
                },
                Stmt::Return {
                    span: span(),
                    value: None,
                },
            ],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Should have DecRef for the string local before return
    let has_decref = mir.blocks.iter().any(|b| {
        b.instructions
            .iter()
            .any(|i| matches!(i, Instruction::DecRef(_)))
    });
    assert!(has_decref, "expected DecRef for string local");
}

#[test]
fn lower_requires_injects_contract_check() {
    // fn positive(x: Int) -> Int requires { x > 0 } { return x }
    let func = Function {
        id: NodeId(0),
        span: span(),
        name: "positive".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span: span(),
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![Expr::BinaryOp {
            left: Box::new(Expr::Ident("x".to_string(), span())),
            op: BinOp::Gt,
            right: Box::new(Expr::IntLit(0, span())),
            span: span(),
        }],
        ensures: vec![],
        body: Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::Ident("x".to_string(), span())),
            }],
        },
    };
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Contract injection creates additional blocks (branch on condition)
    assert!(
        mir.blocks.len() >= 3,
        "requires should add >= 3 blocks, got {}",
        mir.blocks.len()
    );
    // Should have a call to kodo_contract_fail in some block
    let has_contract_fail = mir.blocks.iter().any(|b| {
        b.instructions.iter().any(
            |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
        )
    });
    assert!(
        has_contract_fail,
        "expected kodo_contract_fail call for requires"
    );
}

#[test]
fn lower_module_with_struct_type_registers_fields() {
    // Module with struct -> function that creates and accesses the struct
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: span(),
        name: "reg_test".to_string(),
        imports: vec![],
        meta: None,
        type_aliases: vec![],
        type_decls: vec![TypeDecl {
            id: NodeId(0),
            name: "Vec2".to_string(),
            visibility: Visibility::Private,
            span: span(),
            generic_params: vec![],
            fields: vec![
                FieldDef {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Float64".to_string()),
                    span: span(),
                },
                FieldDef {
                    name: "y".to_string(),
                    ty: TypeExpr::Named("Float64".to_string()),
                    span: span(),
                },
            ],
        }],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![make_fn(
            "origin",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "v".to_string(),
                    ty: None,
                    value: Expr::StructLit {
                        name: "Vec2".to_string(),
                        fields: vec![
                            FieldInit {
                                name: "x".to_string(),
                                value: Expr::FloatLit(0.0, span()),
                                span: span(),
                            },
                            FieldInit {
                                name: "y".to_string(),
                                value: Expr::FloatLit(0.0, span()),
                                span: span(),
                            },
                        ],
                        span: span(),
                    },
                }],
            },
            TypeExpr::Unit,
        )],
    };
    let fns = lower_module(&module).unwrap();
    let mir = &fns[0];
    mir.validate().unwrap();
    // Verify StructLit with float fields
    let has_vec2 = mir.blocks[0].instructions.iter().any(|i| {
        matches!(i, Instruction::Assign(_, Value::StructLit { name, fields })
            if name == "Vec2" && fields.len() == 2)
    });
    assert!(has_vec2, "expected Vec2 struct literal");
}

#[test]
fn lower_tuple_literal() {
    // fn pair() { let t = (1, 2) }
    let func = make_fn(
        "pair",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "t".to_string(),
                ty: None,
                value: Expr::TupleLit(
                    vec![Expr::IntLit(1, span()), Expr::IntLit(2, span())],
                    span(),
                ),
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Tuple is stored as EnumVariant with __Tuple name
    let has_tuple = mir.blocks[0].instructions.iter().any(|i| {
        matches!(i, Instruction::Assign(_, Value::EnumVariant { enum_name, .. })
            if enum_name == "__Tuple")
    });
    assert!(has_tuple, "expected __Tuple variant for tuple literal");
}

#[test]
fn lower_empty_while_loop() {
    // fn spin() { while true { } }
    let func = make_fn(
        "spin",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::While {
                span: span(),
                condition: Expr::BoolLit(true, span()),
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        },
        TypeExpr::Unit,
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    // Loop: entry -> header -> body -> exit
    assert!(mir.blocks.len() >= 4);
}
