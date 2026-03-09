//! # `kodo_codegen` — Code Generation Backend for the Kōdo Compiler
//!
//! This crate translates [`kodo_mir`] into native machine code using the
//! [Cranelift](https://cranelift.dev/) code generator.
//!
//! Cranelift was chosen over LLVM for the initial implementation because:
//! - Faster compilation (critical for tight AI agent feedback loops)
//! - Pure Rust (no C++ dependency)
//! - Good enough optimization for development builds
//!
//! An LLVM backend may be added later for optimized release builds.
//!
//! ## Current Status
//!
//! Stub implementation — the Cranelift infrastructure is set up but no
//! actual code generation is implemented yet.
//!
//! ## Academic References
//!
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 9–11 — Instruction
//!   selection via tree-pattern matching, register allocation via graph coloring.
//! - **\[EC\]** *Engineering a Compiler* Ch. 11–13 — Instruction selection,
//!   scheduling, and register allocation (delegated to Cranelift).
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use thiserror::Error;

/// Errors from code generation.
#[derive(Debug, Error)]
pub enum CodegenError {
    /// A Cranelift error occurred.
    #[error("cranelift error: {0}")]
    Cranelift(String),
    /// An unsupported MIR construct was encountered.
    #[error("unsupported MIR construct: {0}")]
    Unsupported(String),
    /// The target architecture is not supported.
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, CodegenError>;

/// Code generation options.
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    /// Whether to optimize the generated code.
    pub optimize: bool,
    /// Whether to emit debug information.
    pub debug_info: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            optimize: false,
            debug_info: true,
        }
    }
}

/// Compiles a MIR function into native code (stub).
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
pub fn compile_function(
    _function: &kodo_mir::MirFunction,
    _options: &CodegenOptions,
) -> Result<Vec<u8>> {
    // Stub: return empty bytes
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_mir::{BasicBlock, BlockId, MirFunction, Terminator, Value};
    use kodo_types::Type;

    #[test]
    fn compile_stub_returns_empty() {
        let func = MirFunction {
            name: "test".to_string(),
            return_type: Type::Unit,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_function(&func, &CodegenOptions::default());
        assert!(result.is_ok());
    }

    #[test]
    fn default_options_no_optimize() {
        let opts = CodegenOptions::default();
        assert!(!opts.optimize);
        assert!(opts.debug_info);
    }
}
