//! Module-level parsing for the Kōdo parser.
//!
//! This module handles parsing of the top-level `module` construct,
//! including `import` declarations, `meta` blocks, and the dispatch
//! logic for all module-level items (structs, enums, traits, impls,
//! functions, intents, actors, type aliases).
//!
//! It also provides the [`parse_with_recovery`] function for error-tolerant
//! parsing that collects multiple errors instead of stopping at the first.
//! Recovery uses panic-mode synchronization as described in **\[CI\]**
//! *Crafting Interpreters* Ch. 6 and **\[EC\]** *Engineering a Compiler* Ch. 3.4.

use kodo_ast::{
    ActorDecl, EnumDecl, Function, ImplBlock, ImportDecl, IntentDecl, InvariantDecl, Meta,
    MetaEntry, Module, Span, TestDecl, TraitDecl, TypeAlias, TypeDecl, TypeExpr, Visibility,
};
use kodo_lexer::TokenKind;

use crate::error::{ParseError, ParseOutput, Result};
use crate::Parser;

/// Internal bucket for declarations collected during error recovery.
#[derive(Default)]
struct RecoveredDeclarations {
    /// Collected type alias declarations.
    type_aliases: Vec<TypeAlias>,
    /// Collected struct declarations.
    type_decls: Vec<TypeDecl>,
    /// Collected enum declarations.
    enum_decls: Vec<EnumDecl>,
    /// Collected trait declarations.
    trait_decls: Vec<TraitDecl>,
    /// Collected impl blocks.
    impl_blocks: Vec<ImplBlock>,
    /// Collected actor declarations.
    actor_decls: Vec<ActorDecl>,
    /// Collected intent declarations.
    intent_decls: Vec<IntentDecl>,
    /// Collected invariant declarations.
    invariants: Vec<InvariantDecl>,
    /// Collected function declarations.
    functions: Vec<Function>,
    /// Collected test declarations.
    test_decls: Vec<TestDecl>,
}

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

        // Parse import declarations (both `import` and `from ... import`)
        let mut imports = Vec::new();
        while self.check(&TokenKind::Import) || self.check(&TokenKind::From) {
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
        let mut invariants = Vec::new();
        let mut functions = Vec::new();
        let mut test_decls = Vec::new();
        while self.check(&TokenKind::Fn)
            || self.check(&TokenKind::At)
            || self.check(&TokenKind::Struct)
            || self.check(&TokenKind::Enum)
            || self.check(&TokenKind::Trait)
            || self.check(&TokenKind::Impl)
            || self.check(&TokenKind::Actor)
            || self.check(&TokenKind::Async)
            || self.check(&TokenKind::Invariant)
            || self.check(&TokenKind::Intent)
            || self.check(&TokenKind::Type)
            || self.check(&TokenKind::Pub)
            || self.check(&TokenKind::Test)
        {
            if self.check(&TokenKind::Test) {
                test_decls.push(self.parse_test_decl(vec![])?);
            } else if self.check(&TokenKind::Pub) {
                self.advance();
                if self.check(&TokenKind::Struct) {
                    let mut decl = self.parse_struct_decl()?;
                    decl.visibility = Visibility::Public;
                    type_decls.push(decl);
                } else {
                    let mut func = self.parse_annotated_function()?;
                    func.visibility = Visibility::Public;
                    functions.push(func);
                }
            } else if self.check(&TokenKind::Type) {
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
            } else if self.check(&TokenKind::Invariant) {
                invariants.push(self.parse_invariant()?);
            } else if self.check(&TokenKind::Intent) {
                intent_decls.push(self.parse_intent()?);
            } else {
                // Parse annotations first, then dispatch to test or function.
                let annotations = self.parse_annotations()?;
                if self.check(&TokenKind::Test) {
                    test_decls.push(self.parse_test_decl(annotations)?);
                } else {
                    let mut func = self.parse_function()?;
                    func.annotations = annotations;
                    functions.push(func);
                }
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
            invariants,
            functions,
            test_decls,
            describe_decls: vec![],
        })
    }

    /// Returns `true` if `kind` is a token that can begin a new top-level
    /// declaration — i.e. a module-level synchronization point.
    ///
    /// Note: `RBrace` is intentionally excluded from the primary check.
    /// During recovery we want to land on the *start* of the next
    /// declaration, not on a stray closing brace from a malformed block.
    /// The module-closing `}` is handled via brace-depth tracking in
    /// [`synchronize_to_declaration`].
    fn is_module_sync_token(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Fn
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Trait
                | TokenKind::Impl
                | TokenKind::Intent
                | TokenKind::Invariant
                | TokenKind::Actor
                | TokenKind::Module
                | TokenKind::At
                | TokenKind::Async
                | TokenKind::Type
                | TokenKind::Pub
                | TokenKind::Test
        )
    }

    /// Returns `true` if `kind` is a token that can begin a new statement
    /// inside a block — i.e. a statement-level synchronization point.
    ///
    /// This is used for recovery inside function bodies. When a statement
    /// fails to parse, we skip tokens until we land on one of these and
    /// then resume parsing the next statement.
    ///
    /// # Academic Reference
    ///
    /// Statement-level panic-mode recovery as described in **\[CI\]**
    /// *Crafting Interpreters* Ch. 6.3.3 and **\[EC\]** *Engineering a
    /// Compiler* Ch. 3.4 — recovery at multiple granularity levels.
    fn is_stmt_sync_token(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Let
                | TokenKind::Return
                | TokenKind::If
                | TokenKind::While
                | TokenKind::For
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Spawn
                | TokenKind::Parallel
                | TokenKind::Fn
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Trait
                | TokenKind::Impl
                | TokenKind::Intent
                | TokenKind::RBrace
        )
    }

    /// Advances the token stream until a module-level synchronization
    /// token is found, or the token stream is exhausted.
    ///
    /// This is the core of panic-mode recovery: after an error we skip
    /// the damaged portion of the input and resume at the next point
    /// where a fresh declaration can reasonably begin.
    ///
    /// `RBrace` is intentionally skipped over rather than stopping on it,
    /// because during recovery the parser may have consumed an `LBrace`
    /// before the error was detected, and stopping at the matching `}`
    /// would prevent reaching the next valid declaration.
    fn synchronize_to_declaration(&mut self) {
        while let Some(token) = self.peek() {
            if Self::is_module_sync_token(&token.kind) {
                return;
            }
            self.advance();
        }
    }

    /// Advances the token stream until a statement-level synchronization
    /// token is found or the token stream is exhausted.
    ///
    /// Used inside block parsing for statement-level recovery. After a
    /// malformed statement, skip ahead to a token that can begin a new
    /// statement (like `let`, `return`, etc.) or that ends the block (`}`).
    pub(crate) fn synchronize_to_stmt(&mut self) {
        while let Some(token) = self.peek() {
            if Self::is_stmt_sync_token(&token.kind) {
                return;
            }
            self.advance();
        }
    }

    /// Creates an empty [`Module`] with the given name and span.
    fn empty_module(&mut self, name: String, span: Span) -> Module {
        Module {
            id: self.next_id(),
            span,
            name,
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            invariants: vec![],
            functions: vec![],
            test_decls: vec![],
            describe_decls: vec![],
        }
    }

    /// Parses a module with error recovery, collecting all parse errors
    /// instead of bailing on the first one.
    ///
    /// The returned [`ParseOutput`] always contains a [`Module`] — though
    /// it may be missing declarations whose parsing failed — plus every
    /// error that was encountered along the way.
    fn parse_module_with_recovery(&mut self) -> ParseOutput {
        let mut errors: Vec<ParseError> = Vec::new();
        let start = self.peek().map_or(Span::new(0, 0), |t| t.span);

        // Parse: module <name> {
        let name = match self
            .expect(&TokenKind::Module)
            .and_then(|_| self.parse_ident())
        {
            Ok(n) => n,
            Err(e) => {
                errors.push(e);
                return ParseOutput {
                    module: self.empty_module(String::new(), start),
                    errors,
                };
            }
        };

        if let Err(e) = self.expect(&TokenKind::LBrace) {
            errors.push(e);
            return ParseOutput {
                module: self.empty_module(name, start),
                errors,
            };
        }

        let (imports, meta) = self.parse_header_with_recovery(&mut errors);
        let module_body = self.parse_declarations_with_recovery(&mut errors);

        // Try to consume closing brace.
        if let Err(e) = self.expect(&TokenKind::RBrace) {
            errors.push(e);
        }
        let span = start.merge(self.prev_span());

        ParseOutput {
            module: Module {
                id: self.next_id(),
                span,
                name,
                imports,
                meta,
                type_aliases: module_body.type_aliases,
                type_decls: module_body.type_decls,
                enum_decls: module_body.enum_decls,
                trait_decls: module_body.trait_decls,
                impl_blocks: module_body.impl_blocks,
                actor_decls: module_body.actor_decls,
                intent_decls: module_body.intent_decls,
                invariants: module_body.invariants,
                functions: module_body.functions,
                test_decls: module_body.test_decls,
                describe_decls: vec![],
            },
            errors,
        }
    }

    /// Parses imports and meta block, recovering from errors.
    fn parse_header_with_recovery(
        &mut self,
        errors: &mut Vec<ParseError>,
    ) -> (Vec<ImportDecl>, Option<Meta>) {
        let mut imports = Vec::new();
        while self.check(&TokenKind::Import) || self.check(&TokenKind::From) {
            match self.parse_import() {
                Ok(imp) => imports.push(imp),
                Err(e) => {
                    errors.push(e);
                    self.synchronize_to_declaration();
                }
            }
        }
        let meta = if self.check(&TokenKind::Meta) {
            match self.parse_meta() {
                Ok(m) => Some(m),
                Err(e) => {
                    errors.push(e);
                    self.synchronize_to_declaration();
                    None
                }
            }
        } else {
            None
        };
        (imports, meta)
    }

    /// Parses top-level declarations with recovery, collecting errors.
    ///
    /// Each declaration is parsed individually. If parsing fails, the error
    /// is recorded and the parser synchronizes to the next module-level token.
    /// For function declarations that parse successfully up to the body, we
    /// additionally use statement-level recovery inside the body so that
    /// multiple errors within a single function are all reported.
    #[allow(clippy::too_many_lines)]
    fn parse_declarations_with_recovery(
        &mut self,
        errors: &mut Vec<ParseError>,
    ) -> RecoveredDeclarations {
        let mut decls = RecoveredDeclarations::default();

        while self.check(&TokenKind::Fn)
            || self.check(&TokenKind::At)
            || self.check(&TokenKind::Struct)
            || self.check(&TokenKind::Enum)
            || self.check(&TokenKind::Trait)
            || self.check(&TokenKind::Impl)
            || self.check(&TokenKind::Actor)
            || self.check(&TokenKind::Async)
            || self.check(&TokenKind::Invariant)
            || self.check(&TokenKind::Intent)
            || self.check(&TokenKind::Type)
            || self.check(&TokenKind::Pub)
            || self.check(&TokenKind::Test)
        {
            let result: std::result::Result<(), ParseError> = (|| {
                if self.check(&TokenKind::Test) {
                    decls.test_decls.push(self.parse_test_decl(vec![])?);
                } else if self.check(&TokenKind::Pub) {
                    self.advance();
                    if self.check(&TokenKind::Struct) {
                        let mut decl = self.parse_struct_decl()?;
                        decl.visibility = Visibility::Public;
                        decls.type_decls.push(decl);
                    } else {
                        let mut func = self.parse_annotated_function_with_recovery(errors)?;
                        func.visibility = Visibility::Public;
                        decls.functions.push(func);
                    }
                } else if self.check(&TokenKind::Type) {
                    decls.type_aliases.push(self.parse_type_alias()?);
                } else if self.check(&TokenKind::Struct) {
                    decls.type_decls.push(self.parse_struct_decl()?);
                } else if self.check(&TokenKind::Enum) {
                    decls.enum_decls.push(self.parse_enum_decl()?);
                } else if self.check(&TokenKind::Trait) {
                    decls.trait_decls.push(self.parse_trait_decl()?);
                } else if self.check(&TokenKind::Impl) {
                    decls.impl_blocks.push(self.parse_impl_block()?);
                } else if self.check(&TokenKind::Actor) {
                    decls.actor_decls.push(self.parse_actor_decl()?);
                } else if self.check(&TokenKind::Invariant) {
                    decls.invariants.push(self.parse_invariant()?);
                } else if self.check(&TokenKind::Intent) {
                    decls.intent_decls.push(self.parse_intent()?);
                } else {
                    // Parse annotations first, then dispatch to test or function.
                    let annotations = self.parse_annotations()?;
                    if self.check(&TokenKind::Test) {
                        decls.test_decls.push(self.parse_test_decl(annotations)?);
                    } else {
                        let mut func = self.parse_function_with_recovery(errors)?;
                        func.annotations = annotations;
                        decls.functions.push(func);
                    }
                }
                Ok(())
            })();

            if let Err(e) = result {
                errors.push(e);
                self.synchronize_to_declaration();
            }
        }

        decls
    }

    /// Parses annotations followed by a function definition, using
    /// statement-level recovery inside the function body.
    ///
    /// Signature errors still cause the whole function to fail (and the
    /// caller will synchronize to the next declaration). Body errors are
    /// collected in `errors` and parsing continues within the body.
    fn parse_annotated_function_with_recovery(
        &mut self,
        errors: &mut Vec<ParseError>,
    ) -> Result<Function> {
        let annotations = self.parse_annotations()?;
        let mut func = self.parse_function_with_recovery(errors)?;
        func.annotations = annotations;
        Ok(func)
    }

    /// Parses a function definition with statement-level recovery in the body.
    ///
    /// The function signature (name, params, return type, contracts) is parsed
    /// without recovery — a malformed signature causes the whole function to
    /// fail. The body, however, uses [`parse_block_with_recovery`] so that
    /// errors in individual statements are collected and parsing continues.
    fn parse_function_with_recovery(&mut self, errors: &mut Vec<ParseError>) -> Result<Function> {
        let is_async = if self.check(&TokenKind::Async) {
            self.advance();
            true
        } else {
            false
        };
        let start = self.expect(&TokenKind::Fn)?.span;
        let name = self.parse_ident()?;

        let generic_params = self.parse_optional_generic_params()?;

        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
            }
            params.push(self.parse_param()?);
        }
        self.expect(&TokenKind::RParen)?;

        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.parse_type()?
        } else {
            TypeExpr::Unit
        };

        // Use recovery-aware contract clause parsing so errors in
        // requires/ensures don't prevent parsing the function body.
        let (requires, ensures) = self.parse_contract_clauses_with_recovery(errors);

        // Use recovery-aware block parsing for the body.
        let body = self.parse_block_with_recovery(errors)?;
        let end = self.prev_span();

        Ok(Function {
            id: self.next_id(),
            span: start.merge(end),
            name,
            visibility: Visibility::Private,
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

    /// Parses an import declaration.
    ///
    /// Supported forms:
    /// - `import ident(.ident)*` — traditional dot-separated path
    /// - `import ident(::ident)*` — qualified `::` path
    /// - `from ident(::ident)* import name1, name2` — selective import
    fn parse_import(&mut self) -> Result<ImportDecl> {
        // Check for `from` prefix (selective import)
        if self.check(&TokenKind::From) {
            return self.parse_from_import();
        }

        let start = self.expect(&TokenKind::Import)?.span;
        let mut path = vec![self.parse_ident()?];
        // Accept both `::` and `.` as path separators
        while self.check(&TokenKind::ColonColon) || self.check(&TokenKind::Dot) {
            self.advance();
            path.push(self.parse_ident()?);
        }
        // Optional selective import: `import path { name1, name2 }`
        let names = if self.check(&TokenKind::LBrace) {
            self.advance();
            let mut names = vec![self.parse_ident()?];
            while self.check(&TokenKind::Comma) {
                self.advance();
                if self.check(&TokenKind::RBrace) {
                    break;
                }
                names.push(self.parse_ident()?);
            }
            self.expect(&TokenKind::RBrace)?;
            Some(names)
        } else {
            None
        };
        let span = start.merge(self.prev_span());
        Ok(ImportDecl { path, names, span })
    }

    /// Parses a selective import: `from path::to::module import Name1, Name2`
    fn parse_from_import(&mut self) -> Result<ImportDecl> {
        let start = self.expect(&TokenKind::From)?.span;
        let mut path = vec![self.parse_ident()?];
        // Accept both `::` and `.` as path separators
        while self.check(&TokenKind::ColonColon) || self.check(&TokenKind::Dot) {
            self.advance();
            path.push(self.parse_ident()?);
        }
        self.expect(&TokenKind::Import)?;
        let mut names = vec![self.parse_ident()?];
        while self.check(&TokenKind::Comma) {
            self.advance();
            names.push(self.parse_ident()?);
        }
        let span = start.merge(self.prev_span());
        Ok(ImportDecl {
            path,
            names: Some(names),
            span,
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
/// The returned [`ParseOutput`] always contains a [`Module`] (possibly with
/// missing declarations) plus every error that was encountered.
///
/// # Examples
///
/// ```
/// let output = kodo_parser::parse_with_recovery("module m { meta {} fn a() {} }");
/// assert!(output.errors.is_empty());
/// assert_eq!(output.module.name, "m");
/// ```
#[must_use]
pub fn parse_with_recovery(source: &str) -> ParseOutput {
    let tokens = match kodo_lexer::tokenize(source) {
        Ok(t) => t,
        Err(e) => {
            return ParseOutput {
                module: Module {
                    id: kodo_ast::NodeId(0),
                    span: Span::new(0, 0),
                    name: String::new(),
                    imports: vec![],
                    meta: None,
                    type_aliases: vec![],
                    type_decls: vec![],
                    enum_decls: vec![],
                    trait_decls: vec![],
                    impl_blocks: vec![],
                    actor_decls: vec![],
                    intent_decls: vec![],
                    invariants: vec![],
                    functions: vec![],
                    test_decls: vec![],
                    describe_decls: vec![],
                },
                errors: vec![ParseError::from(e)],
            };
        }
    };
    let mut parser = Parser::new(tokens);
    parser.parse_module_with_recovery()
}
