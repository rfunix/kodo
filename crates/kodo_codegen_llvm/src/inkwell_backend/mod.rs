//! Inkwell-based LLVM backend for Kōdo.
//!
//! Uses the inkwell crate (safe Rust bindings to LLVM C API) for
//! programmatic IR construction, enabling the full LLVM optimization
//! pipeline including inlining, loop vectorization, and GVN.
//!
//! This module replaces the textual IR emitter when the `inkwell` feature
//! is enabled, producing significantly faster binaries at the cost of
//! requiring LLVM to be installed on the build system.

// Inkwell builder calls return `Result` for API uniformity but are infallible
// in practice (they only fail when the builder has no insertion point, which
// we always set). Suppress `clippy::unwrap_used` for these modules.
#[cfg(feature = "inkwell")]
#[allow(clippy::unwrap_used)]
mod builtins;
#[cfg(feature = "inkwell")]
#[allow(clippy::unwrap_used)]
mod compiler;
#[cfg(feature = "inkwell")]
#[allow(clippy::unwrap_used)]
mod instruction;
#[cfg(feature = "inkwell")]
#[allow(clippy::unwrap_used)]
mod terminator;
#[cfg(feature = "inkwell")]
pub(crate) mod types;
#[cfg(feature = "inkwell")]
#[allow(clippy::unwrap_used)]
mod value;

#[cfg(feature = "inkwell")]
pub use compiler::{compile_module, emit_ir};
