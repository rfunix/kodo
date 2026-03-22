//! # `kodo_codegen_llvm` — LLVM Code Generation Backend for the Kodo Compiler
//!
//! This crate translates [`kodo_mir`] into native code using the LLVM C API
//! (via the [inkwell](https://github.com/TheDan64/inkwell) crate). It enables
//! the full LLVM optimization pipeline including inlining, loop vectorization,
//! and scalar optimizations.
//!
//! ## Usage
//!
//! ```ignore
//! kodo_codegen_llvm::compile_module(
//!     &mir_functions, &struct_defs, &enum_defs, opt_level,
//!     &output_path, Some(&metadata_json),
//! )?;
//! // Produces output_path.o which can be linked with the runtime.
//! ```
//!
//! ## Academic References
//!
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 9–11 — Instruction
//!   selection and code emission strategies.
//! - **\[EC\]** *Engineering a Compiler* Ch. 11–13 — Target-independent IR
//!   generation and lowering to machine-specific representations.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![warn(clippy::pedantic)]
// Inkwell builder calls return Result for API uniformity but are infallible
// in practice (they only fail when the builder has no insertion point).
#![allow(clippy::unwrap_used)]

mod inkwell_backend;

pub use inkwell_backend::{compile_module, emit_ir};
