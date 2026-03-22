//! # `kodo_codegen_llvm` — LLVM Code Generation Backend for the Kodo Compiler
//!
//! This crate translates [`kodo_mir`] into native code using the LLVM C API
//! (via the [inkwell](https://github.com/TheDan64/inkwell) crate). It enables
//! the full LLVM optimization pipeline including inlining, loop vectorization,
//! and scalar optimizations.
//!
//! Requires the `llvm` feature to be enabled and LLVM to be installed on the
//! build system. Without the feature, the crate is a no-op stub.
//!
//! ## Usage
//!
//! ```ignore
//! kodo_codegen_llvm::compile_module(
//!     &mir_functions, &struct_defs, &enum_defs, opt_level,
//!     &output_path, Some(&metadata_json),
//! )?;
//! ```

#![deny(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::unwrap_used)]

#[cfg(feature = "llvm")]
mod inkwell_backend;

#[cfg(feature = "llvm")]
pub use inkwell_backend::{compile_module, emit_ir};
