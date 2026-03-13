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
    /// The `invariant` keyword (module invariant).
    #[token("invariant")]
    Invariant,
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
    /// The `from` keyword (selective imports: `from std::option import Some`).
    #[token("from")]
    From,
    /// The `while` keyword.
    #[token("while")]
    While,
    /// The `for` keyword.
    #[token("for")]
    For,
    /// The `trait` keyword.
    #[token("trait")]
    Trait,
    /// The `impl` keyword.
    #[token("impl")]
    Impl,
    /// The `self` keyword.
    #[token("self")]
    SelfValue,
    /// The `own` keyword — ownership qualifier.
    #[token("own")]
    Own,
    /// The `ref` keyword — borrow qualifier.
    #[token("ref")]
    Ref,
    /// The `is` keyword — type/variant test operator.
    #[token("is")]
    Is,
    /// The `async` keyword.
    #[token("async")]
    Async,
    /// The `await` keyword.
    #[token("await")]
    Await,
    /// The `spawn` keyword.
    #[token("spawn")]
    Spawn,
    /// The `parallel` keyword.
    #[token("parallel")]
    Parallel,
    /// The `actor` keyword.
    #[token("actor")]
    Actor,
    /// The `type` keyword (for type aliases and refinement types).
    #[token("type")]
    Type,
    /// The `pub` keyword — marks a declaration as publicly visible.
    #[token("pub")]
    Pub,
    /// The `break` keyword — exits the innermost loop.
    #[token("break")]
    Break,
    /// The `continue` keyword — skips to the next iteration of the innermost loop.
    #[token("continue")]
    Continue,

    // --- Literals ---
    /// An integer literal.
    #[regex(r"[0-9][0-9_]*", priority = 2, callback = |lex| lex.slice().replace('_', "").parse::<i64>().ok())]
    IntLit(i64),
    /// A float literal (e.g., `0.95`, `3.14`).
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", priority = 3, callback = |lex| lex.slice().replace('_', "").parse::<f64>().ok())]
    FloatLit(f64),
    /// A string literal (double-quoted).
    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())
    })]
    StringLit(String),
    /// An f-string literal for string interpolation: `f"hello {name}!"`.
    ///
    /// The raw content between the quotes is preserved (including `{expr}` markers).
    /// The parser is responsible for splitting this into literal and expression parts.
    #[regex(r#"f"[^"]*""#, lex_fstring)]
    FStringLit(String),

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
    /// `|`
    #[token("|")]
    Pipe,
    /// `!`
    #[token("!")]
    Bang,
    /// `?.` — optional chaining operator (must precede `?` for logos priority).
    #[token("?.")]
    QuestionDot,
    /// `??` — null coalescing operator (must precede `?` for logos priority).
    #[token("??")]
    QuestionQuestion,
    /// `?` — try / error propagation / optional type operator.
    #[token("?")]
    QuestionMark,
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
    /// `..=` — inclusive range operator (must precede `..` and `.` for logos priority).
    #[token("..=")]
    DotDotEq,
    /// `..` — exclusive range operator (must precede `.` for logos priority).
    #[token("..")]
    DotDot,
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

/// Lexer callback for f-string literals.
///
/// Extracts the content between `f"` and `"`, preserving `{expr}` markers
/// for the parser to process. Returns `Option<String>` as required by the
/// logos callback interface.
#[allow(clippy::unnecessary_wraps)]
fn lex_fstring(lex: &mut logos::Lexer<'_, TokenKind>) -> Option<String> {
    let s = lex.slice();
    // Strip leading `f"` and trailing `"`
    Some(s[2..s.len() - 1].to_string())
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

    #[test]
    fn tokenize_for_keyword() {
        let tokens = tokenize("for").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::For);
    }

    #[test]
    fn tokenize_dot_dot() {
        let tokens = tokenize("0..10").unwrap_or_default();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, TokenKind::IntLit(0));
        assert_eq!(tokens[1].kind, TokenKind::DotDot);
        assert_eq!(tokens[2].kind, TokenKind::IntLit(10));
    }

    #[test]
    fn tokenize_dot_dot_eq() {
        let tokens = tokenize("0..=10").unwrap_or_default();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, TokenKind::IntLit(0));
        assert_eq!(tokens[1].kind, TokenKind::DotDotEq);
        assert_eq!(tokens[2].kind, TokenKind::IntLit(10));
    }

    #[test]
    fn tokenize_dot_still_works() {
        let tokens = tokenize("x.y").unwrap_or_default();
        assert_eq!(tokens.len(), 3);
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "x"));
        assert_eq!(tokens[1].kind, TokenKind::Dot);
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "y"));
    }

    #[test]
    fn tokenize_question_mark() {
        let tokens = tokenize("x?").unwrap_or_default();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "x"));
        assert_eq!(tokens[1].kind, TokenKind::QuestionMark);
    }

    #[test]
    fn tokenize_question_dot() {
        let tokens = tokenize("x?.y").unwrap_or_default();
        assert_eq!(tokens.len(), 3);
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "x"));
        assert_eq!(tokens[1].kind, TokenKind::QuestionDot);
        assert!(matches!(tokens[2].kind, TokenKind::Ident(ref s) if s == "y"));
    }

    #[test]
    fn tokenize_question_question() {
        let tokens = tokenize("x ?? 0").unwrap_or_default();
        assert_eq!(tokens.len(), 3);
        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == "x"));
        assert_eq!(tokens[1].kind, TokenKind::QuestionQuestion);
        assert_eq!(tokens[2].kind, TokenKind::IntLit(0));
    }

    #[test]
    fn tokenize_question_operators_priority() {
        // Ensure ?? is lexed as one token, not two ?
        let tokens = tokenize("??").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::QuestionQuestion);

        // Ensure ?. is lexed as one token, not ? then .
        let tokens = tokenize("?.").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::QuestionDot);
    }

    #[test]
    fn tokenize_pipe() {
        let tokens = tokenize("|x|").unwrap_or_default();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, TokenKind::Pipe);
        assert!(matches!(tokens[1].kind, TokenKind::Ident(ref s) if s == "x"));
        assert_eq!(tokens[2].kind, TokenKind::Pipe);
    }

    #[test]
    fn tokenize_pipe_vs_pipepipe() {
        // Single | should be Pipe
        let tokens = tokenize("| x |").unwrap_or_default();
        assert_eq!(tokens[0].kind, TokenKind::Pipe);
        assert_eq!(tokens[2].kind, TokenKind::Pipe);

        // || should still be PipePipe
        let tokens = tokenize("||").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::PipePipe);
    }

    #[test]
    fn tokenize_trait_keyword() {
        let tokens = tokenize("trait").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Trait);
    }

    #[test]
    fn tokenize_impl_keyword() {
        let tokens = tokenize("impl").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Impl);
    }

    #[test]
    fn tokenize_self_keyword() {
        let tokens = tokenize("self").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::SelfValue);
    }

    #[test]
    fn tokenize_trait_impl_self_together() {
        let tokens = tokenize("trait impl self").unwrap_or_default();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![&TokenKind::Trait, &TokenKind::Impl, &TokenKind::SelfValue]
        );
    }

    #[test]
    fn tokenize_async_keyword() {
        let tokens = tokenize("async").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Async);
    }

    #[test]
    fn tokenize_await_keyword() {
        let tokens = tokenize("await").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Await);
    }

    #[test]
    fn tokenize_spawn_keyword() {
        let tokens = tokenize("spawn").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Spawn);
    }

    #[test]
    fn tokenize_actor_keyword() {
        let tokens = tokenize("actor").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Actor);
    }

    #[test]
    fn tokenize_parallel_keyword() {
        let tokens = tokenize("parallel").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Parallel);
    }

    #[test]
    fn tokenize_concurrency_keywords_together() {
        let tokens = tokenize("async await spawn actor parallel").unwrap_or_default();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::Async,
                &TokenKind::Await,
                &TokenKind::Spawn,
                &TokenKind::Actor,
                &TokenKind::Parallel,
            ]
        );
    }

    #[test]
    fn tokenize_own_keyword() {
        let tokens = tokenize("own").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Own);
    }

    #[test]
    fn tokenize_ref_keyword() {
        let tokens = tokenize("ref").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Ref);
    }

    #[test]
    fn tokenize_is_keyword() {
        let tokens = tokenize("is").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Is);
    }

    #[test]
    fn tokenize_ownership_and_is_keywords_together() {
        let tokens = tokenize("own ref is").unwrap_or_default();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![&TokenKind::Own, &TokenKind::Ref, &TokenKind::Is]
        );
    }

    #[test]
    fn tokenize_break_keyword() {
        let tokens = tokenize("break").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Break);
    }

    #[test]
    fn tokenize_continue_keyword() {
        let tokens = tokenize("continue").unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Continue);
    }

    #[test]
    fn tokenize_break_continue_together() {
        let tokens = tokenize("break continue").unwrap_or_default();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(kinds, vec![&TokenKind::Break, &TokenKind::Continue]);
    }

    #[test]
    fn tokenize_break_in_while_context() {
        let tokens = tokenize("while true { break }").unwrap_or_default();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Break)));
    }

    #[test]
    fn tokenize_continue_in_for_context() {
        let tokens = tokenize("for i in 0..10 { continue }").unwrap_or_default();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Continue)));
    }

    #[test]
    fn tokenize_fstring_simple() {
        let tokens = tokenize(r#"f"hello {name}!""#).unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, TokenKind::FStringLit(ref s) if s == "hello {name}!"));
    }

    #[test]
    fn tokenize_fstring_no_interpolation() {
        let tokens = tokenize(r#"f"just text""#).unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, TokenKind::FStringLit(ref s) if s == "just text"));
    }

    #[test]
    fn tokenize_fstring_empty() {
        let tokens = tokenize(r#"f"""#).unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, TokenKind::FStringLit(ref s) if s.is_empty()));
    }

    #[test]
    fn tokenize_fstring_multiple_exprs() {
        let tokens = tokenize(r#"f"{a} and {b}""#).unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, TokenKind::FStringLit(ref s) if s == "{a} and {b}"));
    }

    #[test]
    fn tokenize_fstring_vs_string() {
        // Regular string should still work
        let tokens_str = tokenize(r#""hello""#).unwrap_or_default();
        assert!(matches!(tokens_str[0].kind, TokenKind::StringLit(_)));

        // f-string is different
        let tokens_fstr = tokenize(r#"f"hello""#).unwrap_or_default();
        assert!(matches!(tokens_fstr[0].kind, TokenKind::FStringLit(_)));
    }

    #[test]
    fn tokenize_fstring_with_expr() {
        let tokens = tokenize(r#"f"value: {x + 1}""#).unwrap_or_default();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, TokenKind::FStringLit(ref s) if s == "value: {x + 1}"));
    }

    mod proptest_fuzz {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Any arbitrary string must tokenize without panicking.
            /// The result may be Ok (valid tokens) or Err (unexpected char),
            /// but the lexer must never panic.
            #[test]
            fn no_panics_on_arbitrary_input(s in "\\PC*") {
                let _ = tokenize(&s);
            }

            /// ASCII-only arbitrary input must not panic either.
            #[test]
            fn no_panics_on_ascii_input(s in "[[:ascii:]]{0,256}") {
                let _ = tokenize(&s);
            }

            /// Valid integer literals must tokenize to IntLit with the correct value.
            #[test]
            fn valid_integer_literals(n in 0i64..=999_999_999i64) {
                let source = n.to_string();
                let tokens = tokenize(&source).unwrap();
                assert_eq!(tokens.len(), 1);
                assert_eq!(tokens[0].kind, TokenKind::IntLit(n));
            }

            /// Integers with underscore separators must parse correctly.
            #[test]
            fn integer_with_underscores(n in 1_000i64..=9_999_999i64) {
                // Format with underscores every 3 digits (manually build a simple case)
                let plain = n.to_string();
                let with_underscores = format!("{}_0", &plain[..plain.len()-1]);
                // Just verify no panic; the value may differ due to our formatting
                let result = tokenize(&with_underscores);
                assert!(result.is_ok());
                let tokens = result.unwrap();
                assert!(!tokens.is_empty());
                assert!(matches!(tokens[0].kind, TokenKind::IntLit(_)));
            }

            /// Zero must tokenize correctly.
            #[test]
            fn zero_tokenizes_correctly(_dummy in 0..1u8) {
                let tokens = tokenize("0").unwrap();
                assert_eq!(tokens.len(), 1);
                assert_eq!(tokens[0].kind, TokenKind::IntLit(0));
            }

            /// Float literals with varying decimal parts must tokenize correctly.
            #[test]
            fn valid_float_literals(
                int_part in 0u32..1000u32,
                frac_part in 0u32..1000u32,
            ) {
                let source = format!("{int_part}.{frac_part}");
                let tokens = tokenize(&source).unwrap();
                assert_eq!(tokens.len(), 1);
                assert!(matches!(tokens[0].kind, TokenKind::FloatLit(_)));
                if let TokenKind::FloatLit(v) = tokens[0].kind {
                    let expected: f64 = source.parse().unwrap();
                    assert!((v - expected).abs() < f64::EPSILON);
                }
            }

            /// Empty string literals must tokenize to an empty StringLit.
            #[test]
            fn empty_string_literal(_dummy in 0..1u8) {
                let tokens = tokenize(r#""""#).unwrap();
                assert_eq!(tokens.len(), 1);
                assert!(matches!(tokens[0].kind, TokenKind::StringLit(ref s) if s.is_empty()));
            }

            /// String literals with arbitrary content (no inner quotes) must tokenize.
            #[test]
            fn string_literals_with_content(content in "[^\"]{0,100}") {
                let source = format!("\"{content}\"");
                let tokens = tokenize(&source).unwrap();
                assert_eq!(tokens.len(), 1);
                assert!(matches!(tokens[0].kind, TokenKind::StringLit(ref s) if s == &content));
            }

            /// Strings with special characters (that aren't quotes) must tokenize.
            #[test]
            fn string_with_special_chars(content in "[a-z !@#$%^&*()\\\\/:;,.<>\\[\\]{}]{0,50}") {
                let source = format!("\"{content}\"");
                let result = tokenize(&source);
                // Should not panic; may succeed or fail depending on content
                let _ = result;
            }

            /// Valid identifiers must tokenize to Ident tokens.
            #[test]
            fn valid_identifiers(name in "[a-zA-Z_][a-zA-Z0-9_]{0,30}") {
                let tokens = tokenize(&name).unwrap();
                assert_eq!(tokens.len(), 1);
                // It may be a keyword or an identifier — both are valid
                match &tokens[0].kind {
                    TokenKind::Ident(s) => assert_eq!(s, &name),
                    // Keywords are also valid results for identifier-like strings
                    _ => {} // keyword match is acceptable
                }
            }

            /// Very long identifiers must not cause panics or crashes.
            #[test]
            fn long_identifiers(len in 100usize..500usize) {
                let name: String = std::iter::once('x').chain(std::iter::repeat('a').take(len)).collect();
                let tokens = tokenize(&name).unwrap();
                assert_eq!(tokens.len(), 1);
                assert!(matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == &name));
            }

            /// Identifiers that are prefixes of keywords must still tokenize correctly.
            #[test]
            fn keyword_prefixes_are_identifiers(
                suffix in "[a-z]{1,5}"
            ) {
                // e.g. "letx", "returnfoo", "ify" — should be identifiers, not keywords
                let keywords = ["let", "fn", "if", "else", "return", "module", "meta",
                                "while", "for", "true", "false", "struct", "enum",
                                "match", "import", "from", "trait", "impl", "self",
                                "own", "ref", "is", "async", "await", "spawn", "actor",
                                "parallel", "break", "continue", "pub"];
                for kw in &keywords {
                    let name = format!("{kw}{suffix}");
                    let tokens = tokenize(&name).unwrap();
                    assert_eq!(tokens.len(), 1);
                    assert!(
                        matches!(tokens[0].kind, TokenKind::Ident(ref s) if s == &name),
                        "Expected Ident for '{name}', got {:?}",
                        tokens[0].kind
                    );
                }
            }

            /// All Kōdo keywords must tokenize as their keyword variant, not Ident.
            #[test]
            fn keywords_are_not_identifiers(idx in 0usize..31usize) {
                let keywords = [
                    ("module", TokenKind::Module), ("meta", TokenKind::Meta),
                    ("fn", TokenKind::Fn), ("let", TokenKind::Let),
                    ("mut", TokenKind::Mut), ("if", TokenKind::If),
                    ("else", TokenKind::Else), ("return", TokenKind::Return),
                    ("true", TokenKind::True), ("false", TokenKind::False),
                    ("requires", TokenKind::Requires), ("ensures", TokenKind::Ensures),
                    ("invariant", TokenKind::Invariant), ("intent", TokenKind::Intent), ("struct", TokenKind::Struct),
                    ("enum", TokenKind::Enum), ("match", TokenKind::Match),
                    ("import", TokenKind::Import), ("from", TokenKind::From),
                    ("while", TokenKind::While),
                    ("for", TokenKind::For), ("trait", TokenKind::Trait),
                    ("impl", TokenKind::Impl), ("self", TokenKind::SelfValue),
                    ("own", TokenKind::Own), ("ref", TokenKind::Ref),
                    ("is", TokenKind::Is), ("async", TokenKind::Async),
                    ("parallel", TokenKind::Parallel),
                    ("break", TokenKind::Break), ("continue", TokenKind::Continue),
                    ("pub", TokenKind::Pub),
                ];
                let (src, expected) = &keywords[idx % keywords.len()];
                let tokens = tokenize(src).unwrap();
                assert_eq!(tokens.len(), 1);
                assert_eq!(&tokens[0].kind, expected);
            }

            /// Multiple tokens separated by spaces must produce the correct count.
            #[test]
            fn whitespace_separated_tokens(
                count in 1usize..20usize,
            ) {
                let source: String = std::iter::repeat("42")
                    .take(count)
                    .collect::<Vec<_>>()
                    .join(" ");
                let tokens = tokenize(&source).unwrap();
                assert_eq!(tokens.len(), count);
            }

            /// Sequences of valid operators must not panic.
            #[test]
            fn random_operator_sequences(ops in prop::collection::vec(
                prop::sample::select(vec![
                    "+", "-", "*", "/", "%", "=", "==", "!=",
                    "<", ">", "<=", ">=", "&&", "||", "!", "->",
                    "::", "=>", "(", ")", "{", "}", "[", "]",
                    ",", ":", ";", ".", "..", "..=", "@", "?",
                ]),
                1..20
            )) {
                let source = ops.join(" ");
                let result = tokenize(&source);
                // Operators are all valid tokens, so this should succeed
                assert!(result.is_ok());
            }
        }
    }
}
