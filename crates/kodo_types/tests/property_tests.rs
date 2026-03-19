//! Property-based tests for the Kōdo type system.
//!
//! Uses `proptest` to verify invariants of the type system, type environment,
//! and type checker behavior. These tests exercise edge cases that unit tests
//! may miss, such as:
//! - Type equality is reflexive, symmetric, and transitive
//! - `is_numeric` and `is_copy` are consistent
//! - TypeEnv scoping behaves correctly under arbitrary insertion/lookup sequences
//! - The type checker never panics on valid AST structures

use kodo_ast::Span;
use kodo_types::{Type, TypeEnv};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Type generators
// ---------------------------------------------------------------------------

/// Generates an arbitrary primitive type.
fn arb_primitive_type() -> impl Strategy<Value = Type> {
    prop_oneof![
        Just(Type::Int),
        Just(Type::Int8),
        Just(Type::Int16),
        Just(Type::Int32),
        Just(Type::Int64),
        Just(Type::Uint),
        Just(Type::Uint8),
        Just(Type::Uint16),
        Just(Type::Uint32),
        Just(Type::Uint64),
        Just(Type::Float32),
        Just(Type::Float64),
        Just(Type::Bool),
        Just(Type::String),
        Just(Type::Byte),
        Just(Type::Unit),
    ]
}

/// Generates an arbitrary type (primitive, struct, enum, or generic).
fn arb_type() -> impl Strategy<Value = Type> {
    let leaf = prop_oneof![
        arb_primitive_type(),
        "[a-zA-Z][a-zA-Z0-9]{0,10}".prop_map(Type::Struct),
        "[a-zA-Z][a-zA-Z0-9]{0,10}".prop_map(Type::Enum),
        Just(Type::Unknown),
    ];

    leaf.prop_recursive(
        3,  // max depth
        16, // max nodes
        4,  // items per collection
        |inner| {
            prop_oneof![
                // Generic types like List<Int>, Map<String, Int>
                (
                    "[A-Z][a-zA-Z]{0,8}",
                    proptest::collection::vec(inner.clone(), 1..=3)
                )
                    .prop_map(|(name, args)| Type::Generic(name, args)),
                // Function types like (Int, Bool) -> String
                (
                    proptest::collection::vec(inner.clone(), 0..=4),
                    inner.clone()
                )
                    .prop_map(|(params, ret)| Type::Function(params, Box::new(ret))),
                // Tuple types like (Int, String)
                proptest::collection::vec(inner, 2..=4).prop_map(Type::Tuple),
            ]
        },
    )
}

/// Generates a valid identifier for variable names.
fn arb_identifier() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,15}"
}

// ---------------------------------------------------------------------------
// Type equality properties
// ---------------------------------------------------------------------------

proptest! {
    /// Type equality is reflexive: `T == T` for all types.
    #[test]
    fn type_eq_reflexive(ty in arb_type()) {
        prop_assert_eq!(&ty, &ty);
    }

    /// Type equality is symmetric: if `T == U` then `U == T`.
    #[test]
    fn type_eq_symmetric(a in arb_type(), b in arb_type()) {
        prop_assert_eq!(a == b, b == a);
    }

    /// Clone preserves equality.
    #[test]
    fn type_clone_preserves_eq(ty in arb_type()) {
        let cloned = ty.clone();
        prop_assert_eq!(&ty, &cloned);
    }
}

// ---------------------------------------------------------------------------
// Type classification properties
// ---------------------------------------------------------------------------

proptest! {
    /// All numeric types have `is_numeric() == true`.
    #[test]
    fn numeric_types_are_numeric(ty in prop_oneof![
        Just(Type::Int), Just(Type::Int8), Just(Type::Int16),
        Just(Type::Int32), Just(Type::Int64), Just(Type::Uint),
        Just(Type::Uint8), Just(Type::Uint16), Just(Type::Uint32),
        Just(Type::Uint64), Just(Type::Float32), Just(Type::Float64),
    ]) {
        prop_assert!(ty.is_numeric());
    }

    /// Non-numeric types have `is_numeric() == false`.
    #[test]
    fn non_numeric_types_are_not_numeric(ty in prop_oneof![
        Just(Type::Bool), Just(Type::String), Just(Type::Byte),
        Just(Type::Unit), Just(Type::Unknown),
        "[a-zA-Z]{1,8}".prop_map(Type::Struct),
        "[a-zA-Z]{1,8}".prop_map(Type::Enum),
    ]) {
        prop_assert!(!ty.is_numeric());
    }

    /// All numeric types are also copy types.
    #[test]
    fn numeric_implies_copy(ty in arb_type()) {
        if ty.is_numeric() {
            prop_assert!(ty.is_copy(), "numeric type {:?} should be copy", ty);
        }
    }

    /// Bool, Byte, Unit are always copy.
    #[test]
    fn primitive_non_numeric_are_copy(ty in prop_oneof![
        Just(Type::Bool), Just(Type::Byte), Just(Type::Unit),
    ]) {
        prop_assert!(ty.is_copy());
    }

    /// String, Struct, Enum, Tuple are NOT copy.
    /// Function types ARE copy (they are just function pointers).
    #[test]
    fn compound_types_are_not_copy(ty in prop_oneof![
        Just(Type::String),
        "[a-zA-Z]{1,8}".prop_map(Type::Struct),
        "[a-zA-Z]{1,8}".prop_map(Type::Enum),
        Just(Type::Tuple(vec![Type::Int, Type::String])),
    ]) {
        prop_assert!(!ty.is_copy(), "type {:?} should not be copy", ty);
    }
}

// ---------------------------------------------------------------------------
// Type display roundtrip properties
// ---------------------------------------------------------------------------

proptest! {
    /// Display never panics for any type.
    #[test]
    fn display_never_panics(ty in arb_type()) {
        let _ = format!("{ty}");
    }

    /// Display produces a non-empty string for all types.
    #[test]
    fn display_non_empty(ty in arb_type()) {
        let s = format!("{ty}");
        prop_assert!(!s.is_empty(), "display should produce non-empty string for {:?}", ty);
    }

    /// Primitive types display to their expected names.
    #[test]
    fn primitive_display_correct(
        (ty, expected) in prop_oneof![
            Just((Type::Int, "Int")),
            Just((Type::Int8, "Int8")),
            Just((Type::Int16, "Int16")),
            Just((Type::Int32, "Int32")),
            Just((Type::Int64, "Int64")),
            Just((Type::Uint, "Uint")),
            Just((Type::Float32, "Float32")),
            Just((Type::Float64, "Float64")),
            Just((Type::Bool, "Bool")),
            Just((Type::String, "String")),
            Just((Type::Byte, "Byte")),
            Just((Type::Unit, "()")),
        ]
    ) {
        prop_assert_eq!(format!("{ty}"), expected);
    }
}

// ---------------------------------------------------------------------------
// TypeEnv properties
// ---------------------------------------------------------------------------

proptest! {
    /// Inserting a binding makes it immediately findable.
    #[test]
    fn env_insert_then_lookup(name in arb_identifier(), ty in arb_primitive_type()) {
        let mut env = TypeEnv::new();
        env.insert(name.clone(), ty.clone());
        let found = env.lookup(&name);
        prop_assert_eq!(found, Some(&ty));
    }

    /// Later bindings shadow earlier ones with the same name.
    #[test]
    fn env_shadowing(
        name in arb_identifier(),
        ty1 in arb_primitive_type(),
        ty2 in arb_primitive_type()
    ) {
        let mut env = TypeEnv::new();
        env.insert(name.clone(), ty1);
        env.insert(name.clone(), ty2.clone());
        let found = env.lookup(&name);
        prop_assert_eq!(found, Some(&ty2), "later binding should shadow earlier one");
    }

    /// Truncating restores the environment to a previous state.
    #[test]
    fn env_scope_restore(
        name in arb_identifier(),
        ty in arb_primitive_type()
    ) {
        let mut env = TypeEnv::new();
        let level = env.scope_level();
        env.insert(name.clone(), ty);
        prop_assert!(env.lookup(&name).is_some());
        env.truncate(level);
        prop_assert!(env.lookup(&name).is_none(), "binding should be removed after truncate");
    }

    /// Multiple scopes nest correctly.
    #[test]
    fn env_nested_scopes(
        outer_name in "[a-z]{3,5}",
        inner_name in "[a-z]{6,8}",
        outer_ty in arb_primitive_type(),
        inner_ty in arb_primitive_type()
    ) {
        let mut env = TypeEnv::new();
        // Outer scope
        env.insert(outer_name.clone(), outer_ty.clone());
        let level1 = env.scope_level();
        // Inner scope
        env.insert(inner_name.clone(), inner_ty);
        prop_assert!(env.lookup(&inner_name).is_some());
        prop_assert!(env.lookup(&outer_name).is_some());
        // Leave inner scope
        env.truncate(level1);
        prop_assert!(env.lookup(&inner_name).is_none());
        prop_assert_eq!(env.lookup(&outer_name), Some(&outer_ty));
    }

    /// Lookup on unknown name returns None.
    #[test]
    fn env_lookup_unknown(name in arb_identifier()) {
        let env = TypeEnv::new();
        prop_assert!(env.lookup(&name).is_none());
    }

    /// Arbitrary sequence of inserts never panics.
    #[test]
    fn env_arbitrary_inserts(
        bindings in proptest::collection::vec(
            (arb_identifier(), arb_primitive_type()),
            0..=50
        )
    ) {
        let mut env = TypeEnv::new();
        for (name, ty) in &bindings {
            env.insert(name.clone(), ty.clone());
        }
        // Last binding for each name should be found
        for (name, _) in &bindings {
            prop_assert!(env.lookup(name).is_some());
        }
    }
}

// ---------------------------------------------------------------------------
// TypeEnv::check_eq properties
// ---------------------------------------------------------------------------

proptest! {
    /// check_eq succeeds for equal types.
    #[test]
    fn check_eq_same_type(ty in arb_type()) {
        let span = Span { start: 0, end: 1 };
        let result = TypeEnv::check_eq(&ty, &ty, span);
        prop_assert!(result.is_ok(), "same types should be equal");
    }

    /// check_eq fails for clearly different types.
    #[test]
    fn check_eq_different_types(
        (a, b) in prop_oneof![
            Just((Type::Int, Type::String)),
            Just((Type::Bool, Type::Float64)),
            Just((Type::String, Type::Int)),
            Just((Type::Unit, Type::Bool)),
            Just((Type::Byte, Type::Int)),
        ]
    ) {
        let span = Span { start: 0, end: 1 };
        let result = TypeEnv::check_eq(&a, &b, span);
        prop_assert!(result.is_err(), "different types {:?} and {:?} should not be equal", a, b);
    }

    /// Channel<T> is compatible with Int (special rule for opaque handles).
    #[test]
    fn check_eq_channel_int_compatible(inner in arb_primitive_type()) {
        let span = Span { start: 0, end: 1 };
        let channel = Type::Generic("Channel".to_string(), vec![inner]);
        // Channel<T> accepted where Int expected
        let result1 = TypeEnv::check_eq(&Type::Int, &channel, span);
        prop_assert!(result1.is_ok(), "Channel<T> should be compatible with Int");
        // Int accepted where Channel<T> expected
        let result2 = TypeEnv::check_eq(&channel, &Type::Int, span);
        prop_assert!(result2.is_ok(), "Int should be compatible with Channel<T>");
    }

    /// dyn Trait accepts any concrete type.
    #[test]
    fn check_eq_dyn_trait_accepts_any(
        concrete in arb_type(),
        trait_name in "[A-Z][a-zA-Z]{0,8}"
    ) {
        let span = Span { start: 0, end: 1 };
        let dyn_trait = Type::DynTrait(trait_name);
        let result = TypeEnv::check_eq(&dyn_trait, &concrete, span);
        prop_assert!(result.is_ok(), "dyn Trait should accept any concrete type");
    }
}

// ---------------------------------------------------------------------------
// TypeChecker never-panic property
// ---------------------------------------------------------------------------

proptest! {
    /// The type checker never panics on well-formed modules.
    /// Tests various valid Kōdo programs and ensures no panic occurs.
    #[test]
    fn type_checker_never_panics_on_valid_int_return(val in -1000i64..1000) {
        let source = format!(
            r#"module test {{
    meta {{ version: "1.0" }}
    fn main() -> Int {{
        return {val}
    }}
}}"#
        );
        if let Ok(module) = kodo_parser::parse(&source) {
            let mut checker = kodo_types::TypeChecker::new();
            // We don't care if it passes or fails — just that it doesn't panic
            let _ = checker.check_module(&module);
        }
    }

    /// The type checker never panics on programs with boolean operations.
    #[test]
    fn type_checker_never_panics_on_bool_ops(a in any::<bool>(), b in any::<bool>()) {
        let a_str = if a { "true" } else { "false" };
        let b_str = if b { "true" } else { "false" };
        let source = format!(
            r#"module test {{
    meta {{ version: "1.0" }}
    fn main() -> Bool {{
        let x: Bool = {a_str}
        let y: Bool = {b_str}
        return x
    }}
}}"#
        );
        if let Ok(module) = kodo_parser::parse(&source) {
            let mut checker = kodo_types::TypeChecker::new();
            let _ = checker.check_module(&module);
        }
    }

    /// The type checker never panics on arithmetic expressions.
    #[test]
    fn type_checker_never_panics_on_arithmetic(
        a in -100i64..100,
        b in 1i64..100,
        op in prop_oneof![Just("+"), Just("-"), Just("*"), Just("/"), Just("%")]
    ) {
        let source = format!(
            r#"module test {{
    meta {{ version: "1.0" }}
    fn main() -> Int {{
        let result: Int = {a} {op} {b}
        return result
    }}
}}"#
        );
        if let Ok(module) = kodo_parser::parse(&source) {
            let mut checker = kodo_types::TypeChecker::new();
            let _ = checker.check_module(&module);
        }
    }

    /// The type checker handles type mismatches gracefully (no panic).
    #[test]
    fn type_checker_no_panic_on_mismatch(val in -1000i64..1000) {
        let source = format!(
            r#"module test {{
    meta {{ version: "1.0" }}
    fn main() -> String {{
        return {val}
    }}
}}"#
        );
        if let Ok(module) = kodo_parser::parse(&source) {
            let mut checker = kodo_types::TypeChecker::new();
            let result = checker.check_module(&module);
            // Should fail with a type error, not panic
            prop_assert!(result.is_err());
        }
    }
}
