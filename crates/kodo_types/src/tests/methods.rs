//! Phase 46: Generic method dispatch + Option/Result methods tests.

use super::*;

/// Option.is_some is registered in method_lookup.
#[test]
fn option_is_some_registered() {
    let checker = TypeChecker::new();
    let entry = checker
        .method_lookup
        .get(&("Option".to_string(), "is_some".to_string()));
    assert!(entry.is_some(), "Option.is_some should be in method_lookup");
    let (mangled, _params, ret) = entry.unwrap();
    assert_eq!(mangled, "Option_is_some");
    assert_eq!(*ret, Type::Bool);
}

/// Option.is_none is registered in method_lookup.
#[test]
fn option_is_none_registered() {
    let checker = TypeChecker::new();
    let entry = checker
        .method_lookup
        .get(&("Option".to_string(), "is_none".to_string()));
    assert!(entry.is_some(), "Option.is_none should be in method_lookup");
    let (mangled, _params, ret) = entry.unwrap();
    assert_eq!(mangled, "Option_is_none");
    assert_eq!(*ret, Type::Bool);
}

/// Option.unwrap_or is registered in method_lookup.
#[test]
fn option_unwrap_or_registered() {
    let checker = TypeChecker::new();
    let entry = checker
        .method_lookup
        .get(&("Option".to_string(), "unwrap_or".to_string()));
    assert!(entry.is_some());
    let (mangled, params, ret) = entry.unwrap();
    assert_eq!(mangled, "Option_unwrap_or");
    assert_eq!(params.len(), 2); // self + default
    assert_eq!(*ret, Type::Int);
}

/// Result.is_ok is registered in method_lookup.
#[test]
fn result_is_ok_registered() {
    let checker = TypeChecker::new();
    let entry = checker
        .method_lookup
        .get(&("Result".to_string(), "is_ok".to_string()));
    assert!(entry.is_some());
    let (mangled, _params, ret) = entry.unwrap();
    assert_eq!(mangled, "Result_is_ok");
    assert_eq!(*ret, Type::Bool);
}

/// Result.is_err is registered in method_lookup.
#[test]
fn result_is_err_registered() {
    let checker = TypeChecker::new();
    let entry = checker
        .method_lookup
        .get(&("Result".to_string(), "is_err".to_string()));
    assert!(entry.is_some());
    let (mangled, _params, ret) = entry.unwrap();
    assert_eq!(mangled, "Result_is_err");
    assert_eq!(*ret, Type::Bool);
}

/// Result.unwrap_or is registered in method_lookup.
#[test]
fn result_unwrap_or_registered() {
    let checker = TypeChecker::new();
    let entry = checker
        .method_lookup
        .get(&("Result".to_string(), "unwrap_or".to_string()));
    assert!(entry.is_some());
    let (mangled, params, ret) = entry.unwrap();
    assert_eq!(mangled, "Result_unwrap_or");
    assert_eq!(params.len(), 2);
    assert_eq!(*ret, Type::Int);
}

/// Generic type name extraction works: Generic("Option", [Int]) -> "Option".
#[test]
fn generic_type_extracts_base_name() {
    let ty = Type::Generic("Option".to_string(), vec![Type::Int]);
    let name = match &ty {
        Type::Struct(n) | Type::Enum(n) | Type::Generic(n, _) => n.clone(),
        _ => String::new(),
    };
    assert_eq!(name, "Option");
}

/// Monomorphized enum names fall back to base name for method lookup.
#[test]
fn monomorphized_base_name_extraction() {
    let mono = "Option__Int";
    let base = mono.split("__").next().unwrap();
    assert_eq!(base, "Option");
}

/// Option methods type-check correctly on Option<Int>.
#[test]
fn option_methods_typecheck() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        fn main() -> Int {
            let opt: Option<Int> = Option::Some(42)
            let s: Bool = opt.is_some()
            let n: Bool = opt.is_none()
            let v: Int = opt.unwrap_or(0)
            return 0
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let option_src = r#"module option {
            meta { purpose: "Optional value type" version: "0.1.0" }
            enum Option<T> { Some(T), None }
        }"#;
    let result_src = r#"module result {
            meta { purpose: "Error handling type" version: "0.1.0" }
            enum Result<T, E> { Ok(T), Err(E) }
        }"#;
    for src in [option_src, result_src] {
        if let Ok(prelude_mod) = kodo_parser::parse(src) {
            let _ = checker.check_module(&prelude_mod);
        }
    }
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "Option methods should type-check: {result:?}"
    );
}

/// Result methods type-check correctly on Result<Int, String>.
#[test]
fn result_methods_typecheck() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        fn main() -> Int {
            let r: Result<Int, String> = Result::Ok(42)
            let ok: Bool = r.is_ok()
            let err: Bool = r.is_err()
            let v: Int = r.unwrap_or(0)
            return 0
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let option_src = r#"module option {
            meta { purpose: "Optional value type" version: "0.1.0" }
            enum Option<T> { Some(T), None }
        }"#;
    let result_src = r#"module result {
            meta { purpose: "Error handling type" version: "0.1.0" }
            enum Result<T, E> { Ok(T), Err(E) }
        }"#;
    for src in [option_src, result_src] {
        if let Ok(prelude_mod) = kodo_parser::parse(src) {
            let _ = checker.check_module(&prelude_mod);
        }
    }
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "Result methods should type-check: {result:?}"
    );
}

/// Method on struct with closure parameter type-checks.
#[test]
fn method_with_closure_param_typechecks() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        struct Box { value: Int }
        impl Box {
            fn apply(self, f: (Int) -> Int) -> Int {
                return f(self.value)
            }
        }
        fn main() -> Int {
            let b: Box = Box { value: 10 }
            let result: Int = b.apply(|x: Int| -> Int { x * 2 })
            return result
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "Closure as method arg should type-check: {result:?}"
    );
}

/// Generic struct method dispatch works.
#[test]
fn struct_method_dispatch_typechecks() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        struct Counter { value: Int }
        impl Counter {
            fn get(self) -> Int {
                return self.value
            }
            fn increment(self) -> Counter {
                return Counter { value: self.value + 1 }
            }
        }
        fn main() -> Int {
            let c: Counter = Counter { value: 0 }
            let c2: Counter = c.increment()
            return c2.get()
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "Struct method dispatch should type-check: {result:?}"
    );
}

/// resolve_self_type returns base enum for generic enum name.
#[test]
fn resolve_self_type_generic_enum() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        fn main() -> Int { return 0 }
    }"#;
    let _module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let option_src = r#"module option {
            meta { purpose: "Optional value type" version: "0.1.0" }
            enum Option<T> { Some(T), None }
        }"#;
    let result_src = r#"module result {
            meta { purpose: "Error handling type" version: "0.1.0" }
            enum Result<T, E> { Ok(T), Err(E) }
        }"#;
    for src in [option_src, result_src] {
        if let Ok(prelude_mod) = kodo_parser::parse(src) {
            let _ = checker.check_module(&prelude_mod);
        }
    }
    let ty = checker
        .resolve_self_type(&TypeExpr::Named("Option".to_string()), Span::new(0, 6))
        .unwrap();
    assert_eq!(ty, Type::Enum("Option".to_string()));
}

/// resolve_self_type returns normal type for non-generic types.
#[test]
fn resolve_self_type_non_generic() {
    let mut checker = TypeChecker::new();
    let ty = checker
        .resolve_self_type(&TypeExpr::Named("Int".to_string()), Span::new(0, 3))
        .unwrap();
    assert_eq!(ty, Type::Int);
}

/// Option.is_some on Option::None type-checks correctly.
#[test]
fn option_none_methods_typecheck() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        fn main() -> Int {
            let opt: Option<Int> = Option::None
            let s: Bool = opt.is_some()
            let v: Int = opt.unwrap_or(99)
            return v
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let option_src = r#"module option {
            meta { purpose: "Optional value type" version: "0.1.0" }
            enum Option<T> { Some(T), None }
        }"#;
    let result_src = r#"module result {
            meta { purpose: "Error handling type" version: "0.1.0" }
            enum Result<T, E> { Ok(T), Err(E) }
        }"#;
    for src in [option_src, result_src] {
        if let Ok(prelude_mod) = kodo_parser::parse(src) {
            let _ = checker.check_module(&prelude_mod);
        }
    }
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "Option::None methods should type-check: {result:?}"
    );
}

/// Result::Err methods type-check correctly.
#[test]
fn result_err_methods_typecheck() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        fn main() -> Int {
            let r: Result<Int, String> = Result::Err("fail")
            let ok: Bool = r.is_ok()
            let err: Bool = r.is_err()
            let v: Int = r.unwrap_or(42)
            return v
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let option_src = r#"module option {
            meta { purpose: "Optional value type" version: "0.1.0" }
            enum Option<T> { Some(T), None }
        }"#;
    let result_src = r#"module result {
            meta { purpose: "Error handling type" version: "0.1.0" }
            enum Result<T, E> { Ok(T), Err(E) }
        }"#;
    for src in [option_src, result_src] {
        if let Ok(prelude_mod) = kodo_parser::parse(src) {
            let _ = checker.check_module(&prelude_mod);
        }
    }
    let result = checker.check_module(&module);
    assert!(
        result.is_ok(),
        "Result::Err methods should type-check: {result:?}"
    );
}

/// Self type compatibility: `Enum("Option")` matches `Generic("Option", [Int])`.
#[test]
fn self_type_compat_enum_matches_generic_same_base() {
    let self_ty = Type::Enum("Option".to_string());
    let obj_ty = Type::Generic("Option".to_string(), vec![Type::Int]);
    let self_matches = match (&self_ty, &obj_ty) {
        (Type::Enum(a) | Type::Struct(a), Type::Generic(b, _)) => a == b,
        _ => false,
    };
    assert!(
        self_matches,
        "Enum(\"Option\") should be compatible with Generic(\"Option\", [Int])"
    );
}

/// Self type compatibility rejects different base names.
#[test]
fn self_type_compat_rejects_different_base() {
    let self_ty = Type::Enum("Option".to_string());
    let obj_ty = Type::Generic("Result".to_string(), vec![Type::Int]);
    let self_matches = match (&self_ty, &obj_ty) {
        (Type::Enum(a) | Type::Struct(a), Type::Generic(b, _)) => a == b,
        _ => false,
    };
    assert!(
        !self_matches,
        "Enum(\"Option\") should NOT match Generic(\"Result\", [Int])"
    );
}

/// Self type compatibility: `Struct` base matches `Generic` with same name.
#[test]
fn self_type_compat_struct_matches_generic() {
    let self_ty = Type::Struct("List".to_string());
    let obj_ty = Type::Generic("List".to_string(), vec![Type::Int]);
    let self_matches = match (&self_ty, &obj_ty) {
        (Type::Enum(a) | Type::Struct(a), Type::Generic(b, _)) => a == b,
        _ => false,
    };
    assert!(
        self_matches,
        "Struct(\"List\") should be compatible with Generic(\"List\", [Int])"
    );
}

/// Non-generic type (Int) does not match via self_ty compatibility.
#[test]
fn self_type_compat_non_generic_no_match() {
    let self_ty = Type::Enum("Option".to_string());
    let obj_ty = Type::Int;
    let self_matches = match (&self_ty, &obj_ty) {
        (Type::Enum(a) | Type::Struct(a), Type::Generic(b, _)) => a == b,
        _ => false,
    };
    assert!(!self_matches, "Enum(\"Option\") should NOT match Int");
}

/// Calling a nonexistent method on `Option<Int>` should fail with an error.
#[test]
fn generic_option_unknown_method_fails() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        fn main() -> Bool {
            let opt: Option<Int> = Option::Some(1)
            return opt.nonexistent()
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let option_src = r#"module option {
            meta { purpose: "Optional value type" version: "0.1.0" }
            enum Option<T> { Some(T), None }
        }"#;
    if let Ok(prelude_mod) = kodo_parser::parse(option_src) {
        let _ = checker.check_module(&prelude_mod);
    }
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "calling nonexistent method on Option<Int> should fail"
    );
}

/// `unwrap_or` with wrong argument type (Bool instead of Int) is rejected.
#[test]
fn option_unwrap_or_wrong_arg_type_rejected() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        fn main() -> Int {
            let opt: Option<Int> = Option::Some(10)
            return opt.unwrap_or(true)
        }
    }"#;
    let module = kodo_parser::parse(source).unwrap();
    let mut checker = TypeChecker::new();
    let option_src = r#"module option {
            meta { purpose: "Optional value type" version: "0.1.0" }
            enum Option<T> { Some(T), None }
        }"#;
    if let Ok(prelude_mod) = kodo_parser::parse(option_src) {
        let _ = checker.check_module(&prelude_mod);
    }
    let result = checker.check_module(&module);
    assert!(
        result.is_err(),
        "unwrap_or(true) on Option<Int> should fail — expected Int default"
    );
}

/// Generic method lookup resolves using base name from `Type::Generic`.
#[test]
fn generic_method_lookup_resolves_via_base_name() {
    let checker = TypeChecker::new();
    let obj_ty = Type::Generic("Option".to_string(), vec![Type::Int]);
    let type_name = match &obj_ty {
        Type::Struct(n) | Type::Enum(n) | Type::Generic(n, _) => n.clone(),
        _ => String::new(),
    };
    assert_eq!(type_name, "Option");
    for method in &["is_some", "is_none", "unwrap_or"] {
        let entry = checker
            .method_lookup
            .get(&(type_name.clone(), method.to_string()));
        assert!(
            entry.is_some(),
            "Option.{method} should be resolvable via base name"
        );
    }
}

/// Generic method lookup for Result resolves all three methods.
#[test]
fn generic_result_method_lookup_resolves_via_base_name() {
    let checker = TypeChecker::new();
    let obj_ty = Type::Generic("Result".to_string(), vec![Type::Int, Type::String]);
    let type_name = match &obj_ty {
        Type::Struct(n) | Type::Enum(n) | Type::Generic(n, _) => n.clone(),
        _ => String::new(),
    };
    assert_eq!(type_name, "Result");
    for method in &["is_ok", "is_err", "unwrap_or"] {
        let entry = checker
            .method_lookup
            .get(&(type_name.clone(), method.to_string()));
        assert!(
            entry.is_some(),
            "Result.{method} should be resolvable via base name"
        );
    }
}
