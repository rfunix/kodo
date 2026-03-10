//! # `kodo_types` — Type System for the Kōdo Language
//!
//! This crate defines the type representation and type checker for Kōdo.
//! Kōdo's type system is explicitly typed with no implicit conversions or
//! coercions — every value has exactly one type known at compile time.
//!
//! Designed for AI agents: no type inference across module boundaries,
//! all types are explicit in function signatures, making it trivially
//! machine-parseable and verifiable.
//!
//! ## Key Types
//!
//! - [`Type`] — The core type representation
//! - [`TypeEnv`] — Type environment for checking
//! - [`TypeChecker`] — Walks the AST and verifies type correctness
//! - [`TypeError`] — Structured type errors with source locations
//!
//! ## Academic References
//!
//! - **\[TAPL\]** *Types and Programming Languages* Ch. 1–11 — Type safety
//!   (progress + preservation), simply typed lambda calculus, and the
//!   theoretical basis for Kōdo's "no implicit conversions" rule.
//! - **\[TAPL\]** *Types and Programming Languages* Ch. 22–26 — System F
//!   and bounded quantification, informing Kōdo's generic type system.
//! - **\[ATAPL\]** *Advanced Topics in Types and PL* Ch. 1 — Linear and affine
//!   type systems, the foundation for `own`/`ref`/`mut` ownership semantics.
//! - **\[PLP\]** *Programming Language Pragmatics* Ch. 7–8 — Type checking
//!   algorithms, type equivalence, and parametric polymorphism.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use kodo_ast::{
    Annotation, AnnotationArg, BinOp, Block, Expr, Function, Module, Pattern, Span, Stmt, UnaryOp,
};
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
    #[error(
        "variable `{name}` was moved at line {moved_at_line} and cannot be used here at {span:?}"
    )]
    UseAfterMove {
        /// The variable name.
        name: String,
        /// The line where the move occurred.
        moved_at_line: u32,
        /// Source location of the invalid use.
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
            | Self::MethodNotFound { span, .. }
            | Self::AwaitOutsideAsync { span, .. }
            | Self::SpawnCaptureMutableRef { span, .. }
            | Self::ActorDirectFieldAccess { span, .. }
            | Self::LowConfidenceWithoutReview { span, .. }
            | Self::ConfidenceThreshold { span, .. }
            | Self::SecuritySensitiveWithoutContract { span, .. }
            | Self::UseAfterMove { span, .. }
            | Self::BorrowEscapesScope { span, .. }
            | Self::MoveWhileBorrowed { span, .. } => Some(*span),
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
            Self::MethodNotFound { .. } => "E0235",
            Self::AwaitOutsideAsync { .. } => "E0250",
            Self::SpawnCaptureMutableRef { .. } => "E0251",
            Self::ActorDirectFieldAccess { .. } => "E0252",
            Self::PolicyViolation { .. } => "E0350",
            Self::LowConfidenceWithoutReview { .. } => "E0260",
            Self::ConfidenceThreshold { .. } => "E0261",
            Self::SecuritySensitiveWithoutContract { .. } => "E0262",
            Self::UseAfterMove { .. } => "E0240",
            Self::BorrowEscapesScope { .. } => "E0241",
            Self::MoveWhileBorrowed { .. } => "E0242",
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

    #[allow(clippy::too_many_lines)]
    fn suggestion(&self) -> Option<String> {
        match self {
            Self::Mismatch { expected, .. } => Some(format!(
                "ensure the expression produces a value of type `{expected}`"
            )),
            Self::Undefined { name, similar, .. } => {
                if let Some(suggestion) = similar {
                    Some(format!("did you mean `{suggestion}`? (check for typos or declare `{name}` before use)"))
                } else {
                    Some(format!("check for typos or declare `{name}` before use"))
                }
            }
            Self::ArityMismatch {
                expected, found, ..
            } => Some(format!(
                "the function expects {expected} argument(s), but {found} were provided"
            )),
            Self::NotCallable { found, .. } => Some(format!(
                "type `{found}` is not a function and cannot be called"
            )),
            Self::MissingMeta => {
                Some("add a `meta { purpose: \"...\" }` block to your module".to_string())
            }
            Self::EmptyPurpose { .. } => Some("provide a non-empty purpose string".to_string()),
            Self::MissingPurpose { .. } => {
                Some("add `purpose: \"description\"` to the meta block".to_string())
            }
            Self::UnknownStruct { name, .. } => Some(format!(
                "define `struct {name} {{ ... }}` or check for typos"
            )),
            Self::MissingStructField { field, .. } => {
                Some(format!("add `{field}: <value>` to the struct literal"))
            }
            Self::ExtraStructField { field, similar, .. } => {
                if let Some(suggestion) = similar {
                    Some(format!(
                        "did you mean `{suggestion}`? (unknown field `{field}`)"
                    ))
                } else {
                    Some(format!("remove field `{field}` from the struct literal"))
                }
            }
            Self::DuplicateStructField { field, .. } => {
                Some(format!("remove the duplicate `{field}` field"))
            }
            Self::NoSuchField {
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
            Self::UnknownEnum { name, .. } => {
                Some(format!("define `enum {name} {{ ... }}` or check for typos"))
            }
            Self::UnknownVariant {
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
            Self::NonExhaustiveMatch { missing, .. } => {
                Some(format!("add match arms for: {}", missing.join(", ")))
            }
            Self::WrongTypeArgCount { name, expected, .. } => Some(format!(
                "`{name}` requires exactly {expected} type argument(s)"
            )),
            Self::UndefinedTypeParam { name, .. } => Some(format!(
                "declare type parameter `{name}` in the generic parameters list"
            )),
            Self::MissingTypeArgs { name, .. } => {
                Some(format!("specify type arguments, e.g. `{name}<Int>`"))
            }
            Self::TryInNonResultFn { .. } => Some(
                "the `?` operator can only be used in functions that return `Result<T, E>`"
                    .to_string(),
            ),
            Self::OptionalChainOnNonOption { .. } => {
                Some("optional chaining `?.` can only be used on `Option<T>` values".to_string())
            }
            Self::CoalesceTypeMismatch { .. } => {
                Some("the left-hand side of `??` must be an `Option<T>` value".to_string())
            }
            Self::ClosureParamTypeMissing { name, .. } => {
                Some(format!("add a type annotation: `{name}: Type`"))
            }
            Self::UnknownTrait { name, .. } => Some(format!(
                "define `trait {name} {{ ... }}` or check for typos"
            )),
            Self::MissingTraitMethod {
                method, trait_name, ..
            } => Some(format!(
                "add method `{method}` to the impl block for trait `{trait_name}`"
            )),
            Self::MethodNotFound {
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
            Self::AwaitOutsideAsync { .. } => {
                Some("move this expression into an `async fn`".to_string())
            }
            Self::SpawnCaptureMutableRef { name, .. } => Some(format!(
                "spawn blocks cannot capture mutable references like `{name}`"
            )),
            Self::ActorDirectFieldAccess { field, .. } => {
                Some(format!("use a handler method to access `{field}` instead"))
            }
            Self::PolicyViolation { .. } => None,
            Self::LowConfidenceWithoutReview { name, .. } => Some(format!(
                "add `@reviewed_by(human: \"reviewer_name\")` to function `{name}`"
            )),
            Self::ConfidenceThreshold {
                weakest_fn,
                threshold,
                ..
            } => Some(format!(
                "increase the confidence of `{weakest_fn}` to at least {threshold}, \
                 or lower `min_confidence` in the module meta block"
            )),
            Self::SecuritySensitiveWithoutContract { name, .. } => Some(format!(
                "add `requires {{ ... }}` or `ensures {{ ... }}` to function `{name}`"
            )),
            Self::UseAfterMove { name, .. } => Some(format!(
                "use `ref` instead of `own` to borrow `{name}` without transferring ownership"
            )),
            Self::BorrowEscapesScope { name, .. } => Some(format!(
                "return an owned value instead of a reference to `{name}`"
            )),
            Self::MoveWhileBorrowed { name, .. } => {
                Some(format!("drop the borrow of `{name}` before moving it"))
            }
        }
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
        match self {
            Self::MissingMeta => Some(kodo_ast::FixPatch {
                description: "add a meta block with a purpose field".to_string(),
                file: std::string::String::new(),
                start_offset: 0,
                end_offset: 0,
                replacement: "    meta { purpose: \"TODO: describe this module\" }\n".to_string(),
            }),
            Self::EmptyPurpose { span } => Some(kodo_ast::FixPatch {
                description: "provide a non-empty purpose string".to_string(),
                file: std::string::String::new(),
                start_offset: span.start as usize,
                end_offset: span.end as usize,
                replacement: "purpose: \"TODO: describe this module\"".to_string(),
            }),
            Self::LowConfidenceWithoutReview { span, .. } => Some(kodo_ast::FixPatch {
                description: "add @reviewed_by annotation for human review".to_string(),
                file: std::string::String::new(),
                start_offset: span.start as usize,
                end_offset: span.start as usize,
                replacement: "@reviewed_by(human: \"reviewer\")\n    ".to_string(),
            }),
            _ => None,
        }
    }
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, TypeError>;

/// Represents a type in the Kōdo type system.
///
/// Kōdo has no null — `Option<T>` is the only way to represent absence.
/// No implicit conversions exist between any types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// A signed integer type (default `Int` is 64-bit).
    Int,
    /// 8-bit signed integer.
    Int8,
    /// 16-bit signed integer.
    Int16,
    /// 32-bit signed integer.
    Int32,
    /// 64-bit signed integer.
    Int64,
    /// An unsigned integer type.
    Uint,
    /// 8-bit unsigned integer.
    Uint8,
    /// 16-bit unsigned integer.
    Uint16,
    /// 32-bit unsigned integer.
    Uint32,
    /// 64-bit unsigned integer.
    Uint64,
    /// 32-bit floating point.
    Float32,
    /// 64-bit floating point.
    Float64,
    /// Boolean type.
    Bool,
    /// UTF-8 string type.
    String,
    /// Single byte.
    Byte,
    /// The unit type (void equivalent, but is a value).
    Unit,
    /// A user-defined struct type.
    Struct(std::string::String),
    /// A user-defined enum type.
    Enum(std::string::String),
    /// A generic type application, e.g. `List<Int>`.
    Generic(std::string::String, Vec<Type>),
    /// A function type: `(params) -> return_type`.
    Function(Vec<Type>, Box<Type>),
    /// An unresolved type (used during type checking).
    Unknown,
}

impl Type {
    /// Returns `true` if the type is a numeric type (integer, unsigned, or float).
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::Int
                | Self::Int8
                | Self::Int16
                | Self::Int32
                | Self::Int64
                | Self::Uint
                | Self::Uint8
                | Self::Uint16
                | Self::Uint32
                | Self::Uint64
                | Self::Float32
                | Self::Float64
        )
    }

    /// Returns `true` if the type has implicit Copy semantics.
    ///
    /// Primitive types (integers, floats, booleans, bytes, unit) are implicitly
    /// copied rather than moved, similar to Rust's `Copy` trait.
    #[must_use]
    pub fn is_copy(&self) -> bool {
        matches!(
            self,
            Self::Int
                | Self::Int8
                | Self::Int16
                | Self::Int32
                | Self::Int64
                | Self::Uint
                | Self::Uint8
                | Self::Uint16
                | Self::Uint32
                | Self::Uint64
                | Self::Float32
                | Self::Float64
                | Self::Bool
                | Self::Byte
                | Self::Unit
                | Self::Generic(..)
        )
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int => write!(f, "Int"),
            Self::Int8 => write!(f, "Int8"),
            Self::Int16 => write!(f, "Int16"),
            Self::Int32 => write!(f, "Int32"),
            Self::Int64 => write!(f, "Int64"),
            Self::Uint => write!(f, "Uint"),
            Self::Uint8 => write!(f, "Uint8"),
            Self::Uint16 => write!(f, "Uint16"),
            Self::Uint32 => write!(f, "Uint32"),
            Self::Uint64 => write!(f, "Uint64"),
            Self::Float32 => write!(f, "Float32"),
            Self::Float64 => write!(f, "Float64"),
            Self::Bool => write!(f, "Bool"),
            Self::String => write!(f, "String"),
            Self::Byte => write!(f, "Byte"),
            Self::Unit => write!(f, "()"),
            Self::Struct(name) | Self::Enum(name) => write!(f, "{name}"),
            Self::Generic(name, args) => {
                write!(f, "{name}<")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ">")
            }
            Self::Function(params, ret) => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                write!(f, ") -> {ret}")
            }
            Self::Unknown => write!(f, "?"),
        }
    }
}

/// A type environment that maps names to their types.
///
/// Uses a `Vec` with reverse lookup to support shadowing.
/// Scoping is managed by saving and restoring the environment length.
#[derive(Debug, Default)]
pub struct TypeEnv {
    bindings: Vec<(std::string::String, Type)>,
}

impl TypeEnv {
    /// Creates an empty type environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a binding into the environment.
    pub fn insert(&mut self, name: std::string::String, ty: Type) {
        self.bindings.push((name, ty));
    }

    /// Looks up a name in the environment.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.bindings
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t)
    }

    /// Returns the current number of bindings, used for scope management.
    ///
    /// Save this value before entering a scope and pass it to
    /// [`truncate`](Self::truncate) when leaving.
    #[must_use]
    pub fn scope_level(&self) -> usize {
        self.bindings.len()
    }

    /// Removes all bindings added after the given scope level.
    ///
    /// Used to restore the environment when leaving a scope.
    pub fn truncate(&mut self, level: usize) {
        self.bindings.truncate(level);
    }

    /// Returns an iterator over all unique binding names currently in scope.
    ///
    /// Used for suggesting similar names via Levenshtein distance.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.bindings.iter().map(|(n, _)| n.as_str())
    }

    /// Checks that two types are equal, returning an error if not.
    ///
    /// # Errors
    ///
    /// Returns [`TypeError::Mismatch`] if the types differ.
    pub fn check_eq(expected: &Type, found: &Type, span: Span) -> Result<()> {
        if expected == found {
            Ok(())
        } else {
            Err(TypeError::Mismatch {
                expected: expected.to_string(),
                found: found.to_string(),
                span,
            })
        }
    }
}

/// Resolves an AST type expression to a concrete [`Type`].
///
/// This is a convenience wrapper around [`resolve_type_with_enums`] that
/// treats all unknown named types as structs.
///
/// # Errors
///
/// Returns [`TypeError::Undefined`] if the type name is not recognized.
pub fn resolve_type(type_expr: &kodo_ast::TypeExpr, span: Span) -> Result<Type> {
    resolve_type_with_enums(type_expr, span, &std::collections::HashSet::new())
}

/// Resolves an AST type expression to a concrete [`Type`], distinguishing
/// enum types from struct types using the provided set of known enum names.
///
/// # Errors
///
/// Returns [`TypeError::Undefined`] if the type name is not recognized.
#[allow(clippy::only_used_in_recursion, clippy::implicit_hasher)]
pub fn resolve_type_with_enums(
    type_expr: &kodo_ast::TypeExpr,
    span: Span,
    enum_names: &std::collections::HashSet<std::string::String>,
) -> Result<Type> {
    match type_expr {
        kodo_ast::TypeExpr::Named(name) => match name.as_str() {
            "Int" => Ok(Type::Int),
            "Int8" => Ok(Type::Int8),
            "Int16" => Ok(Type::Int16),
            "Int32" => Ok(Type::Int32),
            "Int64" => Ok(Type::Int64),
            "Uint" => Ok(Type::Uint),
            "Uint8" => Ok(Type::Uint8),
            "Uint16" => Ok(Type::Uint16),
            "Uint32" => Ok(Type::Uint32),
            "Uint64" => Ok(Type::Uint64),
            "Float32" => Ok(Type::Float32),
            "Float64" => Ok(Type::Float64),
            "Bool" => Ok(Type::Bool),
            "String" => Ok(Type::String),
            "Byte" => Ok(Type::Byte),
            _ => {
                if enum_names.contains(name) {
                    Ok(Type::Enum(name.clone()))
                } else {
                    Ok(Type::Struct(name.clone()))
                }
            }
        },
        kodo_ast::TypeExpr::Unit => Ok(Type::Unit),
        kodo_ast::TypeExpr::Generic(name, args) => {
            let resolved: std::result::Result<Vec<_>, _> = args
                .iter()
                .map(|a| resolve_type_with_enums(a, span, enum_names))
                .collect();
            Ok(Type::Generic(name.clone(), resolved?))
        }
        kodo_ast::TypeExpr::Optional(inner) => {
            // T? is sugar for Option<T>
            let generic =
                kodo_ast::TypeExpr::Generic("Option".to_string(), vec![(**inner).clone()]);
            resolve_type_with_enums(&generic, span, enum_names)
        }
        kodo_ast::TypeExpr::Function(params, ret) => {
            let resolved_params: std::result::Result<Vec<_>, _> = params
                .iter()
                .map(|p| resolve_type_with_enums(p, span, enum_names))
                .collect();
            let resolved_ret = resolve_type_with_enums(ret, span, enum_names)?;
            Ok(Type::Function(resolved_params?, Box::new(resolved_ret)))
        }
    }
}

/// Finds the most similar name among candidates using Levenshtein distance.
///
/// Returns `Some(name)` if a candidate within the distance threshold is found.
/// The threshold is `max(name.len() / 2, 3)`, ensuring reasonable fuzzy matching.
fn find_similar_in<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    let threshold = std::cmp::max(name.len() / 2, 3);
    for candidate in candidates {
        let dist = strsim::levenshtein(name, candidate);
        if dist > 0 && dist <= threshold && best.as_ref().map_or(true, |(d, _)| dist < *d) {
            best = Some((dist, candidate.to_string()));
        }
    }
    best.map(|(_, n)| n)
}

/// Extracts the source [`Span`] from an expression.
fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::IntLit(_, span)
        | Expr::FloatLit(_, span)
        | Expr::StringLit(_, span)
        | Expr::BoolLit(_, span)
        | Expr::Ident(_, span)
        | Expr::BinaryOp { span, .. }
        | Expr::UnaryOp { span, .. }
        | Expr::Call { span, .. }
        | Expr::If { span, .. }
        | Expr::FieldAccess { span, .. }
        | Expr::StructLit { span, .. }
        | Expr::EnumVariantExpr { span, .. }
        | Expr::Match { span, .. }
        | Expr::Try { span, .. }
        | Expr::OptionalChain { span, .. }
        | Expr::NullCoalesce { span, .. }
        | Expr::Range { span, .. }
        | Expr::Closure { span, .. }
        | Expr::Is { span, .. }
        | Expr::Await { span, .. } => *span,
        Expr::Block(block) => block.span,
    }
}

/// Extracts the expression from an annotation argument.
fn annotation_arg_expr(arg: &AnnotationArg) -> &Expr {
    match arg {
        AnnotationArg::Positional(e) | AnnotationArg::Named(_, e) => e,
    }
}

/// Ownership state of a variable, used for linear/affine ownership tracking.
///
/// Based on **\[ATAPL\]** Ch. 1 — substructural type systems. Each variable
/// has a capability (owned/borrowed) that is consumed when moved and
/// temporarily shared when borrowed.
#[derive(Debug, Clone)]
enum OwnershipState {
    /// The variable owns its value and can be used.
    Owned,
    /// The variable's value has been moved away at the given source line.
    Moved(u32),
    /// The variable is borrowed (not consumed).
    Borrowed,
}

/// The type checker walks an AST and verifies that all expressions and
/// statements are well-typed according to Kōdo's type system.
///
/// Implements a single-pass, top-down type checking algorithm based on
/// **\[TAPL\]** Ch. 9 (simply typed lambda calculus). The checker maintains
/// a [`TypeEnv`] with scope-based binding management: the environment length
/// is saved before entering a scope and restored upon exit, ensuring
/// correct variable shadowing and lexical scoping.
/// Definition of a generic struct (before monomorphization).
#[derive(Clone)]
struct GenericStructDef {
    /// Generic parameter names (e.g. `["T", "U"]`).
    params: Vec<std::string::String>,
    /// Fields with types that may reference generic params.
    fields: Vec<(std::string::String, kodo_ast::TypeExpr)>,
}

/// Definition of a generic function (before monomorphization).
#[derive(Clone)]
struct GenericFunctionDef {
    /// Generic parameter names (e.g. `["T"]`).
    params: Vec<std::string::String>,
    /// Parameter type expressions (may reference generic params).
    param_types: Vec<kodo_ast::TypeExpr>,
    /// Return type expression.
    return_type: kodo_ast::TypeExpr,
}

/// Definition of a generic enum (before monomorphization).
#[derive(Clone)]
struct GenericEnumDef {
    /// Generic parameter names.
    params: Vec<std::string::String>,
    /// Variants with field type expressions.
    variants: Vec<(std::string::String, Vec<kodo_ast::TypeExpr>)>,
}

/// The type checker walks an AST and verifies that all expressions and
/// statements are well-typed according to Kōdo's type system.
pub struct TypeChecker {
    /// The type environment for variable and function bindings.
    env: TypeEnv,
    /// The expected return type of the current function being checked.
    current_return_type: Type,
    /// Registry of struct types: name to list of (field name, field type) pairs.
    struct_registry:
        std::collections::HashMap<std::string::String, Vec<(std::string::String, Type)>>,
    /// Registry of enum types: name to list of (variant name, field types) pairs.
    enum_registry:
        std::collections::HashMap<std::string::String, Vec<(std::string::String, Vec<Type>)>>,
    /// Set of known enum type names, used to distinguish enums from structs
    /// during type resolution.
    enum_names: std::collections::HashSet<std::string::String>,
    /// Generic struct definitions (for monomorphization).
    generic_structs: std::collections::HashMap<std::string::String, GenericStructDef>,
    /// Generic enum definitions (for monomorphization).
    generic_enums: std::collections::HashMap<std::string::String, GenericEnumDef>,
    /// Generic function definitions (for monomorphization).
    generic_functions: std::collections::HashMap<std::string::String, GenericFunctionDef>,
    /// Monomorphized function instances: `(base_name, type_args, mono_name)`.
    fn_instances: Vec<(std::string::String, Vec<Type>, std::string::String)>,
    /// Cache of already-monomorphized type names.
    mono_cache: std::collections::HashSet<std::string::String>,
    /// Trait definitions: name to list of method signatures.
    trait_registry:
        std::collections::HashMap<std::string::String, Vec<(std::string::String, Vec<Type>, Type)>>,
    /// Method lookup: (type, method) to (mangled name, params, return type).
    method_lookup: std::collections::HashMap<
        (std::string::String, std::string::String),
        (std::string::String, Vec<Type>, Type),
    >,
    /// Method call resolutions: call span start to mangled function name.
    /// Used by kodoc to rewrite method calls in the AST before MIR lowering.
    method_resolutions: std::collections::HashMap<u32, std::string::String>,
    /// Whether the currently-checked function is `async`.
    in_async_fn: bool,
    /// Call graph: function name → set of called function names.
    ///
    /// Built during `check_function` to support transitive confidence propagation.
    call_graph: std::collections::HashMap<
        std::string::String,
        std::collections::HashSet<std::string::String>,
    >,
    /// Current function name being checked, used for call graph edge recording.
    current_function_name: Option<std::string::String>,
    /// Declared confidence per function, extracted from `@confidence` annotations.
    ///
    /// Functions without an explicit `@confidence` annotation default to 1.0.
    declared_confidence: std::collections::HashMap<std::string::String, f64>,
    /// Ownership state per variable, tracking moves and borrows.
    ///
    /// Maps variable name to its current ownership state. Used for
    /// use-after-move and move-while-borrowed detection.
    ownership_map: std::collections::HashMap<std::string::String, OwnershipState>,
    /// Set of variable names that currently have active borrows.
    ///
    /// When a variable is borrowed (via `ref`), it is added here.
    /// It cannot be moved until the borrow is released (scope exit).
    active_borrows: std::collections::HashSet<std::string::String>,
    /// Saved ownership map states, used for scope management.
    ownership_scopes: Vec<(
        std::collections::HashMap<std::string::String, OwnershipState>,
        std::collections::HashSet<std::string::String>,
    )>,
    /// Parameter ownership qualifiers per function.
    ///
    /// Maps function name to a list of ownership qualifiers for each parameter.
    /// Used during `check_call` to determine whether passing a variable moves it.
    fn_param_ownership: std::collections::HashMap<std::string::String, Vec<kodo_ast::Ownership>>,
    /// Names of imported modules, used to resolve qualified calls like `math.add(1, 2)`.
    ///
    /// When the caller registers module names via [`register_imported_module`],
    /// `check_call` treats `FieldAccess` on module names as qualified function calls.
    imported_module_names: std::collections::HashSet<std::string::String>,
    /// Definition index: maps identifiers to their source spans.
    ///
    /// Used by the LSP for goto-definition. Built during `check_module`.
    definition_spans: std::collections::HashMap<std::string::String, Span>,
}

impl TypeChecker {
    /// Creates a new type checker with an empty environment.
    ///
    /// Builtin functions (`println`, `print`) are registered automatically.
    #[must_use]
    pub fn new() -> Self {
        let mut checker = Self {
            env: TypeEnv::new(),
            current_return_type: Type::Unit,
            struct_registry: std::collections::HashMap::new(),
            enum_registry: std::collections::HashMap::new(),
            enum_names: std::collections::HashSet::new(),
            generic_structs: std::collections::HashMap::new(),
            generic_enums: std::collections::HashMap::new(),
            generic_functions: std::collections::HashMap::new(),
            fn_instances: Vec::new(),
            mono_cache: std::collections::HashSet::new(),
            trait_registry: std::collections::HashMap::new(),
            method_lookup: std::collections::HashMap::new(),
            method_resolutions: std::collections::HashMap::new(),
            in_async_fn: false,
            call_graph: std::collections::HashMap::new(),
            current_function_name: None,
            declared_confidence: std::collections::HashMap::new(),
            ownership_map: std::collections::HashMap::new(),
            active_borrows: std::collections::HashSet::new(),
            ownership_scopes: Vec::new(),
            fn_param_ownership: std::collections::HashMap::new(),
            imported_module_names: std::collections::HashSet::new(),
            definition_spans: std::collections::HashMap::new(),
        };
        checker.register_builtins();
        checker
    }

    /// Registers a module name as imported, enabling qualified calls like `mod.func()`.
    pub fn register_imported_module(&mut self, name: std::string::String) {
        self.imported_module_names.insert(name);
    }

    /// Returns the definition spans index built during type checking.
    ///
    /// Maps identifier names (functions, variables, types) to their definition spans.
    /// Used by the LSP for goto-definition.
    #[must_use]
    pub fn definition_spans(&self) -> &std::collections::HashMap<std::string::String, Span> {
        &self.definition_spans
    }

    /// Registers builtin functions in the type environment.
    ///
    /// These are functions provided by the runtime that do not need to be
    /// declared in user code. Currently registers:
    /// - `println(String) -> ()`
    /// - `print(String) -> ()`
    /// - `print_int(Int) -> ()`
    /// - String methods: `length`, `contains`, `starts_with`, `ends_with`,
    ///   `trim`, `to_upper`, `to_lower`, `substring`
    /// - Int methods: `to_string`, `to_float64`
    /// - Float64 methods: `to_string`, `to_int`
    fn register_builtins(&mut self) {
        self.env.insert(
            "println".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Unit)),
        );
        self.env.insert(
            "print".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Unit)),
        );
        self.env.insert(
            "print_int".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
        // Math builtins
        self.env.insert(
            "abs".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "min".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "max".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "clamp".to_string(),
            Type::Function(vec![Type::Int, Type::Int, Type::Int], Box::new(Type::Int)),
        );

        // File I/O builtins
        self.env.insert(
            "file_exists".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Bool)),
        );
        self.env.insert(
            "file_read".to_string(),
            Type::Function(
                vec![Type::String],
                Box::new(Type::Enum("Result__String_String".to_string())),
            ),
        );
        self.env.insert(
            "file_write".to_string(),
            Type::Function(
                vec![Type::String, Type::String],
                Box::new(Type::Enum("Result__Unit_String".to_string())),
            ),
        );

        self.register_string_methods();
        self.register_int_methods();
        self.register_float_methods();
        self.register_list_functions();
        self.register_map_functions();
    }

    /// Registers builtin methods for the `String` type.
    ///
    /// These methods are available on all String values and are implemented
    /// in the runtime as `kodo_string_*` functions.
    #[allow(clippy::too_many_lines)]
    fn register_string_methods(&mut self) {
        // String.length() -> Int
        self.method_lookup.insert(
            ("String".to_string(), "length".to_string()),
            ("String_length".to_string(), vec![Type::String], Type::Int),
        );
        self.env.insert(
            "String_length".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );

        // String.contains(s: String) -> Bool
        self.method_lookup.insert(
            ("String".to_string(), "contains".to_string()),
            (
                "String_contains".to_string(),
                vec![Type::String, Type::String],
                Type::Bool,
            ),
        );
        self.env.insert(
            "String_contains".to_string(),
            Type::Function(vec![Type::String, Type::String], Box::new(Type::Bool)),
        );

        // String.starts_with(s: String) -> Bool
        self.method_lookup.insert(
            ("String".to_string(), "starts_with".to_string()),
            (
                "String_starts_with".to_string(),
                vec![Type::String, Type::String],
                Type::Bool,
            ),
        );
        self.env.insert(
            "String_starts_with".to_string(),
            Type::Function(vec![Type::String, Type::String], Box::new(Type::Bool)),
        );

        // String.ends_with(s: String) -> Bool
        self.method_lookup.insert(
            ("String".to_string(), "ends_with".to_string()),
            (
                "String_ends_with".to_string(),
                vec![Type::String, Type::String],
                Type::Bool,
            ),
        );
        self.env.insert(
            "String_ends_with".to_string(),
            Type::Function(vec![Type::String, Type::String], Box::new(Type::Bool)),
        );

        // String.trim() -> String
        self.method_lookup.insert(
            ("String".to_string(), "trim".to_string()),
            ("String_trim".to_string(), vec![Type::String], Type::String),
        );
        self.env.insert(
            "String_trim".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::String)),
        );

        // String.to_upper() -> String
        self.method_lookup.insert(
            ("String".to_string(), "to_upper".to_string()),
            (
                "String_to_upper".to_string(),
                vec![Type::String],
                Type::String,
            ),
        );
        self.env.insert(
            "String_to_upper".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::String)),
        );

        // String.to_lower() -> String
        self.method_lookup.insert(
            ("String".to_string(), "to_lower".to_string()),
            (
                "String_to_lower".to_string(),
                vec![Type::String],
                Type::String,
            ),
        );
        self.env.insert(
            "String_to_lower".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::String)),
        );

        // String.substring(start: Int, end: Int) -> String
        self.method_lookup.insert(
            ("String".to_string(), "substring".to_string()),
            (
                "String_substring".to_string(),
                vec![Type::String, Type::Int, Type::Int],
                Type::String,
            ),
        );
        self.env.insert(
            "String_substring".to_string(),
            Type::Function(
                vec![Type::String, Type::Int, Type::Int],
                Box::new(Type::String),
            ),
        );

        // String.split(sep: String) -> List<String>
        self.method_lookup.insert(
            ("String".to_string(), "split".to_string()),
            (
                "String_split".to_string(),
                vec![Type::String, Type::String],
                Type::Generic("List".to_string(), vec![Type::String]),
            ),
        );
        self.env.insert(
            "String_split".to_string(),
            Type::Function(
                vec![Type::String, Type::String],
                Box::new(Type::Generic("List".to_string(), vec![Type::String])),
            ),
        );
    }

    /// Registers builtin methods for the `Int` type.
    fn register_int_methods(&mut self) {
        // Int.to_string() -> String
        self.method_lookup.insert(
            ("Int".to_string(), "to_string".to_string()),
            ("Int_to_string".to_string(), vec![Type::Int], Type::String),
        );
        self.env.insert(
            "Int_to_string".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );

        // Int.to_float64() -> Float64
        self.method_lookup.insert(
            ("Int".to_string(), "to_float64".to_string()),
            ("Int_to_float64".to_string(), vec![Type::Int], Type::Float64),
        );
        self.env.insert(
            "Int_to_float64".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Float64)),
        );
    }

    /// Registers builtin methods for the `Float64` type.
    fn register_float_methods(&mut self) {
        // Float64.to_string() -> String
        self.method_lookup.insert(
            ("Float64".to_string(), "to_string".to_string()),
            (
                "Float64_to_string".to_string(),
                vec![Type::Float64],
                Type::String,
            ),
        );
        self.env.insert(
            "Float64_to_string".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::String)),
        );

        // Float64.to_int() -> Int
        self.method_lookup.insert(
            ("Float64".to_string(), "to_int".to_string()),
            ("Float64_to_int".to_string(), vec![Type::Float64], Type::Int),
        );
        self.env.insert(
            "Float64_to_int".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Int)),
        );
    }

    /// Registers builtin functions for `List<T>` operations.
    ///
    /// These are free functions (not methods) available to all Kōdo programs.
    /// At runtime, lists are opaque heap pointers managed by the runtime.
    fn register_list_functions(&mut self) {
        // list_new() -> List<Int>  (generic in spirit, monomorphic at runtime)
        self.env.insert(
            "list_new".to_string(),
            Type::Function(
                vec![],
                Box::new(Type::Generic("List".to_string(), vec![Type::Int])),
            ),
        );

        // list_push(list: List<Int>, item: Int) -> ()
        self.env.insert(
            "list_push".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Unit),
            ),
        );

        // list_get(list: List<Int>, index: Int) -> Int
        self.env.insert(
            "list_get".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Int),
            ),
        );

        // list_length(list: List<Int>) -> Int
        self.env.insert(
            "list_length".to_string(),
            Type::Function(
                vec![Type::Generic("List".to_string(), vec![Type::Int])],
                Box::new(Type::Int),
            ),
        );

        // list_contains(list: List<Int>, item: Int) -> Bool
        self.env.insert(
            "list_contains".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Bool),
            ),
        );
    }

    /// Registers builtin functions for `Map<K, V>` operations.
    ///
    /// Maps use integer keys and values at the runtime level. All values
    /// are represented as i64 (pointers or values).
    fn register_map_functions(&mut self) {
        // map_new() -> Map<Int, Int>
        self.env.insert(
            "map_new".to_string(),
            Type::Function(
                vec![],
                Box::new(Type::Generic("Map".to_string(), vec![Type::Int, Type::Int])),
            ),
        );

        // map_insert(map: Map<Int, Int>, key: Int, value: Int) -> ()
        self.env.insert(
            "map_insert".to_string(),
            Type::Function(
                vec![
                    Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                    Type::Int,
                    Type::Int,
                ],
                Box::new(Type::Unit),
            ),
        );

        // map_get(map: Map<Int, Int>, key: Int) -> Int
        self.env.insert(
            "map_get".to_string(),
            Type::Function(
                vec![
                    Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Int),
            ),
        );

        // map_contains_key(map: Map<Int, Int>, key: Int) -> Bool
        self.env.insert(
            "map_contains_key".to_string(),
            Type::Function(
                vec![
                    Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Bool),
            ),
        );

        // map_length(map: Map<Int, Int>) -> Int
        self.env.insert(
            "map_length".to_string(),
            Type::Function(
                vec![Type::Generic("Map".to_string(), vec![Type::Int, Type::Int])],
                Box::new(Type::Int),
            ),
        );
    }

    /// Type-checks an entire module.
    ///
    /// Registers all function signatures first (enabling mutual recursion),
    /// then checks each function body.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if any type inconsistency is found.
    #[allow(clippy::too_many_lines)]
    pub fn check_module(&mut self, module: &Module) -> Result<()> {
        // Validate mandatory meta block.
        match &module.meta {
            None => return Err(TypeError::MissingMeta),
            Some(meta) => {
                let purpose = meta.entries.iter().find(|e| e.key == "purpose");
                match purpose {
                    None => return Err(TypeError::MissingPurpose { span: meta.span }),
                    Some(entry) if entry.value.trim().is_empty() => {
                        return Err(TypeError::EmptyPurpose { span: entry.span });
                    }
                    Some(_) => {}
                }
            }
        }

        // Register struct types.
        for type_decl in &module.type_decls {
            if type_decl.generic_params.is_empty() {
                // Concrete struct — register directly.
                let mut fields = Vec::new();
                for field in &type_decl.fields {
                    let ty = resolve_type(&field.ty, field.span)?;
                    fields.push((field.name.clone(), ty));
                }
                self.struct_registry.insert(type_decl.name.clone(), fields);
                self.definition_spans
                    .insert(type_decl.name.clone(), type_decl.span);
            } else {
                // Generic struct — store definition for monomorphization.
                self.generic_structs.insert(
                    type_decl.name.clone(),
                    GenericStructDef {
                        params: type_decl.generic_params.clone(),
                        fields: type_decl
                            .fields
                            .iter()
                            .map(|f| (f.name.clone(), f.ty.clone()))
                            .collect(),
                    },
                );
            }
        }

        // Register enum types.
        for enum_decl in &module.enum_decls {
            self.enum_names.insert(enum_decl.name.clone());
            if enum_decl.generic_params.is_empty() {
                // Concrete enum — register directly.
                let mut variants = Vec::new();
                for variant in &enum_decl.variants {
                    let field_types: std::result::Result<Vec<_>, _> = variant
                        .fields
                        .iter()
                        .map(|f| resolve_type(f, variant.span))
                        .collect();
                    variants.push((variant.name.clone(), field_types?));
                }
                self.enum_registry.insert(enum_decl.name.clone(), variants);
            } else {
                // Generic enum — store definition for monomorphization.
                self.generic_enums.insert(
                    enum_decl.name.clone(),
                    GenericEnumDef {
                        params: enum_decl.generic_params.clone(),
                        variants: enum_decl
                            .variants
                            .iter()
                            .map(|v| (v.name.clone(), v.fields.clone()))
                            .collect(),
                    },
                );
            }
        }

        // Register trait declarations.
        for trait_decl in &module.trait_decls {
            let mut methods = Vec::new();
            for method in &trait_decl.methods {
                let param_types: std::result::Result<Vec<_>, _> = method
                    .params
                    .iter()
                    .map(|p| resolve_type_with_enums(&p.ty, p.span, &self.enum_names))
                    .collect();
                let param_types = param_types?;
                let ret_type =
                    resolve_type_with_enums(&method.return_type, method.span, &self.enum_names)?;
                methods.push((method.name.clone(), param_types, ret_type));
            }
            self.trait_registry.insert(trait_decl.name.clone(), methods);
        }

        // Register impl blocks: validate traits and build method lookup.
        for impl_block in &module.impl_blocks {
            let trait_methods = self
                .trait_registry
                .get(&impl_block.trait_name)
                .ok_or_else(|| TypeError::UnknownTrait {
                    name: impl_block.trait_name.clone(),
                    span: impl_block.span,
                })?
                .clone();

            // Verify all trait methods are implemented.
            for (method_name, _param_types, _ret_type) in &trait_methods {
                let _found = impl_block
                    .methods
                    .iter()
                    .find(|m| m.name == *method_name)
                    .ok_or_else(|| TypeError::MissingTraitMethod {
                        method: method_name.clone(),
                        trait_name: impl_block.trait_name.clone(),
                        span: impl_block.span,
                    })?;
            }

            // Register each method with mangled name: TypeName_methodName
            for method in &impl_block.methods {
                let mangled_name = format!("{}_{}", impl_block.type_name, method.name);
                let param_types: std::result::Result<Vec<_>, _> = method
                    .params
                    .iter()
                    .map(|p| self.resolve_type_mono(&p.ty, p.span))
                    .collect();
                let param_types = param_types?;
                let ret_type = self.resolve_type_mono(&method.return_type, method.span)?;

                // Register in method lookup
                self.method_lookup.insert(
                    (impl_block.type_name.clone(), method.name.clone()),
                    (mangled_name.clone(), param_types.clone(), ret_type.clone()),
                );

                // Register as a regular function in the environment
                self.env.insert(
                    mangled_name,
                    Type::Function(param_types, Box::new(ret_type)),
                );
            }
        }

        // Check impl block method bodies.
        for impl_block in &module.impl_blocks {
            for method in &impl_block.methods {
                self.check_function(method)?;
            }
        }

        // Register actor declarations as structs + handler signatures.
        // This must happen before function checking so that regular functions
        // can reference actor types and call handler functions.
        for actor_decl in &module.actor_decls {
            // Register actor fields like a struct.
            let mut fields = Vec::new();
            for field in &actor_decl.fields {
                let ty = self.resolve_type_mono(&field.ty, field.span)?;
                fields.push((field.name.clone(), ty));
            }
            self.struct_registry.insert(actor_decl.name.clone(), fields);

            // Register handler functions with mangled names.
            for handler in &actor_decl.handlers {
                let mangled_name = format!("{}_{}", actor_decl.name, handler.name);
                let param_types: std::result::Result<Vec<_>, _> = handler
                    .params
                    .iter()
                    .map(|p| self.resolve_type_mono(&p.ty, p.span))
                    .collect();
                let param_types = param_types?;
                let ret_type = self.resolve_type_mono(&handler.return_type, handler.span)?;
                self.env.insert(
                    mangled_name,
                    Type::Function(param_types, Box::new(ret_type)),
                );
            }
        }

        // First pass: register all function signatures so they can call each other.
        for func in &module.functions {
            if !func.generic_params.is_empty() {
                // Generic function — store definition for monomorphization at call sites.
                self.generic_functions.insert(
                    func.name.clone(),
                    GenericFunctionDef {
                        params: func.generic_params.clone(),
                        param_types: func.params.iter().map(|p| p.ty.clone()).collect(),
                        return_type: func.return_type.clone(),
                    },
                );
                continue;
            }
            let param_types: std::result::Result<Vec<_>, _> = func
                .params
                .iter()
                .map(|p| self.resolve_type_mono(&p.ty, p.span))
                .collect();
            let param_types = param_types?;
            let ret_type = self.resolve_type_mono(&func.return_type, func.span)?;
            self.env.insert(
                func.name.clone(),
                Type::Function(param_types, Box::new(ret_type)),
            );
            // Record definition span for LSP goto-definition.
            self.definition_spans.insert(func.name.clone(), func.span);
            // Record parameter ownership qualifiers for ownership tracking in check_call.
            let qualifiers: Vec<kodo_ast::Ownership> =
                func.params.iter().map(|p| p.ownership).collect();
            self.fn_param_ownership
                .insert(func.name.clone(), qualifiers);
        }

        // Second pass: check each function body (skip generic functions).
        for func in &module.functions {
            if func.generic_params.is_empty() {
                self.check_function(func)?;
            }
        }

        // Check actor handler bodies.
        for actor_decl in &module.actor_decls {
            for handler in &actor_decl.handlers {
                self.check_function(handler)?;
            }
        }

        // Third pass: validate trust policies based on annotations.
        let trust_policy = module
            .meta
            .as_ref()
            .and_then(|m| m.entries.iter().find(|e| e.key == "trust_policy"))
            .map(|e| e.value.clone());

        if let Some(policy) = trust_policy {
            if policy == "high_security" {
                for func in &module.functions {
                    Self::validate_trust_policy(func)?;
                }
            }
        }

        // Fourth pass: check annotation-based policies (independent of trust_policy).
        for func in &module.functions {
            Self::check_annotation_policies(func)?;
        }

        // Fifth pass: check confidence threshold declared in the meta block.
        //
        // If `min_confidence` is set, every top-level function's transitive
        // confidence (min across its call graph) must meet the threshold.
        let min_confidence = module
            .meta
            .as_ref()
            .and_then(|m| m.entries.iter().find(|e| e.key == "min_confidence"))
            .and_then(|e| e.value.parse::<f64>().ok());

        if let Some(threshold) = min_confidence {
            for func in &module.functions {
                let computed =
                    self.compute_confidence(&func.name, &mut std::collections::HashSet::new());
                if computed < threshold {
                    let (weakest_fn, weakest_conf) =
                        self.find_weakest_link(&func.name, &mut std::collections::HashSet::new());
                    return Err(TypeError::ConfidenceThreshold {
                        computed: format!("{computed:.2}"),
                        threshold: format!("{threshold:.2}"),
                        weakest_fn,
                        weakest_confidence: format!("{weakest_conf:.2}"),
                        span: func.span,
                    });
                }
            }
        }

        Ok(())
    }

    /// Validates trust policy constraints on a function's annotations.
    ///
    /// In `high_security` mode, every function must have `@authored_by` and
    /// `@confidence`. If confidence is below 0.85, `@reviewed_by` with a
    /// `"human:..."` argument is required.
    fn validate_trust_policy(func: &Function) -> Result<()> {
        let has_authored_by = func.annotations.iter().any(|a| a.name == "authored_by");
        if !has_authored_by {
            return Err(TypeError::PolicyViolation {
                message: format!(
                    "function `{}` is missing `@authored_by` annotation (required by trust_policy)",
                    func.name
                ),
                span: func.span,
            });
        }

        let confidence_ann = func.annotations.iter().find(|a| a.name == "confidence");
        let Some(confidence_ann) = confidence_ann else {
            return Err(TypeError::PolicyViolation {
                message: format!(
                    "function `{}` is missing `@confidence` annotation (required by trust_policy)",
                    func.name
                ),
                span: func.span,
            });
        };

        // Extract confidence value from the first positional arg.
        let confidence_value = Self::extract_confidence_value(confidence_ann);

        if let Some(value) = confidence_value {
            if value < 0.85 {
                let has_human_review = Self::has_human_review(func);
                if !has_human_review {
                    return Err(TypeError::PolicyViolation {
                        message: format!(
                            "function `{}` has @confidence({value}) below 0.85 threshold \
                             and is missing `@reviewed_by` with human reviewer",
                            func.name
                        ),
                        span: func.span,
                    });
                }
            }
        }

        Ok(())
    }

    /// Extracts a numeric confidence value from an annotation.
    ///
    /// Handles patterns like `@confidence(0.95)` where the value is encoded
    /// as an integer literal (representing hundredths, e.g. 95 for 0.95) or
    /// a string literal like `"0.95"`.
    #[allow(clippy::cast_precision_loss)]
    fn extract_confidence_value(ann: &Annotation) -> Option<f64> {
        for arg in &ann.args {
            let expr = annotation_arg_expr(arg);
            match expr {
                // If written as @confidence(95) — treat as percentage
                Expr::IntLit(n, _) => return Some(*n as f64 / 100.0),
                // If written as @confidence(0.95) — use directly
                Expr::FloatLit(v, _) => return Some(*v),
                // If written as @confidence("0.95")
                Expr::StringLit(s, _) => return s.parse::<f64>().ok(),
                _ => {}
            }
        }
        None
    }

    /// Checks if a function has a `@reviewed_by` annotation with a human reviewer.
    ///
    /// Accepts either positional `@reviewed_by("human:alice")` or named
    /// `@reviewed_by(human: "alice")` syntax.
    fn has_human_review(func: &Function) -> bool {
        func.annotations
            .iter()
            .filter(|a| a.name == "reviewed_by")
            .any(|a| {
                a.args.iter().any(|arg| match arg {
                    AnnotationArg::Positional(expr) => {
                        matches!(expr, Expr::StringLit(s, _) if s.starts_with("human:"))
                    }
                    AnnotationArg::Named(key, _) => key == "human",
                })
            })
    }

    /// Checks annotation-based policies that apply regardless of `trust_policy`.
    ///
    /// This enforces two rules:
    /// 1. `@confidence(X)` where X < 0.8 requires `@reviewed_by(human: "...")` (E0260).
    /// 2. `@security_sensitive` requires at least one `requires` or `ensures` clause (E0262).
    fn check_annotation_policies(func: &Function) -> Result<()> {
        // Rule 1: low confidence without human review.
        let confidence_ann = func.annotations.iter().find(|a| a.name == "confidence");
        if let Some(ann) = confidence_ann {
            if let Some(value) = Self::extract_confidence_value(ann) {
                if value < 0.8 && !Self::has_human_review(func) {
                    return Err(TypeError::LowConfidenceWithoutReview {
                        name: func.name.clone(),
                        confidence: format!("{value}"),
                        span: func.span,
                    });
                }
            }
        }

        // Rule 2: @security_sensitive without contracts.
        let is_security_sensitive = func
            .annotations
            .iter()
            .any(|a| a.name == "security_sensitive");
        if is_security_sensitive && func.requires.is_empty() && func.ensures.is_empty() {
            return Err(TypeError::SecuritySensitiveWithoutContract {
                name: func.name.clone(),
                span: func.span,
            });
        }

        Ok(())
    }

    /// Returns the struct registry (including monomorphized instances).
    #[must_use]
    pub fn struct_registry(
        &self,
    ) -> &std::collections::HashMap<std::string::String, Vec<(std::string::String, Type)>> {
        &self.struct_registry
    }

    /// Returns the enum registry (including monomorphized instances).
    #[must_use]
    pub fn enum_registry(
        &self,
    ) -> &std::collections::HashMap<std::string::String, Vec<(std::string::String, Vec<Type>)>>
    {
        &self.enum_registry
    }

    /// Returns the set of known enum type names.
    #[must_use]
    pub fn enum_names(&self) -> &std::collections::HashSet<std::string::String> {
        &self.enum_names
    }

    /// Returns the method lookup table mapping (type, method) pairs to
    /// their mangled name, parameter types, and return type.
    #[must_use]
    pub fn method_lookup(
        &self,
    ) -> &std::collections::HashMap<
        (std::string::String, std::string::String),
        (std::string::String, Vec<Type>, Type),
    > {
        &self.method_lookup
    }

    /// Returns method call resolutions: call span start position to mangled
    /// function name. Used for AST rewriting before MIR lowering.
    #[must_use]
    pub fn method_resolutions(&self) -> &std::collections::HashMap<u32, std::string::String> {
        &self.method_resolutions
    }

    /// Returns the list of monomorphized function instances.
    ///
    /// Each entry is `(base_name, type_args, mono_name)`.
    #[must_use]
    pub fn fn_instances(&self) -> &[(std::string::String, Vec<Type>, std::string::String)] {
        &self.fn_instances
    }

    /// Type-checks a single function definition.
    ///
    /// Opens a new scope for the function parameters, checks the body,
    /// and verifies that the body type is compatible with the declared
    /// return type.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if parameter types cannot be resolved,
    /// the body is ill-typed, or the body type does not match the
    /// declared return type.
    pub fn check_function(&mut self, func: &Function) -> Result<()> {
        let scope = self.env.scope_level();
        let ret_type = self.resolve_type_mono(&func.return_type, func.span)?;
        let prev_return_type = self.current_return_type.clone();
        self.current_return_type = ret_type.clone();
        let prev_async = self.in_async_fn;
        self.in_async_fn = func.is_async;

        // Record declared confidence for transitive confidence propagation.
        if let Some(ann) = func.annotations.iter().find(|a| a.name == "confidence") {
            if let Some(value) = Self::extract_confidence_value(ann) {
                self.declared_confidence.insert(func.name.clone(), value);
            }
        }
        let prev_function_name = self.current_function_name.clone();
        self.current_function_name = Some(func.name.clone());

        // Save ownership state and start fresh for this function.
        self.push_ownership_scope();

        // Bind parameters in the function scope.
        for param in &func.params {
            let ty = self.resolve_type_mono(&param.ty, param.span)?;
            self.env.insert(param.name.clone(), ty);
            // Track ownership based on parameter qualifier.
            match param.ownership {
                kodo_ast::Ownership::Owned => self.track_owned(&param.name),
                kodo_ast::Ownership::Ref => {
                    // `ref` parameters are borrowed references — the caller
                    // retains ownership. Inside the callee, the parameter is
                    // usable but cannot be moved (only its state is Borrowed,
                    // it is NOT added to active_borrows since there is no
                    // source variable to protect within this scope).
                    self.ownership_map
                        .insert(param.name.clone(), OwnershipState::Borrowed);
                }
            }
        }

        self.check_block(&func.body)?;

        // Restore the previous scope, return type, async state, function name, and ownership.
        self.env.truncate(scope);
        self.current_return_type = prev_return_type;
        self.in_async_fn = prev_async;
        self.current_function_name = prev_function_name;
        self.pop_ownership_scope();

        Ok(())
    }

    /// Type-checks a block of statements.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if any statement in the block is ill-typed.
    pub fn check_block(&mut self, block: &Block) -> Result<()> {
        let scope = self.env.scope_level();
        self.push_ownership_scope();
        for stmt in &block.stmts {
            self.check_stmt(stmt)?;
        }
        self.env.truncate(scope);
        self.pop_ownership_scope();
        Ok(())
    }

    /// Type-checks a single statement.
    ///
    /// - `Let`: resolves the type annotation (if any), infers the initializer
    ///   type, checks they match, and binds the variable.
    /// - `Return`: infers the value type and checks it matches the current
    ///   function's return type.
    /// - `Expr`: infers the expression type (for side effects / validation).
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] on type mismatches or undefined variables.
    #[allow(clippy::too_many_lines)]
    pub fn check_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Let {
                span,
                name,
                ty,
                value,
                ..
            } => {
                let value_ty = self.infer_expr(value)?;
                if let Some(annotation) = ty {
                    let expected = self.resolve_type_mono(annotation, *span)?;
                    // For generic enums with unresolved type params (e.g. Option::None
                    // inferred as Option__?), accept if the expected type is a valid
                    // monomorphization of the same base enum.
                    if !Self::compatible_enum_types(&expected, &value_ty) {
                        TypeEnv::check_eq(&expected, &value_ty, *span)?;
                    }
                    self.env.insert(name.clone(), expected);
                } else {
                    self.env.insert(name.clone(), value_ty);
                }
                // Track ownership for the new binding.
                // If the value is an identifier, this is a move or borrow.
                // Primitive (Copy) types are never moved — they are implicitly copied.
                let binding_ty = self.env.lookup(name).cloned();
                if let Expr::Ident(source_name, _) = value {
                    self.track_owned(name);
                    // If source variable is owned, moving it (unless it's a Copy type).
                    let is_copy = binding_ty.as_ref().is_some_and(Type::is_copy);
                    if !is_copy {
                        if let Some(OwnershipState::Owned) = self.ownership_map.get(source_name) {
                            self.check_can_move(source_name, *span)?;
                            self.track_moved(source_name, Self::span_to_line(span.start));
                        }
                    }
                } else {
                    self.track_owned(name);
                }
                Ok(())
            }
            Stmt::Return { span, value } => {
                let value_ty = match value {
                    Some(expr) => self.infer_expr(expr)?,
                    None => Type::Unit,
                };
                TypeEnv::check_eq(&self.current_return_type, &value_ty, *span)?;
                // A borrowed reference cannot escape its scope via return.
                if let Some(Expr::Ident(name, _)) = value {
                    if let Some(OwnershipState::Borrowed) = self.ownership_map.get(name) {
                        return Err(TypeError::BorrowEscapesScope {
                            name: name.clone(),
                            span: *span,
                        });
                    }
                }
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.infer_expr(expr)?;
                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                let cond_ty = self.infer_expr(condition)?;
                TypeEnv::check_eq(&Type::Bool, &cond_ty, expr_span(condition))?;
                self.check_block(body)?;
                Ok(())
            }
            Stmt::For {
                span,
                name,
                start,
                end,
                body,
                ..
            } => {
                let start_ty = self.infer_expr(start)?;
                TypeEnv::check_eq(&Type::Int, &start_ty, expr_span(start)).map_err(|_| {
                    TypeError::Mismatch {
                        expected: "Int".to_string(),
                        found: format!("{start_ty}"),
                        span: expr_span(start),
                    }
                })?;
                let end_ty = self.infer_expr(end)?;
                TypeEnv::check_eq(&Type::Int, &end_ty, expr_span(end)).map_err(|_| {
                    TypeError::Mismatch {
                        expected: "Int".to_string(),
                        found: format!("{end_ty}"),
                        span: expr_span(end),
                    }
                })?;
                let scope = self.env.scope_level();
                self.env.insert(name.clone(), Type::Int);
                self.check_block(body)?;
                self.env.truncate(scope);
                let _ = span;
                Ok(())
            }
            Stmt::Assign {
                span, name, value, ..
            } => {
                let value_ty = self.infer_expr(value)?;
                let existing_ty = self.env.lookup(name).cloned().ok_or_else(|| {
                    let similar = self.find_similar_name(name);
                    TypeError::Undefined {
                        name: name.clone(),
                        span: *span,
                        similar,
                    }
                })?;
                TypeEnv::check_eq(&existing_ty, &value_ty, *span)?;
                Ok(())
            }
            Stmt::IfLet {
                pattern,
                value,
                body,
                else_body,
                ..
            } => {
                // Type check the value expression and introduce pattern
                // bindings into scope for the body block.
                let val_ty = self.infer_expr(value)?;
                let scope = self.env.scope_level();
                self.introduce_pattern_bindings(pattern, &val_ty);
                self.check_block(body)?;
                self.env.truncate(scope);
                if let Some(else_block) = else_body {
                    self.check_block(else_block)?;
                }
                Ok(())
            }
            Stmt::Spawn { body, .. } => {
                // V1: spawn executes inline — just type-check the body.
                self.check_block(body)?;
                Ok(())
            }
        }
    }

    /// Infers the type of an expression.
    ///
    /// This is the core of the type checker. Each expression variant produces
    /// a type according to Kōdo's typing rules:
    ///
    /// - Literals produce their corresponding primitive type.
    /// - Identifiers are looked up in the type environment.
    /// - Binary and unary operators enforce operand type constraints.
    /// - Function calls verify arity and argument types.
    /// - If-expressions require a `Bool` condition and matching branch types.
    /// - Field access returns `Unknown` (struct resolution deferred).
    /// - Block expressions return the type of the last expression, or `Unit`.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if the expression is ill-typed.
    #[allow(clippy::too_many_lines)]
    pub fn infer_expr(&mut self, expr: &Expr) -> Result<Type> {
        match expr {
            Expr::IntLit(_, _) => Ok(Type::Int),
            Expr::FloatLit(_, _) => Ok(Type::Float64),
            Expr::StringLit(_, _) => Ok(Type::String),
            Expr::BoolLit(_, _) => Ok(Type::Bool),

            Expr::Ident(name, span) => {
                // Check for use-after-move before type lookup.
                self.check_not_moved(name, *span)?;
                self.env.lookup(name).cloned().ok_or_else(|| {
                    let similar = self.find_similar_name(name);
                    TypeError::Undefined {
                        name: name.clone(),
                        span: *span,
                        similar,
                    }
                })
            }

            Expr::BinaryOp {
                left,
                op,
                right,
                span,
            } => self.check_binary_op(left, *op, right, *span),

            Expr::UnaryOp { op, operand, span } => self.check_unary_op(*op, operand, *span),

            Expr::Call { callee, args, span } => self.check_call(callee, args, *span),

            Expr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => self.check_if(condition, then_branch, else_branch.as_ref(), *span),

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                let obj_ty = self.infer_expr(object)?;
                match &obj_ty {
                    Type::Struct(name) => {
                        let fields = self.struct_registry.get(name).ok_or_else(|| {
                            TypeError::UnknownStruct {
                                name: name.clone(),
                                span: *span,
                            }
                        })?;
                        let field_ty = fields
                            .iter()
                            .find(|(n, _)| n == field)
                            .map(|(_, t)| t.clone());
                        field_ty.ok_or_else(|| {
                            let similar =
                                find_similar_in(field, fields.iter().map(|(n, _)| n.as_str()));
                            TypeError::NoSuchField {
                                field: field.clone(),
                                type_name: name.clone(),
                                span: *span,
                                similar,
                            }
                        })
                    }
                    _ => {
                        // Non-struct field access — return Unknown for forward compat.
                        Ok(Type::Unknown)
                    }
                }
            }

            Expr::StructLit { name, fields, span } => {
                let expected_fields = self.struct_registry.get(name).cloned().ok_or_else(|| {
                    TypeError::UnknownStruct {
                        name: name.clone(),
                        span: *span,
                    }
                })?;

                // Check for duplicate fields.
                let mut seen = std::collections::HashSet::new();
                for field in fields {
                    if !seen.insert(field.name.clone()) {
                        return Err(TypeError::DuplicateStructField {
                            field: field.name.clone(),
                            struct_name: name.clone(),
                            span: field.span,
                        });
                    }
                }

                // Check for extra fields.
                for field in fields {
                    if !expected_fields.iter().any(|(n, _)| n == &field.name) {
                        let similar = find_similar_in(
                            &field.name,
                            expected_fields.iter().map(|(n, _)| n.as_str()),
                        );
                        return Err(TypeError::ExtraStructField {
                            field: field.name.clone(),
                            struct_name: name.clone(),
                            span: field.span,
                            similar,
                        });
                    }
                }

                // Check for missing fields.
                for (expected_name, _) in &expected_fields {
                    if !fields.iter().any(|f| &f.name == expected_name) {
                        return Err(TypeError::MissingStructField {
                            field: expected_name.clone(),
                            struct_name: name.clone(),
                            span: *span,
                        });
                    }
                }

                // Check field types.
                for field in fields {
                    let value_ty = self.infer_expr(&field.value)?;
                    let expected_ty = expected_fields
                        .iter()
                        .find(|(n, _)| n == &field.name)
                        .map(|(_, t)| t);
                    if let Some(expected) = expected_ty {
                        TypeEnv::check_eq(expected, &value_ty, field.span)?;
                    }
                }

                Ok(Type::Struct(name.clone()))
            }

            Expr::EnumVariantExpr {
                enum_name,
                variant,
                args,
                span,
            } => {
                // Check if this is a concrete enum.
                if let Some(variants) = self.enum_registry.get(enum_name).cloned() {
                    let variant_def =
                        variants.iter().find(|(n, _)| n == variant).ok_or_else(|| {
                            let similar =
                                find_similar_in(variant, variants.iter().map(|(n, _)| n.as_str()));
                            TypeError::UnknownVariant {
                                variant: variant.clone(),
                                enum_name: enum_name.clone(),
                                span: *span,
                                similar,
                            }
                        })?;
                    let expected_field_types = variant_def.1.clone();
                    if args.len() != expected_field_types.len() {
                        return Err(TypeError::ArityMismatch {
                            expected: expected_field_types.len(),
                            found: args.len(),
                            span: *span,
                        });
                    }
                    for (arg, expected_ty) in args.iter().zip(&expected_field_types) {
                        let arg_ty = self.infer_expr(arg)?;
                        TypeEnv::check_eq(expected_ty, &arg_ty, expr_span(arg))?;
                    }
                    return Ok(Type::Enum(enum_name.clone()));
                }

                // Check if this is a generic enum — infer type args from arguments.
                if let Some(def) = self.generic_enums.get(enum_name).cloned() {
                    let variant_def =
                        def.variants
                            .iter()
                            .find(|(n, _)| n == variant)
                            .ok_or_else(|| {
                                let similar = find_similar_in(
                                    variant,
                                    def.variants.iter().map(|(n, _)| n.as_str()),
                                );
                                TypeError::UnknownVariant {
                                    variant: variant.clone(),
                                    enum_name: enum_name.clone(),
                                    span: *span,
                                    similar,
                                }
                            })?;
                    if args.len() != variant_def.1.len() {
                        return Err(TypeError::ArityMismatch {
                            expected: variant_def.1.len(),
                            found: args.len(),
                            span: *span,
                        });
                    }

                    // Infer type args: for each arg, if the corresponding field type
                    // is a type param (Named("T")), map T → inferred type.
                    let mut inferred: std::collections::HashMap<std::string::String, Type> =
                        std::collections::HashMap::new();
                    let mut arg_types = Vec::new();
                    for (arg, field_type_expr) in args.iter().zip(&variant_def.1) {
                        let arg_ty = self.infer_expr(arg)?;
                        arg_types.push(arg_ty.clone());
                        if let kodo_ast::TypeExpr::Named(param_name) = field_type_expr {
                            if def.params.contains(param_name) {
                                inferred.insert(param_name.clone(), arg_ty);
                            }
                        }
                    }

                    // Build type args in param order.
                    let type_args: Vec<Type> = def
                        .params
                        .iter()
                        .map(|p| inferred.get(p).cloned().unwrap_or(Type::Unknown))
                        .collect();

                    let mono_name = Self::mono_name(enum_name, &type_args);
                    self.monomorphize_enum(&mono_name, &def, &type_args, *span)?;

                    // Verify arg types against monomorphized variant.
                    if let Some(mono_variants) = self.enum_registry.get(&mono_name).cloned() {
                        if let Some(mono_variant) = mono_variants.iter().find(|(n, _)| n == variant)
                        {
                            for (arg_ty, expected_ty) in arg_types.iter().zip(&mono_variant.1) {
                                TypeEnv::check_eq(expected_ty, arg_ty, *span)?;
                            }
                        }
                    }

                    return Ok(Type::Enum(mono_name));
                }

                Err(TypeError::UnknownEnum {
                    name: enum_name.clone(),
                    span: *span,
                })
            }

            Expr::Match { expr, arms, span } => {
                let matched_ty = self.infer_expr(expr)?;

                if arms.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 1,
                        found: 0,
                        span: *span,
                    });
                }

                let mut result_ty = None;
                let mut has_wildcard = false;
                let mut covered_variants: Vec<std::string::String> = Vec::new();

                for arm in arms {
                    let scope = self.env.scope_level();

                    match &arm.pattern {
                        kodo_ast::Pattern::Variant {
                            enum_name,
                            variant,
                            bindings,
                            ..
                        } => {
                            // Resolve the enum name from the pattern, or infer
                            // from the matched expression's type.
                            // For generic enums, the pattern may use the base name
                            // (e.g. "Option") while the registry has the monomorphized
                            // name (e.g. "Option__Int"). Prefer the matched type's name.
                            let matched_enum_name = if let Type::Enum(name) = &matched_ty {
                                Some(name.as_str())
                            } else {
                                None
                            };
                            let pattern_name = enum_name.as_deref();
                            let resolved_enum = matched_enum_name
                                .filter(|n| self.enum_registry.contains_key(*n))
                                .or_else(|| {
                                    pattern_name.filter(|n| self.enum_registry.contains_key(*n))
                                })
                                .or(matched_enum_name);
                            // Clone field types out of the registry to release the
                            // immutable borrow before we mutate `self.env`.
                            let field_types_opt = resolved_enum.and_then(|ename| {
                                self.enum_registry.get(ename).and_then(|variants| {
                                    variants
                                        .iter()
                                        .find(|(n, _)| n == variant)
                                        .map(|(_, ft)| ft.clone())
                                })
                            });
                            if let Some(field_types) = field_types_opt {
                                for (binding, ty) in bindings.iter().zip(&field_types) {
                                    self.env.insert(binding.clone(), ty.clone());
                                }
                                covered_variants.push(variant.clone());
                            }
                        }
                        kodo_ast::Pattern::Wildcard(_) => {
                            has_wildcard = true;
                        }
                        kodo_ast::Pattern::Literal(lit_expr) => {
                            self.infer_expr(lit_expr)?;
                        }
                    }

                    let arm_ty = self.infer_expr(&arm.body)?;
                    self.env.truncate(scope);

                    if let Some(ref expected) = result_ty {
                        TypeEnv::check_eq(expected, &arm_ty, arm.span)?;
                    } else {
                        result_ty = Some(arm_ty);
                    }
                }

                // Exhaustiveness check for enum types.
                if let Type::Enum(enum_name) = &matched_ty {
                    if !has_wildcard {
                        if let Some(all_variants) = self.enum_registry.get(enum_name) {
                            let missing: Vec<std::string::String> = all_variants
                                .iter()
                                .filter(|(name, _)| !covered_variants.contains(name))
                                .map(|(name, _)| name.clone())
                                .collect();
                            if !missing.is_empty() {
                                return Err(TypeError::NonExhaustiveMatch {
                                    enum_name: enum_name.clone(),
                                    missing,
                                    span: *span,
                                });
                            }
                        }
                    }
                }

                Ok(result_ty.unwrap_or(Type::Unit))
            }

            Expr::Block(block) => self.infer_block(block),

            Expr::Range {
                start, end, span, ..
            } => {
                let start_ty = self.infer_expr(start)?;
                TypeEnv::check_eq(&Type::Int, &start_ty, expr_span(start)).map_err(|_| {
                    TypeError::Mismatch {
                        expected: "Int".to_string(),
                        found: format!("{start_ty}"),
                        span: expr_span(start),
                    }
                })?;
                let end_ty = self.infer_expr(end)?;
                TypeEnv::check_eq(&Type::Int, &end_ty, expr_span(end)).map_err(|_| {
                    TypeError::Mismatch {
                        expected: "Int".to_string(),
                        found: format!("{end_ty}"),
                        span: expr_span(end),
                    }
                })?;
                let _ = span;
                Ok(Type::Unit)
            }

            Expr::NullCoalesce { left, right, span } => {
                let left_ty = self.infer_expr(left)?;
                let right_ty = self.infer_expr(right)?;
                // Basic validation: left should be an Option-like enum
                let is_option = matches!(&left_ty, Type::Enum(name) if name.starts_with("Option"))
                    || matches!(&left_ty, Type::Generic(name, _) if name == "Option");
                if !is_option && left_ty != Type::Unknown {
                    return Err(TypeError::CoalesceTypeMismatch {
                        found: left_ty.to_string(),
                        span: *span,
                    });
                }
                Ok(right_ty)
            }

            Expr::Try { operand, span } => {
                let operand_ty = self.infer_expr(operand)?;
                // Validate current function returns Result
                let returns_result = matches!(&self.current_return_type, Type::Enum(name) if name.starts_with("Result"))
                    || matches!(&self.current_return_type, Type::Generic(name, _) if name == "Result");
                if !returns_result && self.current_return_type != Type::Unknown {
                    return Err(TypeError::TryInNonResultFn { span: *span });
                }
                // The operand should be a Result type
                let _is_result = matches!(&operand_ty, Type::Enum(name) if name.starts_with("Result"))
                    || matches!(&operand_ty, Type::Generic(name, _) if name == "Result");
                // Result type is Unknown — desugaring will handle proper typing
                Ok(Type::Unknown)
            }

            Expr::OptionalChain {
                object,
                field,
                span,
            } => {
                let obj_ty = self.infer_expr(object)?;
                let is_option = matches!(&obj_ty, Type::Enum(name) if name.starts_with("Option"))
                    || matches!(&obj_ty, Type::Generic(name, _) if name == "Option");
                if !is_option && obj_ty != Type::Unknown {
                    return Err(TypeError::OptionalChainOnNonOption {
                        found: obj_ty.to_string(),
                        span: *span,
                    });
                }
                let _ = field;
                // Result is Option<FieldType> — complex to determine here.
                // The desugared match will handle proper typing.
                Ok(Type::Unknown)
            }

            Expr::Closure {
                params,
                return_type,
                body,
                span,
            } => {
                let scope = self.env.scope_level();

                // Resolve parameter types — all must have annotations.
                let mut param_types = Vec::new();
                for p in params {
                    let ty = if let Some(type_expr) = &p.ty {
                        self.resolve_type_mono(type_expr, p.span)?
                    } else {
                        return Err(TypeError::ClosureParamTypeMissing {
                            name: p.name.clone(),
                            span: p.span,
                        });
                    };
                    self.env.insert(p.name.clone(), ty.clone());
                    param_types.push(ty);
                }

                // Infer body type.
                let body_type = self.infer_expr(body)?;

                // Check return type if annotated.
                let ret_type = if let Some(ret_expr) = return_type {
                    let expected = self.resolve_type_mono(ret_expr, *span)?;
                    TypeEnv::check_eq(&expected, &body_type, *span)?;
                    expected
                } else {
                    body_type
                };

                self.env.truncate(scope);

                Ok(Type::Function(param_types, Box::new(ret_type)))
            }

            Expr::Is { operand, span, .. } => {
                // Type check the operand; the result is always Bool.
                self.infer_expr(operand)?;
                let _ = span;
                Ok(Type::Bool)
            }

            Expr::Await { operand, span } => {
                // Check that we are inside an async fn.
                if !self.in_async_fn {
                    return Err(TypeError::AwaitOutsideAsync { span: *span });
                }
                // V1: await compiles as the inner expression — just return
                // its type. In a future version, this would unwrap Future<T>.
                self.infer_expr(operand)
            }
        }
    }

    /// Infers the type of a block expression.
    ///
    /// The type is determined by the last statement: if it is an `Expr`
    /// statement, its type is the block's type; otherwise the block is `Unit`.
    fn infer_block(&mut self, block: &Block) -> Result<Type> {
        let mut last_ty = Type::Unit;
        for stmt in &block.stmts {
            match stmt {
                Stmt::Expr(expr) => {
                    last_ty = self.infer_expr(expr)?;
                }
                Stmt::Let { value, .. } | Stmt::Assign { value, .. } => {
                    self.infer_expr(value)?;
                    last_ty = Type::Unit;
                }
                Stmt::Return { value, .. } => {
                    if let Some(expr) = value {
                        self.infer_expr(expr)?;
                    }
                    last_ty = Type::Unit;
                }
                Stmt::While {
                    condition, body, ..
                } => {
                    self.infer_expr(condition)?;
                    self.infer_block(body)?;
                    last_ty = Type::Unit;
                }
                Stmt::For {
                    start, end, body, ..
                } => {
                    self.infer_expr(start)?;
                    self.infer_expr(end)?;
                    self.infer_block(body)?;
                    last_ty = Type::Unit;
                }
                Stmt::IfLet {
                    pattern,
                    value,
                    body,
                    else_body,
                    ..
                } => {
                    let val_ty = self.infer_expr(value)?;
                    let scope = self.env.scope_level();
                    self.introduce_pattern_bindings(pattern, &val_ty);
                    self.infer_block(body)?;
                    self.env.truncate(scope);
                    if let Some(else_block) = else_body {
                        self.infer_block(else_block)?;
                    }
                    last_ty = Type::Unit;
                }
                Stmt::Spawn { body, .. } => {
                    self.infer_block(body)?;
                    last_ty = Type::Unit;
                }
            }
        }
        Ok(last_ty)
    }

    /// Introduces pattern bindings into the current type environment.
    ///
    /// For variant patterns like `Option::Some(value)`, looks up the variant's
    /// field types in the enum registry and inserts each binding.
    fn introduce_pattern_bindings(&mut self, pattern: &Pattern, matched_ty: &Type) {
        if let Pattern::Variant {
            enum_name,
            variant,
            bindings,
            ..
        } = pattern
        {
            let matched_enum_name = if let Type::Enum(name) = matched_ty {
                Some(name.as_str())
            } else {
                None
            };
            let pattern_name = enum_name.as_deref();
            let resolved_enum = matched_enum_name
                .filter(|n| self.enum_registry.contains_key(*n))
                .or_else(|| pattern_name.filter(|n| self.enum_registry.contains_key(*n)))
                .or(matched_enum_name);
            let field_types_opt = resolved_enum.and_then(|ename| {
                self.enum_registry.get(ename).and_then(|variants| {
                    variants
                        .iter()
                        .find(|(n, _)| n == variant)
                        .map(|(_, ft)| ft.clone())
                })
            });
            if let Some(field_types) = field_types_opt {
                for (binding, ty) in bindings.iter().zip(&field_types) {
                    self.env.insert(binding.clone(), ty.clone());
                }
            }
        }
    }

    /// Checks a binary operation and returns the result type.
    ///
    /// Arithmetic operators (`+`, `-`, `*`, `/`, `%`) require both operands
    /// to be the same numeric type and return that type. Comparison operators
    /// (`==`, `!=`, `<`, `>`, `<=`, `>=`) require matching numeric operands
    /// and return `Bool`. Logical operators (`&&`, `||`) require `Bool`
    /// operands and return `Bool`.
    fn check_binary_op(
        &mut self,
        left: &Expr,
        op: BinOp,
        right: &Expr,
        span: Span,
    ) -> Result<Type> {
        let left_ty = self.infer_expr(left)?;
        let right_ty = self.infer_expr(right)?;

        match op {
            // Arithmetic operators: both operands must be the same numeric type.
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                if !left_ty.is_numeric() {
                    return Err(TypeError::Mismatch {
                        expected: "numeric type".to_string(),
                        found: left_ty.to_string(),
                        span: expr_span(left),
                    });
                }
                TypeEnv::check_eq(&left_ty, &right_ty, span)?;
                Ok(left_ty)
            }
            // Comparison operators: both operands must be the same numeric type,
            // result is Bool.
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                if !left_ty.is_numeric() {
                    return Err(TypeError::Mismatch {
                        expected: "numeric type".to_string(),
                        found: left_ty.to_string(),
                        span: expr_span(left),
                    });
                }
                TypeEnv::check_eq(&left_ty, &right_ty, span)?;
                Ok(Type::Bool)
            }
            // Logical operators: both operands must be Bool.
            BinOp::And | BinOp::Or => {
                TypeEnv::check_eq(&Type::Bool, &left_ty, expr_span(left))?;
                TypeEnv::check_eq(&Type::Bool, &right_ty, expr_span(right))?;
                Ok(Type::Bool)
            }
        }
    }

    /// Checks a unary operation and returns the result type.
    ///
    /// `Neg` requires a numeric operand; `Not` requires `Bool`.
    fn check_unary_op(&mut self, op: UnaryOp, operand: &Expr, span: Span) -> Result<Type> {
        let operand_ty = self.infer_expr(operand)?;
        match op {
            UnaryOp::Neg => {
                if !operand_ty.is_numeric() {
                    return Err(TypeError::Mismatch {
                        expected: "numeric type".to_string(),
                        found: operand_ty.to_string(),
                        span,
                    });
                }
                Ok(operand_ty)
            }
            UnaryOp::Not => {
                TypeEnv::check_eq(&Type::Bool, &operand_ty, span)?;
                Ok(Type::Bool)
            }
        }
    }

    /// Checks a function call expression.
    ///
    /// Verifies the callee is a function type, the argument count matches,
    /// and each argument type matches the corresponding parameter type.
    /// Also tracks ownership: arguments passed to `own` parameters are moved.
    #[allow(clippy::too_many_lines)]
    fn check_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Result<Type> {
        // Check for qualified call pattern: module.func(args)
        // If the object is an identifier matching an imported module name,
        // resolve field as a function in that module's namespace.
        if let Expr::FieldAccess {
            object,
            field,
            span: _fa_span,
        } = callee
        {
            if let Expr::Ident(module_name, _) = object.as_ref() {
                if self.imported_module_names.contains(module_name) {
                    // Qualified call: treat as a direct call to `field`
                    let field_ident = Expr::Ident(field.clone(), span);
                    return self.check_call(&field_ident, args, span);
                }
            }
        }

        // Check for method call pattern: callee is FieldAccess (e.g. obj.method(args))
        if let Expr::FieldAccess {
            object,
            field,
            span: _fa_span,
        } = callee
        {
            let obj_ty = self.infer_expr(object)?;
            let type_name = match &obj_ty {
                Type::Struct(n) | Type::Enum(n) => n.clone(),
                Type::String => "String".to_string(),
                Type::Int => "Int".to_string(),
                Type::Float64 => "Float64".to_string(),
                _ => std::string::String::new(),
            };
            if !type_name.is_empty() {
                if let Some((mangled_name, param_types, ret_type)) = self
                    .method_lookup
                    .get(&(type_name.clone(), field.clone()))
                    .cloned()
                {
                    // Method call: verify arguments (excluding self param)
                    // param_types[0] is the self parameter
                    let method_params = if param_types.len() > 1 {
                        &param_types[1..]
                    } else {
                        &[]
                    };
                    if method_params.len() != args.len() {
                        return Err(TypeError::ArityMismatch {
                            expected: method_params.len(),
                            found: args.len(),
                            span,
                        });
                    }
                    for (param_ty, arg) in method_params.iter().zip(args) {
                        let arg_ty = self.infer_expr(arg)?;
                        TypeEnv::check_eq(param_ty, &arg_ty, expr_span(arg))?;
                    }
                    // Verify self type matches
                    if let Some(self_ty) = param_types.first() {
                        TypeEnv::check_eq(self_ty, &obj_ty, span)?;
                    }
                    // Record this method call for AST rewriting
                    self.method_resolutions.insert(span.start, mangled_name);
                    return Ok(ret_type);
                }
                // Find similar method names for this type.
                let similar = find_similar_in(
                    field,
                    self.method_lookup
                        .keys()
                        .filter(|(t, _)| t == &type_name)
                        .map(|(_, m)| m.as_str()),
                );
                return Err(TypeError::MethodNotFound {
                    method: field.clone(),
                    type_name,
                    span,
                    similar,
                });
            }
        }

        // Check for generic function call.
        if let Expr::Ident(name, _) = callee {
            if let Some(def) = self.generic_functions.get(name).cloned() {
                // Record call graph edge for confidence propagation.
                if let Some(ref caller) = self.current_function_name.clone() {
                    self.call_graph
                        .entry(caller.clone())
                        .or_default()
                        .insert(name.clone());
                }
                return self.check_generic_call(name, &def, args, span);
            }
        }

        // Record call graph edge for direct Ident calls (non-generic).
        // Also look up parameter ownership qualifiers for move tracking.
        let callee_name = if let Expr::Ident(name, _) = callee {
            if let Some(ref caller) = self.current_function_name.clone() {
                self.call_graph
                    .entry(caller.clone())
                    .or_default()
                    .insert(name.clone());
            }
            Some(name.clone())
        } else {
            None
        };

        let callee_ty = self.infer_expr(callee)?;
        match callee_ty {
            Type::Function(param_types, ret_type) => {
                if param_types.len() != args.len() {
                    return Err(TypeError::ArityMismatch {
                        expected: param_types.len(),
                        found: args.len(),
                        span,
                    });
                }
                // Get ownership qualifiers for tracking moves.
                let qualifiers = callee_name
                    .as_ref()
                    .and_then(|n| self.fn_param_ownership.get(n))
                    .cloned();
                for (i, (param_ty, arg)) in param_types.iter().zip(args).enumerate() {
                    let arg_ty = self.infer_expr(arg)?;
                    TypeEnv::check_eq(param_ty, &arg_ty, expr_span(arg))?;
                    // Track ownership: if parameter is `own` and argument is an ident,
                    // mark the variable as moved (unless it's a Copy type).
                    let is_own = qualifiers
                        .as_ref()
                        .and_then(|q| q.get(i))
                        .map_or(true, |o| *o == kodo_ast::Ownership::Owned);
                    if let Expr::Ident(arg_name, arg_span) = arg {
                        if is_own && !arg_ty.is_copy() {
                            // Only move variables that are actually Owned (not Borrowed refs).
                            if let Some(OwnershipState::Owned) = self.ownership_map.get(arg_name) {
                                self.check_can_move(arg_name, *arg_span)?;
                                self.track_moved(arg_name, Self::span_to_line(arg_span.start));
                            }
                        } else if !is_own {
                            // Parameter is `ref` — the source variable is borrowed.
                            // Use track_borrowed with the arg as both the binding and
                            // source, since the caller's variable is the borrow source.
                            self.track_borrowed(arg_name, arg_name);
                        }
                    }
                }
                Ok(*ret_type)
            }
            _ => Err(TypeError::NotCallable {
                found: callee_ty.to_string(),
                span,
            }),
        }
    }

    /// Type-checks a call to a generic function, inferring type arguments from
    /// the actual arguments.
    fn check_generic_call(
        &mut self,
        name: &str,
        def: &GenericFunctionDef,
        args: &[Expr],
        span: Span,
    ) -> Result<Type> {
        if def.param_types.len() != args.len() {
            return Err(TypeError::ArityMismatch {
                expected: def.param_types.len(),
                found: args.len(),
                span,
            });
        }

        // Infer type arguments from actual argument types.
        let mut inferred: std::collections::HashMap<std::string::String, Type> =
            std::collections::HashMap::new();
        let mut arg_types = Vec::new();
        for (arg, param_type_expr) in args.iter().zip(&def.param_types) {
            let arg_ty = self.infer_expr(arg)?;
            arg_types.push(arg_ty.clone());
            Self::infer_type_param(param_type_expr, &arg_ty, &def.params, &mut inferred);
        }

        // Build resolved type args in param order.
        let type_args: Vec<Type> = def
            .params
            .iter()
            .map(|p| inferred.get(p).cloned().unwrap_or(Type::Unknown))
            .collect();

        // Resolve return type with substitution.
        let subst: std::collections::HashMap<std::string::String, Type> = def
            .params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect();
        let ret_type =
            Self::substitute_type_expr(&def.return_type, &subst, span, &self.enum_names)?;

        // Resolve each param type with substitution and verify.
        for (arg_ty, param_type_expr) in arg_types.iter().zip(&def.param_types) {
            let expected =
                Self::substitute_type_expr(param_type_expr, &subst, span, &self.enum_names)?;
            TypeEnv::check_eq(&expected, arg_ty, span)?;
        }

        // Record the monomorphized instance for codegen.
        let mono_name = Self::mono_name(name, &type_args);
        self.fn_instances
            .push((name.to_string(), type_args, mono_name));

        Ok(ret_type)
    }

    /// Infers type parameter bindings from a type expression and an actual type.
    fn infer_type_param(
        type_expr: &kodo_ast::TypeExpr,
        actual: &Type,
        params: &[std::string::String],
        inferred: &mut std::collections::HashMap<std::string::String, Type>,
    ) {
        match type_expr {
            kodo_ast::TypeExpr::Named(name) if params.contains(name) => {
                inferred
                    .entry(name.clone())
                    .or_insert_with(|| actual.clone());
            }
            kodo_ast::TypeExpr::Generic(_name, args) => {
                // For generic type args, try to match inner types.
                if let Type::Enum(mono_name) | Type::Struct(mono_name) = actual {
                    // Extract type args from monomorphized name.
                    if let Some(suffix) = mono_name.split("__").nth(1) {
                        let actual_args: Vec<&str> = suffix.split('_').collect();
                        for (arg_expr, actual_arg) in args.iter().zip(&actual_args) {
                            if let kodo_ast::TypeExpr::Named(param_name) = arg_expr {
                                if params.contains(param_name) {
                                    // Map the actual arg name back to a Type.
                                    let ty = match *actual_arg {
                                        "Int" => Type::Int,
                                        "Bool" => Type::Bool,
                                        "String" => Type::String,
                                        _ => Type::Struct((*actual_arg).to_string()),
                                    };
                                    inferred.entry(param_name.clone()).or_insert(ty);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Resolves a type expression, triggering monomorphization for generic types.
    ///
    /// When encountering `Generic("Option", [Int])`, checks if `Option` is a
    /// generic enum or struct and monomorphizes it into a concrete type like
    /// `Option__Int`.
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] if the generic type is undefined, has wrong arity,
    /// or contains undefined type parameters.
    fn resolve_type_mono(&mut self, type_expr: &kodo_ast::TypeExpr, span: Span) -> Result<Type> {
        match type_expr {
            kodo_ast::TypeExpr::Generic(name, args) => {
                // Resolve all type arguments first.
                let resolved_args: std::result::Result<Vec<_>, _> = args
                    .iter()
                    .map(|a| self.resolve_type_mono(a, span))
                    .collect();
                let resolved_args = resolved_args?;

                // Try monomorphizing as enum first, then struct.
                if let Some(def) = self.generic_enums.get(name).cloned() {
                    if def.params.len() != resolved_args.len() {
                        return Err(TypeError::WrongTypeArgCount {
                            name: name.clone(),
                            expected: def.params.len(),
                            found: resolved_args.len(),
                            span,
                        });
                    }
                    let mono_name = Self::mono_name(name, &resolved_args);
                    self.monomorphize_enum(&mono_name, &def, &resolved_args, span)?;
                    Ok(Type::Enum(mono_name))
                } else if let Some(def) = self.generic_structs.get(name).cloned() {
                    if def.params.len() != resolved_args.len() {
                        return Err(TypeError::WrongTypeArgCount {
                            name: name.clone(),
                            expected: def.params.len(),
                            found: resolved_args.len(),
                            span,
                        });
                    }
                    let mono_name = Self::mono_name(name, &resolved_args);
                    self.monomorphize_struct(&mono_name, &def, &resolved_args, span)?;
                    Ok(Type::Struct(mono_name))
                } else {
                    // Not a known generic — fall through to standard resolution.
                    Ok(Type::Generic(name.clone(), resolved_args))
                }
            }
            kodo_ast::TypeExpr::Named(name) => {
                // Check if this name refers to a generic type used without args.
                if self.generic_enums.contains_key(name) || self.generic_structs.contains_key(name)
                {
                    return Err(TypeError::MissingTypeArgs {
                        name: name.clone(),
                        span,
                    });
                }
                resolve_type_with_enums(type_expr, span, &self.enum_names)
            }
            _ => resolve_type_with_enums(type_expr, span, &self.enum_names),
        }
    }

    /// Checks if two enum types are compatible, considering generic enums
    /// with partially-inferred type params (e.g. `Option__Int` vs `Option__?`).
    fn compatible_enum_types(expected: &Type, found: &Type) -> bool {
        if let (Type::Enum(e), Type::Enum(f)) = (expected, found) {
            if e == f {
                return true;
            }
            // Check if both are monomorphizations of the same base enum,
            // where the found type has unresolved params (contains "?").
            if let (Some(e_base), Some(f_base)) = (e.split("__").next(), f.split("__").next()) {
                return e_base == f_base && f.contains('?');
            }
        }
        false
    }

    /// Generates a monomorphized name like `Option__Int` or `Pair__Int_Bool`.
    fn mono_name(base: &str, args: &[Type]) -> std::string::String {
        let arg_strs: Vec<std::string::String> =
            args.iter().map(std::string::ToString::to_string).collect();
        format!("{base}__{}", arg_strs.join("_"))
    }

    /// Substitutes type parameters in a type expression.
    fn substitute_type_expr(
        type_expr: &kodo_ast::TypeExpr,
        subst: &std::collections::HashMap<std::string::String, Type>,
        span: Span,
        enum_names: &std::collections::HashSet<std::string::String>,
    ) -> Result<Type> {
        match type_expr {
            kodo_ast::TypeExpr::Named(name) => {
                if let Some(ty) = subst.get(name) {
                    Ok(ty.clone())
                } else {
                    resolve_type_with_enums(type_expr, span, enum_names)
                }
            }
            kodo_ast::TypeExpr::Generic(name, args) => {
                let resolved: std::result::Result<Vec<_>, _> = args
                    .iter()
                    .map(|a| Self::substitute_type_expr(a, subst, span, enum_names))
                    .collect();
                Ok(Type::Generic(name.clone(), resolved?))
            }
            kodo_ast::TypeExpr::Unit => Ok(Type::Unit),
            kodo_ast::TypeExpr::Optional(inner) => {
                // T? is sugar for Option<T>
                let generic =
                    kodo_ast::TypeExpr::Generic("Option".to_string(), vec![(**inner).clone()]);
                Self::substitute_type_expr(&generic, subst, span, enum_names)
            }
            kodo_ast::TypeExpr::Function(params, ret) => {
                let p: std::result::Result<Vec<_>, _> = params
                    .iter()
                    .map(|p| Self::substitute_type_expr(p, subst, span, enum_names))
                    .collect();
                let r = Self::substitute_type_expr(ret, subst, span, enum_names)?;
                Ok(Type::Function(p?, Box::new(r)))
            }
        }
    }

    /// Monomorphizes a generic enum definition with concrete type arguments.
    fn monomorphize_enum(
        &mut self,
        mono_name: &str,
        def: &GenericEnumDef,
        args: &[Type],
        span: Span,
    ) -> Result<()> {
        if self.mono_cache.contains(mono_name) {
            return Ok(());
        }
        self.mono_cache.insert(mono_name.to_string());

        // Build substitution map: T → Int, E → String, etc.
        let subst: std::collections::HashMap<std::string::String, Type> = def
            .params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect();

        let mut variants = Vec::new();
        for (vname, field_exprs) in &def.variants {
            let field_types: std::result::Result<Vec<_>, _> = field_exprs
                .iter()
                .map(|fe| Self::substitute_type_expr(fe, &subst, span, &self.enum_names))
                .collect();
            variants.push((vname.clone(), field_types?));
        }
        self.enum_registry.insert(mono_name.to_string(), variants);
        self.enum_names.insert(mono_name.to_string());
        Ok(())
    }

    /// Monomorphizes a generic struct definition with concrete type arguments.
    fn monomorphize_struct(
        &mut self,
        mono_name: &str,
        def: &GenericStructDef,
        args: &[Type],
        span: Span,
    ) -> Result<()> {
        if self.mono_cache.contains(mono_name) {
            return Ok(());
        }
        self.mono_cache.insert(mono_name.to_string());

        let subst: std::collections::HashMap<std::string::String, Type> = def
            .params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect();

        let mut fields = Vec::new();
        for (fname, ftype_expr) in &def.fields {
            let ty = Self::substitute_type_expr(ftype_expr, &subst, span, &self.enum_names)?;
            fields.push((fname.clone(), ty));
        }
        self.struct_registry.insert(mono_name.to_string(), fields);
        Ok(())
    }

    /// Computes the transitive confidence for a function by following its call graph.
    ///
    /// The effective confidence of a function is the minimum of its own declared
    /// confidence and the effective confidence of all functions it calls.
    /// Functions without `@confidence` default to 1.0 (fully trusted).
    /// Cycles are broken conservatively by returning the declared value on re-entry.
    fn compute_confidence(
        &self,
        func_name: &str,
        visited: &mut std::collections::HashSet<std::string::String>,
    ) -> f64 {
        if !visited.insert(func_name.to_string()) {
            // Cycle detected — return declared or default to avoid infinite recursion.
            return self
                .declared_confidence
                .get(func_name)
                .copied()
                .unwrap_or(1.0);
        }
        let declared = self
            .declared_confidence
            .get(func_name)
            .copied()
            .unwrap_or(1.0);
        let callees = self.call_graph.get(func_name);
        if let Some(callees) = callees {
            let mut min_conf = declared;
            for callee in callees {
                let callee_conf = self.compute_confidence(callee, visited);
                if callee_conf < min_conf {
                    min_conf = callee_conf;
                }
            }
            min_conf
        } else {
            declared
        }
    }

    /// Finds the weakest function in the call chain rooted at `func_name`.
    ///
    /// Returns `(function_name, confidence)` for the function with the lowest
    /// effective confidence reachable from `func_name`.
    fn find_weakest_link(
        &self,
        func_name: &str,
        visited: &mut std::collections::HashSet<std::string::String>,
    ) -> (std::string::String, f64) {
        if !visited.insert(func_name.to_string()) {
            let conf = self
                .declared_confidence
                .get(func_name)
                .copied()
                .unwrap_or(1.0);
            return (func_name.to_string(), conf);
        }
        let declared = self
            .declared_confidence
            .get(func_name)
            .copied()
            .unwrap_or(1.0);
        let mut weakest = (func_name.to_string(), declared);
        if let Some(callees) = self.call_graph.get(func_name) {
            for callee in callees {
                let (link_name, link_conf) = self.find_weakest_link(callee, visited);
                if link_conf < weakest.1 {
                    weakest = (link_name, link_conf);
                }
            }
        }
        weakest
    }

    /// Returns the confidence report for all top-level functions in a module.
    ///
    /// Each entry is `(function_name, declared_confidence, computed_confidence, callees)`.
    /// The computed confidence is the transitive minimum across the call graph.
    /// Functions without `@confidence` have a declared confidence of 1.0.
    #[must_use]
    pub fn confidence_report(
        &self,
        module: &Module,
    ) -> Vec<(std::string::String, f64, f64, Vec<std::string::String>)> {
        let mut report = Vec::new();
        for func in &module.functions {
            let declared = self
                .declared_confidence
                .get(&func.name)
                .copied()
                .unwrap_or(1.0);
            let computed =
                self.compute_confidence(&func.name, &mut std::collections::HashSet::new());
            let callees = self
                .call_graph
                .get(&func.name)
                .map(|s| s.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            report.push((func.name.clone(), declared, computed, callees));
        }
        report
    }

    /// Saves the current ownership state before entering a new scope.
    fn push_ownership_scope(&mut self) {
        self.ownership_scopes
            .push((self.ownership_map.clone(), self.active_borrows.clone()));
    }

    /// Restores the ownership state when leaving a scope.
    fn pop_ownership_scope(&mut self) {
        if let Some((map, borrows)) = self.ownership_scopes.pop() {
            self.ownership_map = map;
            self.active_borrows = borrows;
        }
    }

    /// Records that a variable is owned.
    fn track_owned(&mut self, name: &str) {
        self.ownership_map
            .insert(name.to_string(), OwnershipState::Owned);
    }

    /// Records that a variable is borrowed (via `ref`).
    ///
    /// Marks `name` as borrowed and adds `source_var` to `active_borrows`,
    /// preventing it from being moved until the borrow is released.
    fn track_borrowed(&mut self, name: &str, source_var: &str) {
        self.ownership_map
            .insert(name.to_string(), OwnershipState::Borrowed);
        self.active_borrows.insert(source_var.to_string());
    }

    /// Records that a variable has been moved at the given source line.
    fn track_moved(&mut self, name: &str, line: u32) {
        self.ownership_map
            .insert(name.to_string(), OwnershipState::Moved(line));
    }

    /// Checks if a variable can be used (not moved).
    ///
    /// Returns an error if the variable was previously moved.
    fn check_not_moved(&self, name: &str, span: Span) -> Result<()> {
        if let Some(OwnershipState::Moved(line)) = self.ownership_map.get(name) {
            return Err(TypeError::UseAfterMove {
                name: name.to_string(),
                moved_at_line: *line,
                span,
            });
        }
        Ok(())
    }

    /// Checks if a variable can be moved (not currently borrowed).
    ///
    /// Returns an error if there are active borrows on this variable.
    fn check_can_move(&self, name: &str, span: Span) -> Result<()> {
        if self.active_borrows.contains(name) {
            return Err(TypeError::MoveWhileBorrowed {
                name: name.to_string(),
                span,
            });
        }
        Ok(())
    }

    /// Finds the most similar name in the current environment using Levenshtein distance.
    ///
    /// Returns `Some(name)` if a name within the distance threshold is found,
    /// otherwise `None`.
    fn find_similar_name(&self, name: &str) -> Option<String> {
        find_similar_in(name, self.env.names())
    }

    /// Computes the source line number from a span's byte offset.
    fn span_to_line(source_start: u32) -> u32 {
        // Use byte offset as a rough line proxy (precise line calculation
        // requires source text, which we don't have here). The span start
        // provides enough context for the error message.
        source_start
    }

    /// Checks an if-expression.
    ///
    /// The condition must be `Bool`. If there is an else branch, both branches
    /// must have the same type (which becomes the type of the if-expression).
    /// Without an else branch, the then-branch is checked and the result is `Unit`.
    fn check_if(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
        span: Span,
    ) -> Result<Type> {
        let cond_ty = self.infer_expr(condition)?;
        TypeEnv::check_eq(&Type::Bool, &cond_ty, expr_span(condition))?;

        let then_ty = self.infer_block(then_branch)?;

        if let Some(else_block) = else_branch {
            let else_ty = self.infer_block(else_block)?;
            TypeEnv::check_eq(&then_ty, &else_ty, span)?;
            Ok(then_ty)
        } else {
            Ok(Type::Unit)
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{Meta, MetaEntry, NodeId, Param, TypeExpr};

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
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions,
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
        // After checking, "x" should not be in scope for the next function.
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
        // This should fail because "obj" is undefined, not because of field access.
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
        // while loop body with a type error inside
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
            &Type::Enum("Option__Int".to_string()),
        ));
    }

    #[test]
    fn compatible_enum_types_unresolved_param() {
        // Option__Int should be compatible with Option__? (unresolved)
        assert!(TypeChecker::compatible_enum_types(
            &Type::Enum("Option__Int".to_string()),
            &Type::Enum("Option__?".to_string()),
        ));
    }

    #[test]
    fn compatible_enum_types_different_base() {
        assert!(!TypeChecker::compatible_enum_types(
            &Type::Enum("Option__Int".to_string()),
            &Type::Enum("Result__Int".to_string()),
        ));
    }

    #[test]
    fn compatible_enum_types_non_enum() {
        assert!(!TypeChecker::compatible_enum_types(
            &Type::Int,
            &Type::Enum("Option__Int".to_string()),
        ));
    }

    #[test]
    fn monomorphize_option_int_registers_in_enum_registry() {
        let enum_decl = kodo_ast::EnumDecl {
            id: NodeId(10),
            span: Span::new(0, 50),
            name: "Option".to_string(),
            generic_params: vec!["T".to_string()],
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

        // Use a function that references Option<Int> so monomorphization triggers.
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

        // Verify Option__Int was registered in the enum registry.
        assert!(
            checker.enum_registry().contains_key("Option__Int"),
            "Option__Int should be in enum_registry, got keys: {:?}",
            checker.enum_registry().keys().collect::<Vec<_>>()
        );

        // Verify the monomorphized variants have the correct types.
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
            generic_params: vec!["T".to_string()],
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

        // Option expects 1 type arg, but we give 2.
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
            generic_params: vec!["T".to_string()],
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

        // Use generic name "Option" without type arguments.
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
            generic_params: vec!["T".to_string()],
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

        // Use both Option::Some(42) and Option::None in the same function.
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
            generic_params: vec!["T".to_string()],
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

        // Declare Option<Int> but pass a Bool to Some.
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
            generic_params: vec!["T".to_string()],
            fields: vec![kodo_ast::FieldDef {
                name: "value".to_string(),
                ty: TypeExpr::Named("T".to_string()),
                span: Span::new(0, 20),
            }],
        };

        // Reference Wrapper<Int> in a function param type.
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

        // Verify Wrapper__Int was registered.
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
            generic_params: vec!["T".to_string()],
            fields: vec![kodo_ast::FieldDef {
                name: "value".to_string(),
                ty: TypeExpr::Named("T".to_string()),
                span: Span::new(0, 20),
            }],
        };

        // Wrapper expects 1 type arg, but we give 2.
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
        // |x: Int| x + 1 should infer type (Int) -> Int
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
        // |x| x should error because x has no type annotation
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
                    span: Span::new(85, 115),
                }],
            }],
            impl_blocks: vec![kodo_ast::ImplBlock {
                id: NodeId(3),
                span: Span::new(120, 180),
                trait_name: "Summable".to_string(),
                type_name: "Point".to_string(),
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

        // Verify method lookup was registered
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
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![kodo_ast::ImplBlock {
                id: NodeId(1),
                span: Span::new(50, 80),
                trait_name: "NonExistent".to_string(),
                type_name: "Int".to_string(),
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
                    span: Span::new(68, 78),
                }],
            }],
            impl_blocks: vec![kodo_ast::ImplBlock {
                id: NodeId(3),
                span: Span::new(80, 95),
                trait_name: "Describable".to_string(),
                type_name: "Point".to_string(),
                methods: vec![], // Missing the required `describe` method
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
        // Add a requires clause.
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
        // Add an ensures clause.
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
        // a_func(@confidence(0.95)) calls b_func(@confidence(0.5)).
        // Effective confidence of a_func = min(0.95, 0.5) = 0.5.
        // b_func has @reviewed_by to satisfy E0260 (< 0.8 without review rule).
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
        // weak_fn has @confidence(0.5) + @reviewed_by (passes E0260).
        // main calls weak_fn, so main's computed confidence = 0.5.
        // The module declares min_confidence: 0.9 — must fail with E0261.
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
        // let x = "hello"; let y = x; println(x) — use after move
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
                // Attempt to use x after it was moved to y.
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
        // let x = "hello"; let y = x — no error (x is moved but not reused)
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
        // Primitives (Int, Bool) have implicit Copy semantics and should not trigger move.
        // let x: Int = 42; let y: Int = x; print_int(x) — x is still usable.
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
        // let x: Int = 42; let y: Int = xz should suggest "x"
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
        // let x: Foo = ...; let y: Foo = x; use(x) should fail with E0240
        // We need a struct to test non-Copy move semantics via let.
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
        // fn bad(ref s: String) -> String { return s }
        // Returning a borrowed parameter should fail with E0241.
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
        // fn good(own s: String) -> String { return s }
        // Returning an owned parameter should succeed.
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

        // String.length() -> Int
        let key = ("String".to_string(), "length".to_string());
        let (mangled, params, ret) = lookup
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
        assert_eq!(mangled, "String_length");
        assert_eq!(params, vec![Type::String]);
        assert_eq!(ret, Type::Int);

        // String.contains(String) -> Bool
        let key = ("String".to_string(), "contains".to_string());
        let (mangled, params, ret) = lookup
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
        assert_eq!(mangled, "String_contains");
        assert_eq!(params, vec![Type::String, Type::String]);
        assert_eq!(ret, Type::Bool);

        // String.starts_with(String) -> Bool
        let key = ("String".to_string(), "starts_with".to_string());
        assert!(
            lookup.contains_key(&key),
            "starts_with should be registered"
        );

        // String.ends_with(String) -> Bool
        let key = ("String".to_string(), "ends_with".to_string());
        assert!(lookup.contains_key(&key), "ends_with should be registered");

        // String.trim() -> String
        let key = ("String".to_string(), "trim".to_string());
        let (_, _, ret) = lookup
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
        assert_eq!(ret, Type::String);

        // String.to_upper() -> String
        let key = ("String".to_string(), "to_upper".to_string());
        assert!(lookup.contains_key(&key), "to_upper should be registered");

        // String.to_lower() -> String
        let key = ("String".to_string(), "to_lower".to_string());
        assert!(lookup.contains_key(&key), "to_lower should be registered");

        // String.substring(Int, Int) -> String
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

        // Int.to_string() -> String
        let key = ("Int".to_string(), "to_string".to_string());
        let (mangled, params, ret) = lookup
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
        assert_eq!(mangled, "Int_to_string");
        assert_eq!(params, vec![Type::Int]);
        assert_eq!(ret, Type::String);

        // Int.to_float64() -> Float64
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

        // Float64.to_string() -> String
        let key = ("Float64".to_string(), "to_string".to_string());
        let (mangled, _, ret) = lookup
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
        assert_eq!(mangled, "Float64_to_string");
        assert_eq!(ret, Type::String);

        // Float64.to_int() -> Int
        let key = ("Float64".to_string(), "to_int".to_string());
        let (_, _, ret) = lookup
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (String::new(), vec![], Type::Unit));
        assert_eq!(ret, Type::Int);
    }

    #[test]
    fn string_method_call_typechecks() {
        // Test that "hello".length() type-checks via the full module pipeline.
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
        // Call "hello".lenght() — should suggest "length".
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
            super::find_similar_in("lenght", candidates.into_iter()),
            Some("length".to_string())
        );
        assert_eq!(
            super::find_similar_in("contans", vec!["contains", "length"].into_iter()),
            Some("contains".to_string())
        );
        // No match when distance is too large.
        assert_eq!(
            super::find_similar_in("xyz", vec!["contains", "length"].into_iter()),
            None
        );
    }

    #[test]
    fn list_functions_registered() {
        let checker = TypeChecker::new();
        let list_new_ty = checker.env.lookup("list_new");
        assert!(list_new_ty.is_some(), "list_new should be registered");
        let list_push_ty = checker.env.lookup("list_push");
        assert!(list_push_ty.is_some(), "list_push should be registered");
        let list_get_ty = checker.env.lookup("list_get");
        assert!(list_get_ty.is_some(), "list_get should be registered");
        let list_length_ty = checker.env.lookup("list_length");
        assert!(list_length_ty.is_some(), "list_length should be registered");
        let list_contains_ty = checker.env.lookup("list_contains");
        assert!(
            list_contains_ty.is_some(),
            "list_contains should be registered"
        );
    }

    #[test]
    fn map_functions_registered() {
        let checker = TypeChecker::new();
        let map_new_ty = checker.env.lookup("map_new");
        assert!(map_new_ty.is_some(), "map_new should be registered");
        let map_insert_ty = checker.env.lookup("map_insert");
        assert!(map_insert_ty.is_some(), "map_insert should be registered");
        let map_get_ty = checker.env.lookup("map_get");
        assert!(map_get_ty.is_some(), "map_get should be registered");
    }

    #[test]
    fn string_split_method_registered() {
        let checker = TypeChecker::new();
        let lookup = checker.method_lookup();
        let split = lookup.get(&("String".to_string(), "split".to_string()));
        assert!(split.is_some(), "String.split should be registered");
        let (mangled, params, ret) = split.unwrap();
        assert_eq!(mangled, "String_split");
        assert_eq!(params.len(), 2); // self + separator
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
        // Simulate importing helper module then calling helper.double()
        let module = kodo_parser::parse(source).unwrap();
        let mut checker = TypeChecker::new();
        // First, type check the helper module to register its functions
        let _ = checker.check_module(&module);
        // Register "helper" as an imported module name
        checker.register_imported_module("helper".to_string());
        // Now "double" should be accessible, and "helper.double(1)" should resolve
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
}
