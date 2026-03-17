//! Comprehensive tests for `fix_patch()` on `TypeError`.
//!
//! Verifies that every `TypeError` variant that should produce a `FixPatch`
//! returns `Some` with valid fields, and that variants without patches return `None`.

use kodo_ast::{Diagnostic, FixPatch, Span};
use kodo_types::TypeError;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Asserts that a `FixPatch` has a non-empty description and a non-empty
/// replacement (unless the fix is a deletion, signalled by `allow_empty_replacement`).
fn assert_valid_patch(patch: &FixPatch, allow_empty_replacement: bool) {
    assert!(
        !patch.description.is_empty(),
        "fix_patch description must not be empty"
    );
    if !allow_empty_replacement {
        assert!(
            !patch.replacement.is_empty(),
            "fix_patch replacement must not be empty (description: {})",
            patch.description
        );
    }
    assert!(
        patch.start_offset <= patch.end_offset,
        "start_offset ({}) must be <= end_offset ({})",
        patch.start_offset,
        patch.end_offset,
    );
}

// ===========================================================================
// Meta & policy variants — all return Some
// ===========================================================================

#[test]
fn fix_patch_missing_meta() {
    let err = TypeError::MissingMeta;
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("meta"));
    assert!(patch.replacement.contains("purpose"));
    assert_eq!(patch.start_offset, 0);
    assert_eq!(patch.end_offset, 0);
}

#[test]
fn fix_patch_empty_purpose() {
    let span = Span::new(10, 30);
    let err = TypeError::EmptyPurpose { span };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("purpose"));
    assert_eq!(patch.start_offset, 10);
    assert_eq!(patch.end_offset, 30);
}

#[test]
fn fix_patch_missing_purpose() {
    let span = Span::new(5, 25);
    let err = TypeError::MissingPurpose { span };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("purpose"));
    // Inserts at end of meta block span
    assert_eq!(patch.start_offset, 25);
    assert_eq!(patch.end_offset, 25);
}

#[test]
fn fix_patch_low_confidence_without_review() {
    let span = Span::new(100, 150);
    let err = TypeError::LowConfidenceWithoutReview {
        name: "process_data".to_string(),
        confidence: "0.5".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("@reviewed_by"));
    assert_eq!(patch.start_offset, 100);
    assert_eq!(patch.end_offset, 100);
}

#[test]
fn fix_patch_security_sensitive_without_contract() {
    let span = Span::new(200, 250);
    let err = TypeError::SecuritySensitiveWithoutContract {
        name: "validate_token".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("requires"));
    assert!(patch.replacement.contains("ensures"));
    assert_eq!(patch.start_offset, 200);
    assert_eq!(patch.end_offset, 200);
}

// ===========================================================================
// Name resolution with "similar" suggestions — all return Some
// ===========================================================================

#[test]
fn fix_patch_undefined_with_similar() {
    let span = Span::new(40, 46);
    let err = TypeError::Undefined {
        name: "conut".to_string(),
        span,
        similar: Some("count".to_string()),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert_eq!(patch.replacement, "count");
    assert_eq!(patch.start_offset, 40);
    assert_eq!(patch.end_offset, 46);
}

#[test]
fn fix_patch_undefined_without_similar_returns_none_for_name_patch() {
    // Without a similar suggestion, the names-and-fields pass returns None,
    // but there is no type-level patch either, so the overall result is None.
    let span = Span::new(40, 46);
    let err = TypeError::Undefined {
        name: "xyzzy".to_string(),
        span,
        similar: None,
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_extra_struct_field_with_similar() {
    let span = Span::new(60, 68);
    let err = TypeError::ExtraStructField {
        field: "naem".to_string(),
        struct_name: "User".to_string(),
        span,
        similar: Some("name".to_string()),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert_eq!(patch.replacement, "name");
    assert_eq!(patch.start_offset, 60);
    assert_eq!(patch.end_offset, 68);
}

#[test]
fn fix_patch_extra_struct_field_without_similar() {
    let span = Span::new(60, 68);
    let err = TypeError::ExtraStructField {
        field: "zzz".to_string(),
        struct_name: "User".to_string(),
        span,
        similar: None,
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_no_such_field_with_similar() {
    let span = Span::new(70, 75);
    let err = TypeError::NoSuchField {
        field: "nmae".to_string(),
        type_name: "Point".to_string(),
        span,
        similar: Some("name".to_string()),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert_eq!(patch.replacement, "name");
    assert_eq!(patch.start_offset, 70);
    assert_eq!(patch.end_offset, 75);
}

#[test]
fn fix_patch_no_such_field_without_similar() {
    let span = Span::new(70, 75);
    let err = TypeError::NoSuchField {
        field: "zzz".to_string(),
        type_name: "Point".to_string(),
        span,
        similar: None,
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_method_not_found_with_similar() {
    let span = Span::new(80, 90);
    let err = TypeError::MethodNotFound {
        method: "leng".to_string(),
        type_name: "String".to_string(),
        span,
        similar: Some("length".to_string()),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert_eq!(patch.replacement, "length");
    assert_eq!(patch.start_offset, 80);
    assert_eq!(patch.end_offset, 90);
}

#[test]
fn fix_patch_method_not_found_without_similar() {
    let span = Span::new(80, 90);
    let err = TypeError::MethodNotFound {
        method: "zzz".to_string(),
        type_name: "String".to_string(),
        span,
        similar: None,
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_unknown_variant_with_similar() {
    let span = Span::new(100, 108);
    let err = TypeError::UnknownVariant {
        variant: "Sone".to_string(),
        enum_name: "Option".to_string(),
        span,
        similar: Some("Some".to_string()),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert_eq!(patch.replacement, "Some");
    assert_eq!(patch.start_offset, 100);
    assert_eq!(patch.end_offset, 108);
}

#[test]
fn fix_patch_unknown_variant_without_similar() {
    let span = Span::new(100, 108);
    let err = TypeError::UnknownVariant {
        variant: "Zzz".to_string(),
        enum_name: "Option".to_string(),
        span,
        similar: None,
    };
    assert!(err.fix_patch().is_none());
}

// ===========================================================================
// Struct / match / trait — all return Some
// ===========================================================================

#[test]
fn fix_patch_missing_struct_field() {
    let span = Span::new(50, 80);
    let err = TypeError::MissingStructField {
        field: "age".to_string(),
        struct_name: "User".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("age"));
    // Inserts at end of struct literal span
    assert_eq!(patch.start_offset, 80);
    assert_eq!(patch.end_offset, 80);
}

#[test]
fn fix_patch_duplicate_struct_field() {
    let span = Span::new(55, 65);
    let err = TypeError::DuplicateStructField {
        field: "name".to_string(),
        struct_name: "User".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    // Deletion: replacement is empty
    assert_valid_patch(&patch, true);
    assert!(patch.replacement.is_empty());
    assert_eq!(patch.start_offset, 55);
    assert_eq!(patch.end_offset, 65);
}

#[test]
fn fix_patch_non_exhaustive_match() {
    let span = Span::new(120, 180);
    let err = TypeError::NonExhaustiveMatch {
        enum_name: "Color".to_string(),
        missing: vec!["Red".to_string(), "Blue".to_string()],
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("Red"));
    assert!(patch.replacement.contains("Blue"));
    // Inserts at end of match span
    assert_eq!(patch.start_offset, 180);
    assert_eq!(patch.end_offset, 180);
}

#[test]
fn fix_patch_non_exhaustive_match_single_variant() {
    let span = Span::new(0, 50);
    let err = TypeError::NonExhaustiveMatch {
        enum_name: "Result".to_string(),
        missing: vec!["Err".to_string()],
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("Err"));
}

#[test]
fn fix_patch_missing_trait_method() {
    let span = Span::new(200, 300);
    let err = TypeError::MissingTraitMethod {
        method: "draw".to_string(),
        trait_name: "Drawable".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("fn draw"));
    // Inserts at end of impl block span
    assert_eq!(patch.start_offset, 300);
    assert_eq!(patch.end_offset, 300);
}

#[test]
fn fix_patch_arity_mismatch() {
    let span = Span::new(30, 40);
    let err = TypeError::ArityMismatch {
        expected: 3,
        found: 1,
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("TODO"));
    assert_eq!(patch.start_offset, 30);
    assert_eq!(patch.end_offset, 40);
}

#[test]
fn fix_patch_arity_mismatch_zero_args() {
    let span = Span::new(10, 20);
    let err = TypeError::ArityMismatch {
        expected: 0,
        found: 3,
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert_eq!(patch.replacement, "()");
}

// ===========================================================================
// Type / ownership / annotation variants — all return Some
// ===========================================================================

#[test]
fn fix_patch_mismatch() {
    let span = Span::new(10, 20);
    let err = TypeError::Mismatch {
        expected: "Int".to_string(),
        found: "String".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert_eq!(patch.replacement, "Int");
    assert_eq!(patch.start_offset, 10);
    assert_eq!(patch.end_offset, 20);
}

#[test]
fn fix_patch_use_after_move() {
    let span = Span::new(50, 60);
    let err = TypeError::UseAfterMove {
        name: "data".to_string(),
        moved_at_line: 5,
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("ref"));
    assert!(patch.replacement.contains("data"));
    assert_eq!(patch.start_offset, 50);
    assert_eq!(patch.end_offset, 60);
}

#[test]
fn fix_patch_assign_through_ref() {
    let span = Span::new(70, 80);
    let err = TypeError::AssignThroughRef {
        name: "counter".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("mut"));
    assert!(patch.replacement.contains("counter"));
    assert_eq!(patch.start_offset, 70);
    assert_eq!(patch.end_offset, 80);
}

#[test]
fn fix_patch_closure_param_type_missing() {
    let span = Span::new(30, 35);
    let err = TypeError::ClosureParamTypeMissing {
        name: "x".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("x"));
    assert!(patch.replacement.contains("TODO"));
    assert_eq!(patch.start_offset, 30);
    assert_eq!(patch.end_offset, 35);
}

#[test]
fn fix_patch_try_in_non_result_fn() {
    let span = Span::new(90, 100);
    let err = TypeError::TryInNonResultFn { span };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("Result"));
    assert_eq!(patch.start_offset, 90);
    assert_eq!(patch.end_offset, 100);
}

#[test]
fn fix_patch_optional_chain_on_non_option() {
    let span = Span::new(40, 50);
    let err = TypeError::OptionalChainOnNonOption {
        found: "Int".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("Option"));
    assert!(patch.replacement.contains("Int"));
    assert_eq!(patch.start_offset, 40);
    assert_eq!(patch.end_offset, 50);
}

#[test]
fn fix_patch_missing_type_args() {
    let span = Span::new(20, 24);
    let err = TypeError::MissingTypeArgs {
        name: "List".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("List"));
    assert!(patch.replacement.contains("TODO"));
    assert_eq!(patch.start_offset, 20);
    assert_eq!(patch.end_offset, 24);
}

#[test]
fn fix_patch_wrong_type_arg_count() {
    let span = Span::new(30, 42);
    let err = TypeError::WrongTypeArgCount {
        name: "Map".to_string(),
        expected: 2,
        found: 1,
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("Map"));
    assert!(patch.replacement.contains("TODO"));
    assert_eq!(patch.start_offset, 30);
    assert_eq!(patch.end_offset, 42);
}

#[test]
fn fix_patch_missing_associated_type() {
    let span = Span::new(100, 200);
    let err = TypeError::MissingAssociatedType {
        assoc_type: "Item".to_string(),
        trait_name: "Iterator".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("type Item"));
    // Inserts at end of impl block span
    assert_eq!(patch.start_offset, 200);
    assert_eq!(patch.end_offset, 200);
}

#[test]
fn fix_patch_unexpected_associated_type() {
    let span = Span::new(110, 140);
    let err = TypeError::UnexpectedAssociatedType {
        assoc_type: "Value".to_string(),
        trait_name: "Iterator".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    // Deletion: replacement is empty
    assert_valid_patch(&patch, true);
    assert!(patch.replacement.is_empty());
    assert_eq!(patch.start_offset, 110);
    assert_eq!(patch.end_offset, 140);
}

#[test]
fn fix_patch_move_while_borrowed() {
    let span = Span::new(60, 70);
    let err = TypeError::MoveWhileBorrowed {
        name: "buffer".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("ref"));
    assert!(patch.replacement.contains("buffer"));
    assert_eq!(patch.start_offset, 60);
    assert_eq!(patch.end_offset, 70);
}

#[test]
fn fix_patch_invariant_not_bool() {
    let span = Span::new(300, 310);
    let err = TypeError::InvariantNotBool {
        found: "Int".to_string(),
        span,
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("true"));
    assert_eq!(patch.start_offset, 300);
    assert_eq!(patch.end_offset, 310);
}

// ===========================================================================
// Variants that must return None
// ===========================================================================

#[test]
fn fix_patch_not_callable_returns_some() {
    let err = TypeError::NotCallable {
        found: "Int".to_string(),
        span: Span::new(0, 5),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, true);
}

#[test]
fn fix_patch_policy_violation_returns_none() {
    let err = TypeError::PolicyViolation {
        message: "policy broken".to_string(),
        span: Span::new(0, 10),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_unknown_struct_returns_none() {
    let err = TypeError::UnknownStruct {
        name: "Foo".to_string(),
        span: Span::new(0, 3),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_unknown_enum_returns_none() {
    let err = TypeError::UnknownEnum {
        name: "Bar".to_string(),
        span: Span::new(0, 3),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_unknown_trait_returns_none() {
    let err = TypeError::UnknownTrait {
        name: "Baz".to_string(),
        span: Span::new(0, 3),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_undefined_type_param_returns_none() {
    let err = TypeError::UndefinedTypeParam {
        name: "T".to_string(),
        span: Span::new(0, 1),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_coalesce_type_mismatch_returns_none() {
    let err = TypeError::CoalesceTypeMismatch {
        found: "Int".to_string(),
        span: Span::new(0, 5),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_trait_bound_not_satisfied_returns_none() {
    let err = TypeError::TraitBoundNotSatisfied {
        concrete_type: "MyType".to_string(),
        trait_name: "Ord".to_string(),
        param: "T".to_string(),
        span: Span::new(0, 10),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_await_outside_async_returns_some() {
    let err = TypeError::AwaitOutsideAsync {
        span: Span::new(0, 5),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("async"));
}

#[test]
fn fix_patch_spawn_capture_mutable_ref_returns_some() {
    let err = TypeError::SpawnCaptureMutableRef {
        name: "x".to_string(),
        span: Span::new(0, 5),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("ref x"));
}

#[test]
fn fix_patch_spawn_capture_non_send_returns_none() {
    let err = TypeError::SpawnCaptureNonSend {
        name: "data".to_string(),
        type_name: "ref String".to_string(),
        span: Span::new(0, 10),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_actor_direct_field_access_returns_none() {
    let err = TypeError::ActorDirectFieldAccess {
        field: "count".to_string(),
        actor_name: "Counter".to_string(),
        span: Span::new(0, 10),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_confidence_threshold_returns_none() {
    let err = TypeError::ConfidenceThreshold {
        computed: "0.6".to_string(),
        threshold: "0.8".to_string(),
        weakest_fn: "helper".to_string(),
        weakest_confidence: "0.6".to_string(),
        span: Span::new(0, 10),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_mut_borrow_while_ref_borrowed_returns_none() {
    let err = TypeError::MutBorrowWhileRefBorrowed {
        name: "x".to_string(),
        span: Span::new(0, 5),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_ref_borrow_while_mut_borrowed_returns_none() {
    let err = TypeError::RefBorrowWhileMutBorrowed {
        name: "x".to_string(),
        span: Span::new(0, 5),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_double_mut_borrow_returns_none() {
    let err = TypeError::DoubleMutBorrow {
        name: "x".to_string(),
        span: Span::new(0, 5),
    };
    assert!(err.fix_patch().is_none());
}

#[test]
fn fix_patch_borrow_escapes_scope_returns_some() {
    let err = TypeError::BorrowEscapesScope {
        name: "temp".to_string(),
        span: Span::new(0, 5),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("clone()"));
}

#[test]
fn fix_patch_break_outside_loop_returns_some() {
    let err = TypeError::BreakOutsideLoop {
        span: Span::new(0, 5),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, true); // deletion - empty replacement
}

#[test]
fn fix_patch_continue_outside_loop_returns_some() {
    let err = TypeError::ContinueOutsideLoop {
        span: Span::new(0, 8),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, true); // deletion - empty replacement
}

#[test]
fn fix_patch_tuple_index_out_of_bounds_returns_some() {
    let err = TypeError::TupleIndexOutOfBounds {
        index: 5,
        length: 3,
        span: Span::new(0, 5),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains(".2")); // max valid index = length - 1 = 2
}

#[test]
fn fix_patch_private_access_returns_some() {
    let err = TypeError::PrivateAccess {
        name: "secret".to_string(),
        defining_module: "internal".to_string(),
        span: Span::new(0, 10),
    };
    let patch = err.fix_patch().unwrap();
    assert_valid_patch(&patch, false);
    assert!(patch.replacement.contains("pub"));
}
