//! Ownership enforcement tests.

use super::*;

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

// --- Closure capture ownership tests ---

#[test]
fn closure_capture_moved_variable_errors() {
    // let s: String = "hello"
    // let t: String = s       // moves s
    // let f = |x: Int| -> Int { println(s); x }  // captures already-moved s
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 20),
                mutable: false,
                name: "s".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hello".to_string(), Span::new(15, 22)),
            },
            Stmt::Let {
                span: Span::new(25, 45),
                mutable: false,
                name: "t".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::Ident("s".to_string(), Span::new(35, 36)),
            },
            Stmt::Let {
                span: Span::new(50, 100),
                mutable: false,
                name: "f".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![kodo_ast::ClosureParam {
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        span: Span::new(55, 61),
                    }],
                    return_type: Some(TypeExpr::Named("Int".to_string())),
                    body: Box::new(Expr::Block(Block {
                        span: Span::new(70, 95),
                        stmts: vec![
                            Stmt::Expr(Expr::Call {
                                callee: Box::new(Expr::Ident(
                                    "println".to_string(),
                                    Span::new(72, 79),
                                )),
                                args: vec![Expr::Ident("s".to_string(), Span::new(80, 81))],
                                span: Span::new(72, 82),
                            }),
                            Stmt::Expr(Expr::Ident("x".to_string(), Span::new(84, 85))),
                        ],
                    })),
                    span: Span::new(55, 95),
                },
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "closure capturing already-moved variable should error"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), "E0281", "expected E0281, got {}", err.code());
}

#[test]
fn closure_capture_owned_marks_moved_in_outer_scope() {
    // let s: String = "hello"
    // let f = |x: Int| -> Int { println(s); x }  // captures s (String, non-Copy) => moves it
    // println(s)  // should error: s was moved by closure
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 20),
                mutable: false,
                name: "s".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hello".to_string(), Span::new(15, 22)),
            },
            Stmt::Let {
                span: Span::new(25, 80),
                mutable: false,
                name: "f".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![kodo_ast::ClosureParam {
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        span: Span::new(30, 36),
                    }],
                    return_type: Some(TypeExpr::Named("Int".to_string())),
                    body: Box::new(Expr::Block(Block {
                        span: Span::new(45, 75),
                        stmts: vec![
                            Stmt::Expr(Expr::Call {
                                callee: Box::new(Expr::Ident(
                                    "println".to_string(),
                                    Span::new(47, 54),
                                )),
                                args: vec![Expr::Ident("s".to_string(), Span::new(55, 56))],
                                span: Span::new(47, 57),
                            }),
                            Stmt::Expr(Expr::Ident("x".to_string(), Span::new(60, 61))),
                        ],
                    })),
                    span: Span::new(30, 75),
                },
            },
            // Use s after closure captured it — should error as use-after-move.
            Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("println".to_string(), Span::new(85, 92))),
                args: vec![Expr::Ident("s".to_string(), Span::new(93, 94))],
                span: Span::new(85, 95),
            }),
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "using variable after closure captured it by move should error"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.code(),
        "E0240",
        "expected use-after-move E0240, got {}",
        err.code()
    );
}

#[test]
fn two_closures_capture_same_non_copy_variable_errors() {
    // let s: String = "hello"
    // let f1 = |x: Int| -> Int { println(s); x }  // moves s
    // let f2 = |x: Int| -> Int { println(s); x }  // tries to capture already-moved s
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 20),
                mutable: false,
                name: "s".to_string(),
                ty: Some(TypeExpr::Named("String".to_string())),
                value: Expr::StringLit("hello".to_string(), Span::new(15, 22)),
            },
            Stmt::Let {
                span: Span::new(25, 80),
                mutable: false,
                name: "f1".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![kodo_ast::ClosureParam {
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        span: Span::new(30, 36),
                    }],
                    return_type: Some(TypeExpr::Named("Int".to_string())),
                    body: Box::new(Expr::Block(Block {
                        span: Span::new(45, 75),
                        stmts: vec![
                            Stmt::Expr(Expr::Call {
                                callee: Box::new(Expr::Ident(
                                    "println".to_string(),
                                    Span::new(47, 54),
                                )),
                                args: vec![Expr::Ident("s".to_string(), Span::new(55, 56))],
                                span: Span::new(47, 57),
                            }),
                            Stmt::Expr(Expr::Ident("x".to_string(), Span::new(60, 61))),
                        ],
                    })),
                    span: Span::new(30, 75),
                },
            },
            Stmt::Let {
                span: Span::new(85, 140),
                mutable: false,
                name: "f2".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![kodo_ast::ClosureParam {
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        span: Span::new(90, 96),
                    }],
                    return_type: Some(TypeExpr::Named("Int".to_string())),
                    body: Box::new(Expr::Block(Block {
                        span: Span::new(105, 135),
                        stmts: vec![
                            Stmt::Expr(Expr::Call {
                                callee: Box::new(Expr::Ident(
                                    "println".to_string(),
                                    Span::new(107, 114),
                                )),
                                args: vec![Expr::Ident("s".to_string(), Span::new(115, 116))],
                                span: Span::new(107, 117),
                            }),
                            Stmt::Expr(Expr::Ident("x".to_string(), Span::new(120, 121))),
                        ],
                    })),
                    span: Span::new(90, 135),
                },
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "two closures capturing same non-Copy variable should error"
    );
    let err = result.unwrap_err();
    // Second closure tries to capture already-moved s.
    // E0281 (ClosureCaptureAfterMove) or E0240 (UseAfterMove) are both valid —
    // the important thing is the error IS detected. The general ownership check
    // may fire before the capture-specific one depending on evaluation order.
    let code = err.code();
    assert!(
        code == "E0281" || code == "E0240",
        "expected E0281 or E0240 (capture/use after move), got {code}",
    );
}

#[test]
fn closure_capture_copy_type_is_fine() {
    // let n: Int = 42
    // let f = |x: Int| -> Int { x + n }  // captures n (Int, Copy)
    // let y: Int = n + 1                  // n still usable
    let func = make_function(
        "main",
        vec![],
        TypeExpr::Unit,
        vec![
            Stmt::Let {
                span: Span::new(0, 15),
                mutable: false,
                name: "n".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::IntLit(42, Span::new(12, 14)),
            },
            Stmt::Let {
                span: Span::new(20, 60),
                mutable: false,
                name: "f".to_string(),
                ty: None,
                value: Expr::Closure {
                    params: vec![kodo_ast::ClosureParam {
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        span: Span::new(25, 31),
                    }],
                    return_type: None,
                    body: Box::new(Expr::BinaryOp {
                        left: Box::new(Expr::Ident("x".to_string(), Span::new(35, 36))),
                        op: BinOp::Add,
                        right: Box::new(Expr::Ident("n".to_string(), Span::new(39, 40))),
                        span: Span::new(35, 40),
                    }),
                    span: Span::new(25, 40),
                },
            },
            // n is still usable because Int is Copy.
            Stmt::Let {
                span: Span::new(65, 85),
                mutable: false,
                name: "y".to_string(),
                ty: Some(TypeExpr::Named("Int".to_string())),
                value: Expr::BinaryOp {
                    left: Box::new(Expr::Ident("n".to_string(), Span::new(75, 76))),
                    op: BinOp::Add,
                    right: Box::new(Expr::IntLit(1, Span::new(79, 80))),
                    span: Span::new(75, 80),
                },
            },
        ],
    );
    let module = make_module(vec![func]);
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "closure capturing Copy type should not move it: {result:?}"
    );
}

#[test]
fn closure_capture_error_has_fix_patch() {
    use kodo_ast::Diagnostic;
    let err = TypeError::ClosureCaptureAfterMove {
        name: "data".to_string(),
        moved_at_line: 5,
        span: Span::new(100, 150),
    };
    assert_eq!(err.code(), "E0281");
    let patch = err.fix_patch();
    assert!(patch.is_some(), "E0281 should have a fix patch");
    let patch = patch.unwrap();
    assert!(
        patch.replacement.contains("ref"),
        "fix patch should suggest ref"
    );
    let suggestion = err.suggestion();
    assert!(
        suggestion.is_some(),
        "E0281 should have a suggestion string"
    );
}

#[test]
fn closure_double_capture_error_has_fix_patch() {
    use kodo_ast::Diagnostic;
    let err = TypeError::ClosureDoubleCapture {
        name: "data".to_string(),
        first_capture_line: 3,
        span: Span::new(200, 250),
    };
    assert_eq!(err.code(), "E0283");
    let patch = err.fix_patch();
    assert!(patch.is_some(), "E0283 should have a fix patch");
    let suggestion = err.suggestion();
    assert!(
        suggestion.is_some(),
        "E0283 should have a suggestion string"
    );
}
