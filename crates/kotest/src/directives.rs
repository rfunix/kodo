//! Parser for test directives embedded in `.ko` source files.
//!
//! Directives are comments with the `//@ ` prefix that control how the test
//! harness should compile, run, and verify the test file.
//!
//! Inline error annotations use `//~ ERROR` or `//~ WARN` to mark expected
//! diagnostics at specific source lines.

use std::path::Path;

/// The mode a test should be run in, determined by directives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestMode {
    /// Must compile and run successfully (exit 0).
    RunPass,
    /// Must compile but fail at runtime (exit non-zero).
    RunFail,
    /// Must pass compilation (type check + contracts) — not executed.
    CheckPass,
    /// Must fail compilation.
    CompileFail,
}

/// A single inline error/warning annotation on a source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineAnnotation {
    /// 1-based line number where the annotation appears.
    pub line: usize,
    /// Severity: "ERROR" or "WARN".
    pub severity: String,
    /// Expected error code, e.g. "E0201".
    pub code: Option<String>,
    /// Expected message substring.
    pub message: Option<String>,
}

/// All parsed directives from a test file.
#[derive(Debug, Clone)]
pub struct TestDirectives {
    /// The test mode (required).
    pub mode: TestMode,
    /// Expected error codes (from `//@ error-code: Exxxx`).
    pub error_codes: Vec<String>,
    /// Extra compile flags (from `//@ compile-flags: ...`).
    pub compile_flags: Vec<String>,
    /// Cranelift IR patterns to check (from `//@ check-clif: ...`).
    pub check_clif: Vec<String>,
    /// Inline annotations (`//~ ERROR ...`).
    pub annotations: Vec<InlineAnnotation>,
    /// Whether to check a certificate baseline.
    pub check_cert: bool,
    /// Expected stdout file extension override (default: `.stdout`).
    pub stdout_ext: String,
    /// Expected stderr file extension override (default: `.stderr`).
    pub stderr_ext: String,
}

impl Default for TestDirectives {
    fn default() -> Self {
        Self {
            mode: TestMode::CheckPass,
            error_codes: Vec::new(),
            compile_flags: Vec::new(),
            check_clif: Vec::new(),
            annotations: Vec::new(),
            check_cert: false,
            stdout_ext: ".stdout".to_string(),
            stderr_ext: ".stderr".to_string(),
        }
    }
}

/// Parses directives from a `.ko` source file.
///
/// Returns `None` if no mode directive is found (not a UI test).
pub fn parse_directives(source: &str) -> Option<TestDirectives> {
    let mut directives = TestDirectives::default();
    let mut has_mode = false;

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Parse `//@ directive` comments
        if let Some(directive) = trimmed.strip_prefix("//@") {
            let directive = directive.trim();
            match directive {
                "run-pass" => {
                    directives.mode = TestMode::RunPass;
                    has_mode = true;
                }
                "run-fail" => {
                    directives.mode = TestMode::RunFail;
                    has_mode = true;
                }
                "check-pass" => {
                    directives.mode = TestMode::CheckPass;
                    has_mode = true;
                }
                "compile-fail" => {
                    directives.mode = TestMode::CompileFail;
                    has_mode = true;
                }
                "check-cert" => {
                    directives.check_cert = true;
                }
                _ if directive.starts_with("error-code:") => {
                    let code = directive
                        .strip_prefix("error-code:")
                        .unwrap_or_default()
                        .trim();
                    if !code.is_empty() {
                        directives.error_codes.push(code.to_string());
                    }
                }
                _ if directive.starts_with("compile-flags:") => {
                    let flags = directive
                        .strip_prefix("compile-flags:")
                        .unwrap_or_default()
                        .trim();
                    for flag in flags.split_whitespace() {
                        directives.compile_flags.push(flag.to_string());
                    }
                }
                _ if directive.starts_with("check-clif:") => {
                    let pattern = directive
                        .strip_prefix("check-clif:")
                        .unwrap_or_default()
                        .trim();
                    if !pattern.is_empty() {
                        directives.check_clif.push(pattern.to_string());
                    }
                }
                _ => {
                    // Unknown directive — ignore for forward compatibility
                }
            }
        }

        // Parse inline annotations: `//~ ERROR E0201: message`
        if let Some(ann_start) = trimmed.find("//~") {
            let ann_text = trimmed[ann_start + 3..].trim();
            if let Some(annotation) = parse_inline_annotation(ann_text, line_idx + 1) {
                directives.annotations.push(annotation);
            }
        }
    }

    if has_mode {
        Some(directives)
    } else {
        None
    }
}

/// Parses a single inline annotation from the text after `//~`.
///
/// Expected formats:
///   `ERROR E0201: undefined type`
///   `ERROR E0201`
///   `ERROR: some message`
///   `WARN E0200`
fn parse_inline_annotation(text: &str, line: usize) -> Option<InlineAnnotation> {
    let (severity, rest) = if let Some(rest) = text.strip_prefix("ERROR") {
        ("ERROR".to_string(), rest.trim())
    } else if let Some(rest) = text.strip_prefix("WARN") {
        ("WARN".to_string(), rest.trim())
    } else {
        return None;
    };

    let (code, message) = if rest.is_empty() {
        (None, None)
    } else if rest.starts_with("E0") || rest.starts_with("E1") {
        // Starts with error code
        if let Some(colon_pos) = rest.find(':') {
            let code = rest[..colon_pos].trim().to_string();
            let msg = rest[colon_pos + 1..].trim().to_string();
            (Some(code), if msg.is_empty() { None } else { Some(msg) })
        } else {
            // Just a code, no message
            let code = rest.split_whitespace().next().unwrap_or(rest);
            (Some(code.to_string()), None)
        }
    } else if let Some(msg) = rest.strip_prefix(':') {
        (None, Some(msg.trim().to_string()))
    } else {
        (None, Some(rest.to_string()))
    };

    Some(InlineAnnotation {
        line,
        severity,
        code,
        message,
    })
}

/// Infers the test mode from a file path when no explicit directive is present.
///
/// Falls back to directory name heuristics.
pub fn infer_mode_from_path(path: &Path) -> TestMode {
    let path_str = path.to_string_lossy();
    if path_str.contains("/invalid/") || path_str.contains("/compile-fail/") {
        TestMode::CompileFail
    } else if path_str.contains("/run-fail/") {
        TestMode::RunFail
    } else if path_str.contains("/run-pass/") {
        TestMode::RunPass
    } else {
        TestMode::CheckPass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_run_pass_directive() {
        let source = "//@ run-pass\nmodule test {}";
        let d = parse_directives(source).unwrap();
        assert_eq!(d.mode, TestMode::RunPass);
    }

    #[test]
    fn parse_compile_fail_with_error_code() {
        let source = "//@ compile-fail\n//@ error-code: E0201\nmodule test {}";
        let d = parse_directives(source).unwrap();
        assert_eq!(d.mode, TestMode::CompileFail);
        assert_eq!(d.error_codes, vec!["E0201"]);
    }

    #[test]
    fn parse_inline_annotations() {
        let source = r#"//@ compile-fail
module test {
    meta { purpose: "test" }
    fn main() {
        let x: Strin = "hello"  //~ ERROR E0201: undefined type `Strin`
    }
}"#;
        let d = parse_directives(source).unwrap();
        assert_eq!(d.annotations.len(), 1);
        assert_eq!(d.annotations[0].line, 5);
        assert_eq!(d.annotations[0].severity, "ERROR");
        assert_eq!(d.annotations[0].code, Some("E0201".to_string()));
    }

    #[test]
    fn parse_compile_flags() {
        let source = "//@ check-pass\n//@ compile-flags: --json-errors --contracts=static";
        let d = parse_directives(source).unwrap();
        assert_eq!(d.compile_flags, vec!["--json-errors", "--contracts=static"]);
    }

    #[test]
    fn parse_check_clif() {
        let source = "//@ check-pass\n//@ check-clif: iconst.i64\n//@ check-clif: iadd";
        let d = parse_directives(source).unwrap();
        assert_eq!(d.check_clif, vec!["iconst.i64", "iadd"]);
    }

    #[test]
    fn no_mode_returns_none() {
        let source = "module test {}";
        assert!(parse_directives(source).is_none());
    }
}
