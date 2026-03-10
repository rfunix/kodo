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
    // ═══════════════════════════════════════════════════════════════════
    // Lexer Errors (E0001–E0099)
    // ═══════════════════════════════════════════════════════════════════
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
        code: "E0002",
        title: "Unterminated String Literal",
        explanation: "A string literal was opened with a double quote but never closed. \
            Every string must have a matching closing quote on the same line. Check for \
            missing closing quotes.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        let s: String = "hello
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        let s: String = "hello"
    }
}"#,
    },
    // ═══════════════════════════════════════════════════════════════════
    // Parser Errors (E0100–E0199)
    // ═══════════════════════════════════════════════════════════════════
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
        code: "E0102",
        title: "Missing Module Declaration",
        explanation: "Every Kōdo source file must start with a `module` declaration. \
            The module name serves as the compilation unit identity.",
        bad_example: r#"fn main() -> Int {
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
        code: "E0103",
        title: "Missing Meta Block",
        explanation: "A module was parsed but does not contain a `meta` block. \
            All Kōdo modules must include metadata for self-description.",
        bad_example: r#"module example {
    fn main() -> Int {
        return 0
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "Example module" }
    fn main() -> Int {
        return 0
    }
}"#,
    },
    // ═══════════════════════════════════════════════════════════════════
    // Type Errors (E0200–E0299)
    // ═══════════════════════════════════════════════════════════════════
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
        code: "E0204",
        title: "For Loop Non-Integer Range",
        explanation: "A `for` loop range bound is not of type `Int`. Both `start` and \
            `end` expressions in `for i in start..end` must evaluate to `Int`.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        for i in true..10 {
            print_int(i)
        }
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        for i in 0..10 {
            print_int(i)
        }
    }
}"#,
    },
    ExplanationEntry {
        code: "E0205",
        title: "Range Type Mismatch",
        explanation: "Both operands of a range expression (`..` or `..=`) must be of \
            the same numeric type. Mixing types in ranges is not allowed.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        for i in 0..true {
            print_int(i)
        }
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        for i in 0..10 {
            print_int(i)
        }
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
    ExplanationEntry {
        code: "E0221",
        title: "Wrong Type Argument Count",
        explanation: "A generic type was instantiated with the wrong number of type arguments. \
            Check the definition of the generic type and provide the correct number of type parameters.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        let x: Option = Option::None
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        let x: Option<Int> = Option::None
    }
}"#,
    },
    ExplanationEntry {
        code: "E0223",
        title: "Missing Type Arguments",
        explanation: "A generic type was used without providing required type arguments. \
            Generic types must be instantiated with specific types, e.g. `List<Int>` instead of `List`.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn identity(x: List) -> List {
        return x
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn identity(x: List<Int>) -> List<Int> {
        return x
    }
}"#,
    },
    ExplanationEntry {
        code: "E0227",
        title: "Closure Parameter Missing Type",
        explanation: "A closure parameter is missing its type annotation. In Kōdo v1, \
            all closure parameters must have explicit type annotations to keep code \
            unambiguous for AI agents.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let f = |x| { x + 1 }
        return f(41)
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let f = |x: Int| -> Int { x + 1 }
        return f(41)
    }
}"#,
    },
    ExplanationEntry {
        code: "E0250",
        title: "Await Outside Async",
        explanation: "The `.await` expression can only be used inside an `async fn`. \
            Regular (synchronous) functions cannot await asynchronous operations.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() {
        let val: Int = compute().await
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    async fn run() {
        let val: Int = compute().await
    }
}"#,
    },
    ExplanationEntry {
        code: "E0251",
        title: "Spawn Captures Mutable Reference",
        explanation: "A `spawn` block attempted to capture a mutable reference, which \
            is not allowed in Kōdo's structured concurrency model. Spawn blocks must \
            only capture immutable data.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    async fn run() {
        let mut x: Int = 0
        spawn { x = 1 }
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    async fn run() {
        let x: Int = 0
        spawn { print_int(x) }
    }
}"#,
    },
    ExplanationEntry {
        code: "E0252",
        title: "Actor Direct Field Access",
        explanation: "Actor fields cannot be accessed directly from outside the actor. \
            Use handler methods to interact with actor state. This ensures safe \
            concurrent access.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    actor Counter { count: Int }
    fn main() {
        let c: Counter = Counter { count: 0 }
        let x: Int = c.count
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    actor Counter {
        count: Int
        fn get_count(self) -> Int { return self.count }
    }
}"#,
    },
    ExplanationEntry {
        code: "E0260",
        title: "Low Confidence Without Review",
        explanation: "A function annotated with `@confidence(X)` where X < 0.8 is \
            missing a `@reviewed_by(human: \"...\")` annotation. Agent-generated code \
            with low confidence must be reviewed by a human before it can be compiled.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    @confidence(0.5)
    fn risky() -> Int {
        return 42
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    @confidence(0.5)
    @reviewed_by(human: "alice")
    fn risky() -> Int {
        return 42
    }
}"#,
    },
    ExplanationEntry {
        code: "E0261",
        title: "Module Confidence Below Threshold",
        explanation: "The computed confidence of the module is below the `min_confidence` \
            threshold declared in the `meta` block. Confidence propagates transitively: \
            the module's confidence is limited by the weakest function in the call chain. \
            Review and increase confidence of low-confidence functions.",
        bad_example: r#"module example {
    meta {
        purpose: "test"
        min_confidence: "0.9"
    }
    @confidence(0.5)
    @reviewed_by(human: "alice")
    fn weak_link() -> Int { return 1 }
    fn main() -> Int { return weak_link() }
}"#,
        good_example: r#"module example {
    meta {
        purpose: "test"
        min_confidence: "0.9"
    }
    @confidence(0.95)
    fn strong() -> Int { return 1 }
    fn main() -> Int { return strong() }
}"#,
    },
    ExplanationEntry {
        code: "E0262",
        title: "Security-Sensitive Without Contract",
        explanation: "A function marked `@security_sensitive` has no `requires` or \
            `ensures` clauses. Security-sensitive code must have formal contracts \
            documenting and enforcing security invariants.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    @security_sensitive
    fn process(data: String) -> String {
        return data
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    @security_sensitive
    fn process(data: String) -> String
        requires { data != "" }
    {
        return data
    }
}"#,
    },
    // ═══════════════════════════════════════════════════════════════════
    // Contract Errors (E0300–E0399)
    // ═══════════════════════════════════════════════════════════════════
    ExplanationEntry {
        code: "E0300",
        title: "Precondition Unverifiable",
        explanation: "A `requires` clause cannot be statically proven by the SMT solver. \
            This does not mean the contract is wrong — it means the solver cannot verify \
            it at compile time. Consider simplifying the precondition or using runtime mode.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn complex(x: Int) -> Int
        requires { x * x + 2 * x + 1 > 0 }
    {
        return x
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn simple(x: Int) -> Int
        requires { x > 0 }
    {
        return x
    }
}"#,
    },
    ExplanationEntry {
        code: "E0301",
        title: "Postcondition Unverifiable",
        explanation: "An `ensures` clause cannot be statically proven by the SMT solver. \
            The implementation may or may not satisfy the postcondition. Consider \
            simplifying or using runtime verification mode.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn compute(x: Int) -> Int
        ensures { result > 0 }
    {
        return x - 1
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn compute(x: Int) -> Int
        requires { x > 1 }
        ensures { result > 0 }
    {
        return x - 1
    }
}"#,
    },
    ExplanationEntry {
        code: "E0302",
        title: "Contract Violation",
        explanation: "A contract is provably violated by the implementation. The SMT \
            solver found a counter-example showing the precondition or postcondition \
            does not hold.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn positive() -> Int
        ensures { result > 0 }
    {
        return 0
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn positive() -> Int
        ensures { result > 0 }
    {
        return 1
    }
}"#,
    },
    // ═══════════════════════════════════════════════════════════════════
    // Resolver Errors (E0400–E0499)
    // ═══════════════════════════════════════════════════════════════════
    ExplanationEntry {
        code: "E0400",
        title: "No Resolver Found",
        explanation: "No resolver strategy matches the declared intent. Check the intent \
            name — built-in resolvers include `console_app` and `math_module`. Custom \
            resolvers can be registered via plugins.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    intent unknown_intent {
        key: "value"
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    intent console_app {
        greeting: "Hello!"
    }
}"#,
    },
    ExplanationEntry {
        code: "E0401",
        title: "Intent Contract Violation",
        explanation: "The resolved implementation does not satisfy the intent's contracts. \
            The intent resolver generated code that violates the declared constraints.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    intent console_app {
        greeting: ""
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    intent console_app {
        greeting: "Hello from Kōdo!"
    }
}"#,
    },
    ExplanationEntry {
        code: "E0402",
        title: "Unknown Intent Configuration",
        explanation: "An unrecognized key was used in an intent configuration block. \
            Check the documentation for the intent resolver to see which keys are supported.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    intent console_app {
        unknown_key: "value"
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    intent console_app {
        greeting: "Hello!"
    }
}"#,
    },
    ExplanationEntry {
        code: "E0403",
        title: "Intent Config Type Mismatch",
        explanation: "A configuration value in an intent block has the wrong type. For \
            example, passing a string where an integer is expected.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    intent math_module {
        functions: "not_a_list"
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    intent math_module {
        functions: [add, subtract]
    }
}"#,
    },
    // ═══════════════════════════════════════════════════════════════════
    // Codegen Errors (E0600–E0699)
    // ═══════════════════════════════════════════════════════════════════
    ExplanationEntry {
        code: "E0600",
        title: "Indirect Call Failure",
        explanation: "An indirect (function pointer) call failed during code generation. \
            This typically occurs when referencing a function that cannot be resolved at \
            compile time.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn main() -> Int {
        let f: (Int) -> Int = unknown_fn
        return f(42)
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn double(x: Int) -> Int { return x * 2 }
    fn main() -> Int {
        let f: (Int) -> Int = double
        return f(42)
    }
}"#,
    },
    // ═══════════════════════════════════════════════════════════════════
    // Ownership Errors (E0240–E0249)
    // ═══════════════════════════════════════════════════════════════════
    ExplanationEntry {
        code: "E0240",
        title: "Use After Move",
        explanation: "A variable was used after its ownership was transferred (moved) to \
            another binding or function call. Once a value is moved, it can no longer be \
            accessed. Use `ref` to borrow instead of transferring ownership.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn consume(own x: String) { }
    fn main() {
        let s: String = "hello"
        consume(s)
        println(s)
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn borrow(ref x: String) { }
    fn main() {
        let s: String = "hello"
        borrow(s)
        println(s)
    }
}"#,
    },
    ExplanationEntry {
        code: "E0241",
        title: "Borrow Escapes Scope",
        explanation: "A borrowed reference (`ref`) cannot escape the scope that created it. \
            The original value might be deallocated when the scope ends, leaving a dangling \
            reference.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn escape() -> ref String {
        let s: String = "hello"
        return s
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn no_escape() -> String {
        let s: String = "hello"
        return s
    }
}"#,
    },
    ExplanationEntry {
        code: "E0242",
        title: "Move of Borrowed Value",
        explanation: "A value cannot be moved while it is currently borrowed. The borrow \
            must end (go out of scope) before the value can be moved.",
        bad_example: r#"module example {
    meta { purpose: "test" }
    fn consume(own x: String) { }
    fn main() {
        let s: String = "hello"
        let r: ref String = s
        consume(s)
    }
}"#,
        good_example: r#"module example {
    meta { purpose: "test" }
    fn consume(own x: String) { }
    fn main() {
        let s: String = "hello"
        consume(s)
    }
}"#,
    },
];
