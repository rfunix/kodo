# Kōdo Error Index

Every Kōdo compiler error has a unique code for easy reference and machine consumption.

## Error Code Ranges

| Range | Phase | Description |
|-------|-------|-------------|
| E0001–E0099 | Lexer | Tokenization errors |
| E0100–E0199 | Parser | Syntax errors |
| E0200–E0299 | Types | Type checking errors |
| E0300–E0399 | Contracts | Contract verification errors |
| E0400–E0499 | Resolver | Intent resolution errors |
| E0500–E0599 | MIR | Mid-level IR errors |
| E0600–E0699 | Codegen | Code generation errors |
| E0700–E0799 | Stdlib | Standard library errors |
| W0001–W0999 | Warnings | Compiler warnings |

## Lexer Errors (E0001–E0099)

### E0001: Unexpected Character
An unrecognized character was found in the source code.

```
error[E0001]: unexpected character `@`
  --> src/main.ko:3:15
   |
 3 | let x = 42 @ 3
   |               ^ unexpected character
```

### E0002: Unterminated String Literal
A string literal was opened but never closed.

```
error[E0002]: unterminated string literal
  --> src/main.ko:5:10
   |
 5 | let s = "hello
   |          ^ string literal starts here but is never closed
```

## Parser Errors (E0100–E0199)

### E0100: Unexpected Token
A token was found where a different one was expected.

### E0101: Missing Module Declaration
Every `.ko` file must start with a `module` declaration.

### E0102: Missing Meta Block
Modules must include a `meta` block (enforced in semantic analysis).

### E0103: Unexpected End of File
The file ended before a complete construct was parsed.

## Type Errors (E0200–E0299)

### E0200: Type Mismatch
Two types were expected to match but don't.

### E0201: Undefined Type
A type name was used that doesn't exist in scope.

### E0202: Arity Mismatch
A function was called with the wrong number of arguments.

### E0203: Not Callable
A value was called as a function but its type is not a function type.

### E0204: For Loop Non-Integer Range
A `for` loop range bound is not of type `Int`. Both `start` and `end` must be `Int`.

```
error[E0204]: type mismatch: expected `Int`, found `Bool`
  --> src/main.ko:5:18
   |
 5 | for i in true..10 {
   |          ^^^^ expected `Int`, found `Bool`
```

### E0205: Range Type Mismatch
Both operands of a range expression (`..` or `..=`) must be of the same numeric type.

### E0210: Missing Meta Block
The module does not contain a `meta` block. All modules must be self-describing.

### E0211: Empty Purpose
The `purpose` field in the `meta` block is an empty string.

### E0212: Missing Purpose
The `meta` block does not contain a `purpose` field.

### E0221: Wrong Type Argument Count
A generic type was instantiated with the wrong number of type arguments.

### E0223: Missing Type Arguments
A generic type was used without providing required type arguments.

### E0227: Closure Parameter Missing Type Annotation
A closure parameter is missing its type annotation. In Kōdo v1, all closure parameters must have explicit type annotations.

```
error[E0227]: closure parameter `x` is missing a type annotation
  --> src/main.ko:5:20
   |
 5 | let f = |x| { x + 1 }
   |          ^ add a type annotation: `x: Int`
   |
   = help: Kōdo v1 requires explicit types on closure parameters
```

### E0250: Await Outside Async
An `.await` expression was used outside of an `async fn`. The `.await` syntax is only valid inside async functions.

```
error[E0250]: `.await` can only be used inside an `async fn`
  --> src/main.ko:5:30
   |
 5 | let val: Int = compute().await
   |                          ^^^^^ move this expression into an `async fn`
```

### E0251: Spawn Captures Mutable Reference
A `spawn` block captures a mutable reference, which is not allowed in structured concurrency (reserved for future use).

### E0252: Actor Direct Field Access
An actor's field was accessed directly from outside a handler. Actor fields are private to handler methods.

```
error[E0252]: cannot access actor field `count` directly on `Counter`
  --> src/main.ko:10:20
   |
10 | let x = counter.count
   |                 ^^^^^ use a handler method to access `count` instead
```

### E0240: Use After Move
A variable was used after its ownership was transferred (moved). Once a value is moved, it cannot be accessed.

```
error[E0240]: variable `x` was moved at line 5 and cannot be used here
  --> src/main.ko:6:15
   |
 6 |     println(x)
   |             ^ use `ref` to borrow instead of moving
```

### E0241: Borrow Escapes Scope
A borrowed reference cannot escape the scope that created it.

### E0242: Move of Borrowed Value
A value cannot be moved while it is currently borrowed by another variable.

### E0260: Low Confidence Without Review
A function annotated with `@confidence(X)` where X < 0.8 is missing a `@reviewed_by(human: "...")` annotation. Agent-generated code with low confidence must be reviewed by a human.

```
error[E0260]: function `risky_fn` has @confidence(0.5) < 0.8 and is missing `@reviewed_by(human: "...")`
  --> src/main.ko:5:1
   |
 5 | fn risky_fn() {
   | ^^^^^^^^^^^^^^ add `@reviewed_by(human: "reviewer_name")` to function `risky_fn`
```

### E0261: Module Confidence Below Threshold
The computed confidence of a function is below the `min_confidence` threshold declared in the module's `meta` block. Confidence propagates transitively through the call chain.

```
error[E0261]: module confidence 0.50 is below threshold 0.90. Weakest link: fn `weak_link` at @confidence(0.50)
  --> src/main.ko:10:1
   |
10 | fn main() -> Int {
   | ^^^^^^^^^^^^^^^^ increase confidence of `weak_link` or lower `min_confidence`
```

### E0262: Security-Sensitive Without Contract
A function marked `@security_sensitive` has no `requires` or `ensures` clauses. Security-sensitive code must have formal contracts documenting and enforcing security invariants.

```
error[E0262]: function `process_input` is marked `@security_sensitive` but has no `requires` or `ensures` contracts
  --> src/main.ko:8:1
   |
 8 | fn process_input(data: String) -> String {
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ add `requires { ... }` or `ensures { ... }` to function `process_input`
```

## Contract Errors (E0300–E0399)

### E0300: Precondition Unverifiable
A `requires` clause cannot be statically proven.

### E0301: Postcondition Unverifiable
An `ensures` clause cannot be statically proven.

### E0302: Contract Violation
A contract is provably violated by the implementation.

## Resolver Errors (E0400–E0499)

### E0400: No Resolver Found
No resolver strategy matches the declared intent.

### E0401: Intent Contract Violation
The resolved implementation doesn't satisfy the intent's contracts.

### E0402: Unknown Intent Configuration
An unrecognized key was used in an intent block.

### E0403: Intent Config Type Mismatch
A configuration value in an intent block has the wrong type. For example, passing a string where an integer is expected.

```
error[E0403]: intent config type mismatch: expected Int for key `count`, found String
  --> src/main.ko:5:12
   |
 5 |     count: "three"
   |            ^^^^^^^ expected Int
```

## Codegen Errors (E0600–E0699)

### E0600: Indirect Call Failure
An indirect (function pointer) call failed during code generation. This typically occurs when referencing an unknown function.

```
error[E0600]: indirect call failure: function reference to unknown function `missing_fn`
  --> src/main.ko:8:20
   |
 8 |     let result = f(42)
   |                    ^^ could not resolve function pointer
```

## JSON Error Format

All errors can be emitted as JSON with `--json-errors`:

```json
{
  "code": "E0200",
  "severity": "error",
  "message": "type mismatch: expected `Int`, found `String`",
  "span": {
    "file": "src/main.ko",
    "line": 10,
    "column": 5,
    "length": 12
  },
  "suggestion": "convert the String to Int using `Int.parse(value)`",
  "spec_reference": "§3.1 Type System"
}
```
