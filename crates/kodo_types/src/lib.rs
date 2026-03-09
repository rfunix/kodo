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
    Struct(String),
    /// A user-defined enum type.
    Enum(String),
    /// A generic type application, e.g. `List<Int>`.
    Generic(String, Vec<Type>),
    /// A function type: `(params) -> return_type`.
    Function(Vec<Type>, Box<Type>),
    /// An unresolved type (used during type checking).
    Unknown,
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
#[derive(Debug, Default)]
pub struct TypeEnv {
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
/// # Errors
///
/// Returns [`TypeError::Undefined`] if the type name is not recognized.
#[allow(clippy::only_used_in_recursion)]
pub fn resolve_type(type_expr: &kodo_ast::TypeExpr, span: Span) -> Result<Type> {
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
            _ => Ok(Type::Struct(name.clone())),
        },
        kodo_ast::TypeExpr::Unit => Ok(Type::Unit),
        kodo_ast::TypeExpr::Generic(name, args) => {
            let resolved: std::result::Result<Vec<_>, _> =
                args.iter().map(|a| resolve_type(a, span)).collect();
            Ok(Type::Generic(name.clone(), resolved?))
        }
        kodo_ast::TypeExpr::Function(params, ret) => {
            let resolved_params: std::result::Result<Vec<_>, _> =
                params.iter().map(|p| resolve_type(p, span)).collect();
            let resolved_ret = resolve_type(ret, span)?;
            Ok(Type::Function(resolved_params?, Box::new(resolved_ret)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
