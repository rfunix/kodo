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
