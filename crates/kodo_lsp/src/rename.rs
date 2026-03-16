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
