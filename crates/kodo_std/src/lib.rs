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

/// Source code for the stdlib `Option<T>` module.
pub const OPTION_SOURCE: &str = r#"module option {
    meta {
        purpose: "Optional value type"
        version: "0.1.0"
    }

    enum Option<T> {
        Some(T),
        None
    }
}
"#;

/// Source code for the stdlib `Result<T, E>` module.
pub const RESULT_SOURCE: &str = r#"module result {
    meta {
        purpose: "Error handling type"
        version: "0.1.0"
    }

    enum Result<T, E> {
        Ok(T),
        Err(E)
    }
}
"#;

/// Returns the source code for all prelude modules.
///
/// These modules are implicitly available in every Kōdo program.
#[must_use]
pub fn prelude_sources() -> Vec<(&'static str, &'static str)> {
    vec![("std/option", OPTION_SOURCE), ("std/result", RESULT_SOURCE)]
}

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
#[allow(clippy::too_many_lines)]
pub fn builtin_functions() -> Vec<BuiltinFunction> {
    vec![
        // I/O
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
        // Math
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
        BuiltinFunction {
            name: "kodo::math::min".to_string(),
            description: "Returns the minimum of two integers".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::math::max".to_string(),
            description: "Returns the maximum of two integers".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::math::clamp".to_string(),
            description: "Clamps a value between a minimum and maximum".to_string(),
            param_count: 3,
        },
        // String methods
        BuiltinFunction {
            name: "kodo::string::length".to_string(),
            description: "Returns the length of a string in bytes".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::string::contains".to_string(),
            description: "Returns true if the string contains the given substring".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::string::starts_with".to_string(),
            description: "Returns true if the string starts with the given prefix".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::string::ends_with".to_string(),
            description: "Returns true if the string ends with the given suffix".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::string::trim".to_string(),
            description: "Returns the string with leading and trailing whitespace removed"
                .to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::string::to_upper".to_string(),
            description: "Returns an uppercase copy of the string".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::string::to_lower".to_string(),
            description: "Returns a lowercase copy of the string".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::string::substring".to_string(),
            description: "Returns a substring from start to end byte index".to_string(),
            param_count: 3,
        },
        BuiltinFunction {
            name: "kodo::string::concat".to_string(),
            description: "Concatenates two strings".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::string::index_of".to_string(),
            description:
                "Returns the byte index of the first occurrence of a substring, or -1 if not found"
                    .to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::string::replace".to_string(),
            description: "Replaces all occurrences of a pattern with a replacement string"
                .to_string(),
            param_count: 2,
        },
        // Int methods
        BuiltinFunction {
            name: "kodo::int::to_string".to_string(),
            description: "Converts an integer to its string representation".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::int::to_float64".to_string(),
            description: "Converts an integer to a 64-bit float".to_string(),
            param_count: 1,
        },
        // Float64 methods
        BuiltinFunction {
            name: "kodo::float64::to_string".to_string(),
            description: "Converts a float to its string representation".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::float64::to_int".to_string(),
            description: "Converts a float to an integer (truncates toward zero)".to_string(),
            param_count: 1,
        },
        // File I/O
        BuiltinFunction {
            name: "kodo::io::file_exists".to_string(),
            description: "Checks if a file exists at the given path".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::io::file_read".to_string(),
            description: "Reads a file to a string, returning Result<String, String>".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::io::file_write".to_string(),
            description: "Writes content to a file, returning Result<Unit, String>".to_string(),
            param_count: 2,
        },
        // List operations
        BuiltinFunction {
            name: "kodo::list::new".to_string(),
            description: "Creates a new empty list".to_string(),
            param_count: 0,
        },
        BuiltinFunction {
            name: "kodo::list::push".to_string(),
            description: "Adds an element to the end of a list".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::list::get".to_string(),
            description: "Gets an element by index, returning Option".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::list::length".to_string(),
            description: "Returns the number of elements in a list".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::list::contains".to_string(),
            description: "Returns true if the list contains the given element".to_string(),
            param_count: 2,
        },
        // Map operations
        BuiltinFunction {
            name: "kodo::map::new".to_string(),
            description: "Creates a new empty map".to_string(),
            param_count: 0,
        },
        BuiltinFunction {
            name: "kodo::map::insert".to_string(),
            description: "Inserts a key-value pair into the map".to_string(),
            param_count: 3,
        },
        BuiltinFunction {
            name: "kodo::map::get".to_string(),
            description: "Gets a value by key, returning Option".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::map::contains_key".to_string(),
            description: "Returns true if the map contains the given key".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::map::length".to_string(),
            description: "Returns the number of entries in the map".to_string(),
            param_count: 1,
        },
        // HTTP client
        BuiltinFunction {
            name: "kodo::http::get".to_string(),
            description: "Performs an HTTP GET request and returns the response body".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::http::post".to_string(),
            description:
                "Performs an HTTP POST request with the given body and returns the response"
                    .to_string(),
            param_count: 2,
        },
        // JSON parsing
        BuiltinFunction {
            name: "kodo::json::parse".to_string(),
            description: "Parses a JSON string and returns an opaque handle".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::json::get_string".to_string(),
            description: "Gets a string value from a JSON object by key".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::json::get_int".to_string(),
            description: "Gets an integer value from a JSON object by key".to_string(),
            param_count: 2,
        },
        BuiltinFunction {
            name: "kodo::json::free".to_string(),
            description: "Frees a parsed JSON handle".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::time::now".to_string(),
            description: "Returns the current Unix timestamp in seconds".to_string(),
            param_count: 0,
        },
        BuiltinFunction {
            name: "kodo::time::now_ms".to_string(),
            description: "Returns the current Unix timestamp in milliseconds".to_string(),
            param_count: 0,
        },
        BuiltinFunction {
            name: "kodo::time::format".to_string(),
            description: "Formats a Unix timestamp as an ISO 8601 string".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::time::elapsed_ms".to_string(),
            description: "Returns elapsed milliseconds since a start timestamp".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::env::get".to_string(),
            description: "Gets an environment variable value".to_string(),
            param_count: 1,
        },
        BuiltinFunction {
            name: "kodo::env::set".to_string(),
            description: "Sets an environment variable".to_string(),
            param_count: 2,
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

    #[test]
    fn builtin_functions_count() {
        let builtins = builtin_functions();
        assert_eq!(builtins.len(), 48);
    }

    #[test]
    fn all_builtins_have_descriptions() {
        let builtins = builtin_functions();
        for b in &builtins {
            assert!(
                !b.description.is_empty(),
                "builtin {} has empty description",
                b.name
            );
        }
    }

    #[test]
    fn all_builtins_have_qualified_names() {
        let builtins = builtin_functions();
        for b in &builtins {
            assert!(
                b.name.starts_with("kodo::"),
                "builtin {} should start with kodo::",
                b.name
            );
        }
    }

    #[test]
    fn readln_is_registered() {
        let builtins = builtin_functions();
        let readln = builtins.iter().find(|f| f.name == "kodo::io::readln");
        assert!(readln.is_some());
        let readln = readln.unwrap();
        assert_eq!(readln.param_count, 0);
    }

    #[test]
    fn math_builtins_registered() {
        let builtins = builtin_functions();
        let abs = builtins.iter().find(|f| f.name == "kodo::math::abs");
        assert!(abs.is_some());
        assert_eq!(abs.unwrap().param_count, 1);

        let sqrt = builtins.iter().find(|f| f.name == "kodo::math::sqrt");
        assert!(sqrt.is_some());
        assert_eq!(sqrt.unwrap().param_count, 1);
    }

    #[test]
    fn list_builtins_registered() {
        let builtins = builtin_functions();
        let list_new = builtins.iter().find(|f| f.name == "kodo::list::new");
        assert!(list_new.is_some());
        assert_eq!(list_new.unwrap().param_count, 0);
    }

    #[test]
    fn map_builtins_registered() {
        let builtins = builtin_functions();
        let map_new = builtins.iter().find(|f| f.name == "kodo::map::new");
        assert!(map_new.is_some());
        assert_eq!(map_new.unwrap().param_count, 0);
    }

    #[test]
    fn error_display_formats() {
        let io_err = StdError::Io("disk full".to_string());
        assert!(io_err.to_string().contains("disk full"));

        let range_err = StdError::OutOfRange("index 5".to_string());
        assert!(range_err.to_string().contains("index 5"));

        let arg_err = StdError::InvalidArgument("negative".to_string());
        assert!(arg_err.to_string().contains("negative"));
    }
}
