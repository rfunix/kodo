//! Pattern parsing for the Kōdo parser.
//!
//! Patterns appear in `match` arms, `if let` statements, and `let` tuple
//! destructuring. Supported pattern forms include wildcard (`_`), literal
//! patterns, tuple patterns (`(a, b)`), and enum variant patterns
//! (`Option::Some(v)`).

use kodo_ast::{Pattern, Span};
use kodo_lexer::TokenKind;

use crate::error::Result;
use crate::Parser;

impl Parser {
    /// Parses a pattern in a match arm or `if let` / `let` destructuring.
    pub(crate) fn parse_pattern(&mut self) -> Result<Pattern> {
        // Tuple pattern: `(a, b, c)`
        if self.check(&TokenKind::LParen) {
            let start = self.advance().map_or(Span::new(0, 0), |t| t.span);
            let mut patterns = Vec::new();
            if !self.check(&TokenKind::RParen) {
                patterns.push(self.parse_pattern()?);
                while self.check(&TokenKind::Comma) {
                    self.advance();
                    if self.check(&TokenKind::RParen) {
                        break;
                    }
                    patterns.push(self.parse_pattern()?);
                }
            }
            let end = self.expect(&TokenKind::RParen)?.span;
            return Ok(Pattern::Tuple(patterns, start.merge(end)));
        }

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
                    bindings.push(self.parse_binding_pattern()?);
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
            // Variant without enum prefix: `Ok(v)`, `Err(e)`, `Some(v)`, or
            // a unit variant like `None`.
            let mut bindings = Vec::new();
            if self.check(&TokenKind::LParen) {
                self.advance();
                while !self.check(&TokenKind::RParen) {
                    if !bindings.is_empty() {
                        self.expect(&TokenKind::Comma)?;
                    }
                    bindings.push(self.parse_binding_pattern()?);
                }
                self.expect(&TokenKind::RParen)?;
            }
            let end = self.prev_span();
            Ok(Pattern::Variant {
                enum_name: None,
                variant: first_name,
                bindings,
                span: start_span.merge(end),
            })
        }
    }

    /// Parses a single binding position inside a variant pattern's payload.
    ///
    /// This allows either a simple variable name (`Ok(v)`) or a nested
    /// variant pattern (`Err(AppError::NotFound)`).  Wildcard (`_`) is
    /// also accepted as a binding.
    fn parse_binding_pattern(&mut self) -> Result<Pattern> {
        // Peek ahead: if we see `Ident ::` it is a nested variant pattern.
        if let Some(TokenKind::Ident(name)) = self.peek_kind().cloned() {
            if name != "_" {
                // Look two tokens ahead for `::`.
                if self.peek_nth_kind(1) == Some(&TokenKind::ColonColon) {
                    return self.parse_pattern();
                }
            }
        }

        // Otherwise parse as a simple binding or wildcard.
        let span = self.peek().map_or(Span::new(0, 0), |t| t.span);
        let name = self.parse_ident()?;
        if name == "_" {
            Ok(Pattern::Wildcard(span))
        } else {
            Ok(Pattern::Binding(name, span))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use kodo_ast::{Expr, Pattern, Stmt};

    #[test]
    fn pattern_wildcard_in_match() {
        let source = r#"module test {
            fn main() {
                let r = match x { _ => 0 }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } = &stmts[0]
        {
            assert!(matches!(&arms[0].pattern, Pattern::Wildcard(_)));
        } else {
            panic!("expected Let with Match");
        }
    }

    #[test]
    fn pattern_variant_without_prefix_with_bindings() {
        let source = r#"module test {
            fn main() {
                let r = match x {
                    Ok(v) => v,
                    Err(e) => 0
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } = &stmts[0]
        {
            if let Pattern::Variant {
                enum_name: None,
                variant,
                bindings,
                ..
            } = &arms[0].pattern
            {
                assert_eq!(variant, "Ok");
                assert_eq!(bindings.len(), 1);
                assert!(matches!(&bindings[0], Pattern::Binding(n, _) if n == "v"));
            } else {
                panic!("expected Ok variant pattern");
            }
            if let Pattern::Variant {
                enum_name: None,
                variant,
                bindings,
                ..
            } = &arms[1].pattern
            {
                assert_eq!(variant, "Err");
                assert_eq!(bindings.len(), 1);
                assert!(matches!(&bindings[0], Pattern::Binding(n, _) if n == "e"));
            } else {
                panic!("expected Err variant pattern");
            }
        } else {
            panic!("expected Let with Match");
        }
    }

    #[test]
    fn pattern_variant_without_prefix_unit() {
        let source = r#"module test {
            fn main() {
                let r = match x {
                    Some(v) => v,
                    None => 0
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } = &stmts[0]
        {
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant {
                    enum_name: None,
                    variant,
                    bindings,
                    ..
                } if variant == "Some" && bindings.len() == 1
            ));
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant {
                    enum_name: None,
                    variant,
                    bindings,
                    ..
                } if variant == "None" && bindings.is_empty()
            ));
        } else {
            panic!("expected Let with Match");
        }
    }

    #[test]
    fn pattern_variant_with_bindings() {
        let source = r#"module test {
            fn main() {
                let r = match x {
                    Option::Some(v) => v,
                    Option::None => 0
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } = &stmts[0]
        {
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant {
                    enum_name: Some(name),
                    variant,
                    bindings,
                    ..
                } if name == "Option" && variant == "Some" && bindings.len() == 1
            ));
        } else {
            panic!("expected Let with Match");
        }
    }

    #[test]
    fn pattern_nested_variant_in_binding() {
        let source = r#"module test {
            fn main() {
                let r = match x {
                    Ok(v) => v,
                    Err(AppError::NotFound) => 0
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } = &stmts[0]
        {
            // First arm: Ok(v) — simple binding
            if let Pattern::Variant {
                variant, bindings, ..
            } = &arms[0].pattern
            {
                assert_eq!(variant, "Ok");
                assert!(matches!(&bindings[0], Pattern::Binding(n, _) if n == "v"));
            } else {
                panic!("expected Ok arm");
            }
            // Second arm: Err(AppError::NotFound) — nested variant pattern
            if let Pattern::Variant {
                variant, bindings, ..
            } = &arms[1].pattern
            {
                assert_eq!(variant, "Err");
                assert_eq!(bindings.len(), 1);
                assert!(matches!(
                    &bindings[0],
                    Pattern::Variant { enum_name: Some(en), variant: v, bindings: inner, .. }
                    if en == "AppError" && v == "NotFound" && inner.is_empty()
                ));
            } else {
                panic!("expected Err arm");
            }
        } else {
            panic!("expected Let with Match");
        }
    }

    #[test]
    fn pattern_nested_variant_with_inner_binding() {
        let source = r#"module test {
            fn main() {
                let r = match x {
                    Err(AppError::InvalidInput(msg)) => msg,
                    _ => 0
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        if let Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } = &stmts[0]
        {
            if let Pattern::Variant {
                variant, bindings, ..
            } = &arms[0].pattern
            {
                assert_eq!(variant, "Err");
                if let Pattern::Variant {
                    enum_name: Some(en),
                    variant: inner_v,
                    bindings: inner_bindings,
                    ..
                } = &bindings[0]
                {
                    assert_eq!(en, "AppError");
                    assert_eq!(inner_v, "InvalidInput");
                    assert!(matches!(&inner_bindings[0], Pattern::Binding(n, _) if n == "msg"));
                } else {
                    panic!("expected nested AppError::InvalidInput pattern");
                }
            } else {
                panic!("expected Err arm");
            }
        } else {
            panic!("expected Let with Match");
        }
    }
}
