//! Generics (Phase 2) tests.

use super::*;

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
        visibility: Visibility::Private,
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
        visibility: Visibility::Private,
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
        test_decls: vec![],
        describe_decls: vec![],
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
            visibility: Visibility::Private,
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
                visibility: Visibility::Private,
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
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(5),
            span: Span::new(180, 200),
            name: "main".to_string(),
            visibility: Visibility::Private,
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
        test_decls: vec![],
        describe_decls: vec![],
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
        invariants: vec![],
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
        test_decls: vec![],
        describe_decls: vec![],
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
            visibility: Visibility::Private,
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
        invariants: vec![],
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
        test_decls: vec![],
        describe_decls: vec![],
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
            visibility: Visibility::Private,
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
                visibility: Visibility::Private,
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
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(4),
            span: Span::new(160, 200),
            name: "main".to_string(),
            visibility: Visibility::Private,
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
        test_decls: vec![],
        describe_decls: vec![],
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
            visibility: Visibility::Private,
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
                visibility: Visibility::Private,
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
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(4),
            span: Span::new(140, 180),
            name: "main".to_string(),
            visibility: Visibility::Private,
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
        test_decls: vec![],
        describe_decls: vec![],
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
            visibility: Visibility::Private,
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
                    visibility: Visibility::Private,
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
                    visibility: Visibility::Private,
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
        invariants: vec![],
        functions: vec![Function {
            id: NodeId(7),
            span: Span::new(230, 270),
            name: "main".to_string(),
            visibility: Visibility::Private,
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
fn await_in_non_async_fn_is_allowed() {
    // Since async/await is now wired to real green threads, `await` is valid
    // in both `async fn` and regular `fn`. When called from a non-async
    // context the runtime drains the green-thread scheduler until the future
    // completes, so no compile-time restriction is necessary.
    let module = make_module(vec![Function {
        id: NodeId(0),
        span: Span::new(0, 10),
        name: "sync_fn".to_string(),
        visibility: Visibility::Private,
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
    assert!(
        result.is_ok(),
        "await in non-async fn should be allowed: {result:?}"
    );
}

#[test]
fn await_inside_async_is_ok() {
    let module = make_module(vec![Function {
        id: NodeId(0),
        span: Span::new(0, 10),
        name: "async_fn".to_string(),
        visibility: Visibility::Private,
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

#[test]
fn infer_type_param_from_generic_list() {
    // When a generic function has a parameter List<T> and is called with
    // Type::Generic("List", [Int]), T should be inferred as Int.
    let params = vec!["T".to_string()];
    let type_expr = kodo_ast::TypeExpr::Generic(
        "List".to_string(),
        vec![kodo_ast::TypeExpr::Named("T".to_string())],
    );
    let actual = Type::Generic("List".to_string(), vec![Type::Int]);
    let mut inferred = std::collections::HashMap::new();
    TypeChecker::infer_type_param(&type_expr, &actual, &params, &mut inferred);
    assert_eq!(
        inferred.get("T"),
        Some(&Type::Int),
        "T should be inferred as Int from List<Int>"
    );
}

#[test]
fn infer_type_param_from_generic_map() {
    // When a generic function has a parameter Map<K, V> and is called with
    // Type::Generic("Map", [String, Int]), K and V should be inferred.
    let params = vec!["K".to_string(), "V".to_string()];
    let type_expr = kodo_ast::TypeExpr::Generic(
        "Map".to_string(),
        vec![
            kodo_ast::TypeExpr::Named("K".to_string()),
            kodo_ast::TypeExpr::Named("V".to_string()),
        ],
    );
    let actual = Type::Generic("Map".to_string(), vec![Type::String, Type::Int]);
    let mut inferred = std::collections::HashMap::new();
    TypeChecker::infer_type_param(&type_expr, &actual, &params, &mut inferred);
    assert_eq!(
        inferred.get("K"),
        Some(&Type::String),
        "K should be inferred as String from Map<String, Int>"
    );
    assert_eq!(
        inferred.get("V"),
        Some(&Type::Int),
        "V should be inferred as Int from Map<String, Int>"
    );
}
