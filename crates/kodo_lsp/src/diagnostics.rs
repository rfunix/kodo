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
