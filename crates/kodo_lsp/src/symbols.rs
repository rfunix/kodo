//! Document and workspace symbol providers for the Kōdo LSP server.
//!
//! Provides symbol outline information for documents (functions, structs,
//! enums, intents) and workspace-wide symbol search across all open documents.

#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;

use crate::utils::offset_to_line_col;

/// Returns document symbols (outline) for the given source.
pub(crate) fn document_symbols(source: &str) -> Vec<SymbolInformation> {
    let Ok(module) = kodo_parser::parse(source) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    let Ok(uri) = Url::parse("file:///tmp/dummy") else {
        return Vec::new();
    };

    // Functions
    for func in &module.functions {
        let (line, col) = offset_to_line_col(source, func.span.start);
        let (end_line, end_col) = offset_to_line_col(source, func.span.end);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name: func.name.clone(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: Location::new(
                uri.clone(),
                Range::new(Position::new(line, col), Position::new(end_line, end_col)),
            ),
            container_name: Some(module.name.clone()),
        });
    }

    // Structs
    for type_decl in &module.type_decls {
        let (line, col) = offset_to_line_col(source, type_decl.span.start);
        let (end_line, end_col) = offset_to_line_col(source, type_decl.span.end);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name: type_decl.name.clone(),
            kind: SymbolKind::STRUCT,
            tags: None,
            deprecated: None,
            location: Location::new(
                uri.clone(),
                Range::new(Position::new(line, col), Position::new(end_line, end_col)),
            ),
            container_name: Some(module.name.clone()),
        });
    }

    // Enums
    for enum_decl in &module.enum_decls {
        let (line, col) = offset_to_line_col(source, enum_decl.span.start);
        let (end_line, end_col) = offset_to_line_col(source, enum_decl.span.end);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name: enum_decl.name.clone(),
            kind: SymbolKind::ENUM,
            tags: None,
            deprecated: None,
            location: Location::new(
                uri.clone(),
                Range::new(Position::new(line, col), Position::new(end_line, end_col)),
            ),
            container_name: Some(module.name.clone()),
        });
    }

    // Intents
    for intent in &module.intent_decls {
        let (line, col) = offset_to_line_col(source, intent.span.start);
        let (end_line, end_col) = offset_to_line_col(source, intent.span.end);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name: intent.name.clone(),
            kind: SymbolKind::INTERFACE,
            tags: None,
            deprecated: None,
            location: Location::new(
                uri.clone(),
                Range::new(Position::new(line, col), Position::new(end_line, end_col)),
            ),
            container_name: Some(module.name.clone()),
        });
    }

    symbols
}

/// Returns workspace symbols for a single document, using the real document URI.
pub(crate) fn workspace_symbols_for_source(source: &str, uri: &Url) -> Vec<SymbolInformation> {
    let Ok(module) = kodo_parser::parse(source) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();

    // Functions
    for func in &module.functions {
        let (line, col) = offset_to_line_col(source, func.span.start);
        let (end_line, end_col) = offset_to_line_col(source, func.span.end);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name: func.name.clone(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: Location::new(
                uri.clone(),
                Range::new(Position::new(line, col), Position::new(end_line, end_col)),
            ),
            container_name: Some(module.name.clone()),
        });
    }

    // Structs
    for type_decl in &module.type_decls {
        let (line, col) = offset_to_line_col(source, type_decl.span.start);
        let (end_line, end_col) = offset_to_line_col(source, type_decl.span.end);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name: type_decl.name.clone(),
            kind: SymbolKind::STRUCT,
            tags: None,
            deprecated: None,
            location: Location::new(
                uri.clone(),
                Range::new(Position::new(line, col), Position::new(end_line, end_col)),
            ),
            container_name: Some(module.name.clone()),
        });
    }

    // Enums
    for enum_decl in &module.enum_decls {
        let (line, col) = offset_to_line_col(source, enum_decl.span.start);
        let (end_line, end_col) = offset_to_line_col(source, enum_decl.span.end);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name: enum_decl.name.clone(),
            kind: SymbolKind::ENUM,
            tags: None,
            deprecated: None,
            location: Location::new(
                uri.clone(),
                Range::new(Position::new(line, col), Position::new(end_line, end_col)),
            ),
            container_name: Some(module.name.clone()),
        });
    }

    symbols
}
