//! Inkwell-based LLVM backend for Kōdo.
//!
//! Uses the inkwell crate (safe Rust bindings to LLVM C API) for
//! programmatic IR construction, enabling the full LLVM optimization
//! pipeline including inlining, loop vectorization, and GVN.
//!
//! This module replaces the textual IR emitter when the `inkwell` feature
//! is enabled, producing significantly faster binaries at the cost of
//! requiring LLVM to be installed on the build system.

#[cfg(feature = "inkwell")]
mod compiler;
#[cfg(feature = "inkwell")]
mod types;

#[cfg(feature = "inkwell")]
pub use compiler::compile_module;
