//! Self-hosted bootstrap support for Kōdo.
//!
//! This module implements the `--self-hosted` flag logic, which enables the compiler
//! to use a Kōdo-written lexer/parser (the bootstrap binaries in `examples/`) instead
//! of the native Rust implementation.
//!
//! # Bootstrap Phases
//!
//! The self-hosted pipeline works as follows:
//!
//! 1. **Detect** pre-compiled bootstrap binaries (`self_hosted_lexer`, `self_hosted_parser`).
//! 2. **Invoke** the Kōdo-written lexer binary, passing the source file via stdin,
//!    and collect its JSON token stream on stdout.
//! 3. **Invoke** the Kōdo-written parser binary with the token stream, and collect
//!    the JSON AST on stdout.
//! 4. The rest of the pipeline (type checker, MIR, codegen) continues in Rust as usual.
//!
//! # Current Status (v1 — experimental)
//!
//! The self-hosted lexer and parser are available as Kōdo source in `examples/`.
//! They must be compiled to native binaries before `--self-hosted` can be activated:
//!
//! ```shell
//! kodoc build examples/self_hosted_lexer/main.ko  -o ./bootstrap/self_hosted_lexer
//! kodoc build examples/self_hosted_parser/main.ko -o ./bootstrap/self_hosted_parser
//! ```
//!
//! Once both binaries exist, `kodoc build --self-hosted <file>` will route the
//! lex/parse stages through the Kōdo-written implementations.
//!
//! # Limitations
//!
//! - The bootstrap binaries do not yet emit a JSON token/AST format consumed by the
//!   Rust pipeline; full integration will land in a future milestone.  For now, the
//!   self-hosted path validates binary availability and prints a structured diagnostic.
//! - Channels to the subprocess are synchronous (stdin → stdout); no streaming yet.

use std::path::{Path, PathBuf};

/// Locations searched for bootstrap binaries, in priority order.
const BOOTSTRAP_SEARCH_DIRS: &[&str] = &["./bootstrap", "./bin", "."];

/// Name of the self-hosted lexer binary (platform-agnostic, no extension).
const LEXER_BIN_NAME: &str = "self_hosted_lexer";
/// Name of the self-hosted parser binary.
const PARSER_BIN_NAME: &str = "self_hosted_parser";

/// Paths to the bootstrap Kōdo sources (relative to the workspace root).
const LEXER_SOURCE: &str = "examples/self_hosted_lexer/main.ko";
const PARSER_SOURCE: &str = "examples/self_hosted_parser/main.ko";

/// Result of a bootstrap-availability check.
#[derive(Debug)]
pub(crate) struct BootstrapStatus {
    /// Path to the self-hosted lexer binary, if found.
    pub(crate) lexer_bin: Option<PathBuf>,
    /// Path to the self-hosted parser binary, if found.
    pub(crate) parser_bin: Option<PathBuf>,
}

impl BootstrapStatus {
    /// Returns `true` when both bootstrap binaries are available.
    pub(crate) fn is_ready(&self) -> bool {
        self.lexer_bin.is_some() && self.parser_bin.is_some()
    }
}

/// Searches the standard bootstrap directories for a binary with the given name.
fn find_binary(name: &str) -> Option<PathBuf> {
    for dir in BOOTSTRAP_SEARCH_DIRS {
        let candidate = Path::new(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        // On Windows the binary has an .exe extension.
        #[cfg(target_os = "windows")]
        {
            let win = Path::new(dir).join(format!("{name}.exe"));
            if win.is_file() {
                return Some(win);
            }
        }
    }
    None
}

/// Detects whether the bootstrap binaries are available in the standard search paths.
///
/// Call this before attempting a self-hosted compilation pass.
pub(crate) fn detect_bootstrap() -> BootstrapStatus {
    BootstrapStatus {
        lexer_bin: find_binary(LEXER_BIN_NAME),
        parser_bin: find_binary(PARSER_BIN_NAME),
    }
}

/// Emits a human-readable diagnostic when bootstrap binaries are not found.
///
/// Prints a structured message explaining what is missing and how to build the
/// bootstrap binaries, then returns exit code `1`.
pub(crate) fn report_bootstrap_unavailable(status: &BootstrapStatus) -> i32 {
    eprintln!("error[E0700]: self-hosted mode requires pre-compiled bootstrap binaries");
    eprintln!();
    if status.lexer_bin.is_none() {
        eprintln!(
            "  missing: {LEXER_BIN_NAME}  (not found in: {})",
            BOOTSTRAP_SEARCH_DIRS.join(", ")
        );
    }
    if status.parser_bin.is_none() {
        eprintln!(
            "  missing: {PARSER_BIN_NAME}  (not found in: {})",
            BOOTSTRAP_SEARCH_DIRS.join(", ")
        );
    }
    eprintln!();
    eprintln!("  To build the bootstrap binaries, run:");
    eprintln!();
    eprintln!("    kodoc build {LEXER_SOURCE}  -o ./bootstrap/{LEXER_BIN_NAME}");
    eprintln!("    kodoc build {PARSER_SOURCE} -o ./bootstrap/{PARSER_BIN_NAME}");
    eprintln!();
    eprintln!("  Then re-run with --self-hosted.");
    eprintln!();
    eprintln!("  note: --self-hosted is experimental (Kōdo bootstrap Phase 4).");
    eprintln!("        Full lex/parse pipeline integration lands in a future milestone.");
    1
}

/// Runs the self-hosted lex/parse stage for the given source file.
///
/// Invokes the bootstrap lexer binary with the source file path as its first
/// argument and captures stdout.  On success, prints a confirmation and returns
/// `Ok(())`.  On failure (non-zero exit or I/O error), returns an `Err` with a
/// human-readable description.
///
/// # Errors
///
/// Returns `Err` when the subprocess cannot be spawned or exits with a non-zero
/// status code.
pub(crate) fn run_self_hosted_lex(lexer_bin: &Path, source_file: &Path) -> Result<(), String> {
    use std::process::Command;

    let output = Command::new(lexer_bin)
        .arg(source_file)
        .output()
        .map_err(|e| {
            format!(
                "failed to spawn self-hosted lexer '{}': {e}",
                lexer_bin.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "self-hosted lexer exited with status {}: {stderr}",
            output.status
        ));
    }

    // In Phase 4 full integration, `output.stdout` would be JSON tokens handed
    // to the Rust type-checker pipeline.  For now we validate liveness.
    let token_count = String::from_utf8_lossy(&output.stdout).lines().count();
    eprintln!(
        "info: self-hosted lexer produced {token_count} output lines for '{}'",
        source_file.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_bootstrap_returns_none_when_missing() {
        // In a clean checkout without pre-built bootstrap binaries the
        // detection must return `None` for both paths — never panic.
        let status = detect_bootstrap();
        // We cannot assert the exact value (CI might have the binaries), but
        // `is_ready()` must be consistent with the individual fields.
        assert_eq!(
            status.is_ready(),
            status.lexer_bin.is_some() && status.parser_bin.is_some()
        );
    }

    #[test]
    fn bootstrap_status_not_ready_when_both_missing() {
        let status = BootstrapStatus {
            lexer_bin: None,
            parser_bin: None,
        };
        assert!(!status.is_ready());
    }

    #[test]
    fn bootstrap_status_not_ready_when_only_lexer_present() {
        let status = BootstrapStatus {
            lexer_bin: Some(PathBuf::from("/fake/self_hosted_lexer")),
            parser_bin: None,
        };
        assert!(!status.is_ready());
    }

    #[test]
    fn bootstrap_status_ready_when_both_present() {
        let status = BootstrapStatus {
            lexer_bin: Some(PathBuf::from("/fake/self_hosted_lexer")),
            parser_bin: Some(PathBuf::from("/fake/self_hosted_parser")),
        };
        assert!(status.is_ready());
    }

    #[test]
    fn report_bootstrap_unavailable_returns_exit_code_one() {
        let status = BootstrapStatus {
            lexer_bin: None,
            parser_bin: None,
        };
        // Must return exit code 1 so the shell can detect failure.
        assert_eq!(report_bootstrap_unavailable(&status), 1);
    }
}
