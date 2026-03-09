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
    Annotation, AnnotationArg, BinOp, Block, Expr, Function, Module, Span, Stmt, UnaryOp,
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
            | Self::NonExhaustiveMatch { span, .. } => Some(*span),
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
            Self::PolicyViolation { .. } => "E0350",
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

/// Extracts the source [`Span`] from an expression.
fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::IntLit(_, span)
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
        | Expr::Match { span, .. } => *span,
        Expr::Block(block) => block.span,
    }
}

/// Extracts the expression from an annotation argument.
fn annotation_arg_expr(arg: &AnnotationArg) -> &Expr {
    match arg {
        AnnotationArg::Positional(e) | AnnotationArg::Named(_, e) => e,
    }
}

/// The type checker walks an AST and verifies that all expressions and
/// statements are well-typed according to Kōdo's type system.
///
/// Implements a single-pass, top-down type checking algorithm based on
/// **\[TAPL\]** Ch. 9 (simply typed lambda calculus). The checker maintains
/// a [`TypeEnv`] with scope-based binding management: the environment length
/// is saved before entering a scope and restored upon exit, ensuring
/// correct variable shadowing and lexical scoping.
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
        };
        checker.register_builtins();
        checker
    }

    /// Registers builtin functions in the type environment.
    ///
    /// These are functions provided by the runtime that do not need to be
    /// declared in user code. Currently registers:
    /// - `println(String) -> ()`
    /// - `print(String) -> ()`
    /// - `print_int(Int) -> ()`
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
    }

    /// Type-checks an entire module.
    ///
    /// Registers all function signatures first (enabling mutual recursion),
    /// then checks each function body.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if any type inconsistency is found.
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
            let mut fields = Vec::new();
            for field in &type_decl.fields {
                let ty = resolve_type(&field.ty, field.span)?;
                fields.push((field.name.clone(), ty));
            }
            self.struct_registry.insert(type_decl.name.clone(), fields);
        }

        // Register enum types.
        for enum_decl in &module.enum_decls {
            self.enum_names.insert(enum_decl.name.clone());
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
        }

        // First pass: register all function signatures so they can call each other.
        for func in &module.functions {
            let param_types: std::result::Result<Vec<_>, _> = func
                .params
                .iter()
                .map(|p| resolve_type_with_enums(&p.ty, p.span, &self.enum_names))
                .collect();
            let param_types = param_types?;
            let ret_type = resolve_type_with_enums(&func.return_type, func.span, &self.enum_names)?;
            self.env.insert(
                func.name.clone(),
                Type::Function(param_types, Box::new(ret_type)),
            );
        }

        // Second pass: check each function body.
        for func in &module.functions {
            self.check_function(func)?;
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
                // If written as @confidence("0.95")
                Expr::StringLit(s, _) => return s.parse::<f64>().ok(),
                _ => {}
            }
        }
        None
    }

    /// Checks if a function has a `@reviewed_by` annotation with a human reviewer.
    fn has_human_review(func: &Function) -> bool {
        func.annotations
            .iter()
            .filter(|a| a.name == "reviewed_by")
            .any(|a| {
                a.args.iter().any(|arg| {
                    let expr = annotation_arg_expr(arg);
                    matches!(expr, Expr::StringLit(s, _) if s.starts_with("human:"))
                })
            })
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
        let ret_type = resolve_type_with_enums(&func.return_type, func.span, &self.enum_names)?;
        let prev_return_type = self.current_return_type.clone();
        self.current_return_type = ret_type.clone();

        // Bind parameters in the function scope.
        for param in &func.params {
            let ty = resolve_type_with_enums(&param.ty, param.span, &self.enum_names)?;
            self.env.insert(param.name.clone(), ty);
        }

        self.check_block(&func.body)?;

        // Restore the previous scope and return type.
        self.env.truncate(scope);
        self.current_return_type = prev_return_type;

        Ok(())
    }

    /// Type-checks a block of statements.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if any statement in the block is ill-typed.
    pub fn check_block(&mut self, block: &Block) -> Result<()> {
        let scope = self.env.scope_level();
        for stmt in &block.stmts {
            self.check_stmt(stmt)?;
        }
        self.env.truncate(scope);
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
                    let expected = resolve_type_with_enums(annotation, *span, &self.enum_names)?;
                    TypeEnv::check_eq(&expected, &value_ty, *span)?;
                    self.env.insert(name.clone(), expected);
                } else {
                    self.env.insert(name.clone(), value_ty);
                }
                Ok(())
            }
            Stmt::Return { span, value } => {
                let value_ty = match value {
                    Some(expr) => self.infer_expr(expr)?,
                    None => Type::Unit,
                };
                TypeEnv::check_eq(&self.current_return_type, &value_ty, *span)
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
            Stmt::Assign {
                span, name, value, ..
            } => {
                let value_ty = self.infer_expr(value)?;
                let existing_ty =
                    self.env
                        .lookup(name)
                        .cloned()
                        .ok_or_else(|| TypeError::Undefined {
                            name: name.clone(),
                            span: *span,
                        })?;
                TypeEnv::check_eq(&existing_ty, &value_ty, *span)?;
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
            Expr::StringLit(_, _) => Ok(Type::String),
            Expr::BoolLit(_, _) => Ok(Type::Bool),

            Expr::Ident(name, span) => {
                self.env
                    .lookup(name)
                    .cloned()
                    .ok_or_else(|| TypeError::Undefined {
                        name: name.clone(),
                        span: *span,
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
                        field_ty.ok_or_else(|| TypeError::NoSuchField {
                            field: field.clone(),
                            type_name: name.clone(),
                            span: *span,
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
                        return Err(TypeError::ExtraStructField {
                            field: field.name.clone(),
                            struct_name: name.clone(),
                            span: field.span,
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
                let variants =
                    self.enum_registry
                        .get(enum_name)
                        .ok_or_else(|| TypeError::UnknownEnum {
                            name: enum_name.clone(),
                            span: *span,
                        })?;
                let variant_def = variants.iter().find(|(n, _)| n == variant).ok_or_else(|| {
                    TypeError::UnknownVariant {
                        variant: variant.clone(),
                        enum_name: enum_name.clone(),
                        span: *span,
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
                Ok(Type::Enum(enum_name.clone()))
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
                            let resolved_enum =
                                enum_name
                                    .as_ref()
                                    .or(if let Type::Enum(name) = &matched_ty {
                                        Some(name)
                                    } else {
                                        None
                                    });
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
            }
        }
        Ok(last_ty)
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
    fn check_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Result<Type> {
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
                for (param_ty, arg) in param_types.iter().zip(args) {
                    let arg_ty = self.infer_expr(arg)?;
                    TypeEnv::check_eq(param_ty, &arg_ty, expr_span(arg))?;
                }
                Ok(*ret_type)
            }
            _ => Err(TypeError::NotCallable {
                found: callee_ty.to_string(),
                span,
            }),
        }
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
                },
                Param {
                    name: "b".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: Span::new(7, 12),
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
                },
                Param {
                    name: "b".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: Span::new(7, 12),
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
                },
                Param {
                    name: "b".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: Span::new(7, 12),
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
            meta: Some(Meta {
                id: NodeId(99),
                span: Span::new(0, 50),
                entries,
            }),
            type_decls: vec![],
            enum_decls: vec![],
            functions,
        }
    }

    /// Helper to build a function with annotations.
    fn make_function_with_annotations(name: &str, annotations: Vec<Annotation>) -> Function {
        Function {
            id: NodeId(1),
            span: Span::new(0, 100),
            name: name.to_string(),
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
        };
        assert_eq!(err.span(), Some(Span::new(3, 4)));
    }
}
