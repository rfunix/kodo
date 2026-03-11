//! Statement parsing for the Kōdo parser.
//!
//! Statements are the building blocks of function bodies. This module
//! handles `let` bindings (including tuple destructuring), `return`,
//! `while`, `for`/`for-in` loops, `if let`, `spawn`, `parallel`,
//! assignment, and expression statements.

use kodo_ast::{Expr, Span, Stmt};
use kodo_lexer::TokenKind;

use crate::error::{ParseError, Result};
use crate::Parser;

impl Parser {
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

        // Check for tuple destructuring: `let (a, b) = expr`
        if self.check(&TokenKind::LParen) {
            let pattern = self.parse_pattern()?;

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

            return Ok(Stmt::LetPattern {
                span: start.merge(end),
                mutable,
                pattern,
                ty,
                value,
            });
        }

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

    /// Parses a for loop: either a range-based `for <ident> in <expr>..<expr> { <body> }`
    /// or a collection-based `for <ident> in <expr> { <body> }`.
    ///
    /// The parser distinguishes the two forms by checking whether the expression
    /// after `in` is a range (`Expr::Range`) or any other expression. If a range
    /// is found, we produce `Stmt::For`; otherwise, `Stmt::ForIn`.
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

        // Parse the expression after `in`. If it's a range, produce Stmt::For;
        // otherwise, produce Stmt::ForIn for collection iteration.
        let iter_expr = self.parse_expr()?;
        match iter_expr {
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
            iterable => {
                let body = self.parse_block()?;
                let end_span = body.span;
                Ok(Stmt::ForIn {
                    span: start.merge(end_span),
                    name,
                    iterable,
                    body,
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
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use kodo_ast::{Expr, Stmt};

    #[test]
    fn stmt_let_with_type_inference() {
        let source = r#"module test { fn main() { let x = 42 } }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Let { name, ty, .. } => {
                assert_eq!(name, "x");
                assert!(ty.is_none());
            }
            other => panic!("expected Let, got {other:?}"),
        }
    }

    #[test]
    fn stmt_for_in_method_call_iterable() {
        let source = r#"module test {
            fn main() {
                for x in obj.iter() {
                    print_int(x)
                }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::ForIn { iterable, .. } => {
                assert!(matches!(iterable, Expr::Call { .. }));
            }
            other => panic!("expected ForIn, got {other:?}"),
        }
    }

    #[test]
    fn stmt_spawn_with_body() {
        let source = r#"module test {
            fn main() {
                spawn { let x: Int = 1 }
            }
        }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        assert!(matches!(&stmts[0], Stmt::Spawn { .. }));
    }
}
