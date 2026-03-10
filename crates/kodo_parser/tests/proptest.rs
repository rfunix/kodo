//! Property-based tests for the Kōdo parser.
//!
//! Uses `proptest` to verify invariants that must hold for all inputs.

use proptest::prelude::*;

proptest! {
    /// Any string must not cause a panic in the parser.
    /// It may return an error, but it must never panic.
    #[test]
    fn parser_never_panics(input in "\\PC*") {
        let _ = kodo_parser::parse(&input);
    }

    /// A well-formed module skeleton always parses successfully.
    #[test]
    fn valid_module_skeleton_parses(
        name in "[a-z][a-z0-9_]{0,10}".prop_filter(
            "must not be a keyword",
            |n| !matches!(n.as_str(),
                "module" | "meta" | "fn" | "let" | "mut" | "if" | "else" |
                "return" | "true" | "false" | "requires" | "ensures" |
                "intent" | "struct" | "enum" | "match" | "import" | "while" |
                "for" | "trait" | "impl" | "self" |
                "own" | "ref" | "is" | "async" | "await" | "spawn" | "actor"
            ),
        ),
        purpose in "[a-zA-Z0-9 ]{1,30}",
        version in "[0-9]{1,2}\\.[0-9]{1,2}\\.[0-9]{1,2}",
        author in "[a-zA-Z ]{1,20}"
    ) {
        let source = format!(
            "module {name} {{\n    meta {{\n        purpose: \"{purpose}\",\n        version: \"{version}\",\n        author: \"{author}\"\n    }}\n\n    fn main() {{\n    }}\n}}"
        );
        let result = kodo_parser::parse(&source);
        assert!(
            result.is_ok(),
            "failed to parse valid module skeleton: {result:?}\nsource:\n{source}"
        );
        let module = result.unwrap();
        assert_eq!(module.name, name);
    }
}
