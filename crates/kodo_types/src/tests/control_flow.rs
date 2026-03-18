//! Break/Continue tests and variables inside if blocks tests.

use super::*;

// --- Break / Continue tests ---

#[test]
fn break_inside_while_is_valid() {
    let module = make_module_with_body(vec![Stmt::While {
        span: Span::new(0, 50),
        condition: Expr::BoolLit(true, Span::new(6, 10)),
        body: Block {
            span: Span::new(11, 50),
            stmts: vec![Stmt::Break {
                span: Span::new(13, 18),
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn continue_inside_while_is_valid() {
    let module = make_module_with_body(vec![Stmt::While {
        span: Span::new(0, 50),
        condition: Expr::BoolLit(true, Span::new(6, 10)),
        body: Block {
            span: Span::new(11, 50),
            stmts: vec![Stmt::Continue {
                span: Span::new(13, 21),
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn break_outside_loop_is_error() {
    let module = make_module_with_body(vec![Stmt::Break {
        span: Span::new(5, 10),
    }]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0243");
}

#[test]
fn continue_outside_loop_is_error() {
    let module = make_module_with_body(vec![Stmt::Continue {
        span: Span::new(5, 13),
    }]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0244");
}

#[test]
fn break_in_for_loop_is_valid() {
    let module = make_module_with_body(vec![Stmt::For {
        span: Span::new(0, 50),
        name: "i".to_string(),
        start: Expr::IntLit(0, Span::new(10, 11)),
        end: Expr::IntLit(10, Span::new(13, 15)),
        inclusive: false,
        body: Block {
            span: Span::new(16, 50),
            stmts: vec![Stmt::Break {
                span: Span::new(18, 23),
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn continue_in_for_loop_is_valid() {
    let module = make_module_with_body(vec![Stmt::For {
        span: Span::new(0, 50),
        name: "i".to_string(),
        start: Expr::IntLit(0, Span::new(10, 11)),
        end: Expr::IntLit(10, Span::new(13, 15)),
        inclusive: false,
        body: Block {
            span: Span::new(16, 50),
            stmts: vec![Stmt::Continue {
                span: Span::new(18, 26),
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn break_in_nested_loop_is_valid() {
    let module = make_module_with_body(vec![Stmt::While {
        span: Span::new(0, 80),
        condition: Expr::BoolLit(true, Span::new(6, 10)),
        body: Block {
            span: Span::new(11, 80),
            stmts: vec![Stmt::While {
                span: Span::new(13, 70),
                condition: Expr::BoolLit(true, Span::new(19, 23)),
                body: Block {
                    span: Span::new(24, 70),
                    stmts: vec![Stmt::Break {
                        span: Span::new(26, 31),
                    }],
                },
            }],
        },
    }]);
    let mut checker = TypeChecker::new();
    assert!(checker.check_module(&module).is_ok());
}

#[test]
fn break_outside_loop_error_has_correct_span() {
    let err = TypeError::BreakOutsideLoop {
        span: Span::new(10, 15),
    };
    assert_eq!(err.code(), "E0243");
    assert_eq!(err.span(), Some(Span::new(10, 15)));
}

#[test]
fn continue_outside_loop_error_has_correct_span() {
    let err = TypeError::ContinueOutsideLoop {
        span: Span::new(20, 28),
    };
    assert_eq!(err.code(), "E0244");
    assert_eq!(err.span(), Some(Span::new(20, 28)));
}

#[test]
fn break_error_has_suggestion() {
    let err = TypeError::BreakOutsideLoop {
        span: Span::new(0, 5),
    };
    use kodo_ast::Diagnostic;
    let suggestion = err.suggestion();
    assert!(suggestion.is_some());
    assert!(suggestion.unwrap().contains("loop"));
}

#[test]
fn continue_error_has_suggestion() {
    let err = TypeError::ContinueOutsideLoop {
        span: Span::new(0, 8),
    };
    use kodo_ast::Diagnostic;
    let suggestion = err.suggestion();
    assert!(suggestion.is_some());
    assert!(suggestion.unwrap().contains("loop"));
}

// --- WI2: Variables inside if blocks should be visible within the block ---

#[test]
fn let_inside_if_block_visible_in_same_block() {
    // if true { let x: Int = 1; let y: Int = x }
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::If {
            condition: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
            then_branch: Block {
                span: Span::new(5, 30),
                stmts: vec![
                    Stmt::Let {
                        span: Span::new(6, 18),
                        mutable: false,
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(1, Span::new(15, 16)),
                    },
                    Stmt::Let {
                        span: Span::new(19, 29),
                        mutable: false,
                        name: "y".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::Ident("x".to_string(), Span::new(27, 28)),
                    },
                ],
            },
            else_branch: None,
            span: Span::new(0, 30),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_ok(),
        "let x inside if block should be visible for let y = x in same block"
    );
}

#[test]
fn let_inside_if_block_not_visible_outside() {
    // if true { let x: Int = 1 }; let y: Int = x  — should fail
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
                then_branch: Block {
                    span: Span::new(5, 20),
                    stmts: vec![Stmt::Let {
                        span: Span::new(6, 18),
                        mutable: false,
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(1, Span::new(15, 16)),
                    }],
                },
                else_branch: None,
                span: Span::new(0, 20),
            }),
            Stmt::Let {
                span: Span::new(21, 33),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(31, 32)),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_err(),
        "variable x from if block should NOT be visible after if"
    );
}

#[test]
fn let_inside_if_not_visible_in_else() {
    // if true { let x: Int = 1 } else { let y: Int = x } — should fail
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::If {
            condition: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
            then_branch: Block {
                span: Span::new(5, 20),
                stmts: vec![Stmt::Let {
                    span: Span::new(6, 18),
                    mutable: false,
                    name: "x".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(1, Span::new(15, 16)),
                }],
            },
            else_branch: Some(Block {
                span: Span::new(21, 40),
                stmts: vec![Stmt::Let {
                    span: Span::new(22, 34),
                    mutable: false,
                    name: "y".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::Ident("x".to_string(), Span::new(32, 33)),
                }],
            }),
            span: Span::new(0, 40),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_err(),
        "variable x from then-branch should NOT be visible in else-branch"
    );
}

#[test]
fn outer_variable_accessible_inside_if() {
    // let x: Int = 1; if true { let y: Int = x }
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 12),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(1, Span::new(10, 11)),
            },
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::BoolLit(true, Span::new(14, 18))),
                then_branch: Block {
                    span: Span::new(19, 34),
                    stmts: vec![Stmt::Let {
                        span: Span::new(20, 32),
                        mutable: false,
                        name: "y".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::Ident("x".to_string(), Span::new(30, 31)),
                    }],
                },
                else_branch: None,
                span: Span::new(14, 34),
            }),
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_ok(),
        "outer variable x should be accessible inside if block"
    );
}

#[test]
fn shadowing_inside_if_does_not_affect_outer() {
    // let x: Int = 1; if true { let x: Bool = true }; let y: Int = x
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 12),
                mutable: false,
                name: "x".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(1, Span::new(10, 11)),
            },
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::BoolLit(true, Span::new(14, 18))),
                then_branch: Block {
                    span: Span::new(19, 40),
                    stmts: vec![Stmt::Let {
                        span: Span::new(20, 36),
                        mutable: false,
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Bool".to_string())),
                        value: Expr::BoolLit(true, Span::new(33, 37)),
                    }],
                },
                else_branch: None,
                span: Span::new(14, 40),
            }),
            Stmt::Let {
                span: Span::new(42, 56),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::Ident("x".to_string(), Span::new(54, 55)),
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_ok(),
        "shadowing x to Bool inside if should not change outer x: Int"
    );
}

#[test]
fn let_mut_inside_if_with_reassignment() {
    // if true { let mut x: Int = 1; x = 2 }
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Expr(Expr::If {
            condition: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
            then_branch: Block {
                span: Span::new(5, 35),
                stmts: vec![
                    Stmt::Let {
                        span: Span::new(6, 22),
                        mutable: true,
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(1, Span::new(20, 21)),
                    },
                    Stmt::Assign {
                        span: Span::new(24, 29),
                        name: "x".to_string(),
                        value: Expr::IntLit(2, Span::new(28, 29)),
                    },
                ],
            },
            else_branch: None,
            span: Span::new(0, 35),
        })],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    assert!(
        checker.check_module(&module).is_ok(),
        "let mut x inside if with reassignment should work"
    );
}

#[test]
fn generic_struct_literal_infers_type_args() {
    let struct_decl = kodo_ast::TypeDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Pair".to_string(),
        visibility: Visibility::Private,
        generic_params: vec![gp("T")],
        fields: vec![
            kodo_ast::FieldDef {
                name: "first".to_string(),
                ty: TypeExpr::Named("T".to_string()),
                span: Span::new(0, 20),
            },
            kodo_ast::FieldDef {
                name: "second".to_string(),
                ty: TypeExpr::Named("T".to_string()),
                span: Span::new(0, 20),
            },
        ],
    };
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 50),
            mutable: false,
            name: "p".to_string(),
            ty: Some(TypeExpr::Generic(
                "Pair".to_string(),
                vec![TypeExpr::Named("Int".to_string())],
            )),
            value: Expr::StructLit {
                name: "Pair".to_string(),
                fields: vec![
                    kodo_ast::FieldInit {
                        name: "first".to_string(),
                        value: Expr::IntLit(1, Span::new(0, 1)),
                        span: Span::new(0, 10),
                    },
                    kodo_ast::FieldInit {
                        name: "second".to_string(),
                        value: Expr::IntLit(2, Span::new(0, 1)),
                        span: Span::new(0, 10),
                    },
                ],
                span: Span::new(0, 40),
            },
        }],
    );
    let module = make_module_with_decls(vec![struct_decl], vec![], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "generic struct literal should type-check: {result:?}"
    );
    assert!(
        checker.struct_registry().contains_key("Pair__Int"),
        "Pair__Int should be in struct_registry after monomorphization"
    );
    let fields = checker.struct_registry().get("Pair__Int").unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0], ("first".to_string(), Type::Int));
    assert_eq!(fields[1], ("second".to_string(), Type::Int));
}

#[test]
fn generic_struct_literal_without_annotation_infers_type_args() {
    let struct_decl = kodo_ast::TypeDecl {
        id: NodeId(10),
        span: Span::new(0, 50),
        name: "Pair".to_string(),
        visibility: Visibility::Private,
        generic_params: vec![gp("T")],
        fields: vec![
            kodo_ast::FieldDef {
                name: "first".to_string(),
                ty: TypeExpr::Named("T".to_string()),
                span: Span::new(0, 20),
            },
            kodo_ast::FieldDef {
                name: "second".to_string(),
                ty: TypeExpr::Named("T".to_string()),
                span: Span::new(0, 20),
            },
        ],
    };
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![Stmt::Let {
            span: Span::new(0, 50),
            mutable: false,
            name: "p".to_string(),
            ty: None,
            value: Expr::StructLit {
                name: "Pair".to_string(),
                fields: vec![
                    kodo_ast::FieldInit {
                        name: "first".to_string(),
                        value: Expr::IntLit(1, Span::new(0, 1)),
                        span: Span::new(0, 10),
                    },
                    kodo_ast::FieldInit {
                        name: "second".to_string(),
                        value: Expr::IntLit(2, Span::new(0, 1)),
                        span: Span::new(0, 10),
                    },
                ],
                span: Span::new(0, 40),
            },
        }],
    );
    let module = make_module_with_decls(vec![struct_decl], vec![], vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "generic struct literal without annotation should type-check: {result:?}"
    );
    assert!(
        checker.struct_registry().contains_key("Pair__Int"),
        "Pair__Int should be monomorphized from field inference"
    );
}

/// Static method calls (Type.method() syntax) should resolve through method_lookup
/// without requiring the type name to be a variable in scope.
#[test]
fn static_method_call_on_struct() {
    let module = Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 500),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test static methods".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 90),
            name: "Counter".to_string(),
            visibility: Visibility::Private,
            generic_params: vec![],
            fields: vec![kodo_ast::FieldDef {
                name: "value".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: Span::new(60, 80),
            }],
        }],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![kodo_ast::ImplBlock {
            id: NodeId(2),
            span: Span::new(100, 200),
            trait_name: None,
            type_name: "Counter".to_string(),
            type_bindings: vec![],
            methods: vec![Function {
                id: NodeId(3),
                span: Span::new(110, 190),
                name: "new".to_string(),
                visibility: Visibility::Private,
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![], // No self — static method
                return_type: kodo_ast::TypeExpr::Named("Counter".to_string()),
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(140, 190),
                    stmts: vec![Stmt::Return {
                        span: Span::new(145, 185),
                        value: Some(Expr::StructLit {
                            name: "Counter".to_string(),
                            fields: vec![kodo_ast::FieldInit {
                                name: "value".to_string(),
                                value: Expr::IntLit(0, Span::new(170, 171)),
                                span: Span::new(160, 175),
                            }],
                            span: Span::new(152, 180),
                        }),
                    }],
                },
            }],
        }],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(4),
            span: Span::new(200, 300),
            name: "main".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: kodo_ast::TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: Block {
                span: Span::new(210, 300),
                stmts: vec![Stmt::Let {
                    name: "c".to_string(),
                    ty: Some(kodo_ast::TypeExpr::Named("Counter".to_string())),
                    value: Expr::Call {
                        callee: Box::new(Expr::FieldAccess {
                            object: Box::new(Expr::Ident(
                                "Counter".to_string(),
                                Span::new(230, 237),
                            )),
                            field: "new".to_string(),
                            span: Span::new(230, 241),
                        }),
                        args: vec![],
                        span: Span::new(230, 243),
                    },
                    span: Span::new(220, 250),
                    mutable: false,
                }],
            },
        }],
    };
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "static method call Counter.new() should type-check: {result:?}"
    );
    // Verify method resolution was recorded as static.
    assert!(
        checker.static_method_calls().contains(&230),
        "Counter.new() call should be recorded as a static method call"
    );
    // Verify method resolution maps to mangled name.
    assert_eq!(
        checker.method_resolutions().get(&230),
        Some(&"Counter_new".to_string()),
        "Counter.new() should resolve to Counter_new"
    );
}
