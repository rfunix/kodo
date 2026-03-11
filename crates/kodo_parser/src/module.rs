//! Module-level parsing for the Kōdo parser.
//!
//! This module handles parsing of the top-level `module` construct,
//! including `import` declarations, `meta` blocks, and the dispatch
//! logic for all module-level items (structs, enums, traits, impls,
//! functions, intents, actors, type aliases).
//!
//! It also provides the `parse_with_recovery` function for error-tolerant
//! parsing that collects multiple errors instead of stopping at the first.

use kodo_ast::{ImportDecl, Meta, MetaEntry, Module, Span};
use kodo_lexer::TokenKind;

use crate::error::{ParseError, ParseOutput, Result};
use crate::Parser;

impl Parser {
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

        // Parse import declarations
        let mut imports = Vec::new();
        while self.check(&TokenKind::Import) {
            imports.push(self.parse_import()?);
        }

        // Parse optional meta block
        let meta = if self.check(&TokenKind::Meta) {
            Some(self.parse_meta()?)
        } else {
            None
        };

        // Parse type declarations, trait declarations, impl blocks, intents, and functions
        let mut type_decls = Vec::new();
        let mut enum_decls = Vec::new();
        let mut trait_decls = Vec::new();
        let mut impl_blocks = Vec::new();
        let mut actor_decls = Vec::new();
        let mut type_aliases = Vec::new();
        let mut intent_decls = Vec::new();
        let mut functions = Vec::new();
        while self.check(&TokenKind::Fn)
            || self.check(&TokenKind::At)
            || self.check(&TokenKind::Struct)
            || self.check(&TokenKind::Enum)
            || self.check(&TokenKind::Trait)
            || self.check(&TokenKind::Impl)
            || self.check(&TokenKind::Actor)
            || self.check(&TokenKind::Async)
            || self.check(&TokenKind::Intent)
            || self.check(&TokenKind::Type)
        {
            if self.check(&TokenKind::Type) {
                type_aliases.push(self.parse_type_alias()?);
            } else if self.check(&TokenKind::Struct) {
                type_decls.push(self.parse_struct_decl()?);
            } else if self.check(&TokenKind::Enum) {
                enum_decls.push(self.parse_enum_decl()?);
            } else if self.check(&TokenKind::Trait) {
                trait_decls.push(self.parse_trait_decl()?);
            } else if self.check(&TokenKind::Impl) {
                impl_blocks.push(self.parse_impl_block()?);
            } else if self.check(&TokenKind::Actor) {
                actor_decls.push(self.parse_actor_decl()?);
            } else if self.check(&TokenKind::Intent) {
                intent_decls.push(self.parse_intent()?);
            } else {
                functions.push(self.parse_annotated_function()?);
            }
        }

        let end_token = self.expect(&TokenKind::RBrace)?;
        let span = start.merge(end_token.span);

        Ok(Module {
            id: self.next_id(),
            span,
            name,
            imports,
            meta,
            type_aliases,
            type_decls,
            enum_decls,
            trait_decls,
            impl_blocks,
            actor_decls,
            intent_decls,
            functions,
        })
    }

    /// Parses an import declaration: `import ident(.ident)*`
    fn parse_import(&mut self) -> Result<ImportDecl> {
        let start = self.expect(&TokenKind::Import)?.span;
        let mut path = vec![self.parse_ident()?];
        while self.check(&TokenKind::Dot) {
            self.advance();
            path.push(self.parse_ident()?);
        }
        let span = start.merge(self.prev_span());
        Ok(ImportDecl { path, span })
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
            id: self.next_id(),
            span: start.merge(end),
            entries,
        })
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

/// Parses source code with error recovery, collecting multiple errors.
///
/// Unlike [`parse()`], this function does not stop at the first error.
/// It attempts to synchronize and continue parsing, collecting all errors.
///
/// # Examples
///
/// ```
/// let output = kodo_parser::parse_with_recovery("module m { meta {} fn a() {} }");
/// // output.errors may contain multiple errors
/// // output.module may be Some if recovery was possible
/// ```
#[must_use]
pub fn parse_with_recovery(source: &str) -> ParseOutput {
    let tokens = match kodo_lexer::tokenize(source) {
        Ok(t) => t,
        Err(e) => {
            return ParseOutput {
                module: None,
                errors: vec![ParseError::from(e)],
            };
        }
    };
    let mut parser = Parser::new(tokens);
    match parser.parse_module() {
        Ok(module) => ParseOutput {
            module: Some(module),
            errors: vec![],
        },
        Err(e) => ParseOutput {
            module: None,
            errors: vec![e],
        },
    }
}
