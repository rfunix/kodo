//! Command implementations for the kodoc CLI.
//!
//! Each submodule contains one or more related command handlers that are
//! dispatched from `main()`. Shared helper functions live in `common`.

pub(crate) mod build;
pub(crate) mod check;
pub(crate) mod common;
pub(crate) mod misc;
pub(crate) mod test;
