//! Phase 37 trait bound tests and Phase 43 associated type tests.

use super::*;

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

// --- Phase 43: Associated types and default methods ---

#[test]
fn missing_associated_type_error() {
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
                value: "test associated types".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "MyList".to_string(),
            visibility: Visibility::Private,
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
                visibility: Visibility::Private,
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
        invariants: vec![],
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
                value: "test unexpected associated type".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "MyList".to_string(),
            visibility: Visibility::Private,
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
                visibility: Visibility::Private,
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
        invariants: vec![],
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
    // This is a long test - I'll include the full Module construction
    // from the original tests.rs lines 3997-4118
    let source = r#"module test {
    meta { purpose: "test default methods" version: "1.0.0" }

    trait Greetable {
        fn required_method(self) -> Int
        fn default_method(self) -> Int {
            return 42
        }
    }

    struct Point { x: Int }

    impl Greetable for Point {
        fn required_method(self) -> Int {
            return 1
        }
    }

    fn main() -> Int { return 0 }
}"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn associated_type_provided_passes() {
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
                value: "test passing associated types".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "MyList".to_string(),
            visibility: Visibility::Private,
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
                visibility: Visibility::Private,
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
        invariants: vec![],
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
                value: "test default methods collecting".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Foo".to_string(),
            visibility: Visibility::Private,
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
        invariants: vec![],
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
                value: "test missing assoc type collecting".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls: vec![kodo_ast::TypeDecl {
            id: NodeId(1),
            span: Span::new(50, 80),
            name: "Foo".to_string(),
            visibility: Visibility::Private,
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
        invariants: vec![],
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
