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
}
