//! # SARIF Diagnostic Output
//!
//! Emits compiler diagnostics in [SARIF v2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/)
//! format — the industry-standard JSON schema for static analysis results.
//!
//! SARIF output enables integration with:
//! - GitHub Code Scanning (native SARIF upload)
//! - VS Code Problems panel
//! - Any SARIF-compatible IDE or CI/CD tool
//! - Multi-language AI agents that parse diagnostics from multiple compilers

use kodo_ast::Diagnostic;
use serde::Serialize;

/// SARIF v2.1.0 top-level log object.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLog {
    /// SARIF version — always "2.1.0".
    #[serde(rename = "$schema")]
    schema: &'static str,
    /// SARIF version string.
    version: &'static str,
    /// Analysis runs (one per invocation).
    runs: Vec<SarifRun>,
}

/// A single analysis run.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRun {
    /// The tool that produced the results.
    tool: SarifTool,
    /// The results (diagnostics).
    results: Vec<SarifResult>,
}

/// Tool identification.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifTool {
    /// The tool driver (compiler).
    driver: SarifDriver,
}

/// Tool driver metadata.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifDriver {
    /// Tool name.
    name: &'static str,
    /// Tool version.
    version: String,
    /// Semantic version.
    semantic_version: String,
    /// Information URI.
    information_uri: &'static str,
}

/// A single diagnostic result.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult {
    /// Rule ID (error code).
    rule_id: String,
    /// Severity level.
    level: &'static str,
    /// Human-readable message.
    message: SarifMessage,
    /// Source locations.
    locations: Vec<SarifLocation>,
    /// Fix suggestions.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fixes: Vec<SarifFix>,
}

/// A message object.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifMessage {
    /// The message text.
    text: String,
}

/// A source location.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifLocation {
    /// Physical location in a file.
    physical_location: SarifPhysicalLocation,
}

/// Physical location with file and region.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifPhysicalLocation {
    /// The artifact (file).
    artifact_location: SarifArtifactLocation,
    /// The region within the file.
    region: SarifRegion,
}

/// File reference.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifArtifactLocation {
    /// Relative URI to the file.
    uri: String,
}

/// A region within a file (1-based line/column).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
    /// Start line (1-based).
    start_line: u32,
    /// Start column (1-based).
    start_column: u32,
    /// End line (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<u32>,
    /// End column (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    end_column: Option<u32>,
}

/// A suggested fix.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifFix {
    /// Description of the fix.
    description: SarifMessage,
}

/// Compute 1-based line and column from a byte offset.
fn line_col(source: &str, byte_offset: u32) -> (u32, u32) {
    let offset = (byte_offset as usize).min(source.len());
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Convert a slice of diagnostics into a SARIF log and print it to stdout.
pub fn render_sarif(source: &str, filename: &str, diagnostics: &[&dyn Diagnostic]) {
    let log = build_sarif_log(source, filename, diagnostics);
    if let Ok(json) = serde_json::to_string_pretty(&log) {
        println!("{json}");
    }
}

/// Build a SARIF log from diagnostics.
pub fn build_sarif_log(source: &str, filename: &str, diagnostics: &[&dyn Diagnostic]) -> SarifLog {
    let results: Vec<SarifResult> = diagnostics
        .iter()
        .map(|d| diagnostic_to_sarif(source, filename, *d))
        .collect();

    SarifLog {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        version: "2.1.0",
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "kodoc",
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    semantic_version: env!("CARGO_PKG_VERSION").to_string(),
                    information_uri: "https://kodo-lang.dev",
                },
            },
            results,
        }],
    }
}

/// Convert a single diagnostic to a SARIF result.
fn diagnostic_to_sarif(source: &str, filename: &str, diagnostic: &dyn Diagnostic) -> SarifResult {
    let level = match diagnostic.severity() {
        kodo_ast::Severity::Error => "error",
        kodo_ast::Severity::Warning => "warning",
        kodo_ast::Severity::Note => "note",
    };

    let locations = if let Some(span) = diagnostic.span() {
        let (start_line, start_col) = line_col(source, span.start);
        let (end_line, end_col) = line_col(source, span.end);
        vec![SarifLocation {
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation {
                    uri: filename.to_string(),
                },
                region: SarifRegion {
                    start_line,
                    start_column: start_col,
                    end_line: Some(end_line),
                    end_column: Some(end_col),
                },
            },
        }]
    } else {
        Vec::new()
    };

    let fixes = diagnostic
        .suggestion()
        .map(|s| {
            vec![SarifFix {
                description: SarifMessage { text: s },
            }]
        })
        .unwrap_or_default();

    SarifResult {
        rule_id: diagnostic.code().to_string(),
        level,
        message: SarifMessage {
            text: diagnostic.message(),
        },
        locations,
        fixes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{DiagnosticLabel, Severity, Span};

    /// Minimal diagnostic for testing.
    struct TestDiag {
        code: &'static str,
        msg: String,
        span: Option<Span>,
        suggestion: Option<String>,
    }

    impl std::fmt::Display for TestDiag {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}: {}", self.code, self.msg)
        }
    }

    impl Diagnostic for TestDiag {
        fn code(&self) -> &'static str {
            self.code
        }
        fn message(&self) -> String {
            self.msg.clone()
        }
        fn severity(&self) -> Severity {
            Severity::Error
        }
        fn span(&self) -> Option<Span> {
            self.span
        }
        fn suggestion(&self) -> Option<String> {
            self.suggestion.clone()
        }
        fn labels(&self) -> Vec<DiagnosticLabel> {
            Vec::new()
        }
        fn fix_patch(&self) -> Option<kodo_ast::FixPatch> {
            None
        }
    }

    #[test]
    fn sarif_log_has_correct_version() {
        let diags: Vec<&dyn Diagnostic> = vec![];
        let log = build_sarif_log("", "test.ko", &diags);
        assert_eq!(log.version, "2.1.0");
        assert_eq!(log.runs.len(), 1);
        assert_eq!(log.runs[0].tool.driver.name, "kodoc");
    }

    #[test]
    fn sarif_result_has_correct_rule_id() {
        let diag = TestDiag {
            code: "E0200",
            msg: "type mismatch".to_string(),
            span: Some(Span::new(10, 20)),
            suggestion: None,
        };
        let diags: Vec<&dyn Diagnostic> = vec![&diag];
        let log = build_sarif_log("let x: Int = true\n", "test.ko", &diags);
        assert_eq!(log.runs[0].results.len(), 1);
        assert_eq!(log.runs[0].results[0].rule_id, "E0200");
        assert_eq!(log.runs[0].results[0].level, "error");
    }

    #[test]
    fn sarif_location_has_line_column() {
        let source = "line1\nline2\nline3\n";
        let diag = TestDiag {
            code: "E0100",
            msg: "parse error".to_string(),
            span: Some(Span::new(6, 11)), // "line2"
            suggestion: None,
        };
        let diags: Vec<&dyn Diagnostic> = vec![&diag];
        let log = build_sarif_log(source, "test.ko", &diags);
        let loc = &log.runs[0].results[0].locations[0].physical_location;
        assert_eq!(loc.region.start_line, 2);
        assert_eq!(loc.region.start_column, 1);
    }

    #[test]
    fn sarif_includes_fix_from_suggestion() {
        let diag = TestDiag {
            code: "E0200",
            msg: "type mismatch".to_string(),
            span: Some(Span::new(0, 5)),
            suggestion: Some("change type to Int".to_string()),
        };
        let diags: Vec<&dyn Diagnostic> = vec![&diag];
        let log = build_sarif_log("test", "test.ko", &diags);
        assert_eq!(log.runs[0].results[0].fixes.len(), 1);
        assert_eq!(
            log.runs[0].results[0].fixes[0].description.text,
            "change type to Int"
        );
    }

    #[test]
    fn sarif_json_is_valid() {
        let diag = TestDiag {
            code: "E0200",
            msg: "type mismatch".to_string(),
            span: Some(Span::new(0, 5)),
            suggestion: None,
        };
        let diags: Vec<&dyn Diagnostic> = vec![&diag];
        let log = build_sarif_log("test", "test.ko", &diags);
        let json = serde_json::to_string_pretty(&log).expect("should serialize");
        assert!(json.contains("\"version\": \"2.1.0\""));
        assert!(json.contains("\"ruleId\": \"E0200\""));
        assert!(json.contains("kodoc"));
    }

    #[test]
    fn sarif_empty_diagnostics() {
        let diags: Vec<&dyn Diagnostic> = vec![];
        let log = build_sarif_log("", "test.ko", &diags);
        assert!(log.runs[0].results.is_empty());
        let json = serde_json::to_string(&log).expect("should serialize");
        assert!(json.contains("\"results\":[]"));
    }
}
