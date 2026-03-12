//! Core type representation and type environment for the Kōdo language.
//!
//! Contains the [`Type`] enum representing all Kōdo types, the [`TypeEnv`]
//! for managing variable bindings, and type resolution functions.

use crate::{Result, Type, TypeError};
use kodo_ast::Span;

/// A type environment that maps names to their types.
///
/// Uses a `Vec` with reverse lookup to support shadowing.
/// Scoping is managed by saving and restoring the environment length.
#[derive(Debug, Default)]
pub struct TypeEnv {
    /// Ordered list of bindings; latest first for shadowing.
    bindings: Vec<(String, Type)>,
}

impl TypeEnv {
    /// Creates an empty type environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a binding into the environment.
    pub fn insert(&mut self, name: String, ty: Type) {
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
            return Ok(());
        }
        // Channel<T> is an opaque Int handle at runtime. Allow Channel<T>
        // wherever Int is expected and vice versa, enabling channel functions
        // to accept both typed Channel<T> values and raw Int handles.
        if Self::is_channel_int_compatible(expected, found) {
            return Ok(());
        }
        // dyn Trait accepts any concrete type — trait bound validation
        // is done separately by the type checker with access to trait_impl_set.
        if matches!(expected, Type::DynTrait(_)) {
            return Ok(());
        }
        Err(TypeError::Mismatch {
            expected: expected.to_string(),
            found: found.to_string(),
            span,
        })
    }

    /// Returns `true` if one type is `Channel<T>` and the other is `Int`,
    /// which is allowed because channels are opaque integer handles at runtime.
    fn is_channel_int_compatible(a: &Type, b: &Type) -> bool {
        matches!(
            (a, b),
            (Type::Int, Type::Generic(name, _)) | (Type::Generic(name, _), Type::Int)
            if name == "Channel"
        )
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
    enum_names: &std::collections::HashSet<String>,
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
        kodo_ast::TypeExpr::Tuple(elems) => {
            let resolved: std::result::Result<Vec<_>, _> = elems
                .iter()
                .map(|e| resolve_type_with_enums(e, span, enum_names))
                .collect();
            Ok(Type::Tuple(resolved?))
        }
        kodo_ast::TypeExpr::DynTrait(name) => Ok(Type::DynTrait(name.clone())),
    }
}

/// Finds the most similar name among candidates using Levenshtein distance.
///
/// Returns `Some(name)` if a candidate within the distance threshold is found.
/// The threshold is `max(name.len() / 2, 3)`, ensuring reasonable fuzzy matching.
pub(crate) fn find_similar_in<'a>(
    name: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    let threshold = std::cmp::max(name.len() / 2, 3);
    for candidate in candidates {
        let dist = strsim::levenshtein(name, candidate);
        if dist > 0 && dist <= threshold && best.as_ref().is_none_or(|(d, _)| dist < *d) {
            best = Some((dist, candidate.to_string()));
        }
    }
    best.map(|(_, n)| n)
}

/// Extracts the source [`Span`] from an expression.
pub(crate) fn expr_span(expr: &kodo_ast::Expr) -> Span {
    match expr {
        kodo_ast::Expr::IntLit(_, span)
        | kodo_ast::Expr::FloatLit(_, span)
        | kodo_ast::Expr::StringLit(_, span)
        | kodo_ast::Expr::BoolLit(_, span)
        | kodo_ast::Expr::Ident(_, span)
        | kodo_ast::Expr::BinaryOp { span, .. }
        | kodo_ast::Expr::UnaryOp { span, .. }
        | kodo_ast::Expr::Call { span, .. }
        | kodo_ast::Expr::If { span, .. }
        | kodo_ast::Expr::FieldAccess { span, .. }
        | kodo_ast::Expr::StructLit { span, .. }
        | kodo_ast::Expr::EnumVariantExpr { span, .. }
        | kodo_ast::Expr::Match { span, .. }
        | kodo_ast::Expr::Try { span, .. }
        | kodo_ast::Expr::OptionalChain { span, .. }
        | kodo_ast::Expr::NullCoalesce { span, .. }
        | kodo_ast::Expr::Range { span, .. }
        | kodo_ast::Expr::Closure { span, .. }
        | kodo_ast::Expr::Is { span, .. }
        | kodo_ast::Expr::Await { span, .. }
        | kodo_ast::Expr::StringInterp { span, .. }
        | kodo_ast::Expr::TupleLit(_, span)
        | kodo_ast::Expr::TupleIndex { span, .. } => *span,
        kodo_ast::Expr::Block(block) => block.span,
    }
}

/// Extracts the expression from an annotation argument.
pub(crate) fn annotation_arg_expr(arg: &kodo_ast::AnnotationArg) -> &kodo_ast::Expr {
    match arg {
        kodo_ast::AnnotationArg::Positional(e) | kodo_ast::AnnotationArg::Named(_, e) => e,
    }
}

/// Ownership state of a variable, used for linear/affine ownership tracking.
///
/// Based on **\[ATAPL\]** Ch. 1 — substructural type systems. Each variable
/// has a capability (owned/borrowed) that is consumed when moved and
/// temporarily shared when borrowed.
#[derive(Debug, Clone)]
pub(crate) enum OwnershipState {
    /// The variable owns its value and can be used.
    Owned,
    /// The variable's value has been moved away at the given source line.
    Moved(u32),
    /// The variable is immutably borrowed (shared reference).
    Borrowed,
    /// The variable is mutably borrowed (exclusive reference).
    MutBorrowed,
}

/// Definition of a generic struct (before monomorphization).
#[derive(Clone)]
pub(crate) struct GenericStructDef {
    /// Generic parameter names (e.g. `["T", "U"]`).
    pub(crate) params: Vec<String>,
    /// Trait bounds per parameter (parallel with `params`).
    pub(crate) bounds: Vec<Vec<String>>,
    /// Fields with types that may reference generic params.
    pub(crate) fields: Vec<(String, kodo_ast::TypeExpr)>,
}

/// Definition of a generic function (before monomorphization).
#[derive(Clone)]
pub(crate) struct GenericFunctionDef {
    /// Generic parameter names (e.g. `["T"]`).
    pub(crate) params: Vec<String>,
    /// Trait bounds per parameter (parallel with `params`).
    pub(crate) bounds: Vec<Vec<String>>,
    /// Parameter type expressions (may reference generic params).
    pub(crate) param_types: Vec<kodo_ast::TypeExpr>,
    /// Return type expression.
    pub(crate) return_type: kodo_ast::TypeExpr,
}

/// Definition of a generic enum (before monomorphization).
#[derive(Clone)]
pub(crate) struct GenericEnumDef {
    /// Generic parameter names.
    pub(crate) params: Vec<String>,
    /// Trait bounds per parameter (parallel with `params`).
    pub(crate) bounds: Vec<Vec<String>>,
    /// Variants with field type expressions.
    pub(crate) variants: Vec<(String, Vec<kodo_ast::TypeExpr>)>,
}
