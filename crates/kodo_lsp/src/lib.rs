//! # `kodo_lsp` — Language Server Protocol for the Kōdo Compiler
//!
//! This crate implements an LSP server for the Kōdo programming language,
//! providing real-time diagnostics, hover information, and custom extensions
//! for AI agent integration.
//!
//! ## Features
//!
//! - **Diagnostics**: Real-time error and warning reporting as you type
//! - **Hover**: Type information, contracts, and confidence annotations
//! - **Custom Extensions**: `/kodo/contractStatus`, `/kodo/confidenceReport`
//!
//! ## Usage
//!
//! Start the server with `kodoc lsp` and connect via any LSP client.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use std::collections::HashMap;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// Errors from the LSP server.
#[derive(Debug)]
pub enum LspError {
    /// An I/O error occurred.
    Io(std::io::Error),
}

impl std::fmt::Display for LspError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for LspError {}

impl From<std::io::Error> for LspError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// The Kōdo language server backend.
pub struct KodoLanguageServer {
    /// The LSP client handle for sending notifications.
    client: Client,
    /// In-memory document store: URI → source text.
    documents: Mutex<HashMap<String, String>>,
}

impl KodoLanguageServer {
    /// Creates a new language server instance.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Mutex::new(HashMap::new()),
        }
    }

    /// Analyzes a document and publishes diagnostics.
    async fn analyze_document(&self, uri: &Url, text: &str) {
        let diagnostics = analyze_source(text);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }
}

/// Analyzes Kōdo source code and returns LSP diagnostics.
///
/// Runs the parser and type checker pipeline, collecting
/// any errors as LSP diagnostic entries.
fn analyze_source(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Parse (includes lexing)
    let module = match kodo_parser::parse(source) {
        Ok(module) => module,
        Err(e) => {
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
            return diagnostics;
        }
    };

    // Type check
    let mut checker = kodo_types::TypeChecker::new();
    if let Err(error) = checker.check_module(&module) {
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

    diagnostics
}

/// Converts a byte offset in source to (line, column) for LSP.
fn offset_to_line_col(source: &str, offset: u32) -> (u32, u32) {
    let offset = offset as usize;
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Finds the function at a given position in the source and returns
/// hover information including type, contracts, and annotations.
fn hover_at_position(source: &str, position: Position) -> Option<String> {
    let offset = line_col_to_offset(source, position.line, position.character)?;

    // Parse
    let module = kodo_parser::parse(source).ok()?;

    // Find function at offset
    #[allow(clippy::cast_possible_truncation)]
    let offset_u32 = offset as u32;
    for func in &module.functions {
        if func.span.start <= offset_u32 && offset_u32 <= func.span.end {
            use std::fmt::Write;
            let mut info = format!("**fn {}**", func.name);

            // Add parameter types
            if !func.params.is_empty() {
                info.push_str("\n\nParameters:\n");
                for p in &func.params {
                    let _ = writeln!(info, "- `{}: {:?}`", p.name, p.ty);
                }
            }

            // Add return type
            let _ = write!(info, "\nReturns: `{:?}`", func.return_type);

            // Add contracts
            if !func.requires.is_empty() {
                info.push_str("\n\n**Contracts:**\n");
                for _req in &func.requires {
                    info.push_str("- `requires { ... }`\n");
                }
            }
            if !func.ensures.is_empty() {
                for _ens in &func.ensures {
                    info.push_str("- `ensures { ... }`\n");
                }
            }

            // Add annotations
            for ann in &func.annotations {
                let _ = write!(info, "\n@{}", ann.name);
            }

            return Some(info);
        }
    }

    None
}

/// Converts (line, column) to a byte offset.
fn line_col_to_offset(source: &str, line: u32, col: u32) -> Option<usize> {
    let mut current_line = 0u32;
    let mut current_col = 0u32;
    for (i, ch) in source.char_indices() {
        if current_line == line && current_col == col {
            return Some(i);
        }
        if ch == '\n' {
            if current_line == line {
                return Some(i);
            }
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }
    if current_line == line {
        Some(source.len())
    } else {
        None
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for KodoLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "kodo-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Kōdo LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        if let Ok(mut docs) = self.documents.lock() {
            docs.insert(uri.to_string(), text.clone());
        }
        self.analyze_document(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;
            if let Ok(mut docs) = self.documents.lock() {
                docs.insert(uri.to_string(), text.clone());
            }
            self.analyze_document(&uri, &text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Ok(mut docs) = self.documents.lock() {
            docs.remove(&uri);
        }
        // Clear diagnostics
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let source = {
            let docs = self
                .documents
                .lock()
                .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
            docs.get(&uri.to_string()).cloned()
        };

        if let Some(source) = source {
            if let Some(span) = definition_at_position(&source, position) {
                let (line, col) = offset_to_line_col(&source, span.start);
                let (end_line, end_col) = offset_to_line_col(&source, span.end);
                let range = Range::new(Position::new(line, col), Position::new(end_line, end_col));
                return Ok(Some(GotoDefinitionResponse::Scalar(Location::new(
                    uri, range,
                ))));
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;

        let source = {
            let docs = self
                .documents
                .lock()
                .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
            docs.get(&uri.to_string()).cloned()
        };

        if let Some(source) = source {
            let items = completions_for_source(&source);
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let source = {
            let docs = self
                .documents
                .lock()
                .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
            docs.get(&uri.to_string()).cloned()
        };

        if let Some(source) = source {
            if let Some(info) = hover_at_position(&source, position) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: info,
                    }),
                    range: None,
                }));
            }
        }

        Ok(None)
    }
}

/// Finds the definition span of the identifier at the given position.
///
/// Parses the source, runs the type checker to build the definition index,
/// then looks up the word at the cursor position.
fn definition_at_position(source: &str, position: Position) -> Option<kodo_ast::Span> {
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

/// Extracts the word (identifier) at the given byte offset.
fn word_at_offset(source: &str, offset: usize) -> &str {
    let bytes = source.as_bytes();
    if offset >= bytes.len() {
        return "";
    }
    // Check if the offset is within an identifier character.
    if !is_ident_char(bytes[offset]) {
        return "";
    }
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }
    &source[start..end]
}

/// Returns true if the byte is a valid identifier character.
fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Returns completion items for the current source.
///
/// Provides function names, struct/enum names, and builtin method completions.
fn completions_for_source(source: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    let Ok(module) = kodo_parser::parse(source) else {
        return items;
    };

    let mut checker = kodo_types::TypeChecker::new();
    let _ = checker.check_module(&module);

    // Add function names.
    for func in &module.functions {
        items.push(CompletionItem {
            label: func.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(format!("fn {}(...)", func.name)),
            ..Default::default()
        });
    }

    // Add struct names.
    for type_decl in &module.type_decls {
        items.push(CompletionItem {
            label: type_decl.name.clone(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some(format!("struct {}", type_decl.name)),
            ..Default::default()
        });
    }

    // Add enum names.
    for enum_decl in &module.enum_decls {
        items.push(CompletionItem {
            label: enum_decl.name.clone(),
            kind: Some(CompletionItemKind::ENUM),
            detail: Some(format!("enum {}", enum_decl.name)),
            ..Default::default()
        });
    }

    // Add builtin functions.
    let builtins = [
        "println",
        "print",
        "print_int",
        "abs",
        "min",
        "max",
        "clamp",
        "file_exists",
        "file_read",
        "file_write",
        "list_new",
        "list_push",
        "list_get",
        "list_length",
        "list_contains",
        "map_new",
        "map_insert",
        "map_get",
        "map_contains_key",
        "map_length",
    ];
    for name in &builtins {
        items.push(CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("builtin".to_string()),
            ..Default::default()
        });
    }

    items
}

/// Starts the Kōdo LSP server on stdin/stdout.
///
/// # Errors
///
/// Returns [`LspError`] if the server encounters an I/O or transport error.
pub async fn run_server() -> std::result::Result<(), LspError> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(KodoLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_valid_source() {
        let source = r#"module test {
    meta {
        purpose: "test module",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        let diagnostics = analyze_source(source);
        assert!(
            diagnostics.is_empty(),
            "valid source should produce no diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn analyze_source_with_type_error() {
        let source = r#"module test {
    meta {
        purpose: "test module",
        version: "1.0.0"
    }

    fn bad() -> Int {
        return true
    }
}"#;
        let diagnostics = analyze_source(source);
        assert!(
            !diagnostics.is_empty(),
            "type error should produce diagnostics"
        );
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn analyze_source_with_parse_error() {
        let source = "module test { fn broken( }";
        let diagnostics = analyze_source(source);
        assert!(
            !diagnostics.is_empty(),
            "parse error should produce diagnostics"
        );
    }

    #[test]
    fn offset_to_line_col_basic() {
        let source = "line0\nline1\nline2";
        assert_eq!(offset_to_line_col(source, 0), (0, 0));
        assert_eq!(offset_to_line_col(source, 5), (0, 5));
        assert_eq!(offset_to_line_col(source, 6), (1, 0));
        assert_eq!(offset_to_line_col(source, 8), (1, 2));
    }

    #[test]
    fn line_col_to_offset_basic() {
        let source = "line0\nline1\nline2";
        assert_eq!(line_col_to_offset(source, 0, 0), Some(0));
        assert_eq!(line_col_to_offset(source, 1, 0), Some(6));
        assert_eq!(line_col_to_offset(source, 2, 0), Some(12));
    }

    #[test]
    fn hover_finds_function() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        // Position within the function body
        let hover = hover_at_position(source, Position::new(7, 10));
        assert!(hover.is_some(), "should find hover info for function");
        let info = hover.as_deref().unwrap_or("");
        assert!(
            info.contains("fn add"),
            "hover should contain function name"
        );
    }

    #[test]
    fn hover_returns_none_outside_function() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        let hover = hover_at_position(source, Position::new(0, 0));
        assert!(hover.is_none(), "no hover outside functions");
    }

    #[test]
    fn completions_include_functions() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn my_func(x: Int) -> Int {
        return x
    }
}"#;
        let items = completions_for_source(source);
        let func_names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            func_names.contains(&"my_func"),
            "should contain user function"
        );
        assert!(func_names.contains(&"println"), "should contain builtin");
        assert!(
            func_names.contains(&"list_new"),
            "should contain list builtin"
        );
        assert!(
            func_names.contains(&"map_new"),
            "should contain map builtin"
        );
    }

    #[test]
    fn definition_at_position_finds_function() {
        let source = r#"module test {
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
}"#;
        // Position of "add" in the call at line 11
        let span = definition_at_position(source, Position::new(11, 21));
        assert!(span.is_some(), "should find definition of add");
    }

    #[test]
    fn word_at_offset_extracts_identifier() {
        let source = "let hello = 42";
        assert_eq!(word_at_offset(source, 4), "hello");
        assert_eq!(word_at_offset(source, 5), "hello");
        assert_eq!(word_at_offset(source, 0), "let");
        assert_eq!(word_at_offset(source, 3), ""); // space
    }
}
