//! Type error definitions for the Kōdo type checker.
//!
//! Contains the [`TypeError`] enum and all its implementations including
//! [`Display`], [`Diagnostic`], [`span()`], [`code()`], and [`suggestion()`].
//! Error codes are in the E0200–E0299 range for type system errors,
//! with E0350 for policy violations.

use kodo_ast::Span;
use thiserror::Error;

/// Errors produced by the type checker.
#[derive(Debug, Error)]
pub enum TypeError {
    /// Two types were expected to be equal but differ.
    #[error("type mismatch: expected `{expected}`, found `{found}` at {span:?}")]
    Mismatch {
        /// The expected type.
        expected: String,
        /// The actual type found.
        found: String,
        /// Source location of the mismatch.
        span: Span,
    },
    /// A name was not found in the type environment.
    #[error("undefined type `{name}` at {span:?}")]
    Undefined {
        /// The undefined type name.
        name: String,
        /// Source location.
        span: Span,
        /// A similar name found via Levenshtein distance, if any.
        similar: Option<String>,
    },
    /// A function was called with the wrong number of arguments.
    #[error("expected {expected} arguments, found {found} at {span:?}")]
    ArityMismatch {
        /// Expected argument count.
        expected: usize,
        /// Actual argument count.
        found: usize,
        /// Source location.
        span: Span,
    },
    /// A value was called as a function but is not a function type.
    #[error("not callable: type `{found}` is not a function at {span:?}")]
    NotCallable {
        /// The actual type found.
        found: String,
        /// Source location.
        span: Span,
    },
    /// Module is missing a required `meta` block.
    #[error("module is missing a required `meta` block")]
    MissingMeta,
    /// The `purpose` field in the meta block is empty.
    #[error("meta block has empty `purpose` field at {span:?}")]
    EmptyPurpose {
        /// Source location.
        span: Span,
    },
    /// The `purpose` field is missing from the meta block.
    #[error("meta block is missing required `purpose` field at {span:?}")]
    MissingPurpose {
        /// Source location of the meta block.
        span: Span,
    },
    /// A trust policy violation was detected.
    #[error("{message} at {span:?}")]
    PolicyViolation {
        /// Description of the violation.
        message: String,
        /// Source location of the offending function.
        span: Span,
    },
    /// A struct type was referenced but not defined.
    #[error("unknown struct `{name}` at {span:?}")]
    UnknownStruct {
        /// The struct name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A required field is missing from a struct literal.
    #[error("missing field `{field}` in struct `{struct_name}` at {span:?}")]
    MissingStructField {
        /// The missing field name.
        field: String,
        /// The struct type name.
        struct_name: String,
        /// Source location.
        span: Span,
    },
    /// An extra field was provided in a struct literal.
    #[error("unknown field `{field}` in struct `{struct_name}` at {span:?}")]
    ExtraStructField {
        /// The extra field name.
        field: String,
        /// The struct type name.
        struct_name: String,
        /// Source location.
        span: Span,
        /// A similar field name found via Levenshtein distance.
        similar: Option<String>,
    },
    /// A field was specified more than once in a struct literal.
    #[error("duplicate field `{field}` in struct `{struct_name}` at {span:?}")]
    DuplicateStructField {
        /// The duplicate field name.
        field: String,
        /// The struct type name.
        struct_name: String,
        /// Source location.
        span: Span,
    },
    /// A field access was attempted on a non-existent field.
    #[error("no field `{field}` on type `{type_name}` at {span:?}")]
    NoSuchField {
        /// The field name.
        field: String,
        /// The type name.
        type_name: String,
        /// Source location.
        span: Span,
        /// A similar field name found via Levenshtein distance.
        similar: Option<String>,
    },
    /// An enum type was referenced but not defined.
    #[error("unknown enum `{name}` at {span:?}")]
    UnknownEnum {
        /// The enum name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A variant was referenced that does not exist in the enum.
    #[error("unknown variant `{variant}` in enum `{enum_name}` at {span:?}")]
    UnknownVariant {
        /// The variant name.
        variant: String,
        /// The enum type name.
        enum_name: String,
        /// Source location.
        span: Span,
        /// A similar variant name found via Levenshtein distance.
        similar: Option<String>,
    },
    /// A match expression does not cover all variants of an enum.
    #[error("non-exhaustive match on `{enum_name}`: missing variants {missing:?} at {span:?}")]
    NonExhaustiveMatch {
        /// The enum type name.
        enum_name: String,
        /// The variants not covered by any arm.
        missing: Vec<String>,
        /// Source location.
        span: Span,
    },
    /// A generic type was instantiated with the wrong number of type arguments.
    #[error("expected {expected} type argument(s) for `{name}`, found {found} at {span:?}")]
    WrongTypeArgCount {
        /// The generic type name.
        name: String,
        /// Expected number of type arguments.
        expected: usize,
        /// Actual number of type arguments.
        found: usize,
        /// Source location.
        span: Span,
    },
    /// A type parameter was referenced but not defined.
    #[error("undefined type parameter `{name}` at {span:?}")]
    UndefinedTypeParam {
        /// The type parameter name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A generic type was used without type arguments.
    #[error("generic type `{name}` requires type arguments at {span:?}")]
    MissingTypeArgs {
        /// The generic type name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// The try operator `?` was used in a function that does not return Result.
    #[error("operator `?` can only be used in functions returning Result at {span:?}")]
    TryInNonResultFn {
        /// Source location.
        span: Span,
    },
    /// Optional chaining `?.` was used on a non-Option type.
    #[error("optional chaining `?.` requires Option type, found `{found}` at {span:?}")]
    OptionalChainOnNonOption {
        /// The type found instead of Option.
        found: String,
        /// Source location.
        span: Span,
    },
    /// Null coalescing `??` type mismatch.
    #[error("null coalescing type mismatch: left must be Option, found `{found}` at {span:?}")]
    CoalesceTypeMismatch {
        /// The type found instead of Option.
        found: String,
        /// Source location.
        span: Span,
    },
    /// A closure parameter is missing a type annotation.
    #[error("closure parameter `{name}` requires a type annotation at {span:?}")]
    ClosureParamTypeMissing {
        /// The parameter name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A trait was referenced but not defined.
    #[error("unknown trait `{name}` at {span:?}")]
    UnknownTrait {
        /// The trait name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A required method from a trait is missing in an impl block.
    #[error("missing trait method `{method}` for trait `{trait_name}` at {span:?}")]
    MissingTraitMethod {
        /// The missing method name.
        method: String,
        /// The trait name.
        trait_name: String,
        /// Source location.
        span: Span,
    },
    /// A required associated type from a trait is missing in an impl block.
    #[error(
        "missing associated type `{assoc_type}` in impl block for trait `{trait_name}` at {span:?}"
    )]
    MissingAssociatedType {
        /// The missing associated type name.
        assoc_type: String,
        /// The trait name.
        trait_name: String,
        /// Source location.
        span: Span,
    },
    /// An associated type was provided in an impl block but is not declared in the trait.
    #[error("unexpected associated type `{assoc_type}` in impl block for trait `{trait_name}` at {span:?}")]
    UnexpectedAssociatedType {
        /// The unexpected associated type name.
        assoc_type: String,
        /// The trait name.
        trait_name: String,
        /// Source location.
        span: Span,
    },
    /// A concrete type does not satisfy a trait bound on a generic parameter.
    #[error("type `{concrete_type}` does not implement trait `{trait_name}` required by generic parameter `{param}` at {span:?}")]
    TraitBoundNotSatisfied {
        /// The concrete type that was provided.
        concrete_type: String,
        /// The trait that is required.
        trait_name: String,
        /// The generic parameter name.
        param: String,
        /// Source location.
        span: Span,
    },
    /// A method was called on a type that does not have it.
    #[error("no method `{method}` on type `{type_name}` at {span:?}")]
    MethodNotFound {
        /// The method name.
        method: String,
        /// The type name.
        type_name: String,
        /// Source location.
        span: Span,
        /// A similar method name found via Levenshtein distance.
        similar: Option<String>,
    },
    /// An `await` expression was used outside an `async fn`.
    #[error("`.await` can only be used inside an `async fn` at {span:?}")]
    AwaitOutsideAsync {
        /// Source location.
        span: Span,
    },
    /// A `spawn` block captures a mutable reference (reserved for future use).
    #[error("spawn block cannot capture mutable reference `{name}` at {span:?}")]
    SpawnCaptureMutableRef {
        /// The captured variable name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A `spawn` block captures a value whose type is not safe to send between threads.
    ///
    /// Borrowed references (`ref`) cannot be safely transferred across thread boundaries
    /// because the original value might be deallocated or modified concurrently.
    #[error("type `{type_name}` of variable `{name}` cannot be sent between threads at {span:?}")]
    SpawnCaptureNonSend {
        /// The captured variable name.
        name: String,
        /// The type that is not Send.
        type_name: String,
        /// Source location.
        span: Span,
    },
    /// Direct field access on an actor (fields are private to handlers).
    #[error("cannot access actor field `{field}` directly on `{actor_name}` at {span:?}")]
    ActorDirectFieldAccess {
        /// The field name.
        field: String,
        /// The actor type name.
        actor_name: String,
        /// Source location.
        span: Span,
    },
    /// A function with low `@confidence` is missing a `@reviewed_by(human: "...")` annotation.
    ///
    /// When `@confidence(X)` with X < 0.8 is present, a human review is required
    /// to ensure agent-generated code meets quality standards.
    #[error("function `{name}` has @confidence({confidence}) < 0.8 and is missing `@reviewed_by(human: \"...\")` at {span:?}")]
    LowConfidenceWithoutReview {
        /// The function name.
        name: String,
        /// The confidence value found.
        confidence: String,
        /// Source location of the function.
        span: Span,
    },
    /// Module confidence is below the declared `min_confidence` threshold.
    ///
    /// The overall module confidence is computed transitively: if function A calls
    /// function B with lower confidence, A's effective confidence is min(A, B).
    /// This error fires when any top-level function's computed confidence falls
    /// below the `min_confidence` declared in the module's `meta` block.
    #[error("module confidence {computed} is below threshold {threshold}. Weakest link: fn `{weakest_fn}` at @confidence({weakest_confidence})")]
    ConfidenceThreshold {
        /// The computed overall confidence.
        computed: String,
        /// The declared threshold.
        threshold: String,
        /// The function that is the weakest link.
        weakest_fn: String,
        /// The confidence of the weakest function.
        weakest_confidence: String,
        /// Source location.
        span: Span,
    },
    /// A `@security_sensitive` function is missing contract clauses.
    ///
    /// Functions marked `@security_sensitive` must have at least one `requires`
    /// or `ensures` clause to document and enforce security invariants.
    #[error("function `{name}` is marked `@security_sensitive` but has no `requires` or `ensures` contracts at {span:?}")]
    SecuritySensitiveWithoutContract {
        /// The function name.
        name: String,
        /// Source location of the function.
        span: Span,
    },
    /// A variable was used after its ownership was moved.
    ///
    /// Once a value is moved (e.g. passed to a function taking `own`),
    /// it can no longer be accessed. Use `ref` to borrow instead.
    #[error("cannot use `{name}` after move (moved at line {moved_at_line}) at {span:?}")]
    UseAfterMove {
        /// The variable name.
        name: String,
        /// The line where the move occurred.
        moved_at_line: u32,
        /// Source location of the invalid use.
        span: Span,
    },
    /// Cannot borrow a variable as mutable while it is already immutably borrowed.
    ///
    /// Mutable borrows are exclusive — no other references can exist.
    #[error("cannot borrow `{name}` as mutable while it is immutably borrowed at {span:?}")]
    MutBorrowWhileRefBorrowed {
        /// The variable name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// Cannot borrow a variable as immutable while it is already mutably borrowed.
    ///
    /// A mutable borrow is exclusive — no shared references are allowed.
    #[error("cannot borrow `{name}` as immutable while it is mutably borrowed at {span:?}")]
    RefBorrowWhileMutBorrowed {
        /// The variable name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// Cannot borrow a variable mutably more than once at the same time.
    #[error("cannot borrow `{name}` as mutable more than once at a time at {span:?}")]
    DoubleMutBorrow {
        /// The variable name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// Cannot assign to a variable through an immutable reference.
    #[error("cannot assign to `{name}` through immutable reference at {span:?}")]
    AssignThroughRef {
        /// The variable name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A borrowed reference cannot escape the scope that created it.
    ///
    /// The original value might be deallocated when the scope ends,
    /// leaving a dangling reference.
    #[error("reference to `{name}` cannot escape the current scope at {span:?}")]
    BorrowEscapesScope {
        /// The variable name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A value cannot be moved while it is currently borrowed.
    ///
    /// The borrow must end (go out of scope) before the value can be moved.
    #[error("cannot move `{name}` while it is borrowed at {span:?}")]
    MoveWhileBorrowed {
        /// The variable name.
        name: String,
        /// Source location.
        span: Span,
    },
    /// A `break` statement used outside of a loop.
    #[error("`break` outside of loop at {span:?}")]
    BreakOutsideLoop {
        /// Source location.
        span: Span,
    },
    /// A `continue` statement used outside of a loop.
    #[error("`continue` outside of loop at {span:?}")]
    ContinueOutsideLoop {
        /// Source location.
        span: Span,
    },
    /// A tuple index is out of bounds.
    #[error("tuple index {index} is out of bounds for tuple of length {length} at {span:?}")]
    TupleIndexOutOfBounds {
        /// The index that was used.
        index: usize,
        /// The actual tuple length.
        length: usize,
        /// Source location.
        span: Span,
    },
    /// A module invariant condition is not of type `Bool`.
    #[error("invariant condition must be `Bool`, found `{found}` at {span:?}")]
    InvariantNotBool {
        /// The actual type found.
        found: String,
        /// Source location of the invariant.
        span: Span,
    },
}

impl TypeError {
    /// Returns the source span of this error, if available.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Mismatch { span, .. }
            | Self::Undefined { span, .. }
            | Self::ArityMismatch { span, .. }
            | Self::NotCallable { span, .. }
            | Self::EmptyPurpose { span, .. }
            | Self::MissingPurpose { span, .. }
            | Self::PolicyViolation { span, .. }
            | Self::UnknownStruct { span, .. }
            | Self::MissingStructField { span, .. }
            | Self::ExtraStructField { span, .. }
            | Self::DuplicateStructField { span, .. }
            | Self::NoSuchField { span, .. }
            | Self::UnknownEnum { span, .. }
            | Self::UnknownVariant { span, .. }
            | Self::NonExhaustiveMatch { span, .. }
            | Self::WrongTypeArgCount { span, .. }
            | Self::UndefinedTypeParam { span, .. }
            | Self::MissingTypeArgs { span, .. }
            | Self::TryInNonResultFn { span, .. }
            | Self::OptionalChainOnNonOption { span, .. }
            | Self::CoalesceTypeMismatch { span, .. }
            | Self::ClosureParamTypeMissing { span, .. }
            | Self::UnknownTrait { span, .. }
            | Self::MissingTraitMethod { span, .. }
            | Self::MissingAssociatedType { span, .. }
            | Self::UnexpectedAssociatedType { span, .. }
            | Self::TraitBoundNotSatisfied { span, .. }
            | Self::MethodNotFound { span, .. }
            | Self::AwaitOutsideAsync { span, .. }
            | Self::SpawnCaptureMutableRef { span, .. }
            | Self::SpawnCaptureNonSend { span, .. }
            | Self::ActorDirectFieldAccess { span, .. }
            | Self::LowConfidenceWithoutReview { span, .. }
            | Self::ConfidenceThreshold { span, .. }
            | Self::SecuritySensitiveWithoutContract { span, .. }
            | Self::UseAfterMove { span, .. }
            | Self::MutBorrowWhileRefBorrowed { span, .. }
            | Self::RefBorrowWhileMutBorrowed { span, .. }
            | Self::DoubleMutBorrow { span, .. }
            | Self::AssignThroughRef { span, .. }
            | Self::BorrowEscapesScope { span, .. }
            | Self::MoveWhileBorrowed { span, .. }
            | Self::BreakOutsideLoop { span, .. }
            | Self::ContinueOutsideLoop { span, .. }
            | Self::TupleIndexOutOfBounds { span, .. }
            | Self::InvariantNotBool { span, .. } => Some(*span),
            Self::MissingMeta => None,
        }
    }

    /// Returns the unique error code for this error variant.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Mismatch { .. } => "E0200",
            Self::Undefined { .. } => "E0201",
            Self::ArityMismatch { .. } => "E0202",
            Self::NotCallable { .. } => "E0203",
            Self::MissingMeta => "E0210",
            Self::EmptyPurpose { .. } => "E0211",
            Self::MissingPurpose { .. } => "E0212",
            Self::UnknownStruct { .. } => "E0213",
            Self::MissingStructField { .. } => "E0214",
            Self::ExtraStructField { .. } => "E0215",
            Self::DuplicateStructField { .. } => "E0216",
            Self::NoSuchField { .. } => "E0217",
            Self::UnknownEnum { .. } => "E0218",
            Self::UnknownVariant { .. } => "E0219",
            Self::NonExhaustiveMatch { .. } => "E0220",
            Self::WrongTypeArgCount { .. } => "E0221",
            Self::UndefinedTypeParam { .. } => "E0222",
            Self::MissingTypeArgs { .. } => "E0223",
            Self::TryInNonResultFn { .. } => "E0224",
            Self::OptionalChainOnNonOption { .. } => "E0225",
            Self::CoalesceTypeMismatch { .. } => "E0226",
            Self::ClosureParamTypeMissing { .. } => "E0227",
            Self::UnknownTrait { .. } => "E0230",
            Self::MissingTraitMethod { .. } => "E0231",
            Self::MissingAssociatedType { .. } => "E0233",
            Self::UnexpectedAssociatedType { .. } => "E0234",
            Self::TraitBoundNotSatisfied { .. } => "E0232",
            Self::MethodNotFound { .. } => "E0235",
            Self::AwaitOutsideAsync { .. } => "E0250",
            Self::SpawnCaptureMutableRef { .. } => "E0251",
            Self::SpawnCaptureNonSend { .. } => "E0280",
            Self::ActorDirectFieldAccess { .. } => "E0252",
            Self::PolicyViolation { .. } => "E0350",
            Self::LowConfidenceWithoutReview { .. } => "E0260",
            Self::ConfidenceThreshold { .. } => "E0261",
            Self::SecuritySensitiveWithoutContract { .. } => "E0262",
            Self::UseAfterMove { .. } => "E0240",
            Self::BorrowEscapesScope { .. } => "E0241",
            Self::MoveWhileBorrowed { .. } => "E0242",
            Self::MutBorrowWhileRefBorrowed { .. } => "E0245",
            Self::RefBorrowWhileMutBorrowed { .. } => "E0246",
            Self::DoubleMutBorrow { .. } => "E0247",
            Self::AssignThroughRef { .. } => "E0248",
            Self::BreakOutsideLoop { .. } => "E0243",
            Self::ContinueOutsideLoop { .. } => "E0244",
            Self::TupleIndexOutOfBounds { .. } => "E0253",
            Self::InvariantNotBool { .. } => "E0310",
        }
    }
}

impl kodo_ast::Diagnostic for TypeError {
    fn code(&self) -> &'static str {
        self.code()
    }

    fn severity(&self) -> kodo_ast::Severity {
        kodo_ast::Severity::Error
    }

    fn span(&self) -> Option<kodo_ast::Span> {
        self.span()
    }

    fn message(&self) -> String {
        self.to_string()
    }

    fn suggestion(&self) -> Option<String> {
        suggestion_for_error(self)
    }

    fn labels(&self) -> Vec<kodo_ast::DiagnosticLabel> {
        if let Some(span) = self.span() {
            vec![kodo_ast::DiagnosticLabel {
                span,
                message: self.to_string(),
            }]
        } else {
            Vec::new()
        }
    }

    fn fix_patch(&self) -> Option<kodo_ast::FixPatch> {
        fix_patch_for_error(self)
    }
}

/// Returns a machine-applicable fix patch for the given type error.
///
/// Covers ~22 error variants with auto-applicable patches that AI agents
/// can use to fix code without human interpretation.
fn fix_patch_for_error(err: &TypeError) -> Option<kodo_ast::FixPatch> {
    fix_patch_meta_and_policy(err)
        .or_else(|| fix_patch_names_and_fields(err))
        .or_else(|| fix_patch_types_and_ownership(err))
}

/// Fix patches for meta block, confidence, and security errors.
fn fix_patch_meta_and_policy(err: &TypeError) -> Option<kodo_ast::FixPatch> {
    match err {
        TypeError::MissingMeta => Some(kodo_ast::FixPatch {
            description: "add a meta block with a purpose field".to_string(),
            file: String::new(),
            start_offset: 0,
            end_offset: 0,
            replacement: "    meta { purpose: \"TODO: describe this module\" }\n".to_string(),
        }),
        TypeError::EmptyPurpose { span } => Some(kodo_ast::FixPatch {
            description: "provide a non-empty purpose string".to_string(),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: "purpose: \"TODO: describe this module\"".to_string(),
        }),
        TypeError::MissingPurpose { span } => Some(kodo_ast::FixPatch {
            description: "add purpose field to meta block".to_string(),
            file: String::new(),
            start_offset: span.end as usize,
            end_offset: span.end as usize,
            replacement: "\n    purpose: \"TODO: describe this module\"".to_string(),
        }),
        TypeError::LowConfidenceWithoutReview { span, .. } => Some(kodo_ast::FixPatch {
            description: "add @reviewed_by annotation for human review".to_string(),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.start as usize,
            replacement: "@reviewed_by(human: \"reviewer\")\n    ".to_string(),
        }),
        TypeError::SecuritySensitiveWithoutContract { span, .. } => Some(kodo_ast::FixPatch {
            description: "add requires/ensures contract block".to_string(),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.start as usize,
            replacement: "    requires { true }\n    ensures { true }\n".to_string(),
        }),
        _ => None,
    }
}

/// Fix patches for name resolution, struct/enum fields, and trait methods.
fn fix_patch_names_and_fields(err: &TypeError) -> Option<kodo_ast::FixPatch> {
    match err {
        TypeError::Undefined {
            span,
            similar: Some(suggestion),
            ..
        }
        | TypeError::ExtraStructField {
            span,
            similar: Some(suggestion),
            ..
        }
        | TypeError::NoSuchField {
            span,
            similar: Some(suggestion),
            ..
        }
        | TypeError::MethodNotFound {
            span,
            similar: Some(suggestion),
            ..
        }
        | TypeError::UnknownVariant {
            span,
            similar: Some(suggestion),
            ..
        } => Some(kodo_ast::FixPatch {
            description: format!("rename to `{suggestion}`"),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: suggestion.clone(),
        }),
        TypeError::MissingStructField { field, span, .. } => Some(kodo_ast::FixPatch {
            description: format!("add missing field `{field}`"),
            file: String::new(),
            start_offset: span.end as usize,
            end_offset: span.end as usize,
            replacement: format!("\n    {field}: TODO,"),
        }),
        TypeError::DuplicateStructField { field, span, .. } => Some(kodo_ast::FixPatch {
            description: format!("remove duplicate field `{field}`"),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: String::new(),
        }),
        TypeError::NonExhaustiveMatch { missing, span, .. } => {
            use std::fmt::Write;
            let mut arms = String::new();
            for v in missing {
                let _ = writeln!(arms, "        {v} => {{ TODO }},");
            }
            Some(kodo_ast::FixPatch {
                description: format!("add missing match arms for: {}", missing.join(", ")),
                file: String::new(),
                start_offset: span.end as usize,
                end_offset: span.end as usize,
                replacement: format!("\n{arms}"),
            })
        }
        TypeError::MissingTraitMethod {
            method,
            trait_name,
            span,
        } => Some(kodo_ast::FixPatch {
            description: format!("add missing method `{method}` for trait `{trait_name}`"),
            file: String::new(),
            start_offset: span.end as usize,
            end_offset: span.end as usize,
            replacement: format!("\n    fn {method}() {{\n        TODO\n    }}\n"),
        }),
        TypeError::ArityMismatch { expected, span, .. } => {
            let params = vec!["TODO"; *expected].join(", ");
            Some(kodo_ast::FixPatch {
                description: format!("provide {expected} argument(s)"),
                file: String::new(),
                start_offset: span.start as usize,
                end_offset: span.end as usize,
                replacement: format!("({params})"),
            })
        }
        _ => None,
    }
}

/// Fix patches for type mismatches, ownership, and type annotation errors.
fn fix_patch_types_and_ownership(err: &TypeError) -> Option<kodo_ast::FixPatch> {
    match err {
        TypeError::Mismatch { expected, span, .. } => Some(kodo_ast::FixPatch {
            description: format!("change type to `{expected}`"),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: expected.clone(),
        }),
        TypeError::UseAfterMove { name, span, .. } => Some(kodo_ast::FixPatch {
            description: format!("change `{name}` to use `ref` instead of `own`"),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: format!("ref {name}"),
        }),
        TypeError::AssignThroughRef { name, span } => Some(kodo_ast::FixPatch {
            description: format!("change `ref {name}` to `mut {name}`"),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: format!("mut {name}"),
        }),
        TypeError::ClosureParamTypeMissing { name, span } => Some(kodo_ast::FixPatch {
            description: format!("add type annotation to parameter `{name}`"),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: format!("{name}: TODO"),
        }),
        TypeError::TryInNonResultFn { span } => Some(kodo_ast::FixPatch {
            description: "change return type to Result".to_string(),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: "-> Result<TODO, String>".to_string(),
        }),
        TypeError::OptionalChainOnNonOption { found, span } => Some(kodo_ast::FixPatch {
            description: format!("wrap `{found}` in Option"),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: format!("Option<{found}>"),
        }),
        TypeError::MissingTypeArgs { name, span } => Some(kodo_ast::FixPatch {
            description: format!("add type arguments to `{name}`"),
            file: String::new(),
            start_offset: span.start as usize,
            end_offset: span.end as usize,
            replacement: format!("{name}<TODO>"),
        }),
        _ => None,
    }
}

/// Returns a suggestion string for the given type error.
///
/// Each error variant produces a human-readable suggestion to help AI agents
/// and developers fix the issue.
fn suggestion_for_error(err: &TypeError) -> Option<String> {
    suggestion_for_type_mismatch(err)
        .or_else(|| suggestion_for_struct_enum_error(err))
        .or_else(|| suggestion_for_trait_method_error(err))
        .or_else(|| suggestion_for_policy_error(err))
}

/// Suggestions for type mismatch, undefined, arity, and callability errors.
fn suggestion_for_type_mismatch(err: &TypeError) -> Option<String> {
    match err {
        TypeError::Mismatch { expected, .. } => Some(format!(
            "ensure the expression produces a value of type `{expected}`"
        )),
        TypeError::Undefined { name, similar, .. } => {
            if let Some(suggestion) = similar {
                Some(format!(
                    "did you mean `{suggestion}`? (check for typos or declare `{name}` before use)"
                ))
            } else {
                Some(format!("check for typos or declare `{name}` before use"))
            }
        }
        TypeError::ArityMismatch {
            expected, found, ..
        } => Some(format!(
            "the function expects {expected} argument(s), but {found} were provided"
        )),
        TypeError::NotCallable { found, .. } => Some(format!(
            "type `{found}` is not a function and cannot be called"
        )),
        TypeError::WrongTypeArgCount { name, expected, .. } => Some(format!(
            "`{name}` requires exactly {expected} type argument(s)"
        )),
        TypeError::UndefinedTypeParam { name, .. } => Some(format!(
            "declare type parameter `{name}` in the generic parameters list"
        )),
        TypeError::MissingTypeArgs { name, .. } => {
            Some(format!("specify type arguments, e.g. `{name}<Int>`"))
        }
        TypeError::TryInNonResultFn { .. } => Some(
            "the `?` operator can only be used in functions that return `Result<T, E>`".to_string(),
        ),
        TypeError::OptionalChainOnNonOption { .. } => {
            Some("optional chaining `?.` can only be used on `Option<T>` values".to_string())
        }
        TypeError::CoalesceTypeMismatch { .. } => {
            Some("the left-hand side of `??` must be an `Option<T>` value".to_string())
        }
        TypeError::ClosureParamTypeMissing { name, .. } => {
            Some(format!("add a type annotation: `{name}: Type`"))
        }
        TypeError::AwaitOutsideAsync { .. } => {
            Some("move this expression into an `async fn`".to_string())
        }
        TypeError::UseAfterMove { name, .. } => Some(format!(
            "use `ref` instead of `own` to borrow `{name}` without transferring ownership"
        )),
        TypeError::BorrowEscapesScope { name, .. } => Some(format!(
            "return an owned value instead of a reference to `{name}`"
        )),
        TypeError::MoveWhileBorrowed { name, .. } => {
            Some(format!("drop the borrow of `{name}` before moving it"))
        }
        TypeError::MutBorrowWhileRefBorrowed { name, .. } => Some(format!(
            "cannot borrow `{name}` as mutable while it has active immutable references — drop the `ref` borrows first"
        )),
        TypeError::RefBorrowWhileMutBorrowed { name, .. } => Some(format!(
            "cannot borrow `{name}` as immutable while it is mutably borrowed — drop the `mut` borrow first"
        )),
        TypeError::DoubleMutBorrow { name, .. } => Some(format!(
            "only one mutable borrow of `{name}` is allowed at a time — use `ref` for read-only access"
        )),
        TypeError::AssignThroughRef { name, .. } => Some(format!(
            "cannot assign to `{name}` because it is borrowed as immutable — use `mut` instead of `ref`"
        )),
        TypeError::TupleIndexOutOfBounds { length, .. } => Some(format!(
            "valid indices for this tuple are 0..{}",
            length.saturating_sub(1)
        )),
        TypeError::BreakOutsideLoop { .. } => {
            Some("`break` can only be used inside `while`, `for`, or `for-in` loops".to_string())
        }
        TypeError::ContinueOutsideLoop { .. } => {
            Some("`continue` can only be used inside `while`, `for`, or `for-in` loops".to_string())
        }
        _ => None,
    }
}

/// Suggestions for struct, enum, and field-related errors.
fn suggestion_for_struct_enum_error(err: &TypeError) -> Option<String> {
    match err {
        TypeError::MissingMeta => {
            Some("add a `meta {{ purpose: \"...\" }}` block to your module".to_string())
        }
        TypeError::EmptyPurpose { .. } => Some("provide a non-empty purpose string".to_string()),
        TypeError::MissingPurpose { .. } => {
            Some("add `purpose: \"description\"` to the meta block".to_string())
        }
        TypeError::UnknownStruct { name, .. } => Some(format!(
            "define `struct {name} {{ ... }}` or check for typos"
        )),
        TypeError::MissingStructField { field, .. } => {
            Some(format!("add `{field}: <value>` to the struct literal"))
        }
        TypeError::ExtraStructField { field, similar, .. } => {
            if let Some(suggestion) = similar {
                Some(format!(
                    "did you mean `{suggestion}`? (unknown field `{field}`)"
                ))
            } else {
                Some(format!("remove field `{field}` from the struct literal"))
            }
        }
        TypeError::DuplicateStructField { field, .. } => {
            Some(format!("remove the duplicate `{field}` field"))
        }
        TypeError::NoSuchField {
            field,
            type_name,
            similar,
            ..
        } => {
            if let Some(suggestion) = similar {
                Some(format!(
                    "did you mean `{suggestion}`? (type `{type_name}` has no field `{field}`)"
                ))
            } else {
                Some(format!(
                    "type `{type_name}` does not have a field named `{field}`"
                ))
            }
        }
        TypeError::UnknownEnum { name, .. } => {
            Some(format!("define `enum {name} {{ ... }}` or check for typos"))
        }
        TypeError::UnknownVariant {
            variant,
            enum_name,
            similar,
            ..
        } => {
            if let Some(suggestion) = similar {
                Some(format!(
                    "did you mean `{suggestion}`? (enum `{enum_name}` has no variant `{variant}`)"
                ))
            } else {
                Some(format!(
                    "check the variants of `{enum_name}` — `{variant}` is not one"
                ))
            }
        }
        TypeError::NonExhaustiveMatch { missing, .. } => {
            Some(format!("add match arms for: {}", missing.join(", ")))
        }
        _ => None,
    }
}

/// Suggestions for trait, method, and concurrency-related errors.
fn suggestion_for_trait_method_error(err: &TypeError) -> Option<String> {
    match err {
        TypeError::UnknownTrait { name, .. } => Some(format!(
            "define `trait {name} {{ ... }}` or check for typos"
        )),
        TypeError::MissingTraitMethod {
            method, trait_name, ..
        } => Some(format!(
            "add method `{method}` to the impl block for trait `{trait_name}`"
        )),
        TypeError::MissingAssociatedType {
            assoc_type,
            trait_name,
            ..
        } => Some(format!(
            "add `type {assoc_type} = ConcreteType` to the impl block for trait `{trait_name}`"
        )),
        TypeError::UnexpectedAssociatedType {
            assoc_type,
            trait_name,
            ..
        } => Some(format!(
            "trait `{trait_name}` does not declare associated type `{assoc_type}` — remove it"
        )),
        TypeError::TraitBoundNotSatisfied {
            concrete_type,
            trait_name,
            ..
        } => Some(format!(
            "implement trait `{trait_name}` for type `{concrete_type}`, or use a type that already implements it"
        )),
        TypeError::MethodNotFound {
            method,
            type_name,
            similar,
            ..
        } => {
            if let Some(suggestion) = similar {
                Some(format!(
                    "did you mean `{suggestion}`? (type `{type_name}` has no method `{method}`)"
                ))
            } else {
                Some(format!(
                    "type `{type_name}` does not have a method named `{method}`"
                ))
            }
        }
        TypeError::SpawnCaptureMutableRef { name, .. } => Some(format!(
            "spawn blocks cannot capture mutable references like `{name}`"
        )),
        TypeError::SpawnCaptureNonSend { .. } => Some(
            "use owned values (own) instead of references when sending data to spawned tasks"
                .to_string(),
        ),
        TypeError::ActorDirectFieldAccess { field, .. } => {
            Some(format!("use a handler method to access `{field}` instead"))
        }
        _ => None,
    }
}

/// Suggestions for policy, confidence, and security errors.
fn suggestion_for_policy_error(err: &TypeError) -> Option<String> {
    match err {
        TypeError::LowConfidenceWithoutReview { name, .. } => Some(format!(
            "add `@reviewed_by(human: \"reviewer_name\")` to function `{name}`"
        )),
        TypeError::ConfidenceThreshold {
            weakest_fn,
            threshold,
            ..
        } => Some(format!(
            "increase the confidence of `{weakest_fn}` to at least {threshold}, \
             or lower `min_confidence` in the module meta block"
        )),
        TypeError::SecuritySensitiveWithoutContract { name, .. } => Some(format!(
            "add `requires {{ ... }}` or `ensures {{ ... }}` to function `{name}`"
        )),
        TypeError::InvariantNotBool { .. } => {
            Some("invariant conditions must evaluate to `Bool`".to_string())
        }
        _ => None,
    }
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, TypeError>;
