//! # Error Code Explanations
//!
//! Provides detailed explanations for each error code, with example code
//! showing what triggers the error and how to fix it.
//!
//! Used by the `kodoc explain` subcommand to help both AI agents and humans
//! understand and resolve compiler errors.

/// An explanation entry for an error code.
pub struct ExplanationEntry {
    /// The error code.
    pub code: &'static str,
    /// Short description.
    pub title: &'static str,
    /// Detailed explanation.
    pub explanation: &'static str,
    /// Example code that triggers this error.
    pub bad_example: &'static str,
    /// Fixed version of the example.
    pub good_example: &'static str,
}

/// Returns the explanation for a given error code, if one exists.
pub fn get_explanation(code: &str) -> Option<&'static ExplanationEntry> {
    EXPLANATIONS.iter().find(|e| e.code == code)
}

static EXPLANATIONS: &[ExplanationEntry] = &[
    ExplanationEntry {
        code: "E0001",
        title: "Unexpected Character",
        explanation: "The lexer encountered a character that is not part of any valid token. \
            Kōdo only supports a specific set of operators and punctuation. Check for \
            stray characters or unsupported symbols.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let x: Int = 42 ~ 10
        return x
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let x: Int = 42 + 10
        return x
    }
}"#,
    },
    ExplanationEntry {
        code: "E0100",
        title: "Unexpected Token",
        explanation: "The parser encountered a token that was not expected at this position. \
            This usually indicates a syntax error such as a missing delimiter, an extra \
            comma, or a misplaced keyword.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int
        return 0
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        return 0
    }
}"#,
    },
    ExplanationEntry {
        code: "E0101",
        title: "Unexpected End of File",
        explanation: "The parser reached the end of the source file while still expecting \
            more tokens. This usually means a block or expression was not properly closed \
            with a matching brace or parenthesis.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        return 0
    }"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        return 0
    }
}"#,
    },
    ExplanationEntry {
        code: "E0200",
        title: "Type Mismatch",
        explanation: "An expression produced a type that does not match what was expected. \
            Kōdo has no implicit conversions — all types must match exactly. For example, \
            you cannot assign a String to an Int variable or return a Bool from a function \
            declared to return Int.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let x: Int = "hello"
        return x
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let x: Int = 42
        return x
    }
}"#,
    },
    ExplanationEntry {
        code: "E0201",
        title: "Undefined Variable",
        explanation: "A name was used that has not been defined in the current scope. \
            Check for typos or ensure the variable is declared with `let` before use. \
            Variables must be defined before they can be referenced.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        return x
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let x: Int = 42
        return x
    }
}"#,
    },
    ExplanationEntry {
        code: "E0202",
        title: "Arity Mismatch",
        explanation: "A function was called with the wrong number of arguments. \
            Check the function signature and provide exactly the number of arguments \
            it expects.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
    fn main() -> Int {
        return add(1)
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
    fn main() -> Int {
        return add(1, 2)
    }
}"#,
    },
    ExplanationEntry {
        code: "E0203",
        title: "Not Callable",
        explanation: "A value was used as a function but its type is not a function type. \
            Only function-typed values can be called with parenthesized arguments.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let x: Int = 42
        return x(1)
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let x: Int = 42
        return x + 1
    }
}"#,
    },
    ExplanationEntry {
        code: "E0210",
        title: "Missing Meta Block",
        explanation: "Every Kōdo module must have a `meta` block with at least a `purpose` \
            field. The meta block makes modules self-describing for AI agents, enabling \
            automated reasoning about code intent.",
        bad_example: r#"module example {
    fn main() -> Int {
        return 0
    }
}"#,
        good_example: r#"module example {
    meta {
        purpose: "Example module"
        version: "0.1.0"
    }
    fn main() -> Int {
        return 0
    }
}"#,
    },
    ExplanationEntry {
        code: "E0211",
        title: "Empty Purpose",
        explanation: "The `purpose` field in the meta block must contain a non-empty \
            description of what the module does. An empty purpose defeats the goal of \
            self-describing modules.",
        bad_example: r#"module example {
    meta {
        purpose: ""
    }
    fn main() -> Int {
        return 0
    }
}"#,
        good_example: r#"module example {
    meta {
        purpose: "A meaningful description of the module"
    }
    fn main() -> Int {
        return 0
    }
}"#,
    },
    ExplanationEntry {
        code: "E0212",
        title: "Missing Purpose Field",
        explanation: "The meta block exists but is missing the required `purpose` field. \
            Every meta block must include `purpose: \"description\"` to describe the \
            module's intent.",
        bad_example: r#"module example {
    meta {
        version: "0.1.0"
    }
    fn main() -> Int {
        return 0
    }
}"#,
        good_example: r#"module example {
    meta {
        purpose: "Example module"
        version: "0.1.0"
    }
    fn main() -> Int {
        return 0
    }
}"#,
    },
    ExplanationEntry {
        code: "E0213",
        title: "Unknown Struct",
        explanation: "A struct type was referenced but has not been defined in the current \
            module or any imported module. Check the spelling or define the struct.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let p: Point = Point { x: 1, y: 2 }
        return 0
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    struct Point { x: Int, y: Int }
    fn main() -> Int {
        let p: Point = Point { x: 1, y: 2 }
        return 0
    }
}"#,
    },
];
