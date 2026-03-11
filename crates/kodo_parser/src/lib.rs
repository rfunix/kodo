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

use kodo_ast::{
    ActorDecl, Annotation, AnnotationArg, BinOp, Block, ClosureParam, EnumDecl, EnumVariant, Expr,
    FieldDef, FieldInit, Function, ImplBlock, ImportDecl, IntentConfigEntry, IntentConfigValue,
    IntentDecl, MatchArm, Meta, MetaEntry, Module, NodeIdGen, Ownership, Param, Pattern, Span,
    Stmt, TraitDecl, TraitMethod, TypeAlias, TypeDecl, TypeExpr, UnaryOp,
};
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

    /// Returns the kind of the current token, if any.
    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
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

    /// Returns the span of the most recently consumed token, or a zero-width
    /// span at offset 0 if no tokens have been consumed yet.
    fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map_or(Span::new(0, 0), |t| t.span)
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
            id: self.id_gen.next_id(),
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
            id: self.id_gen.next_id(),
            span: start.merge(end),
            entries,
        })
    }

    /// Parses annotations followed by a function definition.
    fn parse_annotated_function(&mut self) -> Result<Function> {
        let annotations = self.parse_annotations()?;
        let mut func = self.parse_function()?;
        func.annotations = annotations;
        Ok(func)
    }

    /// Parses zero or more annotations: `@name` or `@name(args...)`.
    fn parse_annotations(&mut self) -> Result<Vec<Annotation>> {
        let mut annotations = Vec::new();
        while self.check(&TokenKind::At) {
            let start = self.advance().map_or(Span::new(0, 0), |t| t.span);
            let name = self.parse_ident()?;
            let (args, end) = if self.check(&TokenKind::LParen) {
                self.advance();
                let args = self.parse_annotation_args()?;
                let end = self.expect(&TokenKind::RParen)?.span;
                (args, end)
            } else {
                (vec![], self.prev_span())
            };
            annotations.push(Annotation {
                name,
                args,
                span: start.merge(end),
            });
        }
        Ok(annotations)
    }

    /// Parses annotation arguments (positional or named), comma-separated.
    fn parse_annotation_args(&mut self) -> Result<Vec<AnnotationArg>> {
        let mut args = Vec::new();
        if self.check(&TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            // Check for named arg: ident ':'
            let is_named = matches!(self.peek_kind(), Some(TokenKind::Ident(_)))
                && self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|t| t.kind == TokenKind::Colon);
            if is_named {
                let name = self.parse_ident()?;
                self.expect(&TokenKind::Colon)?;
                let value = self.parse_expr()?;
                args.push(AnnotationArg::Named(name, value));
            } else {
                let value = self.parse_expr()?;
                args.push(AnnotationArg::Positional(value));
            }
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance();
        }
        Ok(args)
    }

    /// Parses a function definition including signature, contracts, and body.
    /// Handles both `fn name(...)` and `async fn name(...)`.
    fn parse_function(&mut self) -> Result<Function> {
        // Check for optional `async` keyword before `fn`.
        let is_async = if self.check(&TokenKind::Async) {
            self.advance();
            true
        } else {
            false
        };
        let start = self.expect(&TokenKind::Fn)?.span;
        let name = self.parse_ident()?;

        // Parse optional generic parameters: <T, U, ...>
        let generic_params = self.parse_optional_generic_params()?;

        // Parse parameters
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
            }
            params.push(self.parse_param()?);
        }
        self.expect(&TokenKind::RParen)?;

        // Parse optional return type
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.parse_type()?
        } else {
            TypeExpr::Unit
        };

        // Parse contract clauses (requires/ensures) before the body
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        loop {
            if self.check(&TokenKind::Requires) {
                self.advance();
                self.expect(&TokenKind::LBrace)?;
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RBrace)?;
                requires.push(expr);
            } else if self.check(&TokenKind::Ensures) {
                self.advance();
                self.expect(&TokenKind::LBrace)?;
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RBrace)?;
                ensures.push(expr);
            } else {
                break;
            }
        }

        // Parse body block
        let body = self.parse_block()?;

        let end = self.prev_span();

        Ok(Function {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            is_async,
            generic_params,
            annotations: vec![],
            params,
            return_type,
            requires,
            ensures,
            body,
        })
    }

    /// Parses a single function parameter, including optional ownership
    /// qualifier (`own` or `ref`): `[own|ref] name: Type`.
    fn parse_param(&mut self) -> Result<Param> {
        let param_start = self.peek().map_or(Span::new(0, 0), |t| t.span);

        // Check for ownership qualifier
        let ownership = if self.check(&TokenKind::Own) {
            self.advance();
            Ownership::Owned
        } else if self.check(&TokenKind::Ref) {
            self.advance();
            Ownership::Ref
        } else {
            Ownership::Owned
        };

        let param_name = self.parse_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let param_end = self.prev_span();
        Ok(Param {
            name: param_name,
            ty,
            span: param_start.merge(param_end),
            ownership,
        })
    }

    /// Parses a type alias: `type Name = BaseType` or `type Name = BaseType requires { expr }`
    fn parse_type_alias(&mut self) -> Result<TypeAlias> {
        let start = self.expect(&TokenKind::Type)?.span;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::Eq)?;
        let base_type = self.parse_type()?;
        let constraint = if self.check(&TokenKind::Requires) {
            self.advance();
            self.expect(&TokenKind::LBrace)?;
            let expr = self.parse_expr()?;
            self.expect(&TokenKind::RBrace)?;
            Some(expr)
        } else {
            None
        };
        let end = self.prev_span();
        Ok(TypeAlias {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            base_type,
            constraint,
        })
    }

    /// Parses a struct declaration: `struct Name<T> { field: Type, ... }`
    fn parse_struct_decl(&mut self) -> Result<TypeDecl> {
        let start = self.expect(&TokenKind::Struct)?.span;
        let name = self.parse_ident()?;

        // Parse optional generic parameters: <T, U, ...>
        let generic_params = self.parse_optional_generic_params()?;

        self.expect(&TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let field_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            let field_name = self.parse_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let field_end = self.prev_span();
            fields.push(FieldDef {
                name: field_name,
                ty,
                span: field_start.merge(field_end),
            });

            // Optional comma
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;

        Ok(TypeDecl {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            generic_params,
            fields,
        })
    }

    /// Parses an enum declaration: `enum Name<T> { Variant1, Variant2(Type, ...) }`
    fn parse_enum_decl(&mut self) -> Result<EnumDecl> {
        let start = self.expect(&TokenKind::Enum)?.span;
        let name = self.parse_ident()?;

        // Parse optional generic parameters: <T, U, ...>
        let generic_params = self.parse_optional_generic_params()?;

        self.expect(&TokenKind::LBrace)?;

        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let var_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            let var_name = self.parse_ident()?;

            // Optional positional fields: Variant(Type, Type)
            let fields = if self.check(&TokenKind::LParen) {
                self.advance();
                let mut field_types = Vec::new();
                while !self.check(&TokenKind::RParen) {
                    if !field_types.is_empty() {
                        self.expect(&TokenKind::Comma)?;
                    }
                    field_types.push(self.parse_type()?);
                }
                self.expect(&TokenKind::RParen)?;
                field_types
            } else {
                vec![]
            };

            let var_end = self.prev_span();
            variants.push(EnumVariant {
                name: var_name,
                fields,
                span: var_start.merge(var_end),
            });

            // Optional comma
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;

        Ok(EnumDecl {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            generic_params,
            variants,
        })
    }

    /// Parses a trait declaration: `trait Name { fn method(self) -> RetType ... }`
    fn parse_trait_decl(&mut self) -> Result<TraitDecl> {
        let start = self.expect(&TokenKind::Trait)?.span;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::LBrace)?;

        let mut methods = Vec::new();
        while self.check(&TokenKind::Fn) {
            let method_start = self.expect(&TokenKind::Fn)?.span;
            let method_name = self.parse_ident()?;
            self.expect(&TokenKind::LParen)?;

            let mut params = Vec::new();
            let mut has_self = false;
            while !self.check(&TokenKind::RParen) {
                if !params.is_empty() {
                    self.expect(&TokenKind::Comma)?;
                }
                let param_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
                // Check for `self` keyword
                if self.check(&TokenKind::SelfValue) {
                    self.advance();
                    has_self = true;
                    let param_end = self.prev_span();
                    params.push(Param {
                        name: "self".to_string(),
                        ty: TypeExpr::Named("Self".to_string()),
                        span: param_start.merge(param_end),
                        ownership: Ownership::Owned,
                    });
                } else {
                    let param_name = self.parse_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    let ty = self.parse_type()?;
                    let param_end = self.prev_span();
                    params.push(Param {
                        name: param_name,
                        ty,
                        span: param_start.merge(param_end),
                        ownership: Ownership::Owned,
                    });
                }
            }
            self.expect(&TokenKind::RParen)?;

            // Parse optional return type
            let return_type = if self.check(&TokenKind::Arrow) {
                self.advance();
                self.parse_type()?
            } else {
                TypeExpr::Unit
            };

            let method_end = self.prev_span();
            methods.push(TraitMethod {
                name: method_name,
                params,
                return_type,
                has_self,
                span: method_start.merge(method_end),
            });
        }

        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(TraitDecl {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            methods,
        })
    }

    /// Parses an impl block: `impl TraitName for TypeName { fn method(self) -> RetType { body } }`
    fn parse_impl_block(&mut self) -> Result<ImplBlock> {
        let start = self.expect(&TokenKind::Impl)?.span;
        let trait_name = self.parse_ident()?;

        // Expect the `for` keyword (reused from for-loop token)
        self.expect(&TokenKind::For)?;
        let type_name = self.parse_ident()?;
        self.expect(&TokenKind::LBrace)?;

        let mut methods = Vec::new();
        while self.check(&TokenKind::Fn) {
            let mut func = self.parse_impl_method()?;
            // Resolve `self` param type to the implementing type
            for param in &mut func.params {
                if param.name == "self" {
                    param.ty = TypeExpr::Named(type_name.clone());
                }
            }
            methods.push(func);
        }

        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(ImplBlock {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            trait_name,
            type_name,
            methods,
        })
    }

    /// Parses a method inside an impl block. Similar to `parse_function` but
    /// handles `self` as first parameter without requiring a type annotation.
    fn parse_impl_method(&mut self) -> Result<Function> {
        let start = self.expect(&TokenKind::Fn)?.span;
        let name = self.parse_ident()?;

        // Parse optional generic parameters
        let generic_params = self.parse_optional_generic_params()?;

        // Parse parameters (first may be `self`)
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
            }
            let param_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            // Check for `self` keyword
            if self.check(&TokenKind::SelfValue) {
                self.advance();
                let param_end = self.prev_span();
                params.push(Param {
                    name: "self".to_string(),
                    ty: TypeExpr::Named("Self".to_string()), // resolved later
                    span: param_start.merge(param_end),
                    ownership: Ownership::Owned,
                });
            } else {
                let param_name = self.parse_ident()?;
                self.expect(&TokenKind::Colon)?;
                let ty = self.parse_type()?;
                let param_end = self.prev_span();
                params.push(Param {
                    name: param_name,
                    ty,
                    span: param_start.merge(param_end),
                    ownership: Ownership::Owned,
                });
            }
        }
        self.expect(&TokenKind::RParen)?;

        // Parse optional return type
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.parse_type()?
        } else {
            TypeExpr::Unit
        };

        // Parse contract clauses
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        loop {
            if self.check(&TokenKind::Requires) {
                self.advance();
                self.expect(&TokenKind::LBrace)?;
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RBrace)?;
                requires.push(expr);
            } else if self.check(&TokenKind::Ensures) {
                self.advance();
                self.expect(&TokenKind::LBrace)?;
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RBrace)?;
                ensures.push(expr);
            } else {
                break;
            }
        }

        // Parse body block
        let body = self.parse_block()?;
        let end = self.prev_span();

        Ok(Function {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            is_async: false,
            generic_params,
            annotations: vec![],
            params,
            return_type,
            requires,
            ensures,
            body,
        })
    }

    /// Parses a struct literal after the name has been consumed:
    /// `{ field: expr, ... }`
    fn parse_struct_literal(&mut self, name: String, start_span: Span) -> Result<Expr> {
        self.expect(&TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let field_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            let field_name = self.parse_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            let field_end = Self::expr_span(&value);
            fields.push(FieldInit {
                name: field_name,
                value,
                span: field_start.merge(field_end),
            });

            // Optional comma
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;

        Ok(Expr::StructLit {
            name,
            fields,
            span: start_span.merge(end),
        })
    }

    /// Parses a match expression: `match expr { pattern => expr, ... }`
    fn parse_match_expr(&mut self) -> Result<Expr> {
        let start = self.expect(&TokenKind::Match)?.span;
        let matched_expr = self.parse_expr()?;
        self.expect(&TokenKind::LBrace)?;

        let mut arms = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let arm_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            let pattern = self.parse_pattern()?;
            self.expect(&TokenKind::FatArrow)?;
            let body = self.parse_expr()?;
            let arm_end = Self::expr_span(&body);
            arms.push(MatchArm {
                pattern,
                body,
                span: arm_start.merge(arm_end),
            });

            // Optional comma
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;

        Ok(Expr::Match {
            expr: Box::new(matched_expr),
            arms,
            span: start.merge(end),
        })
    }

    /// Parses a pattern in a match arm.
    fn parse_pattern(&mut self) -> Result<Pattern> {
        // Wildcard: `_`
        if let Some(TokenKind::Ident(name)) = self.peek_kind().cloned() {
            if name == "_" {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                return Ok(Pattern::Wildcard(span));
            }
        }

        // Literal patterns
        if let Some(
            TokenKind::IntLit(_)
            | TokenKind::FloatLit(_)
            | TokenKind::StringLit(_)
            | TokenKind::True
            | TokenKind::False,
        ) = self.peek_kind().cloned()
        {
            let expr = self.parse_primary_expr()?;
            return Ok(Pattern::Literal(expr));
        }

        // Variant pattern: Name::Variant(bindings) or just Name
        let start_span = self.peek().map_or(Span::new(0, 0), |t| t.span);
        let first_name = self.parse_ident()?;

        if self.check(&TokenKind::ColonColon) {
            self.advance();
            let variant = self.parse_ident()?;
            let mut bindings = Vec::new();
            if self.check(&TokenKind::LParen) {
                self.advance();
                while !self.check(&TokenKind::RParen) {
                    if !bindings.is_empty() {
                        self.expect(&TokenKind::Comma)?;
                    }
                    bindings.push(self.parse_ident()?);
                }
                self.expect(&TokenKind::RParen)?;
            }
            let end = self.prev_span();
            Ok(Pattern::Variant {
                enum_name: Some(first_name),
                variant,
                bindings,
                span: start_span.merge(end),
            })
        } else {
            // Could be a unit variant without enum prefix
            let end = self.prev_span();
            Ok(Pattern::Variant {
                enum_name: None,
                variant: first_name,
                bindings: vec![],
                span: start_span.merge(end),
            })
        }
    }

    /// Checks if the current position looks like a struct literal start:
    /// `{ Ident : ...` (as opposed to a block `{ stmt; ... }`)
    fn is_struct_literal_start(&self) -> bool {
        // Current token should be `{`, look at pos+1 for Ident and pos+2 for `:`
        if !self.check(&TokenKind::LBrace) {
            return false;
        }
        let has_ident = self
            .tokens
            .get(self.pos + 1)
            .is_some_and(|t| matches!(&t.kind, TokenKind::Ident(_)));
        let has_colon = self
            .tokens
            .get(self.pos + 2)
            .is_some_and(|t| t.kind == TokenKind::Colon);
        has_ident && has_colon
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

    /// Parses a single statement.
    ///
    /// Statements are the building blocks of function bodies. The parser
    /// distinguishes `let` bindings, `return` statements, and expression
    /// statements by looking at the leading keyword.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if the statement is malformed or contains
    /// an invalid expression.
    pub fn parse_stmt(&mut self) -> Result<Stmt> {
        match self.peek_kind() {
            Some(TokenKind::Let) => self.parse_let_stmt(),
            Some(TokenKind::Return) => self.parse_return_stmt(),
            Some(TokenKind::While) => self.parse_while_stmt(),
            Some(TokenKind::For) => self.parse_for_stmt(),
            Some(TokenKind::If) => {
                // Look ahead: `if let` is a statement, regular `if` is an expression.
                if self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|t| t.kind == TokenKind::Let)
                {
                    self.parse_if_let_stmt()
                } else {
                    self.parse_expr_or_assign_stmt()
                }
            }
            Some(TokenKind::Spawn) => self.parse_spawn_stmt(),
            Some(TokenKind::Parallel) => self.parse_parallel_stmt(),
            _ => self.parse_expr_or_assign_stmt(),
        }
    }

    /// Parses a let binding: `let [mut] name [: type] = expr`.
    fn parse_let_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect(&TokenKind::Let)?.span;

        // Optional `mut` keyword
        let mutable = if self.check(&TokenKind::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let name = self.parse_ident()?;

        // Optional type annotation
        let ty = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let end = Self::expr_span(&value);

        Ok(Stmt::Let {
            span: start.merge(end),
            mutable,
            name,
            ty,
            value,
        })
    }

    /// Parses a return statement: `return [expr]`.
    fn parse_return_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect(&TokenKind::Return)?.span;

        // Check if there's a value expression following `return`.
        // If the next token could start an expression, parse it.
        let value = if self.is_at_expr_start() {
            let expr = self.parse_expr()?;
            Some(expr)
        } else {
            None
        };

        let end = value.as_ref().map_or(start, Self::expr_span);

        Ok(Stmt::Return {
            span: start.merge(end),
            value,
        })
    }

    /// Parses an expression or assignment statement.
    ///
    /// If the expression is an identifier followed by `=`, it is treated as
    /// an assignment to an existing variable. Otherwise it is an expression
    /// statement.
    fn parse_expr_or_assign_stmt(&mut self) -> Result<Stmt> {
        // Look ahead: if it's `ident =` (but not `ident ==`), it's an assignment.
        if let Some(TokenKind::Ident(_)) = self.peek_kind() {
            if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::Eq) {
                return self.parse_assign_stmt();
            }
        }
        let expr = self.parse_expr()?;
        Ok(Stmt::Expr(expr))
    }

    /// Parses an assignment: `name = expr`.
    fn parse_assign_stmt(&mut self) -> Result<Stmt> {
        let start = self.peek().map_or(Span::new(0, 0), |t| t.span);
        let name = self.parse_ident()?;
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let end = Self::expr_span(&value);
        Ok(Stmt::Assign {
            span: start.merge(end),
            name,
            value,
        })
    }

    /// Parses a while loop: `while <condition> { <body> }`.
    fn parse_while_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect(&TokenKind::While)?.span;
        let condition = self.parse_expr()?;
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Stmt::While {
            span: start.merge(end),
            condition,
            body,
        })
    }

    /// Parses a for loop: `for <ident> in <expr>..<expr> { <body> }` or
    /// `for <ident> in <expr>..=<expr> { <body> }`.
    fn parse_for_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect(&TokenKind::For)?.span;
        let name = self.parse_ident()?;

        // Expect the contextual keyword "in".
        match self.peek() {
            Some(token) if matches!(&token.kind, TokenKind::Ident(s) if s == "in") => {
                self.advance();
            }
            Some(token) => {
                let found = token.kind.clone();
                let span = token.span;
                return Err(ParseError::UnexpectedToken {
                    expected: "in".to_string(),
                    found,
                    span,
                });
            }
            None => {
                return Err(ParseError::UnexpectedEof {
                    expected: "in".to_string(),
                });
            }
        }

        // Parse the range expression (start..end or start..=end).
        let range_expr = self.parse_expr()?;
        match range_expr {
            Expr::Range {
                start: range_start,
                end: range_end,
                inclusive,
                ..
            } => {
                let body = self.parse_block()?;
                let end_span = body.span;
                Ok(Stmt::For {
                    span: start.merge(end_span),
                    name,
                    start: *range_start,
                    end: *range_end,
                    inclusive,
                    body,
                })
            }
            other => {
                let span = Self::expr_span(&other);
                Err(ParseError::UnexpectedToken {
                    expected: "range expression (e.g. 0..10 or 0..=10)".to_string(),
                    found: TokenKind::Ident("expression".to_string()),
                    span,
                })
            }
        }
    }

    /// Parses an `if let` statement: `if let Pattern = expr { body } [else { else_body }]`.
    fn parse_if_let_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect(&TokenKind::If)?.span;
        self.expect(&TokenKind::Let)?;
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let body = self.parse_block()?;

        let else_body = if self.check(&TokenKind::Else) {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };

        let end = else_body.as_ref().map_or(body.span, |b| b.span);

        Ok(Stmt::IfLet {
            span: start.merge(end),
            pattern,
            value,
            body,
            else_body,
        })
    }

    /// Parses a spawn statement: `spawn { body }`.
    fn parse_spawn_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect(&TokenKind::Spawn)?.span;
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Stmt::Spawn {
            span: start.merge(end),
            body,
        })
    }

    /// Parses a parallel block: `parallel { spawn { ... } spawn { ... } }`.
    fn parse_parallel_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect(&TokenKind::Parallel)?.span;
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            stmts.push(self.parse_stmt()?);
        }
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Stmt::Parallel {
            span: start.merge(end),
            body: stmts,
        })
    }

    /// Parses an intent declaration: `intent name { key: value, ... }`.
    ///
    /// Config values can be string literals, integer literals, float literals,
    /// boolean literals, identifiers (function references), or lists.
    fn parse_intent(&mut self) -> Result<IntentDecl> {
        let start = self.expect(&TokenKind::Intent)?.span;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::LBrace)?;

        let mut config = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let entry_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            let key = self.parse_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_intent_config_value()?;
            let entry_end = self.prev_span();
            config.push(IntentConfigEntry {
                key,
                value,
                span: entry_start.merge(entry_end),
            });
            // Optional comma
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(IntentDecl {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            config,
        })
    }

    /// Parses a single intent configuration value.
    fn parse_intent_config_value(&mut self) -> Result<IntentConfigValue> {
        match self.peek_kind().cloned() {
            Some(TokenKind::StringLit(s)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(IntentConfigValue::StringLit(s, span))
            }
            Some(TokenKind::FloatLit(f)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(IntentConfigValue::FloatLit(f, span))
            }
            Some(TokenKind::IntLit(n)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(IntentConfigValue::IntLit(n, span))
            }
            Some(TokenKind::True) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(IntentConfigValue::BoolLit(true, span))
            }
            Some(TokenKind::False) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(IntentConfigValue::BoolLit(false, span))
            }
            Some(TokenKind::LBracket) => {
                let start = self.advance().map_or(Span::new(0, 0), |t| t.span);
                let mut items = Vec::new();
                while !self.check(&TokenKind::RBracket) {
                    if !items.is_empty() {
                        self.expect(&TokenKind::Comma)?;
                    }
                    items.push(self.parse_intent_config_value()?);
                }
                let end = self.expect(&TokenKind::RBracket)?.span;
                Ok(IntentConfigValue::List(items, start.merge(end)))
            }
            Some(TokenKind::Ident(name)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(IntentConfigValue::FnRef(name, span))
            }
            Some(other) => {
                let span = self.peek().map_or(Span::new(0, 0), |t| t.span);
                Err(ParseError::UnexpectedToken {
                    expected: "intent config value (string, int, float, bool, identifier, or list)"
                        .to_string(),
                    found: other,
                    span,
                })
            }
            None => Err(ParseError::UnexpectedEof {
                expected: "intent config value".to_string(),
            }),
        }
    }

    /// Parses an actor declaration: `actor Name { fields... fn handler(self) { ... } ... }`
    fn parse_actor_decl(&mut self) -> Result<ActorDecl> {
        let start = self.expect(&TokenKind::Actor)?.span;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::LBrace)?;

        // Parse fields first (like struct fields), then handler functions.
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Fn) {
            let field_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            let field_name = self.parse_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let field_end = self.prev_span();
            fields.push(FieldDef {
                name: field_name,
                ty,
                span: field_start.merge(field_end),
            });
            // Optional comma
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        // Parse handler functions.
        let mut handlers = Vec::new();
        while self.check(&TokenKind::Fn) {
            let mut func = self.parse_impl_method()?;
            // Resolve `self` param type to the actor type name.
            for param in &mut func.params {
                if param.name == "self" {
                    param.ty = TypeExpr::Named(name.clone());
                }
            }
            handlers.push(func);
        }

        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(ActorDecl {
            id: self.id_gen.next_id(),
            span: start.merge(end),
            name,
            fields,
            handlers,
        })
    }

    /// Returns `true` if the current token could start an expression.
    fn is_at_expr_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(
                TokenKind::IntLit(_)
                    | TokenKind::FloatLit(_)
                    | TokenKind::StringLit(_)
                    | TokenKind::True
                    | TokenKind::False
                    | TokenKind::Ident(_)
                    | TokenKind::SelfValue
                    | TokenKind::If
                    | TokenKind::LBrace
                    | TokenKind::LParen
                    | TokenKind::Bang
                    | TokenKind::Minus
                    | TokenKind::Pipe
                    | TokenKind::PipePipe
            )
        )
    }

    // ===== Expression Parsing =====
    //
    // Each precedence level has its own method, implementing the grammar
    // from `docs/grammar.ebnf`. Left-associative binary operators are
    // handled with a while loop at each level.

    /// Parses an expression starting from the lowest precedence level.
    ///
    /// This is the top-level expression entry point. It dispatches to
    /// `parse_or_expr`, which is the lowest-precedence binary operator.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if the token stream does not form a valid
    /// expression.
    pub fn parse_expr(&mut self) -> Result<Expr> {
        let left = self.parse_coalesce_expr()?;

        // Check for range operators `..` or `..=` at the lowest precedence.
        match self.peek_kind() {
            Some(TokenKind::DotDotEq) => {
                self.advance();
                let right = self.parse_coalesce_expr()?;
                let span = Self::expr_span(&left).merge(Self::expr_span(&right));
                Ok(Expr::Range {
                    start: Box::new(left),
                    end: Box::new(right),
                    inclusive: true,
                    span,
                })
            }
            Some(TokenKind::DotDot) => {
                self.advance();
                let right = self.parse_coalesce_expr()?;
                let span = Self::expr_span(&left).merge(Self::expr_span(&right));
                Ok(Expr::Range {
                    start: Box::new(left),
                    end: Box::new(right),
                    inclusive: false,
                    span,
                })
            }
            _ => Ok(left),
        }
    }

    /// Parses a null coalescing expression: `or_expr ( "??" or_expr )*`.
    ///
    /// `a ?? b` evaluates to `a` if it is `Some`, otherwise `b`.
    fn parse_coalesce_expr(&mut self) -> Result<Expr> {
        let mut left = self.parse_or_expr()?;

        while self.check(&TokenKind::QuestionQuestion) {
            self.advance();
            let right = self.parse_or_expr()?;
            let span = Self::expr_span(&left).merge(Self::expr_span(&right));
            left = Expr::NullCoalesce {
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    /// Parses a logical-or expression: `and_expr ( "||" and_expr )*`.
    fn parse_or_expr(&mut self) -> Result<Expr> {
        let mut left = self.parse_and_expr()?;

        while self.check(&TokenKind::PipePipe) {
            self.advance();
            let right = self.parse_and_expr()?;
            let span = Self::expr_span(&left).merge(Self::expr_span(&right));
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    /// Parses a logical-and expression: `equality_expr ( "&&" equality_expr )*`.
    fn parse_and_expr(&mut self) -> Result<Expr> {
        let mut left = self.parse_equality_expr()?;

        while self.check(&TokenKind::AmpAmp) {
            self.advance();
            let right = self.parse_equality_expr()?;
            let span = Self::expr_span(&left).merge(Self::expr_span(&right));
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    /// Parses an equality expression: `comparison_expr ( ("==" | "!=") comparison_expr )*`.
    fn parse_equality_expr(&mut self) -> Result<Expr> {
        let mut left = self.parse_comparison_expr()?;

        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::EqEq) => BinOp::Eq,
                Some(TokenKind::BangEq) => BinOp::Ne,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison_expr()?;
            let span = Self::expr_span(&left).merge(Self::expr_span(&right));
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    /// Parses a comparison expression: `additive_expr ( ("<" | ">" | "<=" | ">=") additive_expr )*`.
    fn parse_comparison_expr(&mut self) -> Result<Expr> {
        let mut left = self.parse_additive_expr()?;

        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::Lt) => BinOp::Lt,
                Some(TokenKind::Gt) => BinOp::Gt,
                Some(TokenKind::LtEq) => BinOp::Le,
                Some(TokenKind::GtEq) => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive_expr()?;
            let span = Self::expr_span(&left).merge(Self::expr_span(&right));
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    /// Parses an additive expression: `multiplicative_expr ( ("+" | "-") multiplicative_expr )*`.
    fn parse_additive_expr(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplicative_expr()?;

        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::Plus) => BinOp::Add,
                Some(TokenKind::Minus) => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            let span = Self::expr_span(&left).merge(Self::expr_span(&right));
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    /// Parses a multiplicative expression: `unary_expr ( ("*" | "/" | "%") unary_expr )*`.
    fn parse_multiplicative_expr(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary_expr()?;

        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::Star) => BinOp::Mul,
                Some(TokenKind::Slash) => BinOp::Div,
                Some(TokenKind::Percent) => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            let span = Self::expr_span(&left).merge(Self::expr_span(&right));
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(left)
    }

    /// Parses a unary expression: `("!" | "-") unary_expr | postfix_expr`.
    fn parse_unary_expr(&mut self) -> Result<Expr> {
        match self.peek_kind() {
            Some(TokenKind::Bang) => {
                let start = self.advance().map_or(Span::new(0, 0), |t| t.span);
                let operand = self.parse_unary_expr()?;
                let span = start.merge(Self::expr_span(&operand));
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span,
                })
            }
            Some(TokenKind::Minus) => {
                let start = self.advance().map_or(Span::new(0, 0), |t| t.span);
                let operand = self.parse_unary_expr()?;
                let span = start.merge(Self::expr_span(&operand));
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                    span,
                })
            }
            _ => self.parse_postfix_expr(),
        }
    }

    /// Parses a postfix expression: `primary_expr ( call_suffix | field_suffix )*`.
    ///
    /// Call suffix: `(arg_list?)`, field suffix: `.IDENT`.
    fn parse_postfix_expr(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            if self.check(&TokenKind::LParen) {
                // Function call: expr(args...)
                self.advance();
                let mut args = Vec::new();
                if !self.check(&TokenKind::RParen) {
                    args.push(self.parse_expr()?);
                    while self.check(&TokenKind::Comma) {
                        self.advance();
                        args.push(self.parse_expr()?);
                    }
                }
                let end = self.expect(&TokenKind::RParen)?.span;
                let span = Self::expr_span(&expr).merge(end);
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
            } else if self.check(&TokenKind::QuestionDot) {
                // Optional chaining: expr?.field
                self.advance();
                let field = self.parse_ident()?;
                let end = self.prev_span();
                let span = Self::expr_span(&expr).merge(end);
                expr = Expr::OptionalChain {
                    object: Box::new(expr),
                    field,
                    span,
                };
            } else if self.check(&TokenKind::QuestionMark) {
                // Try operator: expr?
                let end = self.advance().map_or(Span::new(0, 0), |t| t.span);
                let span = Self::expr_span(&expr).merge(end);
                expr = Expr::Try {
                    operand: Box::new(expr),
                    span,
                };
            } else if self.check(&TokenKind::Dot) {
                self.advance();
                // Check for `.await` (await is a keyword, not an ident).
                if self.check(&TokenKind::Await) {
                    let end = self.advance().map_or(Span::new(0, 0), |t| t.span);
                    let span = Self::expr_span(&expr).merge(end);
                    expr = Expr::Await {
                        operand: Box::new(expr),
                        span,
                    };
                } else {
                    // Field access: expr.field
                    let field = self.parse_ident()?;
                    let end = self.prev_span();
                    let span = Self::expr_span(&expr).merge(end);
                    expr = Expr::FieldAccess {
                        object: Box::new(expr),
                        field,
                        span,
                    };
                }
            } else if self.check(&TokenKind::Is) {
                // Type test: expr is VariantName
                self.advance();
                let type_name = self.parse_ident()?;
                let end = self.prev_span();
                let span = Self::expr_span(&expr).merge(end);
                expr = Expr::Is {
                    operand: Box::new(expr),
                    type_name,
                    span,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    /// Parses a primary expression (the highest precedence level).
    ///
    /// Primary expressions include literals, identifiers, `if` expressions,
    /// block expressions, and parenthesized expressions.
    fn parse_primary_expr(&mut self) -> Result<Expr> {
        match self.peek_kind().cloned() {
            Some(TokenKind::IntLit(n)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(Expr::IntLit(n, span))
            }
            Some(TokenKind::FloatLit(f)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(Expr::FloatLit(f, span))
            }
            Some(TokenKind::StringLit(s)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(Expr::StringLit(s, span))
            }
            Some(TokenKind::True) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(Expr::BoolLit(true, span))
            }
            Some(TokenKind::False) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(Expr::BoolLit(false, span))
            }
            Some(TokenKind::SelfValue) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                Ok(Expr::Ident("self".to_string(), span))
            }
            Some(TokenKind::Ident(name)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                // Check for enum variant: Name::Variant(args)
                if self.check(&TokenKind::ColonColon) {
                    self.advance();
                    let variant = self.parse_ident()?;
                    let mut args = Vec::new();
                    if self.check(&TokenKind::LParen) {
                        self.advance();
                        while !self.check(&TokenKind::RParen) {
                            if !args.is_empty() {
                                self.expect(&TokenKind::Comma)?;
                            }
                            args.push(self.parse_expr()?);
                        }
                        self.expect(&TokenKind::RParen)?;
                    }
                    let end = self.prev_span();
                    return Ok(Expr::EnumVariantExpr {
                        enum_name: name,
                        variant,
                        args,
                        span: span.merge(end),
                    });
                }
                // Check for struct literal: Name { field: expr, ... }
                if self.is_struct_literal_start() {
                    return self.parse_struct_literal(name, span);
                }
                Ok(Expr::Ident(name, span))
            }
            Some(TokenKind::If) => self.parse_if_expr(),
            Some(TokenKind::Match) => self.parse_match_expr(),
            Some(TokenKind::LBrace) => {
                let block = self.parse_block()?;
                Ok(Expr::Block(block))
            }
            Some(TokenKind::LParen) => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }
            Some(TokenKind::Pipe) => self.parse_closure(),
            Some(TokenKind::PipePipe) => self.parse_empty_closure(),
            Some(other) => {
                let span = self.peek().map_or(Span::new(0, 0), |t| t.span);
                Err(ParseError::UnexpectedToken {
                    expected: "expression".to_string(),
                    found: other,
                    span,
                })
            }
            None => Err(ParseError::UnexpectedEof {
                expected: "expression".to_string(),
            }),
        }
    }

    /// Parses an if expression: `if expr block (else (if_expr | block))?`.
    fn parse_if_expr(&mut self) -> Result<Expr> {
        let start = self.expect(&TokenKind::If)?.span;
        let condition = self.parse_expr()?;
        let then_branch = self.parse_block()?;

        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                // else if — wrap in a block with a single if-expr statement
                let if_expr = self.parse_if_expr()?;
                let span = Self::expr_span(&if_expr);
                Some(Block {
                    span,
                    stmts: vec![Stmt::Expr(if_expr)],
                })
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        let end = else_branch.as_ref().map_or(then_branch.span, |b| b.span);

        Ok(Expr::If {
            condition: Box::new(condition),
            then_branch,
            else_branch,
            span: start.merge(end),
        })
    }

    /// Parses a closure expression: `|x: Int, y: Int| expr` or
    /// `|x: Int| -> Int { body }`.
    ///
    /// Called when the current token is `Pipe` (`|`).
    fn parse_closure(&mut self) -> Result<Expr> {
        let start = self.expect(&TokenKind::Pipe)?.span;

        // Parse closure parameters
        let mut params = Vec::new();
        if !self.check(&TokenKind::Pipe) {
            loop {
                let param_span = self.peek().map_or(Span::new(0, 0), |t| t.span);
                let name = self.parse_ident()?;
                let ty = if self.check(&TokenKind::Colon) {
                    self.advance();
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let end_span = self.prev_span();
                params.push(ClosureParam {
                    name,
                    ty,
                    span: param_span.merge(end_span),
                });
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance(); // consume comma
            }
        }
        self.expect(&TokenKind::Pipe)?; // closing |

        self.parse_closure_body(start, params)
    }

    /// Parses an empty closure expression: `|| expr` or `|| -> Int { body }`.
    ///
    /// Called when the current token is `PipePipe` (`||`), which represents
    /// an empty parameter list closure. In primary position, `||` always
    /// means an empty closure; as a binary operator it is handled in
    /// `parse_or_expr` which never reaches here.
    fn parse_empty_closure(&mut self) -> Result<Expr> {
        let start = self.advance().map_or(Span::new(0, 0), |t| t.span);
        self.parse_closure_body(start, vec![])
    }

    /// Parses the remainder of a closure after the parameters have been
    /// consumed: an optional return type annotation and the body.
    fn parse_closure_body(&mut self, start: Span, params: Vec<ClosureParam>) -> Result<Expr> {
        // Optional return type
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        // Body: block or single expression
        let body = if self.check(&TokenKind::LBrace) {
            Expr::Block(self.parse_block()?)
        } else {
            self.parse_expr()?
        };

        let span = start.merge(Self::expr_span(&body));
        Ok(Expr::Closure {
            params,
            return_type,
            body: Box::new(body),
            span,
        })
    }

    /// Parses optional generic type parameters: `<T, U, ...>`.
    ///
    /// Returns an empty vec if no `<` follows the name.
    fn parse_optional_generic_params(&mut self) -> Result<Vec<String>> {
        if !self.check(&TokenKind::Lt) {
            return Ok(vec![]);
        }
        self.advance(); // consume '<'
        let mut params = vec![self.parse_ident()?];
        while self.check(&TokenKind::Comma) {
            self.advance();
            params.push(self.parse_ident()?);
        }
        self.expect(&TokenKind::Gt)?;
        Ok(params)
    }

    /// Parses a type expression: named types, generic types like `Option<Int>`,
    /// function types like `(Int, Int) -> Int`, and optional shorthand `T?`
    /// (equivalent to `Option<T>`).
    fn parse_type(&mut self) -> Result<TypeExpr> {
        // Check for function type: `(Type, ...) -> RetType`
        if self.check(&TokenKind::LParen) {
            return self.parse_function_type();
        }
        let name = self.parse_ident()?;
        // Check for generic type arguments: Name<Type, Type, ...>
        let base = if self.check(&TokenKind::Lt) {
            self.advance(); // consume '<'
            let mut args = vec![self.parse_type()?];
            while self.check(&TokenKind::Comma) {
                self.advance();
                args.push(self.parse_type()?);
            }
            self.expect(&TokenKind::Gt)?;
            TypeExpr::Generic(name, args)
        } else {
            TypeExpr::Named(name)
        };
        // Check for optional shorthand: `T?` becomes `Option<T>`
        if self.check(&TokenKind::QuestionMark) {
            self.advance();
            return Ok(TypeExpr::Optional(Box::new(base)));
        }
        Ok(base)
    }

    /// Parses a function type: `(Type, ...) -> RetType`.
    fn parse_function_type(&mut self) -> Result<TypeExpr> {
        self.expect(&TokenKind::LParen)?;
        let mut param_types = Vec::new();
        if !self.check(&TokenKind::RParen) {
            param_types.push(self.parse_type()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                param_types.push(self.parse_type()?);
            }
        }
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::Arrow)?;
        let ret_type = self.parse_type()?;
        Ok(TypeExpr::Function(param_types, Box::new(ret_type)))
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

    /// Returns the span of an expression.
    fn expr_span(expr: &Expr) -> Span {
        match expr {
            Expr::IntLit(_, span)
            | Expr::FloatLit(_, span)
            | Expr::StringLit(_, span)
            | Expr::BoolLit(_, span)
            | Expr::Ident(_, span)
            | Expr::BinaryOp { span, .. }
            | Expr::UnaryOp { span, .. }
            | Expr::Call { span, .. }
            | Expr::If { span, .. }
            | Expr::FieldAccess { span, .. }
            | Expr::StructLit { span, .. }
            | Expr::EnumVariantExpr { span, .. }
            | Expr::Match { span, .. }
            | Expr::Try { span, .. }
            | Expr::OptionalChain { span, .. }
            | Expr::NullCoalesce { span, .. }
            | Expr::Range { span, .. }
            | Expr::Closure { span, .. }
            | Expr::Is { span, .. }
            | Expr::Await { span, .. } => *span,
            Expr::Block(block) => block.span,
        }
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

    #[test]
    fn parse_let_binding_with_type() {
        let source = r#"module test {
            fn main() {
                let x: Int = 42
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Let {
                name,
                ty,
                mutable,
                value,
                ..
            } => {
                assert_eq!(name, "x");
                assert!(!mutable);
                assert_eq!(ty.as_ref(), Some(&TypeExpr::Named("Int".to_string())));
                assert!(matches!(value, Expr::IntLit(42, _)));
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_let_binding_mutable() {
        let source = r#"module test {
            fn main() {
                let mut y: Int = 10
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Let { name, mutable, .. } => {
                assert_eq!(name, "y");
                assert!(mutable);
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_let_binding_without_type() {
        let source = r#"module test {
            fn main() {
                let z = 99
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Let { name, ty, .. } => {
                assert_eq!(name, "z");
                assert!(ty.is_none());
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_return_with_value() {
        let source = r#"module test {
            fn answer() -> Int {
                return 42
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Return { value, .. } => {
                assert!(matches!(value, Some(Expr::IntLit(42, _))));
            }
            other => panic!("expected Return, got {other:?}"),
        }
    }

    #[test]
    fn parse_return_without_value() {
        let source = r#"module test {
            fn nothing() {
                return
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Return { value, .. } => {
                assert!(value.is_none());
            }
            other => panic!("expected Return, got {other:?}"),
        }
    }

    #[test]
    fn parse_binary_precedence() {
        // a + b * c should parse as a + (b * c)
        let source = r#"module test {
            fn main() {
                a + b * c
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(Expr::BinaryOp {
                op: BinOp::Add,
                left,
                right,
                ..
            }) => {
                assert!(matches!(left.as_ref(), Expr::Ident(ref n, _) if n == "a"));
                match right.as_ref() {
                    Expr::BinaryOp {
                        op: BinOp::Mul,
                        left: inner_left,
                        right: inner_right,
                        ..
                    } => {
                        assert!(matches!(inner_left.as_ref(), Expr::Ident(ref n, _) if n == "b"));
                        assert!(matches!(inner_right.as_ref(), Expr::Ident(ref n, _) if n == "c"));
                    }
                    other => panic!("expected Mul, got {other:?}"),
                }
            }
            other => panic!("expected Add at top, got {other:?}"),
        }
    }

    #[test]
    fn parse_nested_if_else() {
        let source = r#"module test {
            fn check(x: Int) -> Int {
                if x > 0 {
                    return 1
                } else if x < 0 {
                    return -1
                } else {
                    return 0
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(Expr::If {
                else_branch: Some(else_block),
                ..
            }) => {
                // The else branch should contain another if expression
                assert_eq!(else_block.stmts.len(), 1);
                assert!(matches!(
                    &else_block.stmts[0],
                    Stmt::Expr(Expr::If {
                        else_branch: Some(_),
                        ..
                    })
                ));
            }
            other => panic!("expected If with else, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_call() {
        let source = r#"module test {
            fn main() {
                foo(1, 2, 3)
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(Expr::Call { callee, args, .. }) => {
                assert!(matches!(callee.as_ref(), Expr::Ident(ref n, _) if n == "foo"));
                assert_eq!(args.len(), 3);
                assert!(matches!(&args[0], Expr::IntLit(1, _)));
                assert!(matches!(&args[1], Expr::IntLit(2, _)));
                assert!(matches!(&args[2], Expr::IntLit(3, _)));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_call_no_args() {
        let source = r#"module test {
            fn main() {
                bar()
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(Expr::Call { callee, args, .. }) => {
                assert!(matches!(callee.as_ref(), Expr::Ident(ref n, _) if n == "bar"));
                assert!(args.is_empty());
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn parse_requires_ensures() {
        let source = r#"module test {
            fn divide(a: Int, b: Int) -> Int
                requires { b != 0 }
                ensures { result >= 0 }
            {
                return a / b
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let func = &module.functions[0];
        assert_eq!(func.requires.len(), 1);
        assert_eq!(func.ensures.len(), 1);

        // Check the requires clause is `b != 0`
        match &func.requires[0] {
            Expr::BinaryOp {
                op: BinOp::Ne,
                left,
                right,
                ..
            } => {
                assert!(matches!(left.as_ref(), Expr::Ident(ref n, _) if n == "b"));
                assert!(matches!(right.as_ref(), Expr::IntLit(0, _)));
            }
            other => panic!("expected Ne, got {other:?}"),
        }

        // Check the ensures clause is `result >= 0`
        match &func.ensures[0] {
            Expr::BinaryOp {
                op: BinOp::Ge,
                left,
                right,
                ..
            } => {
                assert!(matches!(left.as_ref(), Expr::Ident(ref n, _) if n == "result"));
                assert!(matches!(right.as_ref(), Expr::IntLit(0, _)));
            }
            other => panic!("expected Ge, got {other:?}"),
        }
    }

    #[test]
    fn parse_complex_expression() {
        // a + b * c - d / e should parse as ((a + (b * c)) - (d / e))
        let source = r#"module test {
            fn main() {
                a + b * c - d / e
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        // Top level should be Sub (left-assoc: (a + b*c) - (d/e))
        match &stmts[0] {
            Stmt::Expr(Expr::BinaryOp {
                op: BinOp::Sub,
                left,
                right,
                ..
            }) => {
                // Left should be Add
                assert!(matches!(
                    left.as_ref(),
                    Expr::BinaryOp { op: BinOp::Add, .. }
                ));
                // Right should be Div
                assert!(matches!(
                    right.as_ref(),
                    Expr::BinaryOp { op: BinOp::Div, .. }
                ));
            }
            other => panic!("expected Sub at top, got {other:?}"),
        }
    }

    #[test]
    fn parse_logical_operators() {
        let source = r#"module test {
            fn main() {
                a && b || c
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        // Should parse as (a && b) || c since || has lower precedence
        match &stmts[0] {
            Stmt::Expr(Expr::BinaryOp {
                op: BinOp::Or,
                left,
                ..
            }) => {
                assert!(matches!(
                    left.as_ref(),
                    Expr::BinaryOp { op: BinOp::And, .. }
                ));
            }
            other => panic!("expected Or at top, got {other:?}"),
        }
    }

    #[test]
    fn parse_unary_negation() {
        let source = r#"module test {
            fn main() {
                -42
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(Expr::UnaryOp {
                op: UnaryOp::Neg,
                operand,
                ..
            }) => {
                assert!(matches!(operand.as_ref(), Expr::IntLit(42, _)));
            }
            other => panic!("expected UnaryOp Neg, got {other:?}"),
        }
    }

    #[test]
    fn parse_unary_not() {
        let source = r#"module test {
            fn main() {
                !flag
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
                ..
            }) => {
                assert!(matches!(operand.as_ref(), Expr::Ident(ref n, _) if n == "flag"));
            }
            other => panic!("expected UnaryOp Not, got {other:?}"),
        }
    }

    #[test]
    fn parse_field_access() {
        let source = r#"module test {
            fn main() {
                x.y
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(Expr::FieldAccess { object, field, .. }) => {
                assert!(matches!(object.as_ref(), Expr::Ident(ref n, _) if n == "x"));
                assert_eq!(field, "y");
            }
            other => panic!("expected FieldAccess, got {other:?}"),
        }
    }

    #[test]
    fn parse_parenthesized_expr() {
        let source = r#"module test {
            fn main() {
                (a + b) * c
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        // Top level should be Mul because parens override precedence
        match &stmts[0] {
            Stmt::Expr(Expr::BinaryOp {
                op: BinOp::Mul,
                left,
                ..
            }) => {
                assert!(matches!(
                    left.as_ref(),
                    Expr::BinaryOp { op: BinOp::Add, .. }
                ));
            }
            other => panic!("expected Mul at top, got {other:?}"),
        }
    }

    #[test]
    fn parse_bool_literals() {
        let source = r#"module test {
            fn main() {
                let a: Bool = true
                let b: Bool = false
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        match &stmts[0] {
            Stmt::Let { value, .. } => {
                assert!(matches!(value, Expr::BoolLit(true, _)));
            }
            other => panic!("expected Let with true, got {other:?}"),
        }
        match &stmts[1] {
            Stmt::Let { value, .. } => {
                assert!(matches!(value, Expr::BoolLit(false, _)));
            }
            other => panic!("expected Let with false, got {other:?}"),
        }
    }

    #[test]
    fn parse_string_literal_expr() {
        let source = r#"module test {
            fn main() {
                let s: String = "hello"
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Let { value, .. } => {
                assert!(matches!(value, Expr::StringLit(ref s, _) if s == "hello"));
            }
            other => panic!("expected Let with string, got {other:?}"),
        }
    }

    #[test]
    fn parse_if_without_else() {
        let source = r#"module test {
            fn main() {
                if x > 0 {
                    return 1
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(Expr::If { else_branch, .. }) => {
                assert!(else_branch.is_none());
            }
            other => panic!("expected If without else, got {other:?}"),
        }
    }

    #[test]
    fn parse_multiple_statements() {
        let source = r#"module test {
            fn main() {
                let x: Int = 1
                let y: Int = 2
                return x + y
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 3);
        assert!(matches!(&stmts[0], Stmt::Let { .. }));
        assert!(matches!(&stmts[1], Stmt::Let { .. }));
        assert!(matches!(&stmts[2], Stmt::Return { .. }));
    }

    #[test]
    fn parse_chained_method_calls() {
        let source = r#"module test {
            fn main() {
                a.b.c(1)
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        // Should be Call(FieldAccess(FieldAccess(a, b), c), [1])
        match &stmts[0] {
            Stmt::Expr(Expr::Call { callee, args, .. }) => {
                assert_eq!(args.len(), 1);
                match callee.as_ref() {
                    Expr::FieldAccess { object, field, .. } => {
                        assert_eq!(field, "c");
                        match object.as_ref() {
                            Expr::FieldAccess {
                                object: inner,
                                field: inner_field,
                                ..
                            } => {
                                assert!(
                                    matches!(inner.as_ref(), Expr::Ident(ref n, _) if n == "a")
                                );
                                assert_eq!(inner_field, "b");
                            }
                            other => panic!("expected inner FieldAccess, got {other:?}"),
                        }
                    }
                    other => panic!("expected FieldAccess callee, got {other:?}"),
                }
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn parse_multiple_contracts() {
        let source = r#"module test {
            fn safe_div(a: Int, b: Int) -> Int
                requires { b != 0 }
                requires { a >= 0 }
                ensures { result >= 0 }
            {
                return a / b
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let func = &module.functions[0];
        assert_eq!(func.requires.len(), 2);
        assert_eq!(func.ensures.len(), 1);
    }

    #[test]
    fn parse_while_simple() {
        let source = r#"module test {
            fn main() {
                let mut i: Int = 5
                while i > 0 {
                    i = i - 1
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        match &stmts[1] {
            Stmt::While {
                condition, body, ..
            } => {
                assert!(matches!(condition, Expr::BinaryOp { op: BinOp::Gt, .. }));
                assert_eq!(body.stmts.len(), 1);
            }
            other => panic!("expected While, got {other:?}"),
        }
    }

    #[test]
    fn parse_while_with_nested_if() {
        let source = r#"module test {
            fn main() {
                let mut x: Int = 10
                while x > 0 {
                    if x == 5 {
                        println("halfway")
                    }
                    x = x - 1
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[1], Stmt::While { .. }));
    }

    #[test]
    fn parse_while_missing_block() {
        let source = r#"module test {
            fn main() {
                while true
            }
        }"#;

        let result = parse(source);
        assert!(result.is_err());
    }

    #[test]
    fn parse_assignment() {
        let source = r#"module test {
            fn main() {
                let mut x: Int = 1
                x = 42
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        match &stmts[1] {
            Stmt::Assign { name, value, .. } => {
                assert_eq!(name, "x");
                assert!(matches!(value, Expr::IntLit(42, _)));
            }
            other => panic!("expected Assign, got {other:?}"),
        }
    }

    #[test]
    fn parse_annotation_simple() {
        let source = r#"module test {
            meta { purpose: "test" }
            @confidence(95)
            fn foo() { }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.functions[0].annotations.len(), 1);
        assert_eq!(module.functions[0].annotations[0].name, "confidence");
    }

    #[test]
    fn parse_annotation_named_args() {
        let source = r#"module test {
            meta { purpose: "test" }
            @authored_by(agent: "claude")
            fn foo() { }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.functions[0].annotations.len(), 1);
        assert_eq!(module.functions[0].annotations[0].name, "authored_by");
        assert!(
            module.functions[0].annotations[0]
                .args
                .iter()
                .any(|a| matches!(a, kodo_ast::AnnotationArg::Named(name, _) if name == "agent")),
            "expected a named arg 'agent'"
        );
    }

    #[test]
    fn parse_multiple_annotations() {
        let source = r#"module test {
            meta { purpose: "test" }
            @authored_by(agent: "claude")
            @confidence(95)
            fn foo() { }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(
            module.functions[0].annotations.len(),
            2,
            "expected 2 annotations, got {}",
            module.functions[0].annotations.len()
        );
    }

    #[test]
    fn parse_error_span() {
        let error = ParseError::UnexpectedToken {
            expected: "expression".to_string(),
            found: TokenKind::RBrace,
            span: Span::new(10, 11),
        };
        assert_eq!(error.span(), Some(Span::new(10, 11)));

        let eof_error = ParseError::UnexpectedEof {
            expected: "expression".to_string(),
        };
        assert_eq!(eof_error.span(), None);
    }

    // ===== Generics (Phase 2) Tests =====

    #[test]
    fn parse_type_generic_single_arg() {
        let source = r#"module test {
            fn main() {
                let x: Option<Int> = 42
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Let { ty, .. } => {
                assert_eq!(
                    ty.as_ref(),
                    Some(&TypeExpr::Generic(
                        "Option".to_string(),
                        vec![TypeExpr::Named("Int".to_string())]
                    ))
                );
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_type_generic_multiple_args() {
        let source = r#"module test {
            fn main() {
                let p: Pair<Int, Bool> = 42
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Let { ty, .. } => {
                assert_eq!(
                    ty.as_ref(),
                    Some(&TypeExpr::Generic(
                        "Pair".to_string(),
                        vec![
                            TypeExpr::Named("Int".to_string()),
                            TypeExpr::Named("Bool".to_string()),
                        ]
                    ))
                );
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_type_generic_nested() {
        let source = r#"module test {
            fn main() {
                let x: Option<List<Int>> = 42
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Let { ty, .. } => {
                assert_eq!(
                    ty.as_ref(),
                    Some(&TypeExpr::Generic(
                        "Option".to_string(),
                        vec![TypeExpr::Generic(
                            "List".to_string(),
                            vec![TypeExpr::Named("Int".to_string())]
                        )]
                    ))
                );
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_type_non_generic_remains_named() {
        let source = r#"module test {
            fn main() {
                let x: Int = 42
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Let { ty, .. } => {
                assert_eq!(ty.as_ref(), Some(&TypeExpr::Named("Int".to_string())));
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_struct_decl_with_generic_params() {
        let source = r#"module test {
            struct Pair<T, U> {
                first: T,
                second: U,
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.type_decls.len(), 1);
        let decl = &module.type_decls[0];
        assert_eq!(decl.name, "Pair");
        assert_eq!(decl.generic_params, vec!["T", "U"]);
        assert_eq!(decl.fields.len(), 2);
        assert_eq!(decl.fields[0].name, "first");
        assert_eq!(decl.fields[0].ty, TypeExpr::Named("T".to_string()));
        assert_eq!(decl.fields[1].name, "second");
        assert_eq!(decl.fields[1].ty, TypeExpr::Named("U".to_string()));
    }

    #[test]
    fn parse_struct_decl_without_generic_params() {
        let source = r#"module test {
            struct Point {
                x: Int,
                y: Int,
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.type_decls.len(), 1);
        let decl = &module.type_decls[0];
        assert_eq!(decl.name, "Point");
        assert!(decl.generic_params.is_empty());
    }

    #[test]
    fn parse_enum_decl_with_generic_params() {
        let source = r#"module test {
            enum Option<T> {
                Some(T),
                None,
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.enum_decls.len(), 1);
        let decl = &module.enum_decls[0];
        assert_eq!(decl.name, "Option");
        assert_eq!(decl.generic_params, vec!["T"]);
        assert_eq!(decl.variants.len(), 2);
        assert_eq!(decl.variants[0].name, "Some");
        assert_eq!(
            decl.variants[0].fields,
            vec![TypeExpr::Named("T".to_string())]
        );
        assert_eq!(decl.variants[1].name, "None");
        assert!(decl.variants[1].fields.is_empty());
    }

    #[test]
    fn parse_enum_decl_without_generic_params() {
        let source = r#"module test {
            enum Color {
                Red,
                Green,
                Blue,
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.enum_decls.len(), 1);
        let decl = &module.enum_decls[0];
        assert_eq!(decl.name, "Color");
        assert!(decl.generic_params.is_empty());
        assert_eq!(decl.variants.len(), 3);
    }

    #[test]
    fn parse_enum_decl_with_multiple_generic_params() {
        let source = r#"module test {
            enum Result<T, E> {
                Ok(T),
                Err(E),
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let decl = &module.enum_decls[0];
        assert_eq!(decl.name, "Result");
        assert_eq!(decl.generic_params, vec!["T", "E"]);
        assert_eq!(decl.variants.len(), 2);
        assert_eq!(decl.variants[0].name, "Ok");
        assert_eq!(
            decl.variants[0].fields,
            vec![TypeExpr::Named("T".to_string())]
        );
        assert_eq!(decl.variants[1].name, "Err");
        assert_eq!(
            decl.variants[1].fields,
            vec![TypeExpr::Named("E".to_string())]
        );
    }

    #[test]
    fn parse_function_param_with_generic_type() {
        let source = r#"module test {
            fn process(val: Option<Int>) -> Int {
                return 0
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let func = &module.functions[0];
        assert_eq!(func.params.len(), 1);
        assert_eq!(
            func.params[0].ty,
            TypeExpr::Generic(
                "Option".to_string(),
                vec![TypeExpr::Named("Int".to_string())]
            )
        );
    }

    #[test]
    fn parse_function_return_type_generic() {
        let source = r#"module test {
            fn wrap(x: Int) -> Option<Int> {
                return x
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let func = &module.functions[0];
        assert_eq!(
            func.return_type,
            TypeExpr::Generic(
                "Option".to_string(),
                vec![TypeExpr::Named("Int".to_string())]
            )
        );
    }

    #[test]
    fn parse_struct_field_with_generic_type() {
        let source = r#"module test {
            struct Container<T> {
                value: Option<T>,
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let decl = &module.type_decls[0];
        assert_eq!(decl.generic_params, vec!["T"]);
        assert_eq!(
            decl.fields[0].ty,
            TypeExpr::Generic("Option".to_string(), vec![TypeExpr::Named("T".to_string())])
        );
    }

    #[test]
    fn parse_for_loop_exclusive() {
        let source = r#"module test {
            fn main() {
                for i in 0..10 {
                    print_int(i)
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::For {
                name,
                start,
                end,
                inclusive,
                body,
                ..
            } => {
                assert_eq!(name, "i");
                assert!(matches!(start, Expr::IntLit(0, _)));
                assert!(matches!(end, Expr::IntLit(10, _)));
                assert!(!inclusive);
                assert_eq!(body.stmts.len(), 1);
            }
            other => panic!("expected For, got {other:?}"),
        }
    }

    #[test]
    fn parse_for_loop_inclusive() {
        let source = r#"module test {
            fn main() {
                for i in 0..=10 {
                    print_int(i)
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::For { inclusive, .. } => {
                assert!(inclusive);
            }
            other => panic!("expected For, got {other:?}"),
        }
    }

    #[test]
    fn parse_for_loop_missing_in() {
        let source = r#"module test {
            fn main() {
                for i 0..10 {
                }
            }
        }"#;

        let result = parse(source);
        assert!(result.is_err());
    }

    #[test]
    fn parse_range_expression() {
        let source = r#"module test {
            fn main() {
                let mut x: Int = 0
                for i in 1..5 {
                    x = x + i
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[1], Stmt::For { .. }));
    }

    #[test]
    fn parse_optional_type_shorthand() {
        let source = r#"module test {
            fn get_value(opt: Int?) -> Int {
                return 0
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let param_ty = &module.functions[0].params[0].ty;
        assert!(
            matches!(param_ty, TypeExpr::Optional(inner) if matches!(inner.as_ref(), TypeExpr::Named(n) if n == "Int"))
        );
    }

    #[test]
    fn parse_try_operator() {
        let source = r#"module test {
            fn do_thing() -> Int {
                let x: Int = result?
                return x
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let { value, .. } = &stmts[0] {
            assert!(matches!(value, Expr::Try { .. }));
        } else {
            panic!("expected Let statement");
        }
    }

    #[test]
    fn parse_optional_chain() {
        let source = r#"module test {
            fn get_x() -> Int {
                let v: Int = opt?.x
                return v
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let { value, .. } = &stmts[0] {
            assert!(matches!(value, Expr::OptionalChain { field, .. } if field == "x"));
        } else {
            panic!("expected Let statement");
        }
    }

    #[test]
    fn parse_null_coalesce() {
        let source = r#"module test {
            fn get_value() -> Int {
                let x: Int = opt ?? 0
                return x
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let { value, .. } = &stmts[0] {
            assert!(matches!(value, Expr::NullCoalesce { .. }));
        } else {
            panic!("expected Let statement");
        }
    }

    #[test]
    fn parse_chained_null_coalesce() {
        let source = r#"module test {
            fn get_value() -> Int {
                let x: Int = a ?? b ?? 0
                return x
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let { value, .. } = &stmts[0] {
            // Should be left-associative: (a ?? b) ?? 0
            assert!(
                matches!(value, Expr::NullCoalesce { left, .. } if matches!(left.as_ref(), Expr::NullCoalesce { .. }))
            );
        } else {
            panic!("expected Let statement");
        }
    }

    #[test]
    fn parse_closure_with_typed_params() {
        let source = r#"module test {
            fn main() {
                let f = |x: Int, y: Int| x + y
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Let { value, .. } => {
                assert!(matches!(value, Expr::Closure { params, .. } if params.len() == 2));
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_closure_with_return_type() {
        let source = r#"module test {
            fn main() {
                let f = |x: Int| -> Int { x * 2 }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Let { value, .. } => {
                assert!(matches!(
                    value,
                    Expr::Closure {
                        return_type: Some(TypeExpr::Named(_)),
                        ..
                    }
                ));
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_empty_closure() {
        let source = r#"module test {
            fn main() {
                let f = || 42
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Let { value, .. } => {
                assert!(matches!(value, Expr::Closure { params, .. } if params.is_empty()));
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_type_annotation() {
        let source = r#"module test {
            fn apply(f: (Int) -> Int, x: Int) -> Int {
                return f(x)
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let func = &module.functions[0];
        assert_eq!(func.params.len(), 2);
        assert_eq!(
            func.params[0].ty,
            TypeExpr::Function(
                vec![TypeExpr::Named("Int".to_string())],
                Box::new(TypeExpr::Named("Int".to_string()))
            )
        );
    }

    #[test]
    fn parse_or_still_works_with_pipe() {
        // Ensure || as logical OR still works in binary position
        let source = r#"module test {
            fn main() {
                let x = true || false
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Let { value, .. } => {
                assert!(matches!(value, Expr::BinaryOp { op: BinOp::Or, .. }));
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn parse_trait_declaration() {
        let source = r#"module test {
            meta { purpose: "test" }
            trait Describable {
                fn describe(self) -> Int
            }
            fn main() -> Int { return 0 }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.trait_decls.len(), 1);
        assert_eq!(module.trait_decls[0].name, "Describable");
        assert_eq!(module.trait_decls[0].methods.len(), 1);
        assert_eq!(module.trait_decls[0].methods[0].name, "describe");
        assert!(module.trait_decls[0].methods[0].has_self);
        assert_eq!(
            module.trait_decls[0].methods[0].return_type,
            TypeExpr::Named("Int".to_string())
        );
    }

    #[test]
    fn parse_trait_with_multiple_methods() {
        let source = r#"module test {
            meta { purpose: "test" }
            trait Shape {
                fn area(self) -> Int
                fn perimeter(self) -> Int
            }
            fn main() -> Int { return 0 }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.trait_decls[0].methods.len(), 2);
        assert_eq!(module.trait_decls[0].methods[0].name, "area");
        assert_eq!(module.trait_decls[0].methods[1].name, "perimeter");
    }

    #[test]
    fn parse_impl_block() {
        let source = r#"module test {
            meta { purpose: "test" }
            struct Point {
                x: Int
                y: Int
            }
            trait Describable {
                fn describe(self) -> Int
            }
            impl Describable for Point {
                fn describe(self) -> Int {
                    return self.x + self.y
                }
            }
            fn main() -> Int { return 0 }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.impl_blocks.len(), 1);
        assert_eq!(module.impl_blocks[0].trait_name, "Describable");
        assert_eq!(module.impl_blocks[0].type_name, "Point");
        assert_eq!(module.impl_blocks[0].methods.len(), 1);
        assert_eq!(module.impl_blocks[0].methods[0].name, "describe");
        // Self param should be resolved to Point
        assert_eq!(
            module.impl_blocks[0].methods[0].params[0].ty,
            TypeExpr::Named("Point".to_string())
        );
    }

    #[test]
    fn parse_method_call_as_field_access_then_call() {
        let source = r#"module test {
            meta { purpose: "test" }
            fn main() -> Int {
                let x: Int = p.describe()
                return 0
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let { value, .. } = &stmts[0] {
            // p.describe() should parse as Call { callee: FieldAccess { object: p, field: "describe" }, args: [] }
            assert!(matches!(value, Expr::Call { callee, args, .. }
                if args.is_empty()
                && matches!(callee.as_ref(), Expr::FieldAccess { field, .. } if field == "describe")
            ));
        } else {
            panic!("expected Let statement");
        }
    }

    #[test]
    fn parse_trait_method_with_extra_params() {
        let source = r#"module test {
            meta { purpose: "test" }
            trait Adder {
                fn add(self, other: Int) -> Int
            }
            fn main() -> Int { return 0 }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let method = &module.trait_decls[0].methods[0];
        assert_eq!(method.name, "add");
        assert!(method.has_self);
        assert_eq!(method.params.len(), 2);
        assert_eq!(method.params[0].name, "self");
        assert_eq!(method.params[1].name, "other");
    }

    #[test]
    fn parse_if_let_statement() {
        let source = r#"module test {
            fn main() -> Int {
                let opt: Option<Int> = Option::Some(42)
                if let Option::Some(v) = opt {
                    return v
                } else {
                    return 0
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        assert!(
            matches!(&stmts[1], Stmt::IfLet { .. }),
            "expected IfLet, got {:?}",
            stmts[1]
        );
    }

    #[test]
    fn parse_if_let_without_else() {
        let source = r#"module test {
            fn main() {
                let opt: Option<Int> = Option::Some(42)
                if let Option::Some(v) = opt {
                    return
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        if let Stmt::IfLet { else_body, .. } = &stmts[1] {
            assert!(else_body.is_none());
        } else {
            panic!("expected IfLet");
        }
    }

    #[test]
    fn parse_is_expression() {
        let source = r#"module test {
            fn main() -> Bool {
                let opt: Option<Int> = Option::Some(42)
                return opt is Some
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);
        if let Stmt::Return {
            value: Some(expr), ..
        } = &stmts[1]
        {
            assert!(
                matches!(expr, Expr::Is { type_name, .. } if type_name == "Some"),
                "expected Is expression, got {expr:?}"
            );
        } else {
            panic!("expected Return with Is expression");
        }
    }

    #[test]
    fn parse_ownership_qualifiers() {
        let source = r#"module test {
            fn transfer(own x: Int, ref y: Int) -> Int {
                return x
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let params = &module.functions[0].params;
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "x");
        assert_eq!(params[0].ownership, Ownership::Owned);
        assert_eq!(params[1].name, "y");
        assert_eq!(params[1].ownership, Ownership::Ref);
    }

    #[test]
    fn parse_default_ownership_is_owned() {
        let source = r#"module test {
            fn foo(x: Int) -> Int {
                return x
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let params = &module.functions[0].params;
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].ownership, Ownership::Owned);
    }

    #[test]
    fn parse_async_fn() {
        let source = r#"module test {
            async fn fetch(x: Int) -> Int {
                return x
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.functions.len(), 1);
        assert!(
            module.functions[0].is_async,
            "function should be marked async"
        );
        assert_eq!(module.functions[0].name, "fetch");
    }

    #[test]
    fn parse_await_expression() {
        let source = r#"module test {
            async fn compute() -> Int {
                let val: Int = foo().await
                return val
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let body = &module.functions[0].body;
        if let Stmt::Let { value, .. } = &body.stmts[0] {
            assert!(
                matches!(value, Expr::Await { .. }),
                "expected Await expression, got {value:?}"
            );
        } else {
            panic!("expected Let statement");
        }
    }

    #[test]
    fn parse_spawn_stmt() {
        let source = r#"module test {
            fn main() {
                spawn {
                    let x: Int = 1
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let body = &module.functions[0].body;
        assert!(
            matches!(&body.stmts[0], Stmt::Spawn { .. }),
            "expected Spawn statement, got {:?}",
            body.stmts[0]
        );
    }

    #[test]
    fn parse_parallel_stmt() {
        let source = r#"module test {
            fn main() {
                parallel {
                    spawn {
                        let x: Int = 1
                    }
                    spawn {
                        let y: Int = 2
                    }
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let body = &module.functions[0].body;
        if let Stmt::Parallel { body: stmts, .. } = &body.stmts[0] {
            assert_eq!(stmts.len(), 2);
            assert!(matches!(&stmts[0], Stmt::Spawn { .. }));
            assert!(matches!(&stmts[1], Stmt::Spawn { .. }));
        } else {
            panic!("expected Parallel statement");
        }
    }

    #[test]
    fn parse_actor_decl() {
        let source = r#"module test {
            actor Counter {
                count: Int

                fn increment(self) -> Int {
                    return self.count + 1
                }
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.actor_decls.len(), 1);
        let actor = &module.actor_decls[0];
        assert_eq!(actor.name, "Counter");
        assert_eq!(actor.fields.len(), 1);
        assert_eq!(actor.fields[0].name, "count");
        assert_eq!(actor.handlers.len(), 1);
        assert_eq!(actor.handlers[0].name, "increment");
    }

    #[test]
    fn parse_intent_basic() {
        let source = r#"module test {
            meta { purpose: "test" }
            intent console_app {
                greeting: "hello"
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.intent_decls.len(), 1);
        let intent = &module.intent_decls[0];
        assert_eq!(intent.name, "console_app");
        assert_eq!(intent.config.len(), 1);
        assert_eq!(intent.config[0].key, "greeting");
        assert!(
            matches!(&intent.config[0].value, IntentConfigValue::StringLit(s, _) if s == "hello"),
            "expected StringLit(\"hello\"), got {:?}",
            intent.config[0].value
        );
    }

    #[test]
    fn parse_intent_with_list() {
        let source = r#"module test {
            meta { purpose: "test" }
            intent math_module {
                functions: [add, sub]
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.intent_decls.len(), 1);
        let intent = &module.intent_decls[0];
        assert_eq!(intent.name, "math_module");
        assert_eq!(intent.config.len(), 1);
        assert_eq!(intent.config[0].key, "functions");
        if let IntentConfigValue::List(items, _) = &intent.config[0].value {
            assert_eq!(items.len(), 2);
            assert!(
                matches!(&items[0], IntentConfigValue::FnRef(name, _) if name == "add"),
                "expected FnRef(\"add\"), got {:?}",
                items[0]
            );
            assert!(
                matches!(&items[1], IntentConfigValue::FnRef(name, _) if name == "sub"),
                "expected FnRef(\"sub\"), got {:?}",
                items[1]
            );
        } else {
            panic!(
                "expected List config value, got {:?}",
                intent.config[0].value
            );
        }
    }

    #[test]
    fn parse_intent_bool_and_int_config() {
        let source = r#"module test {
            meta { purpose: "test" }
            intent custom_intent {
                enabled: true
                max_retries: 3
                verbose: false
            }
        }"#;

        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.intent_decls.len(), 1);
        let intent = &module.intent_decls[0];
        assert_eq!(intent.name, "custom_intent");
        assert_eq!(intent.config.len(), 3);

        assert_eq!(intent.config[0].key, "enabled");
        assert!(
            matches!(&intent.config[0].value, IntentConfigValue::BoolLit(true, _)),
            "expected BoolLit(true), got {:?}",
            intent.config[0].value
        );

        assert_eq!(intent.config[1].key, "max_retries");
        assert!(
            matches!(&intent.config[1].value, IntentConfigValue::IntLit(3, _)),
            "expected IntLit(3), got {:?}",
            intent.config[1].value
        );

        assert_eq!(intent.config[2].key, "verbose");
        assert!(
            matches!(
                &intent.config[2].value,
                IntentConfigValue::BoolLit(false, _)
            ),
            "expected BoolLit(false), got {:?}",
            intent.config[2].value
        );
    }

    #[test]
    fn fix_patch_missing_rbrace() {
        use kodo_ast::Diagnostic;
        // Missing closing brace for module — parser expects RBrace but finds something else
        let err = parse("module test { fn main() { }").unwrap_err();
        let patch = err.fix_patch();
        assert!(patch.is_some(), "expected a fix patch for missing `}}`");
        let patch = patch.unwrap();
        assert_eq!(patch.replacement, "}");
        assert!(
            patch.description.contains('}'),
            "description should mention `}}`: {}",
            patch.description
        );
    }

    #[test]
    fn fix_patch_eof_missing_rbrace() {
        use kodo_ast::Diagnostic;
        // Module with unclosed brace — parser hits EOF expecting RBrace
        let err = parse("module test {").unwrap_err();
        let patch = err.fix_patch();
        assert!(patch.is_some(), "expected a fix patch for EOF missing `}}`");
        let patch = patch.unwrap();
        assert_eq!(patch.replacement, "}");
        assert!(
            patch.description.contains("end of file"),
            "description should mention end of file: {}",
            patch.description
        );
    }

    #[test]
    fn fix_patch_none_for_non_delimiter() {
        use kodo_ast::Diagnostic;
        // Missing module keyword — no delimiter fix expected
        let err = parse("hello { }").unwrap_err();
        let patch = err.fix_patch();
        assert!(
            patch.is_none(),
            "should not produce fix patch for non-delimiter errors"
        );
    }

    mod snapshot_tests {
        use super::*;

        #[test]
        fn snapshot_simple_module() {
            let source = r#"module hello { meta { purpose: "test" } }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_function_with_params_and_return() {
            let source = r#"module m {
                meta { purpose: "t" }
                fn add(a: Int, b: Int) -> Int {
                    return a + b
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_let_bindings() {
            let source = r#"module test {
                fn main() {
                    let x: Int = 42
                    let name: String = "hello"
                    let flag: Bool = true
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_if_else() {
            let source = r#"module test {
                fn main() {
                    let result: Int = if x > 0 { 1 } else { 0 }
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_nested_if_else() {
            let source = r#"module test {
                fn main() {
                    let result: Int = if x > 0 { 1 } else if x < 0 { 2 } else { 0 }
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_while_loop() {
            let source = r#"module test {
                fn main() {
                    while x > 0 {
                        x = x - 1
                    }
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_match_expression() {
            let source = r#"module test {
                fn main() {
                    let r: Int = match x {
                        Option::Some(v) => v,
                        Option::None => 0,
                        _ => 1
                    }
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_struct_decl() {
            let source = r#"module test {
                struct Point {
                    x: Float64,
                    y: Float64
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_enum_decl() {
            let source = r#"module test {
                enum Shape {
                    Circle(Float64),
                    Rectangle(Float64, Float64),
                    Point
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_closure_no_params() {
            let source = r#"module test {
                fn main() {
                    let f = || 42
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_closure_with_params() {
            let source = r#"module test {
                fn main() {
                    let f = |x: Int, y: Int| x + y
                }
            }"#;
            let module = parse(source).unwrap();
            insta::assert_debug_snapshot!(module);
        }

        #[test]
        fn snapshot_error_missing_module_keyword() {
            let err = parse("hello { }").unwrap_err();
            insta::assert_snapshot!(err.to_string());
        }

        #[test]
        fn snapshot_error_unexpected_eof() {
            let err = parse("module test {").unwrap_err();
            insta::assert_snapshot!(err.to_string());
        }

        #[test]
        fn snapshot_error_missing_brace() {
            let err = parse("module test { fn foo() }").unwrap_err();
            insta::assert_snapshot!(err.to_string());
        }
    }
}
