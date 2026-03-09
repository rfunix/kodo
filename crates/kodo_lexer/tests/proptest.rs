//! Property-based tests for the Kōdo lexer.
//!
//! Uses `proptest` to verify invariants that must hold for all inputs.

use proptest::prelude::*;

proptest! {
    /// Any valid UTF-8 string must not cause a panic in the lexer.
    /// It may return an error, but it must never panic.
    #[test]
    fn lexer_never_panics(input in "\\PC*") {
        let _ = kodo_lexer::tokenize(&input);
    }

    /// Valid integer strings always produce an IntLit token.
    #[test]
    fn valid_integers_produce_int_token(n in 0i64..1_000_000) {
        let source = n.to_string();
        let tokens = kodo_lexer::tokenize(&source).unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(
            matches!(tokens[0].kind, kodo_lexer::TokenKind::IntLit(v) if v == n),
            "expected IntLit({n}), got {:?}",
            tokens[0].kind
        );
    }

    /// String literals roundtrip: wrapping text in quotes produces a StringLit.
    #[test]
    fn string_literals_roundtrip(content in "[a-zA-Z0-9 _]{0,50}") {
        let source = format!("\"{content}\"");
        let tokens = kodo_lexer::tokenize(&source).unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(
            matches!(&tokens[0].kind, kodo_lexer::TokenKind::StringLit(s) if s == &content),
            "expected StringLit(\"{content}\"), got {:?}",
            tokens[0].kind
        );
    }
}
