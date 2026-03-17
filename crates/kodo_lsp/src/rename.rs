//! Rename symbol provider for the Kōdo LSP server.
//!
//! Supports prepare-rename validation and full rename operations by
//! finding all occurrences of an identifier and generating text edits.

#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;

use crate::utils::{
    find_all_occurrences, is_ident_char, line_col_to_offset, offset_to_line_col, word_at_offset,
};

/// Prepares a rename at the given position, returning the range and current name.
///
/// Returns `None` if the cursor is not on a renamable identifier.
pub(crate) fn prepare_rename_at(source: &str, position: Position) -> Option<(Range, String)> {
    let offset = line_col_to_offset(source, position.line, position.character)?;
    let word = word_at_offset(source, offset);
    if word.is_empty() {
        return None;
    }

    // Verify it's a known symbol by parsing and checking
    let module = kodo_parser::parse(source).ok()?;
    let mut checker = kodo_types::TypeChecker::new();
    let _ = checker.check_module(&module);

    // Check if the word is a function name, struct name, enum name, or parameter/variable
    let is_known = checker.definition_spans().contains_key(word)
        || module.functions.iter().any(|f| f.name == word)
        || module.type_decls.iter().any(|t| t.name == word)
        || module.enum_decls.iter().any(|e| e.name == word)
        || module
            .functions
            .iter()
            .any(|f| f.params.iter().any(|p| p.name == word));

    if !is_known {
        return None;
    }

    // Find the range of the word at the cursor
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    #[allow(clippy::cast_possible_truncation)]
    let start_u32 = start as u32;
    #[allow(clippy::cast_possible_truncation)]
    let end_u32 = end as u32;
    let (sl, sc) = offset_to_line_col(source, start_u32);
    let (el, ec) = offset_to_line_col(source, end_u32);
    let range = Range::new(Position::new(sl, sc), Position::new(el, ec));

    Some((range, word.to_string()))
}

/// Performs a rename of the symbol at the given position, returning text edits.
///
/// Finds all occurrences of the identifier at the cursor and creates
/// edits to replace them with `new_name`.
pub(crate) fn rename_symbol(source: &str, position: Position, new_name: &str) -> Vec<TextEdit> {
    let Some(offset) = line_col_to_offset(source, position.line, position.character) else {
        return Vec::new();
    };
    let word = word_at_offset(source, offset);
    if word.is_empty() {
        return Vec::new();
    }

    let occurrences = find_all_occurrences(source, word);
    occurrences
        .into_iter()
        .map(|range| TextEdit {
            range,
            new_text: new_name.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    fn source_with_fn() -> &'static str {
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
    fn prepare_rename_on_function_name() {
        let source = source_with_fn();
        // Position on "add" in the function definition (line 6, col 7)
        let result = prepare_rename_at(source, Position::new(6, 7));
        assert!(result.is_some(), "should find renamable function name");
        let (range, name) = result.unwrap();
        assert_eq!(name, "add");
        // Range should span exactly the word "add"
        assert_eq!(range.start.line, range.end.line);
    }

    #[test]
    fn prepare_rename_on_parameter_name() {
        let source = source_with_fn();
        // Position on "a" in "fn add(a: Int, ...)" (line 6)
        let result = prepare_rename_at(source, Position::new(6, 11));
        assert!(result.is_some(), "should find renamable parameter name");
        let (_, name) = result.unwrap();
        assert_eq!(name, "a");
    }

    #[test]
    fn rename_symbol_changes_all_occurrences() {
        let source = source_with_fn();
        // Position on "add" in the function definition
        let edits = rename_symbol(source, Position::new(6, 7), "sum");
        assert!(
            edits.len() >= 2,
            "should create at least 2 rename edits (definition + call), got {}",
            edits.len()
        );
        for edit in &edits {
            assert_eq!(edit.new_text, "sum");
        }
    }

    #[test]
    fn prepare_rename_at_invalid_position_returns_none() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        // Position on "meta" — not a user-defined symbol
        let result = prepare_rename_at(source, Position::new(1, 5));
        assert!(
            result.is_none(),
            "should return None for non-renamable positions"
        );
    }

    #[test]
    fn prepare_rename_on_space_returns_none() {
        let source = "   ";
        let result = prepare_rename_at(source, Position::new(0, 1));
        assert!(result.is_none(), "should return None on whitespace");
    }

    #[test]
    fn rename_empty_source_returns_empty() {
        let edits = rename_symbol("", Position::new(0, 0), "foo");
        assert!(edits.is_empty(), "empty source should produce no edits");
    }

    #[test]
    fn rename_symbol_on_parameter() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        // Position on "a" in function body "return a + b"
        let edits = rename_symbol(source, Position::new(7, 15), "first");
        assert!(
            edits.len() >= 2,
            "should rename parameter in definition and usage, got {} edits",
            edits.len()
        );
        for edit in &edits {
            assert_eq!(edit.new_text, "first");
        }
    }
}
