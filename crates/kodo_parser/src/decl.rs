//! Declaration parsing for the Kōdo parser.
//!
//! This module handles parsing of all top-level declarations that can
//! appear inside a module: functions (with annotations), structs, enums,
//! trait declarations, impl blocks, intent declarations, actor declarations,
//! and type aliases.

use kodo_ast::{
    ActorDecl, Annotation, AnnotationArg, AssociatedType, DescribeDecl, EnumDecl, EnumVariant,
    FieldDef, Function, ImplBlock, IntentConfigEntry, IntentConfigValue, IntentDecl, InvariantDecl,
    Ownership, Param, Span, TestDecl, TraitDecl, TraitMethod, TypeAlias, TypeDecl, TypeExpr,
    Visibility,
};
use kodo_lexer::TokenKind;

use crate::error::{ParseError, Result};
use crate::Parser;

impl Parser {
    /// Synchronizes the parser to the closing `}` at the current brace depth.
    ///
    /// Tracks nested `{`/`}` pairs so that inner blocks are correctly skipped.
    /// After this method returns, the parser is positioned just after the
    /// closing `}` that matches the opening brace. If no matching `}` is
    /// found before EOF, the parser is left at EOF.
    ///
    /// This is used for contract clause recovery: when the expression inside
    /// a `requires { ... }` or `ensures { ... }` is malformed, we skip to the
    /// matching `}` and continue parsing the rest of the function.
    ///
    /// # Academic Reference
    ///
    /// Brace-depth tracking for panic-mode recovery as described in
    /// **\[EC\]** *Engineering a Compiler* Ch. 3.4.
    pub(crate) fn synchronize_to_brace_close(&mut self) {
        let mut depth: u32 = 0;
        while let Some(token) = self.peek() {
            match token.kind {
                TokenKind::LBrace => {
                    depth = depth.saturating_add(1);
                    self.advance();
                }
                TokenKind::RBrace => {
                    if depth == 0 {
                        // Consume the closing brace and return.
                        self.advance();
                        return;
                    }
                    depth = depth.saturating_sub(1);
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Parses contract clauses (`requires`/`ensures`) with error recovery.
    ///
    /// When a syntax error occurs inside a contract clause, the error is
    /// recorded and the parser synchronizes to the closing `}` of that
    /// clause. Parsing then continues with the next clause or the function
    /// body, allowing the compiler to report more errors per compilation.
    ///
    /// Malformed clauses are omitted from the returned vectors — only
    /// successfully parsed clauses appear in the AST. The errors are
    /// collected into the `errors` vector for later reporting.
    ///
    /// # Academic Reference
    ///
    /// Clause-level panic-mode recovery extending the multi-level strategy
    /// described in **\[CI\]** *Crafting Interpreters* Ch. 6.3.3.
    pub(crate) fn parse_contract_clauses_with_recovery(
        &mut self,
        errors: &mut Vec<ParseError>,
    ) -> (Vec<kodo_ast::Expr>, Vec<kodo_ast::Expr>) {
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        loop {
            if self.check(&TokenKind::Requires) {
                let clause_span = self.advance().map_or(kodo_ast::Span::new(0, 0), |t| t.span);
                match self.parse_single_contract_clause() {
                    Ok(expr) => requires.push(expr),
                    Err(inner_err) => {
                        errors.push(ParseError::ContractClauseError {
                            clause_kind: "requires".to_string(),
                            message: inner_err.to_string(),
                            span: clause_span,
                        });
                        // Synchronize to the closing `}` of this clause.
                        self.synchronize_to_brace_close();
                    }
                }
            } else if self.check(&TokenKind::Ensures) {
                let clause_span = self.advance().map_or(kodo_ast::Span::new(0, 0), |t| t.span);
                match self.parse_single_contract_clause() {
                    Ok(expr) => ensures.push(expr),
                    Err(inner_err) => {
                        errors.push(ParseError::ContractClauseError {
                            clause_kind: "ensures".to_string(),
                            message: inner_err.to_string(),
                            span: clause_span,
                        });
                        self.synchronize_to_brace_close();
                    }
                }
            } else {
                break;
            }
        }
        (requires, ensures)
    }

    /// Parses the body of a single contract clause: `{ expr }`.
    ///
    /// Expects the `requires`/`ensures` keyword to already be consumed.
    /// Returns the parsed expression or an error.
    fn parse_single_contract_clause(&mut self) -> Result<kodo_ast::Expr> {
        self.expect(&TokenKind::LBrace)?;
        let expr = self.parse_expr()?;
        self.expect(&TokenKind::RBrace)?;
        Ok(expr)
    }

    /// Parses annotations followed by a function definition.
    pub(crate) fn parse_annotated_function(&mut self) -> Result<Function> {
        let annotations = self.parse_annotations()?;
        let mut func = self.parse_function()?;
        func.annotations = annotations;
        Ok(func)
    }

    /// Parses zero or more annotations: `@name` or `@name(args...)`.
    pub(crate) fn parse_annotations(&mut self) -> Result<Vec<Annotation>> {
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
    pub(crate) fn parse_function(&mut self) -> Result<Function> {
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

    /// Parses a single function parameter, including optional ownership
    /// qualifier (`own` or `ref`): `[own|ref] name: Type`.
    pub(crate) fn parse_param(&mut self) -> Result<Param> {
        let param_start = self.peek().map_or(Span::new(0, 0), |t| t.span);

        // Check for ownership qualifier
        let ownership = if self.check(&TokenKind::Own) {
            self.advance();
            Ownership::Owned
        } else if self.check(&TokenKind::Ref) {
            self.advance();
            Ownership::Ref
        } else if self.check(&TokenKind::Mut) {
            self.advance();
            Ownership::Mut
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
    pub(crate) fn parse_type_alias(&mut self) -> Result<TypeAlias> {
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
            id: self.next_id(),
            span: start.merge(end),
            name,
            base_type,
            constraint,
        })
    }

    /// Parses a struct declaration: `struct Name<T> { field: Type, ... }`
    pub(crate) fn parse_struct_decl(&mut self) -> Result<TypeDecl> {
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
            id: self.next_id(),
            span: start.merge(end),
            name,
            visibility: Visibility::Private,
            generic_params,
            fields,
        })
    }

    /// Parses an enum declaration: `enum Name<T> { Variant1, Variant2(Type, ...) }`
    pub(crate) fn parse_enum_decl(&mut self) -> Result<EnumDecl> {
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
            id: self.next_id(),
            span: start.merge(end),
            name,
            generic_params,
            variants,
        })
    }

    /// Parses a trait declaration: `trait Name { type Item; fn method(self) -> RetType ... }`
    ///
    /// Supports associated type declarations (`type Name` with optional bounds)
    /// and default method bodies (a block after the signature).
    pub(crate) fn parse_trait_decl(&mut self) -> Result<TraitDecl> {
        let start = self.expect(&TokenKind::Trait)?.span;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::LBrace)?;

        let mut associated_types = Vec::new();
        let mut methods = Vec::new();
        while self.check(&TokenKind::Fn) || self.check(&TokenKind::Type) {
            if self.check(&TokenKind::Type) {
                // Parse associated type: `type Name` or `type Name: Bound1 + Bound2`
                let type_start = self.expect(&TokenKind::Type)?.span;
                let type_name = self.parse_ident()?;
                let mut bounds = Vec::new();
                if self.check(&TokenKind::Colon) {
                    self.advance();
                    bounds.push(self.parse_ident()?);
                    while self.check(&TokenKind::Plus) {
                        self.advance();
                        bounds.push(self.parse_ident()?);
                    }
                }
                let type_end = self.prev_span();
                associated_types.push(AssociatedType {
                    name: type_name,
                    bounds,
                    span: type_start.merge(type_end),
                });
            } else {
                // Parse method signature with optional default body
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

                // Parse optional default body
                let body = if self.check(&TokenKind::LBrace) {
                    Some(self.parse_block()?)
                } else {
                    None
                };

                let method_end = self.prev_span();
                methods.push(TraitMethod {
                    name: method_name,
                    params,
                    return_type,
                    has_self,
                    body,
                    span: method_start.merge(method_end),
                });
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(TraitDecl {
            id: self.next_id(),
            span: start.merge(end),
            name,
            associated_types,
            methods,
        })
    }

    /// Parses an impl block: `impl TraitName for TypeName { type Item = Int; fn method(self) -> RetType { body } }`
    /// or an inherent impl block: `impl TypeName { fn method(self) -> RetType { body } }`
    ///
    /// Supports associated type bindings (`type Name = ConcreteType`) inside impl blocks.
    pub(crate) fn parse_impl_block(&mut self) -> Result<ImplBlock> {
        let start = self.expect(&TokenKind::Impl)?.span;
        let first_name = self.parse_ident()?;

        // Determine if this is a trait impl (`impl Trait for Type { ... }`)
        // or an inherent impl (`impl Type { ... }`).
        let (trait_name, type_name) = if self.check(&TokenKind::For) {
            self.advance();
            let type_name = self.parse_ident()?;
            (Some(first_name), type_name)
        } else {
            (None, first_name)
        };
        self.expect(&TokenKind::LBrace)?;

        let mut type_bindings = Vec::new();
        let mut methods = Vec::new();
        while self.check(&TokenKind::Fn) || self.check(&TokenKind::Type) {
            if self.check(&TokenKind::Type) {
                // Parse type binding: `type Name = ConcreteType`
                self.expect(&TokenKind::Type)?;
                let binding_name = self.parse_ident()?;
                self.expect(&TokenKind::Eq)?;
                let binding_type = self.parse_type()?;
                type_bindings.push((binding_name, binding_type));
            } else {
                let mut func = self.parse_impl_method()?;
                // Resolve `self` param type to the implementing type
                for param in &mut func.params {
                    if param.name == "self" {
                        param.ty = TypeExpr::Named(type_name.clone());
                    }
                }
                methods.push(func);
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(ImplBlock {
            id: self.next_id(),
            span: start.merge(end),
            trait_name,
            type_name,
            type_bindings,
            methods,
        })
    }

    /// Parses a method inside an impl block. Similar to `parse_function` but
    /// handles `self` as first parameter without requiring a type annotation.
    pub(crate) fn parse_impl_method(&mut self) -> Result<Function> {
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
            id: self.next_id(),
            span: start.merge(end),
            name,
            visibility: Visibility::Private,
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

    /// Parses an intent declaration: `intent name { key: value, ... }`.
    ///
    /// Config values can be string literals, integer literals, float literals,
    /// boolean literals, identifiers (function references), or lists.
    pub(crate) fn parse_intent(&mut self) -> Result<IntentDecl> {
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
            id: self.next_id(),
            span: start.merge(end),
            name,
            config,
        })
    }

    /// Parses a module invariant: `invariant { condition_expr }`
    ///
    /// Module invariants declare boolean conditions that must hold for every
    /// public function in the module. They are verified statically when
    /// possible and injected as runtime checks otherwise.
    pub(crate) fn parse_invariant(&mut self) -> Result<InvariantDecl> {
        let start = self.expect(&TokenKind::Invariant)?.span;
        self.expect(&TokenKind::LBrace)?;
        let condition = self.parse_expr()?;
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(InvariantDecl {
            span: start.merge(end),
            condition,
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

    /// Parses a test declaration: `test "name" { body }`
    ///
    /// Annotations are parsed externally and passed in so that the dispatch
    /// in `parse_module` can decide between `test` and `fn` after seeing `@`.
    pub(crate) fn parse_test_decl(&mut self, annotations: Vec<Annotation>) -> Result<TestDecl> {
        let start = self.expect(&TokenKind::Test)?.span;
        let name_token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "string literal for test name".to_string(),
        })?;
        let name = match &name_token.kind {
            TokenKind::StringLit(s) => s.clone(),
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "string literal for test name".to_string(),
                    found: other.clone(),
                    span: name_token.span,
                });
            }
        };
        let body = self.parse_block()?;
        let end = self.prev_span();
        Ok(TestDecl {
            id: self.next_id(),
            span: start.merge(end),
            name,
            annotations,
            body,
        })
    }

    /// Parses a `describe` block: `describe "name" { setup? teardown? (test|describe)* }`.
    ///
    /// Annotations are passed in from the caller (parsed before the `describe` keyword).
    /// Nested `describe` blocks and `test` declarations are recursively parsed.
    /// `setup` and `teardown` blocks are optional and may appear at most once each.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if an unexpected token is encountered inside the block.
    pub(crate) fn parse_describe_decl(
        &mut self,
        annotations: Vec<Annotation>,
    ) -> crate::error::Result<DescribeDecl> {
        let start = self.expect(&TokenKind::Describe)?.span;

        // The describe name must be a string literal (like test names).
        let name_token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "string literal for describe name".to_string(),
        })?;
        let name = match &name_token.kind {
            TokenKind::StringLit(s) => s.clone(),
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "string literal for describe name".to_string(),
                    found: other.clone(),
                    span: name_token.span,
                });
            }
        };

        self.expect(&TokenKind::LBrace)?;

        let mut setup = None;
        let mut teardown = None;
        let mut tests = Vec::new();
        let mut describes = Vec::new();

        while !self.check(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(ParseError::UnexpectedEof {
                    expected: "}".to_string(),
                });
            }

            // Parse optional annotations that may precede `test` or `describe`.
            let inner_annotations = if self.check(&TokenKind::At) {
                self.parse_annotations()?
            } else {
                vec![]
            };

            if self.check(&TokenKind::Setup) {
                self.advance();
                setup = Some(self.parse_block()?);
            } else if self.check(&TokenKind::Teardown) {
                self.advance();
                teardown = Some(self.parse_block()?);
            } else if self.check(&TokenKind::Test) {
                tests.push(self.parse_test_decl(inner_annotations)?);
            } else if self.check(&TokenKind::Describe) {
                describes.push(self.parse_describe_decl(inner_annotations)?);
            } else {
                // peek() is Some here (checked for is_none above), so this is always valid.
                let token = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "setup, teardown, test, describe, or `}`".to_string(),
                })?;
                return Err(ParseError::UnexpectedToken {
                    expected: "setup, teardown, test, describe, or `}`".to_string(),
                    found: token.kind.clone(),
                    span: token.span,
                });
            }
        }

        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(DescribeDecl {
            id: self.next_id(),
            span: start.merge(end),
            name,
            annotations,
            setup,
            teardown,
            tests,
            describes,
        })
    }

    /// Parses an actor declaration: `actor Name { fields... fn handler(self) { ... } ... }`
    pub(crate) fn parse_actor_decl(&mut self) -> Result<ActorDecl> {
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
            id: self.next_id(),
            span: start.merge(end),
            name,
            fields,
            handlers,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse, parse_with_recovery};
    use kodo_ast::{TypeExpr, Visibility};

    #[test]
    fn decl_struct_with_generic_params() {
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
        assert_eq!(decl.generic_params.len(), 2);
    }

    #[test]
    fn decl_enum_with_variants() {
        let source = r#"module test {
            enum Color { Red, Green, Blue }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.enum_decls.len(), 1);
        assert_eq!(module.enum_decls[0].variants.len(), 3);
    }

    #[test]
    fn decl_trait_with_method() {
        let source = r#"module test {
            trait Show { fn show(self) -> String }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.trait_decls.len(), 1);
        assert_eq!(module.trait_decls[0].methods[0].name, "show");
        assert!(module.trait_decls[0].methods[0].has_self);
    }

    #[test]
    fn decl_impl_block_resolves_self_type() {
        let source = r#"module test {
            struct Foo { x: Int }
            impl Foo {
                fn get_x(self) -> Int { return self.x }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.impl_blocks.len(), 1);
        assert_eq!(
            module.impl_blocks[0].methods[0].params[0].ty,
            TypeExpr::Named("Foo".to_string())
        );
    }

    #[test]
    fn decl_pub_function() {
        let source = r#"module test {
            pub fn greet(name: String) -> String {
                return name
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "greet");
        assert_eq!(module.functions[0].visibility, Visibility::Public);
    }

    #[test]
    fn decl_private_function_by_default() {
        let source = r#"module test {
            fn helper() -> Int { return 0 }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].visibility, Visibility::Private);
    }

    #[test]
    fn decl_pub_struct() {
        let source = r#"module test {
            pub struct Point { x: Int, y: Int }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.type_decls.len(), 1);
        assert_eq!(module.type_decls[0].name, "Point");
        assert_eq!(module.type_decls[0].visibility, Visibility::Public);
    }

    #[test]
    fn decl_test_basic() {
        let source = r#"module test_mod {
            test "addition works" {
                let x: Int = 2
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.test_decls.len(), 1);
        assert_eq!(module.test_decls[0].name, "addition works");
    }

    #[test]
    fn decl_test_with_annotations() {
        let source = r#"module test_mod {
            @confidence(0.95)
            test "annotated test" {
                let x: Int = 1
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.test_decls.len(), 1);
        assert_eq!(module.test_decls[0].name, "annotated test");
        assert_eq!(module.test_decls[0].annotations.len(), 1);
        assert_eq!(module.test_decls[0].annotations[0].name, "confidence");
    }

    #[test]
    fn decl_test_alongside_functions() {
        let source = r#"module test_mod {
            fn add(a: Int, b: Int) -> Int { return a + b }
            test "add works" {
                let r: Int = add(1, 2)
            }
            fn sub(a: Int, b: Int) -> Int { return a - b }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.functions.len(), 2);
        assert_eq!(module.test_decls.len(), 1);
    }

    #[test]
    fn decl_multiple_tests() {
        let source = r#"module test_mod {
            test "first" { let x: Int = 1 }
            test "second" { let y: Int = 2 }
            test "third" { let z: Int = 3 }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.test_decls.len(), 3);
        assert_eq!(module.test_decls[0].name, "first");
        assert_eq!(module.test_decls[1].name, "second");
        assert_eq!(module.test_decls[2].name, "third");
    }

    #[test]
    fn parse_describe_basic() {
        let source = r#"module test {
            meta { purpose: "test" version: "0.1.0" }
            describe "math" {
                test "add" {
                    let x: Int = 1
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.describe_decls.len(), 1);
        assert_eq!(module.describe_decls[0].name, "math");
        assert_eq!(module.describe_decls[0].tests.len(), 1);
        assert_eq!(module.describe_decls[0].tests[0].name, "add");
    }

    #[test]
    fn parse_describe_with_setup_teardown() {
        let source = r#"module test {
            meta { purpose: "test" version: "0.1.0" }
            describe "group" {
                setup {
                    let x: Int = 1
                }
                teardown {
                    let y: Int = 2
                }
                test "a" {
                    let z: Int = 3
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.describe_decls.len(), 1);
        assert!(module.describe_decls[0].setup.is_some());
        assert!(module.describe_decls[0].teardown.is_some());
        assert_eq!(module.describe_decls[0].tests.len(), 1);
    }

    #[test]
    fn parse_nested_describe() {
        let source = r#"module test {
            meta { purpose: "test" version: "0.1.0" }
            describe "outer" {
                describe "inner" {
                    test "nested" {
                        let x: Int = 1
                    }
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.describe_decls.len(), 1);
        assert_eq!(module.describe_decls[0].name, "outer");
        assert_eq!(module.describe_decls[0].describes.len(), 1);
        assert_eq!(module.describe_decls[0].describes[0].name, "inner");
        assert_eq!(module.describe_decls[0].describes[0].tests.len(), 1);
        assert_eq!(
            module.describe_decls[0].describes[0].tests[0].name,
            "nested"
        );
    }

    #[test]
    fn parse_describe_with_annotations() {
        let source = r#"module test {
            meta { purpose: "test" version: "0.1.0" }
            @confidence(0.9)
            describe "annotated group" {
                test "one" {
                    let x: Int = 1
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.describe_decls.len(), 1);
        assert_eq!(module.describe_decls[0].name, "annotated group");
        assert_eq!(module.describe_decls[0].annotations.len(), 1);
        assert_eq!(module.describe_decls[0].annotations[0].name, "confidence");
    }

    #[test]
    fn parse_describe_multiple_tests() {
        let source = r#"module test {
            describe "suite" {
                test "first" { let a: Int = 1 }
                test "second" { let b: Int = 2 }
                test "third" { let c: Int = 3 }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.describe_decls[0].tests.len(), 3);
        assert_eq!(module.describe_decls[0].tests[0].name, "first");
        assert_eq!(module.describe_decls[0].tests[1].name, "second");
        assert_eq!(module.describe_decls[0].tests[2].name, "third");
    }

    #[test]
    fn parse_describe_empty_block() {
        let source = r#"module test {
            describe "empty" {}
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.describe_decls.len(), 1);
        assert_eq!(module.describe_decls[0].name, "empty");
        assert!(module.describe_decls[0].setup.is_none());
        assert!(module.describe_decls[0].teardown.is_none());
        assert!(module.describe_decls[0].tests.is_empty());
        assert!(module.describe_decls[0].describes.is_empty());
    }

    #[test]
    fn decl_import_with_names() {
        let source = r#"module test {
            import math { sin, cos }
            fn main() -> Int { return 0 }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(module.imports.len(), 1);
        assert_eq!(module.imports[0].path, vec!["math"]);
        assert_eq!(
            module.imports[0].names,
            Some(vec!["sin".to_string(), "cos".to_string()])
        );
    }

    // -- Contract clause error recovery tests --

    #[test]
    fn recovery_malformed_requires_continues_to_body() {
        // The `requires` clause has a syntax error (missing operand after `>`),
        // but the function body should still be parsed.
        let source = r#"module test {
            fn check(x: Int) -> Int
                requires { x > }
            {
                return x
            }
        }"#;
        let output = parse_with_recovery(source);
        // The function should be recovered even though requires failed.
        assert_eq!(output.module.functions.len(), 1);
        assert_eq!(output.module.functions[0].name, "check");
        // The malformed requires clause is omitted.
        assert!(output.module.functions[0].requires.is_empty());
        // There should be at least one error about the contract clause.
        assert!(!output.errors.is_empty(), "expected errors but got none");
        assert!(
            output.errors.iter().any(|e| e.code() == "E0104"),
            "expected E0104 contract clause error, got: {:?}",
            output.errors
        );
    }

    #[test]
    fn recovery_malformed_ensures_continues_to_body() {
        let source = r#"module test {
            fn check(x: Int) -> Int
                ensures { + }
            {
                return x
            }
        }"#;
        let output = parse_with_recovery(source);
        assert_eq!(output.module.functions.len(), 1);
        assert_eq!(output.module.functions[0].name, "check");
        assert!(output.module.functions[0].ensures.is_empty());
        assert!(output.errors.iter().any(|e| e.code() == "E0104"));
    }

    #[test]
    fn recovery_valid_requires_then_malformed_ensures() {
        // First requires is valid; ensures has a syntax error.
        // The valid requires should be kept; ensures is dropped.
        let source = r#"module test {
            fn bounded(x: Int) -> Int
                requires { x > 0 }
                ensures { ??? }
            {
                return x
            }
        }"#;
        let output = parse_with_recovery(source);
        assert_eq!(output.module.functions.len(), 1);
        let func = &output.module.functions[0];
        assert_eq!(func.name, "bounded");
        // Valid requires is preserved.
        assert_eq!(func.requires.len(), 1);
        // Malformed ensures is dropped.
        assert!(func.ensures.is_empty());
        assert!(output.errors.iter().any(|e| e.code() == "E0104"));
    }

    #[test]
    fn recovery_malformed_requires_then_valid_ensures() {
        // requires has error, ensures is valid.
        let source = r#"module test {
            fn bounded(x: Int) -> Int
                requires { > > > }
                ensures { x > 0 }
            {
                return x
            }
        }"#;
        let output = parse_with_recovery(source);
        assert_eq!(output.module.functions.len(), 1);
        let func = &output.module.functions[0];
        assert_eq!(func.name, "bounded");
        assert!(func.requires.is_empty());
        assert_eq!(func.ensures.len(), 1);
        assert!(output.errors.iter().any(|e| e.code() == "E0104"));
    }

    #[test]
    fn recovery_contract_error_still_parses_next_function() {
        // Two functions: first has a malformed contract, second is valid.
        // Both should appear in the parsed module.
        let source = r#"module test {
            fn bad(x: Int) -> Int
                requires { x > }
            {
                return x
            }
            fn good(y: Int) -> Int {
                return y
            }
        }"#;
        let output = parse_with_recovery(source);
        assert_eq!(
            output.module.functions.len(),
            2,
            "expected both functions to be parsed, got: {:?}",
            output
                .module
                .functions
                .iter()
                .map(|f| &f.name)
                .collect::<Vec<_>>()
        );
        assert_eq!(output.module.functions[0].name, "bad");
        assert_eq!(output.module.functions[1].name, "good");
        assert!(!output.errors.is_empty());
    }

    #[test]
    fn recovery_nested_braces_in_malformed_contract() {
        // The malformed clause contains nested braces — recovery must track depth.
        let source = r#"module test {
            fn nested(x: Int) -> Int
                requires { if true { x } }
            {
                return x
            }
        }"#;
        let output = parse_with_recovery(source);
        // The function should still be parsed regardless of nested braces
        // in the contract clause (whether it succeeds or recovers).
        assert_eq!(output.module.functions.len(), 1);
        assert_eq!(output.module.functions[0].name, "nested");
    }

    #[test]
    fn recovery_both_contracts_malformed() {
        let source = r#"module test {
            fn both_bad(x: Int) -> Int
                requires { > }
                ensures { < }
            {
                return x
            }
        }"#;
        let output = parse_with_recovery(source);
        assert_eq!(output.module.functions.len(), 1);
        assert_eq!(output.module.functions[0].name, "both_bad");
        assert!(output.module.functions[0].requires.is_empty());
        assert!(output.module.functions[0].ensures.is_empty());
        // Should have two E0104 errors (one per clause).
        let contract_errors: Vec<_> = output
            .errors
            .iter()
            .filter(|e| e.code() == "E0104")
            .collect();
        assert_eq!(
            contract_errors.len(),
            2,
            "expected 2 contract errors, got {}: {:?}",
            contract_errors.len(),
            contract_errors
        );
    }
}
