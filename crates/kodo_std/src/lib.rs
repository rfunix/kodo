//! # `kodo_std` — Standard Library for the Kōdo Language
//!
//! This crate provides the standard library types and functions available
//! to all Kōdo programs. It defines the builtin types, core traits, and
//! essential data structures.
//!
//! ## Modules
//!
//! - `core` — Primitives, Result, Option, basic traits
//! - `collections` — List, Map, Set
//! - `string` — UTF-8 string operations
//! - `io` — File I/O, stdin/stdout
//! - `math` — Mathematical operations
//!
//! ## Current Status
//!
//! Stub implementation defining the shape of the standard library.
//! Actual implementations will be added as the compiler matures.
//!
//! ## Academic References
//!
//! - **\[PLP\]** *Programming Language Pragmatics* Ch. 6 — Control flow and
//!   iterator protocol design for collections.
//! - **\[PLP\]** *Programming Language Pragmatics* Ch. 9 — Subroutine calling
//!   conventions and parameter passing modes (`own`/`ref`/`mut`).
//! - **\[PLP\]** *Programming Language Pragmatics* Ch. 13 — Structured
//!   concurrency model: scoped tasks with ownership, no raw threads.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use thiserror::Error;

/// Errors from the standard library.
#[derive(Debug, Error)]
pub enum StdError {
    /// An I/O operation failed.
    #[error("I/O error: {0}")]
    Io(String),
    /// A value was out of the expected range.
    #[error("value out of range: {0}")]
    OutOfRange(String),
    /// An invalid argument was provided.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, StdError>;

/// Describes a builtin function available in the standard library.
#[derive(Debug, Clone)]
pub struct BuiltinFunction {
    /// The fully qualified name (e.g., `kodo::io::println`).
    pub name: String,
    /// Description for documentation.
    pub description: String,
    /// Number of parameters.
    pub param_count: usize,
}

/// Returns the list of builtin functions provided by the standard library.
#[must_use]
pub fn builtin_functions() -> Vec<BuiltinFunction> {
    vec![
        BuiltinFunction {
            name: "kodo::io::println".to_string(),
            description: "Prints a line to standard output".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::io::print".to_string(),
            description: "Prints to standard output without newline".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::io::readln".to_string(),
            description: "Reads a line from standard input".to_string(),
            param_count: 0,
        },
        BuiltinFunction {
            name: "kodo::math::abs".to_string(),
            description: "Returns the absolute value".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::math::sqrt".to_string(),
            description: "Returns the square root".to_string(),
            param_count: 1,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_functions_are_not_empty() {
        let builtins = builtin_functions();
        assert!(!builtins.is_empty());
    }

    #[test]
    fn println_is_registered() {
        let builtins = builtin_functions();
        let println = builtins.iter().find(|f| f.name == "kodo::io::println");
        assert!(println.is_some());
        let println = println.unwrap_or_else(|| panic!("already checked"));
        assert_eq!(println.param_count, 1);
    }
}
