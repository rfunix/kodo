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
///
/// When `include_declaration` is `true`, the definition site is included
/// as the first location in the returned list (if it exists in the
/// definition index). This matches the LSP `ReferenceContext.includeDeclaration`
/// semantics.
pub(crate) fn references_at_position(
    source: &str,
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    let offset = line_col_to_offset(source, position.line, position.character)?;
    let word = word_at_offset(source, offset);
    if word.is_empty() {
        return None;
    }

    let module = kodo_parser::parse(source).ok()?;
    let mut checker = kodo_types::TypeChecker::new();
    let _ = checker.check_module(&module);

    let ref_spans = checker.reference_spans().get(word).cloned();
    let def_span = checker.definition_spans().get(word).copied();

    // If neither references nor definition exist, nothing to return.
    let has_refs = ref_spans.as_ref().is_some_and(|s| !s.is_empty());
    if !(has_refs || (include_declaration && def_span.is_some())) {
        return None;
    }

    let span_to_location = |span: &kodo_ast::Span| {
        let (line, col) = offset_to_line_col(source, span.start);
        let (end_line, end_col) = offset_to_line_col(source, span.end);
        let range = Range::new(Position::new(line, col), Position::new(end_line, end_col));
        Location::new(uri.clone(), range)
    };

    let mut locations = Vec::new();

    // Include the declaration site first when requested.
    if include_declaration {
        if let Some(def) = &def_span {
            locations.push(span_to_location(def));
        }
    }

    // Append all reference (usage) sites.
    if let Some(spans) = &ref_spans {
        for span in spans {
            locations.push(span_to_location(span));
        }
    }

    if locations.is_empty() {
        return None;
    }

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

    // ── goto-definition: function call ────────────────────────────

    #[test]
    fn goto_definition_of_function_call() {
        let source = source_with_call();
        // Position of "add" in the call "add(1, 2)" at line 11
        let span = definition_at_position(source, Position::new(11, 21));
        assert!(span.is_some(), "should find definition of add");
        let def = span.unwrap();
        let (line, _col) = offset_to_line_col(source, def.start);
        assert_eq!(line, 6, "definition of add should be on line 6 (fn add)");
    }

    // ── goto-definition: local variable (let binding) ─────────────

    #[test]
    fn goto_definition_of_local_variable() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let x: Int = 42
        let y: Int = x + 1
        return y
    }
}"#;
        // Position of "x" in "let y: Int = x + 1" at line 8
        let span = definition_at_position(source, Position::new(8, 21));
        assert!(span.is_some(), "should find definition of x");
        let def = span.unwrap();
        let (line, _col) = offset_to_line_col(source, def.start);
        assert_eq!(line, 7, "definition of x should be on line 7 (let x)");
    }

    // ── goto-definition: struct type ──────────────────────────────

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
        let span = definition_at_position(source, Position::new(12, 15));
        assert!(span.is_some(), "should find definition of struct Point");
        let def = span.unwrap();
        let (line, _col) = offset_to_line_col(source, def.start);
        assert_eq!(
            line, 6,
            "definition of Point should be on line 6 (struct Point)"
        );
    }

    // ── goto-definition: enum type ────────────────────────────────

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
        let span = definition_at_position(source, Position::new(13, 15));
        assert!(span.is_some(), "should find definition of enum Color");
        let def = span.unwrap();
        let (line, _col) = offset_to_line_col(source, def.start);
        assert_eq!(
            line, 6,
            "definition of Color should be on line 6 (enum Color)"
        );
    }

    // ── goto-definition: invalid position ─────────────────────────

    #[test]
    fn goto_at_invalid_position_returns_none() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        // Position on a space character
        let span_space = definition_at_position(source, Position::new(0, 6));
        assert!(
            span_space.is_none(),
            "space character should not resolve to a definition"
        );
    }

    // ── goto-definition: parameter ────────────────────────────────

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
        let span = definition_at_position(source, Position::new(7, 15));
        assert!(span.is_some(), "should find definition of parameter a");
        let def = span.unwrap();
        let (line, _col) = offset_to_line_col(source, def.start);
        assert_eq!(
            line, 6,
            "definition of parameter a should be on line 6 (fn add(a: ...))"
        );
    }

    // ── goto-definition: variable used in return ──────────────────

    #[test]
    fn goto_definition_of_variable_in_return() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn compute() -> Int {
        let result: Int = 10
        return result
    }
}"#;
        let span = definition_at_position(source, Position::new(8, 15));
        assert!(span.is_some(), "should find definition of result");
        let def = span.unwrap();
        let (line, _col) = offset_to_line_col(source, def.start);
        assert_eq!(
            line, 7,
            "definition of result should be on line 7 (let result)"
        );
    }

    // ── find-references: basic (no declaration) ───────────────────

    #[test]
    fn references_without_declaration() {
        let source = source_with_call();
        let uri = Url::parse("file:///test.ko").unwrap();
        let refs = references_at_position(source, &uri, Position::new(11, 21), false);
        assert!(refs.is_some(), "should find references to add");
        let locations = refs.unwrap();
        assert!(
            !locations.is_empty(),
            "should find at least one reference to add"
        );
        for loc in &locations {
            assert_eq!(loc.uri.as_str(), "file:///test.ko");
        }
    }

    // ── find-references: include_declaration adds the def site ────

    #[test]
    fn references_with_declaration_includes_definition_site() {
        let source = source_with_call();
        let uri = Url::parse("file:///test.ko").unwrap();
        let refs_with = references_at_position(source, &uri, Position::new(11, 21), true);
        let refs_without = references_at_position(source, &uri, Position::new(11, 21), false);

        assert!(refs_with.is_some());
        assert!(refs_without.is_some());

        let with_decl = refs_with.unwrap();
        let without_decl = refs_without.unwrap();

        assert!(
            with_decl.len() > without_decl.len(),
            "include_declaration=true should return more locations ({} vs {})",
            with_decl.len(),
            without_decl.len()
        );
        // First location with include_declaration should be the fn definition
        assert_eq!(
            with_decl[0].range.start.line, 6,
            "definition of add should be on line 6"
        );
    }

    // ── find-references: parameter used multiple times ────────────

    #[test]
    fn references_for_parameter_used_multiple_times() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn compute(a: Int, b: Int) -> Int {
        let sum: Int = a + b
        let doubled: Int = a + a
        return sum + doubled
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let refs = references_at_position(source, &uri, Position::new(7, 23), false);
        assert!(refs.is_some(), "should find references to parameter a");
        let locations = refs.unwrap();
        assert!(
            locations.len() >= 3,
            "should find at least 3 references to a, found {}",
            locations.len()
        );
    }

    // ── find-references: multiple call sites ──────────────────────

    #[test]
    fn references_for_function_call_sites() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn helper(x: Int) -> Int {
        return x
    }

    fn main() {
        let a: Int = helper(1)
        let b: Int = helper(2)
        let c: Int = helper(3)
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let refs = references_at_position(source, &uri, Position::new(11, 21), false);
        assert!(refs.is_some(), "should find references to helper");
        let locations = refs.unwrap();
        assert_eq!(
            locations.len(),
            3,
            "should find exactly 3 call sites for helper, found {}",
            locations.len()
        );
    }

    // ── find-references: untracked identifier ─────────────────────

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
        let refs = references_at_position(source, &uri, Position::new(2, 10), false);
        assert!(
            refs.is_none(),
            "should return None for identifier not in reference_spans"
        );
    }

    // ── find-references: URI preserved ────────────────────────────

    #[test]
    fn references_uri_is_preserved() {
        let source = source_with_call();
        let uri = Url::parse("file:///my/project/test.ko").unwrap();
        let refs = references_at_position(source, &uri, Position::new(11, 21), false);
        assert!(refs.is_some());
        for loc in refs.unwrap() {
            assert_eq!(
                loc.uri.as_str(),
                "file:///my/project/test.ko",
                "URI should be preserved from input"
            );
        }
    }
}
