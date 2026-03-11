//! Parse error types and diagnostic integration.
//!
//! This module defines the [`ParseError`] enum that represents all errors the
//! parser can produce, along with its [`Diagnostic`](kodo_ast::Diagnostic)
//! implementation for rich error reporting with source spans, fix suggestions,
//! and structured error codes.

use kodo_ast::Span;
use kodo_lexer::TokenKind;
use thiserror::Error;

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    /// An unexpected token was encountered.
    #[error("expected {expected}, found {found:?} at {span:?}")]
    UnexpectedToken {
        /// What was expected.
        expected: String,
        /// What was actually found.
        found: TokenKind,
        /// Source location.
        span: Span,
    },
    /// Unexpected end of input.
    #[error("unexpected end of input, expected {expected}")]
    UnexpectedEof {
        /// What was expected.
        expected: String,
    },
    /// A lexer error propagated up.
    #[error("lexer error: {0}")]
    LexError(#[from] kodo_lexer::LexError),
}

impl ParseError {
    /// Returns the source span of this error, if available.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::UnexpectedToken { span, .. } => Some(*span),
            Self::UnexpectedEof { .. } | Self::LexError(_) => None,
        }
    }

    /// Returns the unique error code for this error variant.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnexpectedToken { .. } => "E0100",
            Self::UnexpectedEof { .. } => "E0101",
            Self::LexError(_) => "E0001",
        }
    }
}

impl kodo_ast::Diagnostic for ParseError {
    fn code(&self) -> &'static str {
        self.code()
    }

    fn severity(&self) -> kodo_ast::Severity {
        kodo_ast::Severity::Error
    }

    fn span(&self) -> Option<kodo_ast::Span> {
        self.span()
    }

    fn message(&self) -> String {
        self.to_string()
    }

    fn suggestion(&self) -> Option<String> {
        match self {
            Self::UnexpectedToken { .. } => {
                Some("check for missing delimiters or keywords".to_string())
            }
            Self::UnexpectedEof { expected } => {
                Some(format!("the file ended before the parser found {expected}"))
            }
            Self::LexError(_) => Some("check for invalid characters in the source".to_string()),
        }
    }

    fn labels(&self) -> Vec<kodo_ast::DiagnosticLabel> {
        if let Some(span) = self.span() {
            vec![kodo_ast::DiagnosticLabel {
                span,
                message: self.to_string(),
            }]
        } else {
            Vec::new()
        }
    }

    fn fix_patch(&self) -> Option<kodo_ast::FixPatch> {
        match self {
            Self::UnexpectedToken { expected, span, .. } => {
                let insert = match expected.as_str() {
                    "RBrace" => Some(("}", "insert closing `}`")),
                    "RParen" => Some((")", "insert closing `)`")),
                    "RBracket" => Some(("]", "insert closing `]`")),
                    _ => None,
                };
                insert.map(|(text, desc)| kodo_ast::FixPatch {
                    description: desc.to_string(),
                    file: String::new(),
                    start_offset: span.start as usize,
                    end_offset: span.start as usize,
                    replacement: text.to_string(),
                })
            }
            Self::UnexpectedEof { expected } => {
                let insert = match expected.as_str() {
                    "RBrace" => Some(("}", "append closing `}` at end of file")),
                    "RParen" => Some((")", "append closing `)` at end of file")),
                    "RBracket" => Some(("]", "append closing `]` at end of file")),
                    _ => None,
                };
                insert.map(|(text, desc)| kodo_ast::FixPatch {
                    description: desc.to_string(),
                    file: String::new(),
                    start_offset: 0,
                    end_offset: 0,
                    replacement: text.to_string(),
                })
            }
            Self::LexError(_) => None,
        }
    }
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Result of parsing with error recovery — contains partial AST and accumulated errors.
///
/// When the parser encounters an error it synchronizes to the next reliable
/// token boundary and continues, collecting all diagnostics in one pass.
/// This is essential for LSP and IDE integration where reporting every error
/// at once gives the programmer (or AI agent) a complete picture.
///
/// # Academic Reference
///
/// Panic-mode recovery as described in **\[CI\]** *Crafting Interpreters* Ch. 6
/// and **\[EC\]** *Engineering a Compiler* Ch. 3.4.
pub struct ParseOutput {
    /// The (possibly incomplete) parsed module.
    pub module: kodo_ast::Module,
    /// All parse errors encountered during parsing.
    pub errors: Vec<ParseError>,
}
