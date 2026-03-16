//! Go to definition and find references providers for the Kōdo LSP server.
//!
//! Implements identifier resolution by parsing the source, running the
//! type checker to build definition and reference indices, and mapping
//! results back to LSP locations.

#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;

use crate::utils::{line_col_to_offset, offset_to_line_col, word_at_offset};

/// Finds the definition span of the identifier at the given position.
///
/// Parses the source, runs the type checker to build the definition index,
/// then looks up the word at the cursor position.
pub(crate) fn definition_at_position(source: &str, position: Position) -> Option<kodo_ast::Span> {
    let offset = line_col_to_offset(source, position.line, position.character)?;
    let word = word_at_offset(source, offset);
    if word.is_empty() {
        return None;
    }

    let module = kodo_parser::parse(source).ok()?;
    let mut checker = kodo_types::TypeChecker::new();
    let _ = checker.check_module(&module);

    checker.definition_spans().get(word).copied()
}

/// Finds all reference locations of the identifier at the given position.
///
/// Parses the source, runs the type checker to build the reference index,
/// then looks up the word at the cursor position and converts all usage
/// spans to LSP locations.
pub(crate) fn references_at_position(
    source: &str,
    uri: &Url,
    position: Position,
) -> Option<Vec<Location>> {
    let offset = line_col_to_offset(source, position.line, position.character)?;
    let word = word_at_offset(source, offset);
    if word.is_empty() {
        return None;
    }

    let module = kodo_parser::parse(source).ok()?;
    let mut checker = kodo_types::TypeChecker::new();
    let _ = checker.check_module(&module);

    let spans = checker.reference_spans().get(word)?;
    if spans.is_empty() {
        return None;
    }

    let locations: Vec<Location> = spans
        .iter()
        .map(|span| {
            let (line, col) = offset_to_line_col(source, span.start);
            let (end_line, end_col) = offset_to_line_col(source, span.end);
            let range = Range::new(Position::new(line, col), Position::new(end_line, end_col));
            Location::new(uri.clone(), range)
        })
        .collect();

    Some(locations)
}
