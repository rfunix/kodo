//! # `kodo_parser` — Recursive Descent Parser for the Kōdo Language
//!
//! This crate transforms a token stream from [`kodo_lexer`] into an AST
//! defined in [`kodo_ast`]. It uses a hand-written recursive descent LL(1)
//! parser for maximum control over error recovery and diagnostics.
//!
//! Kōdo's syntax is intentionally simple and unambiguous to make it easy
//! for AI agents to generate correct programs and for humans to audit them.
//!
//! ## Expression Parsing
//!
//! Expressions are parsed using a recursive descent approach with one method
//! per precedence level, following the grammar in `docs/grammar.ebnf`.
//! Precedence levels (lowest to highest):
//!
//! 1. `||` (logical or)
//! 2. `&&` (logical and)
//! 3. `==`, `!=` (equality)
//! 4. `<`, `>`, `<=`, `>=` (comparison)
//! 5. `+`, `-` (additive)
//! 6. `*`, `/`, `%` (multiplicative)
//! 7. Unary: `!`, `-`
//! 8. Postfix: function calls, field access
//! 9. Primary: literals, identifiers, `if`/`else`, blocks, parenthesized
//!
//! ## Module Structure
//!
//! The parser is split across several modules for maintainability:
//!
//! - `error` — Error types, diagnostic integration, and `ParseOutput`
//! - `expr` — Expression parsing (all precedence levels)
//! - `stmt` — Statement parsing (let, return, for, while, etc.)
//! - `decl` — Declaration parsing (functions, structs, enums, traits, impls, etc.)
//! - `types` — Type expression and generic parameter parsing
//! - `pattern` — Pattern matching (match arms, if-let, destructuring)
//! - `module` — Module-level parsing, imports, meta blocks, and recovery
//!
//! ## Academic References
//!
//! - **\[CI\]** *Crafting Interpreters* Ch. 6–8 — Recursive descent parsing,
//!   Pratt parsing for expression precedence, and error recovery.
//! - **\[EC\]** *Engineering a Compiler* Ch. 3 — LL(1) parsing theory, FIRST/FOLLOW
//!   sets, and the formal basis for our grammar design.
//! - **\[PLP\]** *Programming Language Pragmatics* Ch. 2.3 — Top-down predictive
//!   parsing and LL grammar construction.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

pub mod error;

mod decl;
mod expr;
mod module;
mod pattern;
mod stmt;
mod types;

#[cfg(test)]
mod tests;

pub use error::{ParseError, ParseOutput, Result};
pub use module::{parse, parse_with_recovery};

use kodo_ast::{Block, NodeIdGen, Span};
use kodo_lexer::{Token, TokenKind};

/// The parser state, holding the token stream and current position.
pub struct Parser {
    /// The token stream produced by the lexer.
    pub(crate) tokens: Vec<Token>,
    /// Current position in the token stream.
    pub(crate) pos: usize,
    /// Generator for unique AST node IDs.
    id_gen: NodeIdGen,
}

impl Parser {
    /// Creates a new parser from a token stream.
    #[must_use]
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            id_gen: NodeIdGen::new(),
        }
    }

    /// Peeks at the current token without consuming it.
    pub(crate) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// Returns the kind of the current token, if any.
    pub(crate) fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    /// Advances the parser and returns the consumed token.
    pub(crate) fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.pos);
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    /// Expects and consumes a specific token kind.
    pub(crate) fn expect(&mut self, expected: &TokenKind) -> Result<Token> {
        match self.peek() {
            Some(token)
                if std::mem::discriminant(&token.kind) == std::mem::discriminant(expected) =>
            {
                let token = token.clone();
                self.pos += 1;
                Ok(token)
            }
            Some(token) => Err(ParseError::UnexpectedToken {
                expected: format!("{expected:?}"),
                found: token.kind.clone(),
                span: token.span,
            }),
            None => Err(ParseError::UnexpectedEof {
                expected: format!("{expected:?}"),
            }),
        }
    }

    /// Returns the span of the most recently consumed token, or a zero-width
    /// span at offset 0 if no tokens have been consumed yet.
    pub(crate) fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map_or(Span::new(0, 0), |t| t.span)
    }

    /// Checks if the current token matches the expected kind without consuming it.
    pub(crate) fn check(&self, expected: &TokenKind) -> bool {
        self.peek()
            .is_some_and(|t| std::mem::discriminant(&t.kind) == std::mem::discriminant(expected))
    }

    /// Parses an identifier and returns its string value.
    pub(crate) fn parse_ident(&mut self) -> Result<String> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::Ident(name),
                ..
            }) => Ok(name.clone()),
            // `test` is a contextual keyword — it can be used as an identifier
            // in most positions (e.g., module name, variable name).
            Some(Token {
                kind: TokenKind::Test,
                ..
            }) => Ok("test".to_string()),
            // `describe`, `setup`, `teardown`, `forall` are contextual testing keywords —
            // they can be used as identifiers outside test contexts.
            Some(Token {
                kind: TokenKind::Describe,
                ..
            }) => Ok("describe".to_string()),
            Some(Token {
                kind: TokenKind::Setup,
                ..
            }) => Ok("setup".to_string()),
            Some(Token {
                kind: TokenKind::Teardown,
                ..
            }) => Ok("teardown".to_string()),
            Some(Token {
                kind: TokenKind::Forall,
                ..
            }) => Ok("forall".to_string()),
            Some(token) => Err(ParseError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: token.kind.clone(),
                span: token.span,
            }),
            None => Err(ParseError::UnexpectedEof {
                expected: "identifier".to_string(),
            }),
        }
    }

    /// Parses a block: `{ statement* }`.
    ///
    /// A block is a sequence of statements enclosed in braces. It is used
    /// for function bodies, if/else branches, and standalone block expressions.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if the block is malformed (missing braces,
    /// invalid statements, or unexpected end of input).
    pub fn parse_block(&mut self) -> Result<Block> {
        let start = self.expect(&TokenKind::LBrace)?.span;

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(ParseError::UnexpectedEof {
                    expected: "}".to_string(),
                });
            }
            stmts.push(self.parse_stmt()?);
        }

        let end = self.expect(&TokenKind::RBrace)?.span;

        Ok(Block {
            span: start.merge(end),
            stmts,
        })
    }

    /// Parses a block with statement-level error recovery, collecting errors
    /// instead of bailing on the first malformed statement.
    ///
    /// When a statement fails to parse, the error is recorded and the parser
    /// synchronizes to the next statement-level token (`let`, `return`, `if`,
    /// `while`, `for`, `}`, etc.) before resuming. This allows reporting
    /// multiple errors within a single function body.
    ///
    /// # Academic Reference
    ///
    /// Multi-level panic-mode recovery: **\[CI\]** *Crafting Interpreters*
    /// Ch. 6.3.3 — recovering at statement boundaries within blocks, not
    /// just at declaration boundaries.
    pub(crate) fn parse_block_with_recovery(
        &mut self,
        errors: &mut Vec<ParseError>,
    ) -> std::result::Result<Block, ParseError> {
        let start = self.expect(&TokenKind::LBrace)?.span;

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.peek().is_none() {
                errors.push(ParseError::UnexpectedEof {
                    expected: "}".to_string(),
                });
                break;
            }
            match self.parse_stmt() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) => {
                    errors.push(e);
                    self.synchronize_to_stmt();
                }
            }
        }

        let end = if self.check(&TokenKind::RBrace) {
            self.expect(&TokenKind::RBrace)
                .map_or(self.prev_span(), |t| t.span)
        } else {
            self.prev_span()
        };

        Ok(Block {
            span: start.merge(end),
            stmts,
        })
    }

    /// Generates the next unique node ID for AST nodes.
    pub(crate) fn next_id(&mut self) -> kodo_ast::NodeId {
        self.id_gen.next_id()
    }
}
