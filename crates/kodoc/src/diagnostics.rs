//! # Diagnostics — Pretty Error Rendering
//!
//! Uses [`ariadne`] to render type errors and parse errors with coloured
//! source spans, providing a high-quality developer experience.
//!
//! The unified [`render`] and [`render_json`] functions accept any type
//! implementing [`kodo_ast::Diagnostic`], enabling consistent rendering
//! across all compiler phases.

use ariadne::{Color, Label, Report, ReportKind, Source};
use kodo_ast::Diagnostic;
use serde::Serialize;

/// Renders any diagnostic using ariadne.
///
/// Accepts any type implementing [`kodo_ast::Diagnostic`] and renders it
/// with coloured source spans. Falls back to plain `eprintln` if the
/// diagnostic has no span.
pub fn render(source: &str, filename: &str, diagnostic: &dyn Diagnostic) {
    if let Some(span) = diagnostic.span() {
        let start = (span.start as usize).min(source.len());
        let end = (span.end as usize).min(source.len()).max(start);

        let kind = match diagnostic.severity() {
            kodo_ast::Severity::Error => ReportKind::Error,
            kodo_ast::Severity::Warning => ReportKind::Warning,
            kodo_ast::Severity::Note => ReportKind::Advice,
        };

        let mut report = Report::build(kind, (filename, start..end))
            .with_code(diagnostic.code())
            .with_message(diagnostic.message());

        // Add primary label.
        report = report.with_label(
            Label::new((filename, start..end))
                .with_message(diagnostic.message())
                .with_color(Color::Red),
        );

        // Add suggestion if available.
        if let Some(suggestion) = diagnostic.suggestion() {
            report = report.with_help(suggestion);
        }

        // Add additional labels from the diagnostic.
        for label in diagnostic.labels() {
            let ls = (label.span.start as usize).min(source.len());
            let le = (label.span.end as usize).min(source.len()).max(ls);
            // Skip labels that overlap exactly with the primary span.
            if ls == start && le == end {
                continue;
            }
            report = report.with_label(
                Label::new((filename, ls..le))
                    .with_message(&label.message)
                    .with_color(Color::Blue),
            );
        }

        report
            .finish()
            .eprint((filename, Source::from(source)))
            .ok();
    } else {
        eprintln!("{}: {}", diagnostic.code(), diagnostic.message());
    }
}

/// Renders any diagnostic as structured JSON to stdout.
///
/// Accepts any type implementing [`kodo_ast::Diagnostic`] and produces
/// a single-error JSON output suitable for consumption by AI agents.
pub fn render_json(source: &str, filename: &str, diagnostic: &dyn Diagnostic) {
    let fix_patch = diagnostic.fix_patch().map(|p| JsonFixPatch {
        description: p.description,
        file: p.file,
        start_offset: p.start_offset,
        end_offset: p.end_offset,
        replacement: p.replacement,
    });
    let json_diag = JsonDiagnostic {
        code: diagnostic.code(),
        severity: match diagnostic.severity() {
            kodo_ast::Severity::Error => "error",
            kodo_ast::Severity::Warning => "warning",
            kodo_ast::Severity::Note => "note",
        },
        message: diagnostic.message(),
        span: diagnostic
            .span()
            .map(|s| make_json_span(source, filename, s)),
        suggestion: diagnostic.suggestion(),
        fix_patch,
        see_also: diagnostic.see_also(),
    };
    let output = JsonOutput {
        errors: vec![json_diag],
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

/// Renders a [`kodo_types::TypeError`] with source spans using ariadne.
///
/// Falls back to plain `eprintln` if the error has no span.
/// Delegates to the unified [`render`] function.
pub fn render_type_error(source: &str, filename: &str, error: &kodo_types::TypeError) {
    render(source, filename, error);
}

/// Renders a [`kodo_parser::ParseError`] with source spans using ariadne.
///
/// Falls back to plain `eprintln` if the error has no span.
/// Delegates to the unified [`render`] function.
pub fn render_parse_error(source: &str, filename: &str, error: &kodo_parser::ParseError) {
    render(source, filename, error);
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

/// A machine-applicable fix patch in JSON output.
#[derive(Serialize)]
struct JsonFixPatch {
    description: String,
    file: String,
    start_offset: usize,
    end_offset: usize,
    replacement: String,
}

/// A single diagnostic entry in JSON output.
#[derive(Serialize)]
struct JsonDiagnostic {
    code: &'static str,
    severity: &'static str,
    message: String,
    span: Option<JsonSpan>,
    suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix_patch: Option<JsonFixPatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    see_also: Option<String>,
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
///
/// Delegates to the unified [`render_json`] function.
pub fn render_parse_error_json(source: &str, filename: &str, error: &kodo_parser::ParseError) {
    render_json(source, filename, error);
}

/// Renders a [`kodo_types::TypeError`] as JSON to stdout.
///
/// Delegates to the unified [`render_json`] function.
pub fn render_type_error_json(source: &str, filename: &str, error: &kodo_types::TypeError) {
    render_json(source, filename, error);
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
