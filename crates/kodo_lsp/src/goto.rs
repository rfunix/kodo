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

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Url};

    fn source_with_call() -> &'static str {
        r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() {
        let x: Int = add(1, 2)
    }
}"#
    }

    #[test]
    fn goto_definition_of_function_call() {
        let source = source_with_call();
        // Position of "add" in the call "add(1, 2)" at line 11
        let span = definition_at_position(source, Position::new(11, 21));
        assert!(span.is_some(), "should find definition of add");
    }

    #[test]
    fn goto_definition_of_struct_type() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    struct Point {
        x: Int,
        y: Int
    }

    fn main() {
        let p: Point = Point { x: 1, y: 2 }
    }
}"#;
        // Position of "Point" in "let p: Point" at line 12
        let span = definition_at_position(source, Position::new(12, 15));
        assert!(span.is_some(), "should find definition of struct Point");
    }

    #[test]
    fn goto_definition_of_enum_type() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    enum Color {
        Red,
        Green,
        Blue
    }

    fn main() {
        let c: Color = Color::Red
    }
}"#;
        // Position of "Color" in "let c: Color" at line 13
        let span = definition_at_position(source, Position::new(13, 15));
        assert!(span.is_some(), "should find definition of enum Color");
    }

    #[test]
    fn goto_at_invalid_position_returns_none() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        // Position on whitespace/keyword
        let _span = definition_at_position(source, Position::new(0, 0));
        // "module" is found but is not a definition tracked by type checker
        // Either None or a span — depends on whether checker tracks it.
        // Let's test a position on a space character
        let span_space = definition_at_position(source, Position::new(0, 6));
        assert!(
            span_space.is_none(),
            "space character should not resolve to a definition"
        );
    }

    #[test]
    fn goto_definition_of_parameter() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        // Position of "a" in "return a + b" at line 7
        let span = definition_at_position(source, Position::new(7, 15));
        assert!(span.is_some(), "should find definition of parameter a");
    }

    #[test]
    fn references_at_position_finds_usages() {
        let source = source_with_call();
        let uri = Url::parse("file:///test.ko").unwrap();
        // Position of "add" in the call at line 11
        let refs = references_at_position(source, &uri, Position::new(11, 21));
        assert!(refs.is_some(), "should find references to add");
        let locations = refs.unwrap();
        assert!(
            !locations.is_empty(),
            "should find at least one reference to add"
        );
        // All locations should use our URI
        for loc in &locations {
            assert_eq!(loc.uri.as_str(), "file:///test.ko");
        }
    }

    #[test]
    fn references_at_invalid_position_returns_none() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let x: Int = 42
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        // Position in meta block — "purpose" is not tracked
        let refs = references_at_position(source, &uri, Position::new(2, 10));
        assert!(
            refs.is_none(),
            "should return None for identifier not in reference_spans"
        );
    }
}
