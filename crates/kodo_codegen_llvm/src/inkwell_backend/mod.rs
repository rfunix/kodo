//! LLVM backend for Kōdo using the inkwell crate (LLVM C API).
//!
//! Provides programmatic IR construction with the full LLVM optimization
//! pipeline including inlining, loop vectorization, and GVN.

mod builtins;
mod compiler;
mod instruction;
mod terminator;
pub(crate) mod types;
mod value;

pub use compiler::{compile_module, emit_ir};
