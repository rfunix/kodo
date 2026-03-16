//! Declaration parsing for the Kōdo parser.
//!
//! This module handles parsing of all top-level declarations that can
//! appear inside a module: functions (with annotations), structs, enums,
//! trait declarations, impl blocks, intent declarations, actor declarations,
//! and type aliases.

use kodo_ast::{
    ActorDecl, Annotation, AnnotationArg, AssociatedType, EnumDecl, EnumVariant, FieldDef,
    Function, ImplBlock, IntentConfigEntry, IntentConfigValue, IntentDecl, InvariantDecl,
    Ownership, Param, Span, TestDecl, TraitDecl, TraitMethod, TypeAlias, TypeDecl, TypeExpr,
    Visibility,
};
use kodo_lexer::TokenKind;

use crate::error::{ParseError, Result};
use crate::Parser;

impl Parser {
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
    use crate::parse;
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
}
