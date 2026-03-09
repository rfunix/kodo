//! # `kodo_parser` — Recursive Descent Parser for the Kōdo Language
//!
//! This crate transforms a token stream from [`kodo_lexer`] into an AST
//! defined in [`kodo_ast`]. It uses a hand-written recursive descent LL(1)
//! parser for maximum control over error recovery and diagnostics.
//!
//! Kōdo's syntax is intentionally simple and unambiguous to make it easy
//! for AI agents to generate correct programs and for humans to audit them.
//!
//! ## Current Status
//!
//! This is a stub implementation that can parse minimal module declarations.
//! Full expression and statement parsing will be added incrementally.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use kodo_ast::{Block, Function, Meta, MetaEntry, Module, NodeIdGen, Param, Span, TypeExpr};
use kodo_lexer::{Token, TokenKind};
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

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, ParseError>;

/// The parser state, holding the token stream and current position.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
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
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// Advances the parser and returns the consumed token.
    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.pos);
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    /// Expects and consumes a specific token kind.
    fn expect(&mut self, expected: &TokenKind) -> Result<Token> {
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

    /// Parses a complete module from the token stream.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if the token stream does not form a valid module.
    pub fn parse_module(&mut self) -> Result<Module> {
        let start = self.peek().map_or(Span::new(0, 0), |t| t.span);

        // Parse: module <name> {
        self.expect(&TokenKind::Module)?;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::LBrace)?;

        // Parse optional meta block
        let meta = if self.check(&TokenKind::Meta) {
            Some(self.parse_meta()?)
        } else {
            None
        };

        // Parse functions
        let mut functions = Vec::new();
        while self.check(&TokenKind::Fn) {
            functions.push(self.parse_function()?);
        }

        let end_token = self.expect(&TokenKind::RBrace)?;
        let span = start.merge(end_token.span);

        Ok(Module {
            id: self.id_gen.next_id(),
            span,
            name,
            meta,
            functions,
        })
    }

    /// Parses a meta block: `meta { key: "value", ... }`
    fn parse_meta(&mut self) -> Result<Meta> {
        let start = self.expect(&TokenKind::Meta)?.span;
        self.expect(&TokenKind::LBrace)?;

        let mut entries = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let key = self.parse_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value_token = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "string literal".to_string(),
            })?;
            let (value, value_span) = match &value_token.kind {
                TokenKind::StringLit(s) => (s.clone(), value_token.span),
                other => {
                    return Err(ParseError::UnexpectedToken {
                        expected: "string literal".to_string(),
                        found: other.clone(),
                        span: value_token.span,
                    });
                }
            };
            let entry_span = Span::new(start.start, value_span.end);
            entries.push(MetaEntry {
                key,
                value,
                span: entry_span,
            });

            // Optional comma
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;

        Ok(Meta {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            entries,
        })
    }

    /// Parses a function definition (stub — parses signature and empty body).
    fn parse_function(&mut self) -> Result<Function> {
        let start = self.expect(&TokenKind::Fn)?.span;
        let name = self.parse_ident()?;

        // Parse parameters
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
            }
            let param_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            let param_name = self.parse_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let param_end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map_or(param_start, |t| t.span);
            params.push(Param {
                name: param_name,
                ty,
                span: param_start.merge(param_end),
            });
        }
        self.expect(&TokenKind::RParen)?;

        // Parse optional return type
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.parse_type()?
        } else {
            TypeExpr::Unit
        };

        // Parse body
        self.expect(&TokenKind::LBrace)?;
        let body_start = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map_or(Span::new(0, 0), |t| t.span);

        // For now, skip everything until matching closing brace
        let mut depth = 1u32;
        while depth > 0 {
            match self.advance() {
                Some(t) if t.kind == TokenKind::LBrace => depth += 1,
                Some(t) if t.kind == TokenKind::RBrace => depth -= 1,
                Some(_) => {}
                None => {
                    return Err(ParseError::UnexpectedEof {
                        expected: "}".to_string(),
                    });
                }
            }
        }

        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map_or(body_start, |t| t.span);

        Ok(Function {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            params,
            return_type,
            requires: Vec::new(),
            ensures: Vec::new(),
            body: Block {
                span: body_start.merge(end),
                stmts: Vec::new(),
            },
        })
    }

    /// Parses a type expression (stub — only named types for now).
    fn parse_type(&mut self) -> Result<TypeExpr> {
        let name = self.parse_ident()?;
        Ok(TypeExpr::Named(name))
    }

    /// Parses an identifier and returns its string value.
    fn parse_ident(&mut self) -> Result<String> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::Ident(name),
                ..
            }) => Ok(name.clone()),
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

    /// Checks if the current token matches the expected kind without consuming it.
    fn check(&self, expected: &TokenKind) -> bool {
        self.peek()
            .is_some_and(|t| std::mem::discriminant(&t.kind) == std::mem::discriminant(expected))
    }
}

/// Parses source code into a [`Module`] AST node.
///
/// This is the main entry point for parsing. It first tokenizes the source,
/// then runs the recursive descent parser.
///
/// # Errors
///
/// Returns a [`ParseError`] if the source code is not valid Kōdo syntax.
pub fn parse(source: &str) -> Result<Module> {
    let tokens = kodo_lexer::tokenize(source)?;
    let mut parser = Parser::new(tokens);
    parser.parse_module()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_module() {
        let source = r#"module hello {
            meta {
                version: "0.1.0",
                author: "Kōdo Team"
            }

            fn main() {
            }
        }"#;

        let module = parse(source);
        assert!(module.is_ok(), "parse failed: {module:?}");
        let module = module.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(module.name, "hello");
        assert!(module.meta.is_some());
        let meta = module
            .meta
            .as_ref()
            .unwrap_or_else(|| panic!("already checked"));
        assert_eq!(meta.entries.len(), 2);
        assert_eq!(meta.entries[0].key, "version");
        assert_eq!(meta.entries[0].value, "0.1.0");
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "main");
    }

    #[test]
    fn parse_function_with_params() {
        let source = r#"module math {
            fn add(a: Int, b: Int) -> Int {
            }
        }"#;

        let module = parse(source);
        assert!(module.is_ok(), "parse failed: {module:?}");
        let module = module.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(module.functions[0].name, "add");
        assert_eq!(module.functions[0].params.len(), 2);
        assert_eq!(module.functions[0].params[0].name, "a");
        assert_eq!(
            module.functions[0].return_type,
            TypeExpr::Named("Int".to_string())
        );
    }

    #[test]
    fn parse_missing_module_keyword_fails() {
        let result = parse("hello { }");
        assert!(result.is_err());
    }
}
