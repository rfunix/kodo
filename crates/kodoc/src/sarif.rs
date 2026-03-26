//! # SARIF Output — Static Analysis Results Interchange Format
//!
//! Converts Kodo compiler diagnostics into [SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/)
//! JSON format, enabling integration with GitHub Code Scanning, VS Code SARIF Viewer,
//! and other tools that consume SARIF. This is particularly valuable for AI agents
//! operating in CI/CD pipelines where SARIF is the standard interchange format.

use kodo_ast::Diagnostic;
use serde::Serialize;

/// SARIF schema URL for version 2.1.0.
const SARIF_SCHEMA: &str =
    "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json";

/// SARIF specification version.
const SARIF_VERSION: &str = "2.1.0";

/// The tool driver name used in SARIF output.
const TOOL_NAME: &str = "kodoc";

/// Top-level SARIF log object.
#[derive(Serialize)]
pub(crate) struct SarifLog {
    /// JSON schema reference.
    #[serde(rename = "$schema")]
    schema: String,
    /// SARIF version.
    version: String,
    /// Analysis runs (always a single run for kodoc).
    runs: Vec<SarifRun>,
}

/// A single analysis run.
#[derive(Serialize)]
struct SarifRun {
    /// Tool information.
    tool: SarifTool,
    /// Diagnostic results.
    results: Vec<SarifResult>,
}

/// Tool descriptor.
#[derive(Serialize)]
struct SarifTool {
    /// The primary tool component.
    driver: SarifDriver,
}

/// Tool driver (main component) descriptor.
#[derive(Serialize)]
struct SarifDriver {
    /// Tool name.
    name: String,
    /// Tool version.
    version: String,
    /// Information URI for the tool.
    #[serde(rename = "informationUri")]
    information_uri: String,
    /// Rules referenced by results.
    rules: Vec<SarifRule>,
}

/// A SARIF rule descriptor — maps to a Kodo error code.
#[derive(Serialize)]
struct SarifRule {
    /// Rule identifier (the Kodo error code, e.g. "E0200").
    id: String,
    /// Short description of the rule.
    #[serde(rename = "shortDescription")]
    short_description: SarifMessage,
    /// Help text with fix suggestion, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<SarifMessage>,
}

/// A SARIF result — one per diagnostic.
#[derive(Serialize)]
struct SarifResult {
    /// Rule ID this result refers to.
    #[serde(rename = "ruleId")]
    rule_id: String,
    /// Severity level mapped to SARIF vocabulary.
    level: String,
    /// The diagnostic message.
    message: SarifMessage,
    /// Source locations.
    locations: Vec<SarifLocation>,
    /// Fix suggestions, if available.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fixes: Vec<SarifFix>,
}

/// A simple text message.
#[derive(Serialize)]
struct SarifMessage {
    /// The message text.
    text: String,
}

/// A source location.
#[derive(Serialize)]
struct SarifLocation {
    /// Physical location in a file.
    #[serde(rename = "physicalLocation")]
    physical_location: SarifPhysicalLocation,
}

/// Physical location within a source artifact.
#[derive(Serialize)]
struct SarifPhysicalLocation {
    /// The artifact (file) reference.
    #[serde(rename = "artifactLocation")]
    artifact_location: SarifArtifactLocation,
    /// The region within the file.
    region: SarifRegion,
}

/// A file reference.
#[derive(Serialize)]
struct SarifArtifactLocation {
    /// The file path (URI-encoded).
    uri: String,
}

/// A source region (line/column based).
#[derive(Serialize)]
struct SarifRegion {
    /// Start line (1-based).
    #[serde(rename = "startLine")]
    start_line: u32,
    /// Start column (1-based).
    #[serde(rename = "startColumn")]
    start_column: u32,
    /// End line (1-based).
    #[serde(rename = "endLine")]
    end_line: u32,
    /// End column (1-based).
    #[serde(rename = "endColumn")]
    end_column: u32,
}

/// A suggested fix.
#[derive(Serialize)]
struct SarifFix {
    /// Description of the fix.
    description: SarifMessage,
    /// Artifact changes that constitute the fix.
    #[serde(rename = "artifactChanges")]
    artifact_changes: Vec<SarifArtifactChange>,
}

/// Changes to a single artifact (file).
#[derive(Serialize)]
struct SarifArtifactChange {
    /// The file to change.
    #[serde(rename = "artifactLocation")]
    artifact_location: SarifArtifactLocation,
    /// Replacements within the file.
    replacements: Vec<SarifReplacement>,
}

/// A single text replacement.
#[derive(Serialize)]
struct SarifReplacement {
    /// Region to delete.
    #[serde(rename = "deletedRegion")]
    deleted_region: SarifRegion,
    /// Content to insert.
    #[serde(rename = "insertedContent")]
    inserted_content: SarifInsertedContent,
}

/// Content to insert as part of a replacement.
#[derive(Serialize)]
struct SarifInsertedContent {
    /// The replacement text.
    text: String,
}

/// Computes line and column (1-based) from a byte offset in source text.
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

/// Maps Kodo severity to SARIF level string.
fn severity_to_level(severity: kodo_ast::Severity) -> &'static str {
    match severity {
        kodo_ast::Severity::Error => "error",
        kodo_ast::Severity::Warning => "warning",
        kodo_ast::Severity::Note => "note",
    }
}

/// Converts a slice of diagnostics into a SARIF JSON string.
///
/// Each diagnostic is mapped to a SARIF result with location information,
/// severity level, and optional fix suggestions derived from `fix_patch()`.
/// Rules are deduplicated by error code.
pub(crate) fn render_sarif(
    source: &str,
    filename: &str,
    diagnostics: &[&dyn Diagnostic],
) -> String {
    let mut rules: Vec<SarifRule> = Vec::new();
    let mut seen_rules: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut results: Vec<SarifResult> = Vec::new();

    for diag in diagnostics {
        let code = diag.code().to_string();

        // Register rule if not yet seen.
        if seen_rules.insert(code.clone()) {
            rules.push(SarifRule {
                id: code.clone(),
                short_description: SarifMessage {
                    text: diag.message(),
                },
                help: diag.suggestion().map(|s| SarifMessage { text: s }),
            });
        }

        // Build location.
        let locations = if let Some(span) = diag.span() {
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
                        end_line,
                        end_column: end_col,
                    },
                },
            }]
        } else {
            Vec::new()
        };

        // Build fixes from fix_patch.
        let fixes = if let Some(patch) = diag.fix_patch() {
            let (del_start_line, del_start_col) = line_col(source, patch.start_offset as u32);
            let (del_end_line, del_end_col) = line_col(source, patch.end_offset as u32);
            vec![SarifFix {
                description: SarifMessage {
                    text: patch.description.clone(),
                },
                artifact_changes: vec![SarifArtifactChange {
                    artifact_location: SarifArtifactLocation {
                        uri: if patch.file.is_empty() {
                            filename.to_string()
                        } else {
                            patch.file
                        },
                    },
                    replacements: vec![SarifReplacement {
                        deleted_region: SarifRegion {
                            start_line: del_start_line,
                            start_column: del_start_col,
                            end_line: del_end_line,
                            end_column: del_end_col,
                        },
                        inserted_content: SarifInsertedContent {
                            text: patch.replacement,
                        },
                    }],
                }],
            }]
        } else {
            Vec::new()
        };

        results.push(SarifResult {
            rule_id: code,
            level: severity_to_level(diag.severity()).to_string(),
            message: SarifMessage {
                text: diag.message(),
            },
            locations,
            fixes,
        });
    }

    let log = SarifLog {
        schema: SARIF_SCHEMA.to_string(),
        version: SARIF_VERSION.to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: TOOL_NAME.to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    information_uri: "https://kodo-lang.dev".to_string(),
                    rules,
                },
            },
            results,
        }],
    };

    serde_json::to_string_pretty(&log)
        .unwrap_or_else(|e| format!("{{\"error\": \"SARIF serialization failed: {e}\"}}"))
}

/// Renders diagnostics as SARIF and prints to stdout.
///
/// Convenience wrapper that calls [`render_sarif`] and prints the result.
pub(crate) fn print_sarif(source: &str, filename: &str, diagnostics: &[&dyn Diagnostic]) {
    println!("{}", render_sarif(source, filename, diagnostics));
}

/// Renders a successful (no errors) SARIF output to stdout.
///
/// Produces a valid SARIF log with zero results, indicating a clean analysis.
pub(crate) fn print_sarif_success() {
    let log = SarifLog {
        schema: SARIF_SCHEMA.to_string(),
        version: SARIF_VERSION.to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: TOOL_NAME.to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    information_uri: "https://kodo-lang.dev".to_string(),
                    rules: Vec::new(),
                },
            },
            results: Vec::new(),
        }],
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&log)
            .unwrap_or_else(|e| format!("{{\"error\": \"SARIF serialization failed: {e}\"}}"))
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal diagnostic for testing.
    struct TestDiag {
        code: &'static str,
        message: String,
        span: Option<kodo_ast::Span>,
        suggestion: Option<String>,
        fix: Option<kodo_ast::FixPatch>,
    }

    impl std::fmt::Display for TestDiag {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}: {}", self.code, self.message)
        }
    }

    impl Diagnostic for TestDiag {
        fn code(&self) -> &'static str {
            self.code
        }
        fn severity(&self) -> kodo_ast::Severity {
            kodo_ast::Severity::Error
        }
        fn span(&self) -> Option<kodo_ast::Span> {
            self.span
        }
        fn message(&self) -> String {
            self.message.clone()
        }
        fn suggestion(&self) -> Option<String> {
            self.suggestion.clone()
        }
        fn fix_patch(&self) -> Option<kodo_ast::FixPatch> {
            self.fix.clone()
        }
    }

    #[test]
    fn sarif_empty_produces_valid_schema() {
        let output = render_sarif("", "test.ko", &[]);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["version"], "2.1.0");
        assert_eq!(parsed["$schema"], SARIF_SCHEMA);
        assert_eq!(parsed["runs"][0]["results"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["runs"][0]["tool"]["driver"]["name"], "kodoc");
    }

    #[test]
    fn sarif_single_error_with_span() {
        let source = "let x: Int = true\n";
        let diag = TestDiag {
            code: "E0200",
            message: "type mismatch: expected Int, got Bool".to_string(),
            span: Some(kodo_ast::Span { start: 13, end: 17 }),
            suggestion: Some("use an Int literal".to_string()),
            fix: None,
        };
        let diagnostics: Vec<&dyn Diagnostic> = vec![&diag];
        let output = render_sarif(source, "test.ko", &diagnostics);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        let results = parsed["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["ruleId"], "E0200");
        assert_eq!(results[0]["level"], "error");
        assert_eq!(
            results[0]["message"]["text"],
            "type mismatch: expected Int, got Bool"
        );

        let loc = &results[0]["locations"][0]["physicalLocation"];
        assert_eq!(loc["artifactLocation"]["uri"], "test.ko");
        assert_eq!(loc["region"]["startLine"], 1);
        assert_eq!(loc["region"]["startColumn"], 14);

        // Check rule was registered.
        let rules = parsed["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["id"], "E0200");
        assert_eq!(rules[0]["help"]["text"], "use an Int literal");
    }

    #[test]
    fn sarif_with_fix_patch() {
        let source = "let x = 42\n";
        let diag = TestDiag {
            code: "E0100",
            message: "missing type annotation".to_string(),
            span: Some(kodo_ast::Span { start: 4, end: 5 }),
            suggestion: None,
            fix: Some(kodo_ast::FixPatch {
                description: "add type annotation".to_string(),
                file: String::new(),
                start_offset: 5,
                end_offset: 5,
                replacement: ": Int".to_string(),
            }),
        };
        let diagnostics: Vec<&dyn Diagnostic> = vec![&diag];
        let output = render_sarif(source, "test.ko", &diagnostics);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        let fixes = parsed["runs"][0]["results"][0]["fixes"].as_array().unwrap();
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0]["description"]["text"], "add type annotation");
        assert_eq!(
            fixes[0]["artifactChanges"][0]["replacements"][0]["insertedContent"]["text"],
            ": Int"
        );
    }

    #[test]
    fn sarif_deduplicates_rules() {
        let source = "abc\ndef\n";
        let diag1 = TestDiag {
            code: "E0200",
            message: "first error".to_string(),
            span: Some(kodo_ast::Span { start: 0, end: 3 }),
            suggestion: None,
            fix: None,
        };
        let diag2 = TestDiag {
            code: "E0200",
            message: "second error".to_string(),
            span: Some(kodo_ast::Span { start: 4, end: 7 }),
            suggestion: None,
            fix: None,
        };
        let diagnostics: Vec<&dyn Diagnostic> = vec![&diag1, &diag2];
        let output = render_sarif(source, "test.ko", &diagnostics);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        // Two results but only one rule.
        let results = parsed["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        let rules = parsed["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn sarif_no_span_produces_empty_locations() {
        let diag = TestDiag {
            code: "E0001",
            message: "no span".to_string(),
            span: None,
            suggestion: None,
            fix: None,
        };
        let diagnostics: Vec<&dyn Diagnostic> = vec![&diag];
        let output = render_sarif("", "test.ko", &diagnostics);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let locations = parsed["runs"][0]["results"][0]["locations"]
            .as_array()
            .unwrap();
        assert!(locations.is_empty());
    }
}
