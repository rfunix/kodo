//! Type expression parsing for the Kōdo parser.
//!
//! This module handles parsing of type annotations that appear in `let`
//! bindings, function parameters, return types, and struct fields. It
//! supports named types, generic types (`Option<Int>`), function types
//! (`(Int, Int) -> Int`), tuple types (`(Int, String)`), optional
//! shorthand (`T?`), and generic parameter declarations with trait bounds
//! (`<T: Ord + Display>`).

use kodo_ast::{GenericParam, Span, TypeExpr};
use kodo_lexer::TokenKind;

use crate::error::Result;
use crate::Parser;

impl Parser {
    /// Parses a type expression: named types, generic types like `Option<Int>`,
    /// function types like `(Int, Int) -> Int`, and optional shorthand `T?`
    /// (equivalent to `Option<T>`).
    pub(crate) fn parse_type(&mut self) -> Result<TypeExpr> {
        // Check for parenthesized type: function, tuple, or unit.
        if self.check(&TokenKind::LParen) {
            return self.parse_paren_type();
        }
        let name = self.parse_ident()?;
        // Check for `dyn TraitName` — dynamic trait object type.
        if name == "dyn" {
            let trait_name = self.parse_ident()?;
            return Ok(TypeExpr::DynTrait(trait_name));
        }
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

    /// Parses a parenthesized type: function type `(Type, ...) -> RetType`,
    /// tuple type `(Type, Type)`, or unit type `()`.
    fn parse_paren_type(&mut self) -> Result<TypeExpr> {
        self.expect(&TokenKind::LParen)?;
        let mut types = Vec::new();
        let mut has_trailing_comma = false;
        if !self.check(&TokenKind::RParen) {
            types.push(self.parse_type()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                has_trailing_comma = true;
                if self.check(&TokenKind::RParen) {
                    break;
                }
                types.push(self.parse_type()?);
                has_trailing_comma = false;
            }
        }
        self.expect(&TokenKind::RParen)?;
        if self.check(&TokenKind::Arrow) {
            self.advance();
            let ret_type = self.parse_type()?;
            return Ok(TypeExpr::Function(types, Box::new(ret_type)));
        }
        if types.is_empty() {
            return Ok(TypeExpr::Unit);
        }
        if types.len() > 1 || has_trailing_comma {
            return Ok(TypeExpr::Tuple(types));
        }
        Ok(types.into_iter().next().unwrap_or(TypeExpr::Unit))
    }

    /// Parses optional generic type parameters with optional trait bounds:
    /// `<T, U>` or `<T: Ord + Display, U: Clone>`.
    ///
    /// Returns an empty vec if no `<` follows the name.
    ///
    /// # Grammar
    ///
    /// ```text
    /// generic_params  = "<" generic_param ("," generic_param)* ">" ;
    /// generic_param   = IDENT ( ":" IDENT ( "+" IDENT )* )? ;
    /// ```
    pub(crate) fn parse_optional_generic_params(&mut self) -> Result<Vec<GenericParam>> {
        if !self.check(&TokenKind::Lt) {
            return Ok(vec![]);
        }
        self.advance(); // consume '<'
        let mut params = vec![self.parse_generic_param()?];
        while self.check(&TokenKind::Comma) {
            self.advance();
            params.push(self.parse_generic_param()?);
        }
        self.expect(&TokenKind::Gt)?;
        Ok(params)
    }

    /// Parses a single generic parameter with optional trait bounds: `T` or `T: Ord + Display`.
    fn parse_generic_param(&mut self) -> Result<GenericParam> {
        let start = self.peek().map_or(Span::new(0, 0), |t| t.span);
        let name = self.parse_ident()?;
        let mut bounds = Vec::new();
        if self.check(&TokenKind::Colon) {
            self.advance(); // consume ':'
            bounds.push(self.parse_ident()?);
            while self.check(&TokenKind::Plus) {
                self.advance(); // consume '+'
                bounds.push(self.parse_ident()?);
            }
        }
        let end = self.prev_span();
        Ok(GenericParam {
            name,
            bounds,
            span: start.merge(end),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use kodo_ast::TypeExpr;

    #[test]
    fn type_optional_shorthand() {
        let source = r#"module test { fn foo(x: Int?) {} }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let param_ty = &module.functions[0].params[0].ty;
        assert!(matches!(
            param_ty,
            TypeExpr::Optional(inner) if matches!(inner.as_ref(), TypeExpr::Named(n) if n == "Int")
        ));
    }

    #[test]
    fn type_function_type() {
        let source = r#"module test { fn apply(f: (Int) -> Bool) {} }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert!(matches!(
            &module.functions[0].params[0].ty,
            TypeExpr::Function(_, _)
        ));
    }

    #[test]
    fn type_tuple_pair() {
        let source = r#"module test { fn foo(t: (Int, String)) {} }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert!(matches!(
            &module.functions[0].params[0].ty,
            TypeExpr::Tuple(elems) if elems.len() == 2
        ));
    }
}
