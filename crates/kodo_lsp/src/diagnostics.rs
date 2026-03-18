//! Diagnostic publishing for the Kōdo LSP server.
//!
//! Runs the parser and type checker pipeline on source text and converts
//! any errors into LSP diagnostic entries for real-time error reporting.

#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;

use crate::utils::offset_to_line_col;

/// Analyzes Kōdo source code and returns LSP diagnostics.
///
/// Runs the parser and type checker pipeline, collecting
/// any errors as LSP diagnostic entries.
pub(crate) fn analyze_source(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Parse with error recovery so we report ALL diagnostics at once.
    let output = kodo_parser::parse_with_recovery(source);

    // Collect all parse errors as diagnostics.
    for e in &output.errors {
        let (line, col) = if let Some(span) = e.span() {
            offset_to_line_col(source, span.start)
        } else {
            (0, 0)
        };
        diagnostics.push(Diagnostic {
            range: Range::new(Position::new(line, col), Position::new(line, col + 1)),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(e.code().to_string())),
            source: Some("kodo".to_string()),
            message: e.to_string(),
            ..Default::default()
        });
    }

    // Only run type checking if parsing produced no errors (partial ASTs
    // from recovery would generate misleading type errors).
    if output.errors.is_empty() {
        let mut checker = kodo_types::TypeChecker::new();
        if let Err(error) = checker.check_module(&output.module) {
            let (line, col, end_line, end_col) = if let Some(span) = error.span() {
                let (l, c) = offset_to_line_col(source, span.start);
                let (el, ec) = offset_to_line_col(source, span.end);
                (l, c, el, ec)
            } else {
                (0, 0, 0, 1)
            };
            diagnostics.push(Diagnostic {
                range: Range::new(Position::new(line, col), Position::new(end_line, end_col)),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String(error.code().to_string())),
                source: Some("kodo".to_string()),
                message: error.to_string(),
                ..Default::default()
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_source_produces_no_diagnostics() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        let diags = analyze_source(source);
        assert!(
            diags.is_empty(),
            "valid source should produce no diagnostics, got: {diags:?}"
        );
    }

    #[test]
    fn parse_error_produces_diagnostic_with_error_code() {
        // Missing closing brace — parse error
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
"#;
        let diags = analyze_source(source);
        assert!(
            !diags.is_empty(),
            "incomplete source should produce at least one diagnostic"
        );
        for d in &diags {
            assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
            assert_eq!(d.source.as_deref(), Some("kodo"));
            assert!(d.code.is_some(), "diagnostic should have an error code");
        }
    }

    #[test]
    fn type_error_produces_diagnostic() {
        // Missing meta block — type error
        let source = r#"module test {
    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        let diags = analyze_source(source);
        assert!(
            !diags.is_empty(),
            "source with type error should produce diagnostics"
        );
        let type_diag = diags.iter().find(|d| {
            if let Some(NumberOrString::String(code)) = &d.code {
                code.starts_with("E02")
            } else {
                false
            }
        });
        assert!(
            type_diag.is_some(),
            "should produce a type error diagnostic (E02xx), got: {diags:?}"
        );
    }

    #[test]
    fn diagnostics_have_correct_source_field() {
        let source = "invalid kodo source !!!";
        let diags = analyze_source(source);
        for d in &diags {
            assert_eq!(
                d.source.as_deref(),
                Some("kodo"),
                "all diagnostics should have source 'kodo'"
            );
        }
    }

    #[test]
    fn multiple_parse_errors_reported() {
        // Source with multiple issues via recovery parser
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn foo( -> Int {
        return 1
    }

    fn bar(a: ) -> Int {
        return 2
    }
}"#;
        let diags = analyze_source(source);
        assert!(
            diags.len() >= 2,
            "recovery parser should report multiple errors, got {}",
            diags.len()
        );
    }

    #[test]
    fn snapshot_diagnostics_for_missing_meta() {
        let source = r#"module test {
    fn main() {
        return
    }
}"#;
        let diags = analyze_source(source);
        let summary: Vec<String> = diags
            .iter()
            .map(|d| {
                let code = match &d.code {
                    Some(NumberOrString::String(s)) => s.clone(),
                    _ => "?".to_string(),
                };
                format!(
                    "[{}] L{}:{} {}",
                    code, d.range.start.line, d.range.start.character, d.message
                )
            })
            .collect();
        insta::assert_snapshot!(summary.join("\n"));
    }

    #[test]
    fn snapshot_diagnostics_for_parse_error() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn foo(x: Int -> Int {
        return x
    }
}"#;
        let diags = analyze_source(source);
        let summary: Vec<String> = diags
            .iter()
            .map(|d| {
                let code = match &d.code {
                    Some(NumberOrString::String(s)) => s.clone(),
                    _ => "?".to_string(),
                };
                format!(
                    "[{}] L{}:{} {}",
                    code, d.range.start.line, d.range.start.character, d.message
                )
            })
            .collect();
        insta::assert_snapshot!(summary.join("\n"));
    }
}
