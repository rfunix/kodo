//! Expression parsing for the Kōdo parser.
//!
//! This module implements all expression-related parsing methods using
//! recursive descent with one method per precedence level. Precedence
//! levels (lowest to highest):
//!
//! 1. Range: `..`, `..=`
//! 2. Null coalesce: `??`
//! 3. Logical or: `||`
//! 4. Logical and: `&&`
//! 5. Equality: `==`, `!=`
//! 6. Comparison: `<`, `>`, `<=`, `>=`
//! 7. Additive: `+`, `-`
//! 8. Multiplicative: `*`, `/`, `%`
//! 9. Unary: `!`, `-`
//! 10. Postfix: calls, field access, `?`, `.await`, `is`
//! 11. Primary: literals, identifiers, `if`, blocks, parens, closures
//!
//! See `docs/grammar.ebnf` for the formal grammar and **\[CI\]** Ch. 6-8
//! for the recursive descent approach.

use kodo_ast::{BinOp, Block, ClosureParam, Expr, Span, Stmt, StringPart, UnaryOp};
use kodo_lexer::TokenKind;

use crate::error::{ParseError, Result};
use crate::Parser;

impl Parser {
    /// Parses an expression starting from the lowest precedence level.
    ///
    /// This is the top-level expression entry point. It dispatches to
    /// `parse_coalesce_expr`, which is the lowest-precedence binary operator,
    /// and then checks for range operators.
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
    pub(crate) fn parse_coalesce_expr(&mut self) -> Result<Expr> {
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
    /// Call suffix: `(arg_list?)`, field suffix: `.IDENT`, try: `?`,
    /// optional chain: `?.IDENT`, await: `.await`, tuple index: `.0`,
    /// type test: `is TypeName`.
    pub(crate) fn parse_postfix_expr(&mut self) -> Result<Expr> {
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
                } else if let Some(TokenKind::IntLit(n)) = self.peek_kind().cloned() {
                    // Tuple index: expr.0, expr.1, etc.
                    let end = self.advance().map_or(Span::new(0, 0), |t| t.span);
                    let span = Self::expr_span(&expr).merge(end);
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let index = n as usize;
                    expr = Expr::TupleIndex {
                        tuple: Box::new(expr),
                        index,
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
    pub(crate) fn parse_primary_expr(&mut self) -> Result<Expr> {
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
            Some(TokenKind::FStringLit(raw)) => {
                let span = self.advance().map_or(Span::new(0, 0), |t| t.span);
                let parts = Self::parse_fstring_parts(&raw, span)?;
                Ok(Expr::StringInterp { parts, span })
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
            Some(TokenKind::LParen) => self.parse_paren_or_tuple_expr(),
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

    /// Parses a parenthesized expression, which may be a grouping `(expr)`,
    /// a tuple literal `(a, b)`, or an empty unit `()`.
    fn parse_paren_or_tuple_expr(&mut self) -> Result<Expr> {
        let start = self.advance().map_or(Span::new(0, 0), |t| t.span);
        // Check for empty tuple: `()`
        if self.check(&TokenKind::RParen) {
            let end = self.advance().map_or(Span::new(0, 0), |t| t.span);
            return Ok(Expr::Block(Block {
                span: start.merge(end),
                stmts: vec![],
            }));
        }
        let first = self.parse_expr()?;
        // If comma follows, it's a tuple literal.
        if self.check(&TokenKind::Comma) {
            let mut elements = vec![first];
            while self.check(&TokenKind::Comma) {
                self.advance();
                if self.check(&TokenKind::RParen) {
                    break;
                }
                elements.push(self.parse_expr()?);
            }
            let end = self.expect(&TokenKind::RParen)?.span;
            Ok(Expr::TupleLit(elements, start.merge(end)))
        } else {
            // Grouping: `(expr)`
            self.expect(&TokenKind::RParen)?;
            Ok(first)
        }
    }

    /// Parses an if expression: `if expr block (else (if_expr | block))?`.
    pub(crate) fn parse_if_expr(&mut self) -> Result<Expr> {
        let start = self.expect(&TokenKind::If)?.span;
        let condition = self.parse_expr()?;
        let then_branch = self.parse_block()?;

        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                // else if -- wrap in a block with a single if-expr statement
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
            arms.push(kodo_ast::MatchArm {
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

    /// Parses a struct literal after the name has been consumed:
    /// `{ field: expr, ... }`
    pub(crate) fn parse_struct_literal(&mut self, name: String, start_span: Span) -> Result<Expr> {
        self.expect(&TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let field_start = self.peek().map_or(Span::new(0, 0), |t| t.span);
            let field_name = self.parse_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            let field_end = Self::expr_span(&value);
            fields.push(kodo_ast::FieldInit {
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

    /// Parses the raw content of an f-string into [`StringPart`] segments.
    ///
    /// Splits on `{` and `}` delimiters. Text outside braces becomes
    /// [`StringPart::Literal`], and text inside braces is parsed as an
    /// expression using a sub-parser.
    pub(crate) fn parse_fstring_parts(raw: &str, span: Span) -> Result<Vec<StringPart>> {
        let mut parts = Vec::new();
        let mut chars = raw.chars().peekable();
        let mut buf = String::new();

        while let Some(&ch) = chars.peek() {
            if ch == '{' {
                // Flush any accumulated literal text
                if !buf.is_empty() {
                    parts.push(StringPart::Literal(std::mem::take(&mut buf)));
                }
                chars.next(); // consume '{'
                let mut expr_str = String::new();
                let mut depth = 1u32;
                for c in chars.by_ref() {
                    if c == '{' {
                        depth += 1;
                        expr_str.push(c);
                    } else if c == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        expr_str.push(c);
                    } else {
                        expr_str.push(c);
                    }
                }
                if depth != 0 {
                    return Err(ParseError::UnexpectedEof {
                        expected: "closing `}` in f-string interpolation".to_string(),
                    });
                }
                // Parse the expression text using a sub-parser
                let tokens = kodo_lexer::tokenize(&expr_str).map_err(ParseError::LexError)?;
                let mut sub_parser = Parser::new(tokens);
                let expr = sub_parser.parse_expr()?;
                parts.push(StringPart::Expr(Box::new(expr)));
            } else {
                buf.push(ch);
                chars.next();
            }
        }

        // Flush any trailing literal text
        if !buf.is_empty() {
            parts.push(StringPart::Literal(buf));
        }

        // If there are no parts at all (empty f-string), produce a single empty literal
        if parts.is_empty() {
            parts.push(StringPart::Literal(String::new()));
        }

        // Suppress unused variable warning -- span is used for error context
        let _ = span;

        Ok(parts)
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

    /// Returns `true` if the current token could start an expression.
    pub(crate) fn is_at_expr_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(
                TokenKind::IntLit(_)
                    | TokenKind::FloatLit(_)
                    | TokenKind::StringLit(_)
                    | TokenKind::FStringLit(_)
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

    /// Checks if the current position looks like a struct literal start:
    /// `{ Ident : ...` (as opposed to a block `{ stmt; ... }`)
    pub(crate) fn is_struct_literal_start(&self) -> bool {
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

    /// Returns the span of an expression.
    pub(crate) fn expr_span(expr: &Expr) -> Span {
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
            | Expr::Await { span, .. }
            | Expr::StringInterp { span, .. }
            | Expr::TupleLit(_, span)
            | Expr::TupleIndex { span, .. } => *span,
            Expr::Block(block) => block.span,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use kodo_ast::{BinOp, Expr, Stmt, UnaryOp};

    #[test]
    fn expr_precedence_add_mul() {
        // a + b * c should parse as a + (b * c)
        let source = r#"module test { fn main() { a + b * c } }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Expr(Expr::BinaryOp {
                op: BinOp::Add,
                right,
                ..
            }) => {
                assert!(matches!(
                    right.as_ref(),
                    Expr::BinaryOp { op: BinOp::Mul, .. }
                ));
            }
            other => panic!("expected Add at top, got {other:?}"),
        }
    }

    #[test]
    fn expr_unary_neg_with_add() {
        let source = r#"module test { fn main() { -a + b } }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Expr(Expr::BinaryOp {
                op: BinOp::Add,
                left,
                ..
            }) => {
                assert!(matches!(
                    left.as_ref(),
                    Expr::UnaryOp {
                        op: UnaryOp::Neg,
                        ..
                    }
                ));
            }
            other => panic!("expected Add with Neg on left, got {other:?}"),
        }
    }

    #[test]
    fn expr_closure_in_call_arg() {
        let source = r#"module test { fn main() { map(|x: Int| x + 1) } }"#;
        let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let stmts = &module.functions[0].body.stmts;
        match &stmts[0] {
            Stmt::Expr(Expr::Call { args, .. }) => {
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0], Expr::Closure { .. }));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }
}
