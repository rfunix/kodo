//! Output comparison utilities for the kotest harness.
//!
//! Handles comparison of actual compiler output against baseline `.stderr`,
//! `.stdout`, and `.cert.json` files. Supports `--bless` mode for auto-updating
//! baselines.

use std::path::{Path, PathBuf};

use crate::directives::InlineAnnotation;

/// The result of comparing actual output against a baseline.
#[derive(Debug)]
#[allow(dead_code)]
pub enum CompareResult {
    /// Output matches the baseline.
    Match,
    /// Baseline does not exist yet (needs `--bless`).
    NoBaseline(PathBuf),
    /// Output differs from baseline.
    Mismatch {
        /// Path to the baseline file.
        baseline_path: PathBuf,
        /// Expected content (from baseline).
        expected: String,
        /// Actual content (from compiler).
        actual: String,
        /// Unified diff for display.
        diff: String,
    },
}

/// Compares actual output against a baseline file.
///
/// If `bless` is true, writes the actual output as the new baseline.
pub fn compare_output(
    test_path: &Path,
    extension: &str,
    actual: &str,
    bless: bool,
) -> CompareResult {
    let baseline_path = test_path.with_extension(extension.trim_start_matches('.'));

    if bless {
        if actual.is_empty() {
            // Remove baseline if output is empty
            let _ = std::fs::remove_file(&baseline_path);
        } else {
            std::fs::write(&baseline_path, actual).unwrap_or_else(|e| {
                panic!("failed to write baseline {}: {e}", baseline_path.display())
            });
        }
        return CompareResult::Match;
    }

    if !baseline_path.exists() {
        if actual.is_empty() {
            return CompareResult::Match;
        }
        return CompareResult::NoBaseline(baseline_path);
    }

    let expected = std::fs::read_to_string(&baseline_path)
        .unwrap_or_else(|e| panic!("failed to read baseline {}: {e}", baseline_path.display()));

    let expected_normalized = normalize_output(&expected);
    let actual_normalized = normalize_output(actual);

    if expected_normalized == actual_normalized {
        CompareResult::Match
    } else {
        let diff = unified_diff(&expected, actual, &baseline_path);
        CompareResult::Mismatch {
            baseline_path,
            expected,
            actual: actual.to_string(),
            diff,
        }
    }
}

/// Verifies inline annotations against actual compiler errors from JSON output.
///
/// Returns a list of error messages for any mismatches.
pub fn verify_annotations(annotations: &[InlineAnnotation], json_output: &str) -> Vec<String> {
    let mut errors = Vec::new();

    let json: serde_json::Value = match serde_json::from_str(json_output) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("failed to parse JSON output: {e}"));
            return errors;
        }
    };

    let reported_errors: Vec<ReportedError> = extract_reported_errors(&json);

    // Check 1: every annotation must match a reported error
    for ann in annotations {
        let matched = reported_errors.iter().any(|re| {
            let severity_match = ann.severity == re.severity;
            let code_match = ann
                .code
                .as_ref()
                .is_none_or(|c| re.code.as_ref() == Some(c));
            let line_match = re.line.is_none_or(|l| l == ann.line);
            let msg_match = ann
                .message
                .as_ref()
                .is_none_or(|m| re.message.to_lowercase().contains(&m.to_lowercase()));
            severity_match && code_match && line_match && msg_match
        });

        if !matched {
            let desc = format!(
                "line {}: expected {} {}{}",
                ann.line,
                ann.severity,
                ann.code.as_deref().unwrap_or(""),
                ann.message
                    .as_ref()
                    .map(|m| format!(": {m}"))
                    .unwrap_or_default()
            );
            errors.push(format!("MISSING: {desc}"));
        }
    }

    // Check 2 (bidirectional): every reported error must have a matching annotation
    for re in &reported_errors {
        if re.severity != "ERROR" {
            continue; // Only require annotations for errors, not warnings
        }
        let matched = annotations.iter().any(|ann| {
            let severity_match = ann.severity == re.severity;
            let code_match = ann
                .code
                .as_ref()
                .is_none_or(|c| re.code.as_ref() == Some(c));
            let line_match = re.line.is_none_or(|l| l == ann.line);
            severity_match && code_match && line_match
        });

        if !matched {
            let desc = format!(
                "line {}: {} {}",
                re.line.map_or("?".to_string(), |l| l.to_string()),
                re.severity,
                re.code.as_deref().unwrap_or(""),
            );
            errors.push(format!("UNEXPECTED: {desc} — {}", re.message));
        }
    }

    errors
}

/// A diagnostic error extracted from JSON output.
#[derive(Debug)]
struct ReportedError {
    severity: String,
    code: Option<String>,
    message: String,
    line: Option<usize>,
}

/// Extracts reported errors from structured JSON output.
fn extract_reported_errors(json: &serde_json::Value) -> Vec<ReportedError> {
    let mut result = Vec::new();

    if let Some(errors) = json.get("errors").and_then(|e| e.as_array()) {
        for err in errors {
            result.push(ReportedError {
                severity: "ERROR".to_string(),
                code: err.get("code").and_then(|c| c.as_str()).map(String::from),
                message: err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string(),
                line: err
                    .get("span")
                    .and_then(|s| s.get("line"))
                    .and_then(|l| l.as_u64())
                    .map(|l| l as usize),
            });
        }
    }

    if let Some(warnings) = json.get("warnings").and_then(|w| w.as_array()) {
        for warn in warnings {
            result.push(ReportedError {
                severity: "WARN".to_string(),
                code: warn.get("code").and_then(|c| c.as_str()).map(String::from),
                message: warn
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string(),
                line: warn
                    .get("span")
                    .and_then(|s| s.get("line"))
                    .and_then(|l| l.as_u64())
                    .map(|l| l as usize),
            });
        }
    }

    result
}

/// Normalizes output for comparison (trim trailing whitespace, normalize line endings).
fn normalize_output(s: &str) -> String {
    s.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// Generates a simple unified diff between two strings.
fn unified_diff(expected: &str, actual: &str, path: &Path) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();

    let mut diff = String::new();
    diff.push_str(&format!("--- {}\n", path.display()));
    diff.push_str(&format!("+++ {} (actual)\n", path.display()));

    let max_lines = expected_lines.len().max(actual_lines.len());
    let mut in_hunk = false;

    for i in 0..max_lines {
        let exp = expected_lines.get(i).copied().unwrap_or("");
        let act = actual_lines.get(i).copied().unwrap_or("");

        if exp != act {
            if !in_hunk {
                diff.push_str(&format!("@@ line {} @@\n", i + 1));
                in_hunk = true;
            }
            if i < expected_lines.len() {
                diff.push_str(&format!("-{exp}\n"));
            }
            if i < actual_lines.len() {
                diff.push_str(&format!("+{act}\n"));
            }
        } else {
            in_hunk = false;
        }
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_trailing_whitespace() {
        let input = "hello   \nworld  \n";
        assert_eq!(normalize_output(input), "hello\nworld");
    }

    #[test]
    fn verify_annotations_missing_error() {
        let annotations = vec![InlineAnnotation {
            line: 5,
            severity: "ERROR".to_string(),
            code: Some("E0201".to_string()),
            message: None,
        }];
        let json = r#"{"status":"failed","errors":[],"warnings":[]}"#;
        let errs = verify_annotations(&annotations, json);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("MISSING"));
    }

    #[test]
    fn verify_annotations_unexpected_error() {
        let annotations = vec![];
        let json = r#"{"status":"failed","errors":[{"code":"E0201","message":"undefined type","severity":"error","span":{"line":5}}],"warnings":[]}"#;
        let errs = verify_annotations(&annotations, json);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("UNEXPECTED"));
    }

    #[test]
    fn verify_annotations_matching() {
        let annotations = vec![InlineAnnotation {
            line: 5,
            severity: "ERROR".to_string(),
            code: Some("E0201".to_string()),
            message: None,
        }];
        let json = r#"{"status":"failed","errors":[{"code":"E0201","message":"undefined type","severity":"error","span":{"line":5}}],"warnings":[]}"#;
        let errs = verify_annotations(&annotations, json);
        assert!(errs.is_empty(), "expected no errors, got: {errs:?}");
    }
}
