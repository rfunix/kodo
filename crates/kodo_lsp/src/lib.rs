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
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
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

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
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
            if let Some(sig) = signature_at_position(&source, position) {
                return Ok(Some(sig));
            }
        }

        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let source = {
            let docs = self
                .documents
                .lock()
                .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
            docs.get(&uri.to_string()).cloned()
        };

        if let Some(source) = source {
            let symbols = document_symbols(&source);
            if !symbols.is_empty() {
                return Ok(Some(DocumentSymbolResponse::Flat(symbols)));
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
/// Provides function names, struct/enum names, builtin functions,
/// string method completions, and struct field completions.
#[allow(clippy::too_many_lines)]
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

    // Add string method completions (triggered by dot on String values).
    let string_methods = [
        ("length", "Returns the length of the string", "() -> Int"),
        (
            "contains",
            "Checks if the string contains a substring",
            "(sub: String) -> Bool",
        ),
        (
            "starts_with",
            "Checks if the string starts with a prefix",
            "(prefix: String) -> Bool",
        ),
        (
            "ends_with",
            "Checks if the string ends with a suffix",
            "(suffix: String) -> Bool",
        ),
        (
            "trim",
            "Removes leading and trailing whitespace",
            "() -> String",
        ),
        ("to_upper", "Converts to uppercase", "() -> String"),
        ("to_lower", "Converts to lowercase", "() -> String"),
        (
            "substring",
            "Extracts a substring",
            "(start: Int, end: Int) -> String",
        ),
        (
            "to_string",
            "Converts to string representation",
            "() -> String",
        ),
    ];
    for (name, doc, signature) in &string_methods {
        items.push(CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(format!("String.{name}{signature}")),
            documentation: Some(Documentation::String((*doc).to_string())),
            ..Default::default()
        });
    }

    // Add struct field names for dot-completion.
    for type_decl in &module.type_decls {
        for field in &type_decl.fields {
            items.push(CompletionItem {
                label: field.name.clone(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(format!("{}.{}", type_decl.name, field.name)),
                ..Default::default()
            });
        }
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

/// Returns signature help for the function call at the given position.
fn signature_at_position(source: &str, position: Position) -> Option<SignatureHelp> {
    let offset = line_col_to_offset(source, position.line, position.character)?;

    // Walk backwards from cursor to find the function name before '('
    let bytes = source.as_bytes();
    let mut paren_pos = offset;
    let mut depth = 0i32;

    // Find the matching '(' by walking back
    while paren_pos > 0 {
        paren_pos -= 1;
        match bytes[paren_pos] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    if paren_pos == 0 && bytes[0] != b'(' {
        return None;
    }

    // Extract function name before '('
    let func_name = {
        let mut end = paren_pos;
        while end > 0 && bytes[end - 1] == b' ' {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && is_ident_char(bytes[start - 1]) {
            start -= 1;
        }
        &source[start..end]
    };

    if func_name.is_empty() {
        return None;
    }

    // Count which parameter we're on (count commas at depth 0)
    let mut active_param = 0u32;
    let mut scan = paren_pos + 1;
    let mut scan_depth = 0i32;
    while scan < offset {
        match bytes[scan] {
            b'(' => scan_depth += 1,
            b')' => scan_depth -= 1,
            b',' if scan_depth == 0 => active_param += 1,
            _ => {}
        }
        scan += 1;
    }

    // Parse and find the function
    let module = kodo_parser::parse(source).ok()?;

    for func in &module.functions {
        if func.name == func_name {
            let params_str: Vec<String> = func
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, format_type_expr(&p.ty)))
                .collect();
            let ret_str = format_type_expr(&func.return_type);
            let label = format!("fn {}({}) -> {}", func.name, params_str.join(", "), ret_str);

            let param_infos: Vec<ParameterInformation> = func
                .params
                .iter()
                .map(|p| ParameterInformation {
                    label: ParameterLabel::Simple(format!(
                        "{}: {}",
                        p.name,
                        format_type_expr(&p.ty)
                    )),
                    documentation: None,
                })
                .collect();

            // Build documentation with contracts
            let mut doc_parts = Vec::new();
            for req in &func.requires {
                doc_parts.push(format!("requires {{ {} }}", format_expr(req)));
            }
            for ens in &func.ensures {
                doc_parts.push(format!("ensures {{ {} }}", format_expr(ens)));
            }
            let documentation = if doc_parts.is_empty() {
                None
            } else {
                Some(Documentation::String(doc_parts.join("\n")))
            };

            return Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label,
                    documentation,
                    parameters: Some(param_infos),
                    active_parameter: Some(active_param),
                }],
                active_signature: Some(0),
                active_parameter: Some(active_param),
            });
        }
    }

    None
}

/// Formats a type expression as a string for display.
fn format_type_expr(ty: &kodo_ast::TypeExpr) -> String {
    match ty {
        kodo_ast::TypeExpr::Named(name) => name.clone(),
        kodo_ast::TypeExpr::Unit => "Unit".to_string(),
        kodo_ast::TypeExpr::Generic(name, args) => {
            let args_str: Vec<String> = args.iter().map(format_type_expr).collect();
            format!("{name}<{}>", args_str.join(", "))
        }
        kodo_ast::TypeExpr::Function(params, ret) => {
            let params_str: Vec<String> = params.iter().map(format_type_expr).collect();
            format!("({}) -> {}", params_str.join(", "), format_type_expr(ret))
        }
        kodo_ast::TypeExpr::Optional(inner) => {
            format!("{}?", format_type_expr(inner))
        }
    }
}

/// Formats an expression as a string for display (used in contract display).
fn format_expr(expr: &kodo_ast::Expr) -> String {
    match expr {
        kodo_ast::Expr::Ident(name, _) => name.clone(),
        kodo_ast::Expr::IntLit(n, _) => n.to_string(),
        kodo_ast::Expr::BoolLit(b, _) => b.to_string(),
        kodo_ast::Expr::StringLit(s, _) => format!("\"{s}\""),
        kodo_ast::Expr::BinaryOp {
            left, op, right, ..
        } => {
            format!("{} {op:?} {}", format_expr(left), format_expr(right))
        }
        _ => "...".to_string(),
    }
}

/// Returns document symbols (outline) for the given source.
fn document_symbols(source: &str) -> Vec<SymbolInformation> {
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

    #[test]
    fn completions_include_string_methods() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let s: String = "hello"
    }
}"#;
        let items = completions_for_source(source);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"length"),
            "should contain string method length"
        );
        assert!(
            labels.contains(&"contains"),
            "should contain string method contains"
        );
        assert!(
            labels.contains(&"trim"),
            "should contain string method trim"
        );
    }

    #[test]
    fn completions_include_struct_fields() {
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
        let items = completions_for_source(source);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"x"), "should contain struct field x");
        assert!(labels.contains(&"y"), "should contain struct field y");
    }

    #[test]
    fn document_symbols_lists_declarations() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    struct Point {
        x: Int,
        y: Int
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() {
        let p: Point = Point { x: 1, y: 2 }
    }
}"#;
        let symbols = document_symbols(source);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Point"), "should contain struct Point");
        assert!(names.contains(&"add"), "should contain fn add");
        assert!(names.contains(&"main"), "should contain fn main");
    }

    #[test]
    fn signature_help_finds_function() {
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
        // Position inside add(1, 2) — after the opening paren on line 11
        let sig = signature_at_position(source, Position::new(11, 26));
        assert!(sig.is_some(), "should find signature help for add");
        let help = sig.unwrap();
        assert_eq!(help.signatures.len(), 1);
        assert!(help.signatures[0].label.contains("add"));
    }

    #[test]
    fn analyze_source_contract_diagnostics() {
        // A function with contracts that has a type error should produce diagnostics
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn divide(a: Int, b: Int) -> Int
        requires { b > 0 }
        ensures { result >= 0 }
    {
        return true
    }
}"#;
        let diagnostics = analyze_source(source);
        assert!(
            !diagnostics.is_empty(),
            "function with contract and type error should produce diagnostics"
        );
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn completions_include_all_string_methods() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let s: String = "hello"
    }
}"#;
        let items = completions_for_source(source);
        let method_items: Vec<&CompletionItem> = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::METHOD))
            .collect();

        let method_names: Vec<&str> = method_items.iter().map(|i| i.label.as_str()).collect();
        // Verify all 9 string methods are present
        for expected in &[
            "length",
            "contains",
            "starts_with",
            "ends_with",
            "trim",
            "to_upper",
            "to_lower",
            "substring",
            "to_string",
        ] {
            assert!(
                method_names.contains(expected),
                "missing string method: {expected}"
            );
        }

        // Verify methods have documentation
        for item in &method_items {
            assert!(
                item.documentation.is_some(),
                "string method '{}' should have documentation",
                item.label
            );
        }
    }

    #[test]
    fn hover_shows_parameter_and_return_type() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn greet(name: String, count: Int) -> String {
        return name
    }
}"#;
        // Position within the function body (line 7)
        let hover = hover_at_position(source, Position::new(7, 10));
        assert!(hover.is_some(), "should find hover info");
        let info = hover.unwrap();
        assert!(info.contains("fn greet"), "should contain function name");
        assert!(info.contains("name"), "should contain parameter name");
        assert!(info.contains("Returns:"), "should contain return type");
    }

    #[test]
    fn signature_help_returns_none_outside_parens() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() {
        let x: Int = 42
    }
}"#;
        // Position on `let x: Int = 42` — no parentheses around cursor
        let sig = signature_at_position(source, Position::new(11, 10));
        assert!(
            sig.is_none(),
            "should return None when cursor is not inside parentheses"
        );
    }

    #[test]
    fn analyze_empty_document() {
        let source = "";
        let diagnostics = analyze_source(source);
        // An empty document should produce a parse error diagnostic
        assert!(
            !diagnostics.is_empty(),
            "empty document should produce diagnostics (parse error)"
        );
    }

    #[test]
    fn document_symbols_include_struct_fields_via_kind() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    struct Person {
        name: String,
        age: Int
    }

    fn main() {
        let p: Person = Person { name: "Alice", age: 30 }
    }
}"#;
        let symbols = document_symbols(source);
        // Verify struct appears in symbols
        let struct_symbols: Vec<&SymbolInformation> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::STRUCT)
            .collect();
        assert!(!struct_symbols.is_empty(), "should have struct symbols");
        assert_eq!(struct_symbols[0].name, "Person");
        // Verify container_name is set to module name
        assert_eq!(
            struct_symbols[0].container_name,
            Some("test".to_string()),
            "struct should be contained in module"
        );
    }

    #[test]
    fn completions_include_builtin_functions() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let x: Int = 1
    }
}"#;
        let items = completions_for_source(source);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        // Verify core builtin functions are present
        for builtin in &[
            "println",
            "print",
            "abs",
            "min",
            "max",
            "clamp",
            "file_exists",
        ] {
            assert!(
                labels.contains(builtin),
                "should contain builtin function: {builtin}"
            );
        }

        // Verify builtins have "builtin" detail
        let builtin_items: Vec<&CompletionItem> = items
            .iter()
            .filter(|i| i.detail.as_deref() == Some("builtin"))
            .collect();
        assert!(
            builtin_items.len() >= 18,
            "should have at least 18 builtin completions, got {}",
            builtin_items.len()
        );
    }

    #[test]
    fn analyze_source_parse_error_has_error_code() {
        let source = "module test { fn }";
        let diagnostics = analyze_source(source);
        assert!(!diagnostics.is_empty(), "should produce diagnostics");
        // Verify the diagnostic has a code and source
        assert!(
            diagnostics[0].code.is_some(),
            "diagnostic should have an error code"
        );
        assert_eq!(
            diagnostics[0].source,
            Some("kodo".to_string()),
            "diagnostic source should be 'kodo'"
        );
    }

    #[test]
    fn format_type_expr_generic() {
        let ty = kodo_ast::TypeExpr::Generic(
            "List".to_string(),
            vec![kodo_ast::TypeExpr::Named("Int".to_string())],
        );
        let formatted = format_type_expr(&ty);
        assert_eq!(formatted, "List<Int>");
    }

    #[test]
    fn format_type_expr_optional() {
        let ty =
            kodo_ast::TypeExpr::Optional(Box::new(kodo_ast::TypeExpr::Named("String".to_string())));
        let formatted = format_type_expr(&ty);
        assert_eq!(formatted, "String?");
    }

    #[test]
    fn format_type_expr_function() {
        let ty = kodo_ast::TypeExpr::Function(
            vec![
                kodo_ast::TypeExpr::Named("Int".to_string()),
                kodo_ast::TypeExpr::Named("Int".to_string()),
            ],
            Box::new(kodo_ast::TypeExpr::Named("Bool".to_string())),
        );
        let formatted = format_type_expr(&ty);
        assert_eq!(formatted, "(Int, Int) -> Bool");
    }

    #[test]
    fn document_symbols_for_enums() {
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
        let symbols = document_symbols(source);
        let enum_symbols: Vec<&SymbolInformation> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::ENUM)
            .collect();
        assert!(!enum_symbols.is_empty(), "should have enum symbols");
        assert_eq!(enum_symbols[0].name, "Color");
    }

    #[test]
    fn hover_shows_contracts() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn divide(a: Int, b: Int) -> Int
        requires { b > 0 }
    {
        return a / b
    }
}"#;
        // Position within the function body
        let hover = hover_at_position(source, Position::new(9, 10));
        assert!(hover.is_some(), "should find hover info for function");
        let info = hover.unwrap();
        assert!(info.contains("fn divide"), "should contain function name");
        assert!(
            info.contains("requires"),
            "should contain contract information"
        );
    }

    #[test]
    fn signature_help_active_parameter_advances() {
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
        // Position after the comma — should be on the second parameter
        let sig = signature_at_position(source, Position::new(11, 29));
        assert!(sig.is_some(), "should find signature help");
        let help = sig.unwrap();
        assert_eq!(
            help.active_parameter,
            Some(1),
            "active parameter should be 1 (second param) after the comma"
        );
    }

    #[test]
    fn completions_include_enum_names() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    enum Direction {
        North,
        South,
        East,
        West
    }

    fn main() {
        let d: Direction = Direction::North
    }
}"#;
        let items = completions_for_source(source);
        let enum_items: Vec<&CompletionItem> = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::ENUM))
            .collect();
        assert!(!enum_items.is_empty(), "should have enum completion items");
        assert_eq!(enum_items[0].label, "Direction");
    }
}
