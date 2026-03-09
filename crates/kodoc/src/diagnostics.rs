//! # Diagnostics — Pretty Error Rendering
//!
//! Uses [`ariadne`] to render type errors and parse errors with coloured
//! source spans, providing a high-quality developer experience.

use ariadne::{Color, Label, Report, ReportKind, Source};
use serde::Serialize;

/// Renders a [`kodo_types::TypeError`] with source spans using ariadne.
///
/// Falls back to plain `eprintln` if the error has no span.
pub fn render_type_error(source: &str, filename: &str, error: &kodo_types::TypeError) {
    if let Some(span) = error.span() {
        let start = span.start as usize;
        let end = span.end as usize;
        let start = start.min(source.len());
        let end = end.min(source.len()).max(start);

        Report::build(ReportKind::Error, (filename, start..end))
            .with_message(error.to_string())
            .with_label(
                Label::new((filename, start..end))
                    .with_message(error.to_string())
                    .with_color(Color::Red),
            )
            .finish()
            .eprint((filename, Source::from(source)))
            .ok();
    } else {
        eprintln!("type error: {error}");
    }
}

/// Renders a [`kodo_parser::ParseError`] with source spans using ariadne.
///
/// Falls back to plain `eprintln` if the error has no span.
pub fn render_parse_error(source: &str, filename: &str, error: &kodo_parser::ParseError) {
    if let Some(span) = error.span() {
        let start = span.start as usize;
        let end = span.end as usize;
        let start = start.min(source.len());
        let end = end.min(source.len()).max(start);

        Report::build(ReportKind::Error, (filename, start..end))
            .with_message(error.to_string())
            .with_label(
                Label::new((filename, start..end))
                    .with_message(error.to_string())
                    .with_color(Color::Red),
            )
            .finish()
            .eprint((filename, Source::from(source)))
            .ok();
    } else {
        eprintln!("parse error: {error}");
    }
}

/// A structured span for JSON output.
#[derive(Serialize)]
struct JsonSpan {
    file: String,
    start: u32,
    end: u32,
    line: u32,
    column: u32,
}

/// A single diagnostic entry in JSON output.
#[derive(Serialize)]
struct JsonDiagnostic {
    code: &'static str,
    severity: &'static str,
    message: String,
    span: Option<JsonSpan>,
    suggestion: Option<String>,
}

/// The top-level JSON diagnostics output.
#[derive(Serialize)]
pub struct JsonOutput {
    errors: Vec<JsonDiagnostic>,
    warnings: Vec<JsonDiagnostic>,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<JsonMeta>,
}

/// Meta information included in JSON output when available.
#[derive(Serialize)]
pub struct JsonMeta {
    /// Module name.
    pub module: String,
    /// Purpose from the meta block.
    pub purpose: Option<String>,
    /// Version from the meta block.
    pub version: Option<String>,
}

/// Computes line and column (1-based) from a byte offset in source text.
fn line_col(source: &str, byte_offset: u32) -> (u32, u32) {
    let offset = byte_offset as usize;
    let offset = offset.min(source.len());
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

fn make_json_span(source: &str, filename: &str, span: kodo_ast::Span) -> JsonSpan {
    let (line, column) = line_col(source, span.start);
    JsonSpan {
        file: filename.to_string(),
        start: span.start,
        end: span.end,
        line,
        column,
    }
}

/// Renders a [`kodo_parser::ParseError`] as JSON to stdout.
pub fn render_parse_error_json(source: &str, filename: &str, error: &kodo_parser::ParseError) {
    let diagnostic = JsonDiagnostic {
        code: error.code(),
        severity: "error",
        message: error.to_string(),
        span: error.span().map(|s| make_json_span(source, filename, s)),
        suggestion: None,
    };
    let output = JsonOutput {
        errors: vec![diagnostic],
        warnings: vec![],
        status: "failed".to_string(),
        meta: None,
    };
    // In a binary crate, println is fine for structured output
    println!(
        "{}",
        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"json serialization failed: {e}\"}}"))
    );
}

/// Renders a [`kodo_types::TypeError`] as JSON to stdout.
pub fn render_type_error_json(source: &str, filename: &str, error: &kodo_types::TypeError) {
    let diagnostic = JsonDiagnostic {
        code: error.code(),
        severity: "error",
        message: error.to_string(),
        span: error.span().map(|s| make_json_span(source, filename, s)),
        suggestion: None,
    };
    let output = JsonOutput {
        errors: vec![diagnostic],
        warnings: vec![],
        status: "failed".to_string(),
        meta: None,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"json serialization failed: {e}\"}}"))
    );
}

/// Renders a success JSON output to stdout.
pub fn render_success_json(module: &kodo_ast::Module) {
    let meta = module.meta.as_ref().map(|m| {
        let purpose = m
            .entries
            .iter()
            .find(|e| e.key == "purpose")
            .map(|e| e.value.clone());
        let version = m
            .entries
            .iter()
            .find(|e| e.key == "version")
            .map(|e| e.value.clone());
        JsonMeta {
            module: module.name.clone(),
            purpose,
            version,
        }
    });
    let output = JsonOutput {
        errors: vec![],
        warnings: vec![],
        status: "ok".to_string(),
        meta,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"json serialization failed: {e}\"}}"))
    );
}
