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

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{SymbolKind, Url};

    #[test]
    fn document_symbols_finds_functions() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() {
        let x: Int = 1
    }
}"#;
        let symbols = document_symbols(source);
        let func_names: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::FUNCTION)
            .map(|s| s.name.as_str())
            .collect();
        assert!(func_names.contains(&"add"), "should find function add");
        assert!(func_names.contains(&"main"), "should find function main");
    }

    #[test]
    fn document_symbols_finds_structs_and_enums() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    struct Point {
        x: Int,
        y: Int
    }

    enum Color {
        Red,
        Green,
        Blue
    }

    fn main() {
        let x: Int = 1
    }
}"#;
        let symbols = document_symbols(source);
        let struct_syms: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::STRUCT)
            .map(|s| s.name.as_str())
            .collect();
        let enum_syms: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::ENUM)
            .map(|s| s.name.as_str())
            .collect();
        assert!(struct_syms.contains(&"Point"), "should find struct Point");
        assert!(enum_syms.contains(&"Color"), "should find enum Color");
    }

    #[test]
    fn document_symbols_container_is_module_name() {
        let source = r#"module mymod {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn hello() {
        return
    }
}"#;
        let symbols = document_symbols(source);
        assert!(!symbols.is_empty());
        for sym in &symbols {
            assert_eq!(
                sym.container_name,
                Some("mymod".to_string()),
                "container_name should be the module name"
            );
        }
    }

    #[test]
    fn document_symbols_empty_module() {
        let source = r#"module empty {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        let symbols = document_symbols(source);
        assert!(
            symbols.is_empty(),
            "module with no declarations should have no symbols"
        );
    }

    #[test]
    fn workspace_symbols_returns_all_declarations() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    struct Point {
        x: Int,
        y: Int
    }

    enum Color {
        Red,
        Green,
        Blue
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn multiply(a: Int, b: Int) -> Int {
        return a
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let symbols = workspace_symbols_for_source(source, &uri);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Point"), "should contain struct Point");
        assert!(names.contains(&"Color"), "should contain enum Color");
        assert!(names.contains(&"add"), "should contain fn add");
        assert!(names.contains(&"multiply"), "should contain fn multiply");

        // Verify real URI is used
        for s in &symbols {
            assert_eq!(s.location.uri.as_str(), "file:///test.ko");
        }
    }

    #[test]
    fn workspace_symbols_empty_returns_empty() {
        let source = r#"module empty {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let symbols = workspace_symbols_for_source(source, &uri);
        assert!(symbols.is_empty());
    }

    #[test]
    fn document_symbols_finds_intents() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    intent api {
        route: "/hello"
    }
}"#;
        let symbols = document_symbols(source);
        let intent_syms: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::INTERFACE)
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            intent_syms.contains(&"api"),
            "should find intent declarations"
        );
    }
}
