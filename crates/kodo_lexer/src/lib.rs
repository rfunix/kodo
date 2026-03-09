//! # `kodo_lexer` — Tokenizer for the Kōdo Language
//!
//! This crate converts raw source text into a stream of tokens using the
//! [`logos`] lexer generator. It is the first phase of the compiler pipeline.
//!
//! Kōdo's lexer is designed to produce clear, unambiguous tokens that AI agents
//! can reliably parse and reason about, while preserving full source location
//! information for human-readable error messages.
//!
//! ## Usage
//!
//! ```
//! use kodo_lexer::tokenize;
//!
//! let tokens = tokenize("let x: Int = 42");
//! assert!(tokens.is_ok());
//! ```
//!
//! ## Academic References
//!
//! - **\[CI\]** *Crafting Interpreters* Ch. 4 — Scanner architecture, token
//!   representation, and error reporting strategy.
//! - **\[EC\]** *Engineering a Compiler* Ch. 2 — DFA-based scanning, maximal munch
//!   rule for multi-character operators (`->`, `==`, `!=`).
//! - **\[PLP\]** *Programming Language Pragmatics* Ch. 2 — Regular expressions,
//!   finite automata, and the classification of lexical elements.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use kodo_ast::Span;
use logos::Logos;
use thiserror::Error;

/// Errors that can occur during lexing.
#[derive(Debug, Error)]
pub enum LexError {
    /// An unexpected character was encountered in the source.
    #[error("unexpected character at {span:?}")]
    UnexpectedChar {
        /// Location of the unexpected character.
        span: Span,
    },
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, LexError>;

/// The kinds of tokens recognized by the Kōdo lexer.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r]+")]
pub enum TokenKind {
    // --- Keywords ---
    /// The `module` keyword.
    #[token("module")]
    Module,
    /// The `meta` keyword.
    #[token("meta")]
    Meta,
    /// The `fn` keyword.
    #[token("fn")]
    Fn,
    /// The `let` keyword.
    #[token("let")]
    Let,
    /// The `mut` keyword.
    #[token("mut")]
    Mut,
    /// The `if` keyword.
    #[token("if")]
    If,
    /// The `else` keyword.
    #[token("else")]
    Else,
    /// The `return` keyword.
    #[token("return")]
    Return,
    /// The `true` keyword.
    #[token("true")]
    True,
    /// The `false` keyword.
    #[token("false")]
    False,
    /// The `requires` keyword (precondition contract).
    #[token("requires")]
    Requires,
    /// The `ensures` keyword (postcondition contract).
    #[token("ensures")]
    Ensures,
    /// The `intent` keyword.
    #[token("intent")]
    Intent,
    /// The `struct` keyword.
    #[token("struct")]
    Struct,
    /// The `enum` keyword.
    #[token("enum")]
    Enum,
    /// The `match` keyword.
    #[token("match")]
    Match,
    /// The `import` keyword.
    #[token("import")]
    Import,
    /// The `while` keyword.
    #[token("while")]
    While,

    // --- Literals ---
    /// An integer literal.
    #[regex(r"[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<i64>().ok())]
    IntLit(i64),
    /// A string literal (double-quoted).
    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())
    })]
    StringLit(String),

    // --- Identifiers ---
    /// An identifier.
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", priority = 1, callback = |lex| lex.slice().to_string())]
    Ident(String),

    // --- Operators ---
    /// `+`
    #[token("+")]
    Plus,
    /// `-`
    #[token("-")]
    Minus,
    /// `*`
    #[token("*")]
    Star,
    /// `/`
    #[token("/")]
    Slash,
    /// `%`
    #[token("%")]
    Percent,
    /// `=`
    #[token("=")]
    Eq,
    /// `==`
    #[token("==")]
    EqEq,
    /// `!=`
    #[token("!=")]
    BangEq,
    /// `<`
    #[token("<")]
    Lt,
    /// `>`
    #[token(">")]
    Gt,
    /// `<=`
    #[token("<=")]
    LtEq,
    /// `>=`
    #[token(">=")]
    GtEq,
    /// `&&`
    #[token("&&")]
    AmpAmp,
    /// `||`
    #[token("||")]
    PipePipe,
    /// `!`
    #[token("!")]
    Bang,
    /// `->`
    #[token("->")]
    Arrow,
    /// `::`
    #[token("::")]
    ColonColon,
    /// `=>`
    #[token("=>")]
    FatArrow,

    // --- Delimiters ---
    /// `(`
    #[token("(")]
    LParen,
    /// `)`
    #[token(")")]
    RParen,
    /// `{`
    #[token("{")]
    LBrace,
    /// `}`
    #[token("}")]
    RBrace,
    /// `[`
    #[token("[")]
    LBracket,
    /// `]`
    #[token("]")]
    RBracket,

    // --- Punctuation ---
    /// `,`
    #[token(",")]
    Comma,
    /// `:`
    #[token(":")]
    Colon,
    /// `;`
    #[token(";")]
    Semicolon,
    /// `.`
    #[token(".")]
    Dot,

    // --- Annotations ---
    /// `@` — annotation prefix.
    #[token("@")]
    At,

    // --- Whitespace & Comments ---
    /// A newline character.
    #[token("\n")]
    Newline,
    /// A line comment starting with `//`.
    #[regex(r"//[^\n]*", allow_greedy = true)]
    LineComment,
}

/// A token with its kind and source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The kind of token.
    pub kind: TokenKind,
    /// The source span of this token.
    pub span: Span,
}

/// Tokenizes the given source text into a vector of tokens.
///
/// Skips newlines and comments from the output. Returns an error
/// if any unexpected character is encountered.
///
/// # Errors
///
/// Returns [`LexError::UnexpectedChar`] if the source contains characters
/// that cannot be recognized as any valid token.
pub fn tokenize(source: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut lexer = TokenKind::lexer(source);

    while let Some(result) = lexer.next() {
        let span = lexer.span();
        let kodo_span = Span::new(
            u32::try_from(span.start).unwrap_or(u32::MAX),
            u32::try_from(span.end).unwrap_or(u32::MAX),
        );

        match result {
            Ok(kind) => {
                // Skip whitespace tokens (newlines and comments)
                if matches!(kind, TokenKind::Newline | TokenKind::LineComment) {
                    continue;
                }
                tokens.push(Token {
                    kind,
                    span: kodo_span,
                });
            }
            Err(()) => {
                return Err(LexError::UnexpectedChar { span: kodo_span });
            }
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_let_binding() {
        let tokens = tokenize("let x: Int = 42").unwrap_or_default();
        assert_eq!(tokens.len(), 6);
        assert_eq!(tokens[0].kind, TokenKind::Let);
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "x"));
        assert_eq!(tokens[2].kind, TokenKind::Colon);
        assert!(matches!(tokens[3].kind, TokenKind::Ident(ref s) if s == "Int"));
        assert_eq!(tokens[4].kind, TokenKind::Eq);
        assert_eq!(tokens[5].kind, TokenKind::IntLit(42));
    }

    #[test]
    fn tokenize_keywords() {
        let tokens = tokenize("module meta fn let if else return").unwrap_or_default();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::Module,
                &TokenKind::Meta,
                &TokenKind::Fn,
                &TokenKind::Let,
                &TokenKind::If,
                &TokenKind::Else,
                &TokenKind::Return,
            ]
        );
    }

    #[test]
    fn tokenize_string_literal() {
        let tokens = tokenize(r#""hello world""#).unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, TokenKind::StringLit(ref s) if s == "hello world"));
    }

    #[test]
    fn tokenize_skips_comments() {
        let tokens = tokenize("let x = 1 // this is a comment\nlet y = 2").unwrap_or_default();
        // Should have tokens for both let statements, but no comment token
        assert!(tokens
            .iter()
            .all(|t| !matches!(t.kind, TokenKind::LineComment)));
    }

    #[test]
    fn tokenize_unexpected_char_returns_error() {
        let result = tokenize("let x = ~");
        assert!(result.is_err());
    }

    #[test]
    fn tokenize_at_sign() {
        let tokens = tokenize("@confidence").unwrap_or_default();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::At);
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "confidence"));
    }

    #[test]
    fn tokenize_empty_source() {
        let tokens = tokenize("").unwrap_or_default();
        assert!(tokens.is_empty());
    }

    #[test]
    fn tokenize_integer_with_underscores() {
        let tokens = tokenize("1_000_000").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::IntLit(1_000_000));
    }

    #[test]
    fn tokenize_while_keyword() {
        let tokens = tokenize("while").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::While);
    }

    #[test]
    fn tokenize_operators() {
        let tokens = tokenize("== != <= >= -> && ||").unwrap_or_default();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::EqEq,
                &TokenKind::BangEq,
                &TokenKind::LtEq,
                &TokenKind::GtEq,
                &TokenKind::Arrow,
                &TokenKind::AmpAmp,
                &TokenKind::PipePipe,
            ]
        );
    }
}
