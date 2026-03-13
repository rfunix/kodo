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

```rust
error[E0001]: unexpected character `@`
  --> src/main.ko:3:15
   |
 3 | let x = 42 @ 3
   |               ^ unexpected character
```

### E0002: Unterminated String Literal
A string literal was opened but never closed.

```rust
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

```rust
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

### E0213: Unknown Struct
A struct type was referenced but has not been defined in the current module or any imported module.

```rust
error[E0213]: unknown struct `Point` at 5:20
  --> src/main.ko:5:20
   |
 5 |     let p: Point = Point { x: 1, y: 2 }
   |                    ^^^^^ struct not defined
```

### E0214: Missing Struct Field
A required field is missing from a struct literal.

```rust
error[E0214]: missing field `y` in struct `Point` at 5:20
  --> src/main.ko:5:20
   |
 5 |     let p: Point = Point { x: 1 }
   |                    ^^^^^^^^^^^^^^ missing field `y`
```

### E0215: Extra Struct Field
An unknown field was provided in a struct literal.

```rust
error[E0215]: unknown field `z` in struct `Point` at 5:20
  --> src/main.ko:5:20
   |
 5 |     let p: Point = Point { x: 1, y: 2, z: 3 }
   |                                         ^ unknown field
```

### E0216: Duplicate Struct Field
A field was specified more than once in a struct literal.

```rust
error[E0216]: duplicate field `x` in struct `Point` at 5:20
  --> src/main.ko:5:20
   |
 5 |     let p: Point = Point { x: 1, x: 2 }
   |                                   ^ duplicate field
```

### E0217: No Such Field
A field access was attempted on a non-existent field.

```rust
error[E0217]: no field `z` on type `Point` at 6:20
  --> src/main.ko:6:20
   |
 6 |     let val: Int = p.z
   |                      ^ field does not exist
```

### E0218: Unknown Enum
An enum type was referenced but has not been defined.

```rust
error[E0218]: unknown enum `Color` at 5:20
  --> src/main.ko:5:20
   |
 5 |     let c: Color = Color::Red
   |                    ^^^^^ enum not defined
```

### E0219: Unknown Variant
A variant was referenced that does not exist in the enum.

```rust
error[E0219]: unknown variant `Purple` in enum `Color` at 5:20
  --> src/main.ko:5:20
   |
 5 |     let c: Color = Color::Purple
   |                          ^^^^^^^ variant does not exist
```

### E0220: Non-Exhaustive Match
A match expression does not cover all variants of an enum.

```rust
error[E0220]: non-exhaustive match on `Color`: missing variants ["Blue"] at 6:5
  --> src/main.ko:6:5
   |
 6 |     match c {
   |     ^^^^^ add missing arm: `Color::Blue => { ... }`
```

### E0221: Wrong Type Argument Count
A generic type was instantiated with the wrong number of type arguments.

### E0222: Undefined Type Parameter
A type parameter was referenced but not defined.

```rust
error[E0222]: undefined type parameter `U` at 5:30
  --> src/main.ko:5:30
   |
 5 |     fn identity<T>(x: U) -> T {
   |                        ^ type parameter `U` not in scope
```

### E0223: Missing Type Arguments
A generic type was used without providing required type arguments.

### E0224: Try in Non-Result Function
The try operator `?` was used in a function that does not return `Result`.

```rust
error[E0224]: operator `?` can only be used in functions returning Result at 6:25
  --> src/main.ko:6:25
   |
 6 |     let val: Int = risky()?
   |                           ^ function must return Result to use `?`
```

### E0225: Optional Chain on Non-Option
Optional chaining `?.` was used on a non-Option type.

```rust
error[E0225]: optional chaining `?.` requires Option type, found `Int` at 6:20
  --> src/main.ko:6:20
   |
 6 |     let val: Int = x?.value
   |                     ^^ `x` is `Int`, not `Option<T>`
```

### E0226: Coalesce Type Mismatch
Null coalescing `??` was used on a non-Option type.

```rust
error[E0226]: null coalescing type mismatch: left must be Option, found `Int` at 6:20
  --> src/main.ko:6:20
   |
 6 |     let val: Int = x ?? 0
   |                    ^ left side must be `Option<T>`
```

### E0227: Closure Parameter Missing Type Annotation
A closure parameter is missing its type annotation. In Kōdo v1, all closure parameters must have explicit type annotations.

```rust
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

```rust
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

```rust
error[E0252]: cannot access actor field `count` directly on `Counter`
  --> src/main.ko:10:20
   |
10 | let x = counter.count
   |                 ^^^^^ use a handler method to access `count` instead
```

### E0230: Unknown Trait
A trait was referenced but has not been defined.

```rust
error[E0230]: unknown trait `Printable` at 5:10
  --> src/main.ko:5:10
   |
 5 | impl Printable for Point {
   |      ^^^^^^^^^ trait not defined
```

### E0231: Missing Trait Method
A required method from a trait is missing in an impl block.

```rust
error[E0231]: missing trait method `to_string` for trait `Printable` at 5:1
  --> src/main.ko:5:1
   |
 5 | impl Printable for Point {
   | ^^^^^^^^^^^^^^^^^^^^^^^^ add missing method: `fn to_string(self) -> String`
```

### E0232: Trait Bound Not Satisfied
A concrete type does not satisfy a trait bound required by a generic parameter.
This implements bounded quantification (System F<:) — when a generic parameter
declares trait bounds like `T: Ord + Display`, any concrete type argument must
implement all specified traits.

```rust
error[E0232]: type `Int` does not implement trait `Hashable` required by generic parameter `T` at 10:12
  --> src/main.ko:10:12
   |
10 |     lookup(42)
   |            ^^ type `Int` does not implement trait `Hashable`
   |
   = help: implement trait `Hashable` for type `Int`, or use a type that already implements it
```

### E0233: Missing Associated Type

An `impl` block for a trait is missing a required associated type binding. Every
associated type declared in the trait must be bound to a concrete type via
`type Name = ConcreteType` in the impl block.

```rust
error[E0233]: missing associated type `Item` for trait `Iterator` at 8:1
  --> src/main.ko:8:1
   |
 8 | impl Iterator for MyList {
   | ^^^^^^^^^^^^^^^^^^^^^^^^^ missing `type Item = ...`
   |
   = help: add `type Item = <concrete type>` inside the impl block
```

### E0234: Unexpected Associated Type

An `impl` block provides a `type` binding for a name that is not declared as an
associated type in the trait.

```rust
error[E0234]: unexpected associated type `Output` for trait `Iterator` at 8:1
  --> src/main.ko:8:1
   |
 8 | impl Iterator for MyList {
   | ^^^^^^^^^^^^^^^^^^^^^^^^^ `Output` is not an associated type of `Iterator`
   |
   = help: remove the `type Output = ...` binding, or check the trait definition
```

### E0235: Method Not Found
A method was called on a type that does not have it.

```rust
error[E0235]: no method `length` on type `Int` at 6:20
  --> src/main.ko:6:20
   |
 6 |     let n: Int = x.length()
   |                    ^^^^^^ method does not exist on `Int`
```

### E0240: Use After Move
A variable was used after its ownership was transferred (moved). Once a value is moved, it cannot be accessed.

```rust
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

### E0243: Break Outside of Loop
A `break` statement was used outside of a `while`, `for`, or `for-in` loop.

```rust
error[E0243]: `break` outside of loop
  --> src/main.ko:5:5
   |
 5 |     break
   |     ^^^^^ `break` can only be used inside `while`, `for`, or `for-in` loops
```

### E0244: Continue Outside of Loop
A `continue` statement was used outside of a `while`, `for`, or `for-in` loop.

```rust
error[E0244]: `continue` outside of loop
  --> src/main.ko:5:5
   |
 5 |     continue
   |     ^^^^^^^^ `continue` can only be used inside `while`, `for`, or `for-in` loops
```

### E0245: Mut Borrow While Ref Borrowed
A mutable borrow (`mut`) was attempted on a variable that already has an active immutable borrow (`ref`). A `mut` borrow is exclusive and cannot coexist with any other borrow.

```rust
error[E0245]: cannot borrow `x` as mutable while it is immutably borrowed
  --> src/main.ko:6:20
   |
 6 |     two_args(ref x, mut x)
   |                     ^^^^^ `x` is already borrowed as `ref`
```

### E0246: Ref Borrow While Mut Borrowed
An immutable borrow (`ref`) was attempted on a variable that already has an active mutable borrow (`mut`). A `mut` borrow is exclusive.

```rust
error[E0246]: cannot borrow `x` as `ref` while it is mutably borrowed
  --> src/main.ko:6:25
   |
 6 |     two_args(mut x, ref x)
   |                     ^^^^^ `x` is already borrowed as `mut`
```

### E0247: Double Mut Borrow
A variable was mutably borrowed (`mut`) twice simultaneously. Only one `mut` borrow is allowed at a time.

```rust
error[E0247]: cannot borrow `x` as mutable more than once
  --> src/main.ko:6:25
   |
 6 |     two_args(mut x, mut x)
   |                     ^^^^^ `x` is already borrowed as `mut`
```

### E0248: Assign Through Ref
An assignment was attempted on a variable that is immutably borrowed (`ref`). Immutable borrows do not allow mutation.

```rust
error[E0248]: cannot assign to `x` because it is borrowed as `ref`
  --> src/main.ko:6:5
   |
 6 |     x = 42
   |     ^^^^^^ `x` is an immutable reference
```

### E0253: Tuple Index Out of Bounds
A tuple index exceeds the number of elements in the tuple type.

```rust
error[E0253]: tuple index 3 is out of bounds for tuple of length 2
  --> src/main.ko:6:20
   |
 6 |     let x = pair.3
   |                  ^ tuple has 2 elements, valid indices are 0..1
```

### E0270: Private Access

A private function or type from another module was accessed. Only `pub` items can be used across module boundaries.

```rust
error[E0270]: `secret` is private to module `utils` and cannot be accessed from here
  --> src/main.ko:8:15
   |
 8 |     let x = secret()
   |             ^^^^^^ `secret` is private to module `utils`
   |
   = help: add `pub` to the declaration: `pub fn secret(...)`
```

### E0280: Spawn Captures Non-Send Type
A `spawn` block captures a variable with a borrowed reference (`ref`), which cannot be safely sent to another thread. Only owned values can cross thread boundaries.

```rust
error[E0280]: type `ref Int` cannot be sent between threads
  --> src/main.ko:8:20
   |
 8 |     spawn {
   |            ^ variable `x` of type `ref Int` is captured but is not Send
   |
   = help: use owned values (own) instead of references when sending data to spawned tasks
```

### E0260: Low Confidence Without Review
A function annotated with `@confidence(X)` where X < 0.8 is missing a `@reviewed_by(human: "...")` annotation. Agent-generated code with low confidence must be reviewed by a human.

```rust
error[E0260]: function `risky_fn` has @confidence(0.5) < 0.8 and is missing `@reviewed_by(human: "...")`
  --> src/main.ko:5:1
   |
 5 | fn risky_fn() {
   | ^^^^^^^^^^^^^^ add `@reviewed_by(human: "reviewer_name")` to function `risky_fn`
```

### E0261: Module Confidence Below Threshold
The computed confidence of a function is below the `min_confidence` threshold declared in the module's `meta` block. Confidence propagates transitively through the call chain.

```rust
error[E0261]: module confidence 0.50 is below threshold 0.90. Weakest link: fn `weak_link` at @confidence(0.50)
  --> src/main.ko:10:1
   |
10 | fn main() -> Int {
   | ^^^^^^^^^^^^^^^^ increase confidence of `weak_link` or lower `min_confidence`
```

### E0262: Security-Sensitive Without Contract
A function marked `@security_sensitive` has no `requires` or `ensures` clauses. Security-sensitive code must have formal contracts documenting and enforcing security invariants.

```rust
error[E0262]: function `process_input` is marked `@security_sensitive` but has no `requires` or `ensures` contracts
  --> src/main.ko:8:1
   |
 8 | fn process_input(data: String) -> String {
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ add `requires { ... }` or `ensures { ... }` to function `process_input`
```

### E0350: Policy Violation
A trust policy violation was detected. This occurs when module-level constraints are not met.

## Contract Errors (E0300–E0399)

### E0300: Precondition Unverifiable
A `requires` clause cannot be statically proven.

### E0301: Postcondition Unverifiable
An `ensures` clause cannot be statically proven.

### E0302: Contract Violation
A contract is provably violated by the implementation.

### E0303: Contract Statically Refuted
The Z3 SMT solver found a counter-example disproving the contract. This occurs
when using `--contracts=static` or `--contracts=both`.

```rust
error[E0303]: contract refuted at 10..16: counter-example: b -> 0
  --> src/main.ko:3:9
   |
 3 |     requires { b != 0 }
   |              ^^^^^^^^^^ Z3 found counter-example: b = 0
```

### E0310: Invariant Not Bool
A module invariant condition does not evaluate to `Bool`. Invariant conditions
must be boolean expressions.

```rust
error[E0310]: invariant condition must be `Bool`, found `Int` at 42..50
  --> src/main.ko:4:5
   |
 4 |     invariant { 42 }
   |                 ^^ expected Bool, found Int
```

## Resolver Errors (E0400–E0499)

### E0400: No Resolver Found
No resolver strategy matches the declared intent.

### E0401: Intent Contract Violation
The resolved implementation doesn't satisfy the intent's contracts.

### E0402: Unknown Intent Configuration
An unrecognized key was used in an intent block.

### E0403: Intent Config Type Mismatch
A configuration value in an intent block has the wrong type. For example, passing a string where an integer is expected.

```rust
error[E0403]: intent config type mismatch: expected Int for key `count`, found String
  --> src/main.ko:5:12
   |
 5 |     count: "three"
   |            ^^^^^^^ expected Int
```

## Codegen Errors (E0600–E0699)

### E0600: Indirect Call Failure
An indirect (function pointer) call failed during code generation. This typically occurs when referencing an unknown function.

```rust
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
  "fix_patch": {
    "description": "wrap value in Int.parse()",
    "start_offset": 120,
    "end_offset": 132,
    "replacement": "Int.parse(value)"
  },
  "repair_plan": [
    {
      "description": "wrap value in Result::Ok()",
      "patches": [{ "start_offset": 120, "end_offset": 132, "replacement": "Result::Ok(value)" }]
    }
  ],
  "spec_reference": "§3.1 Type System"
}
```

The `fix_patch` field contains a single machine-applicable patch. For complex errors requiring
multiple steps, `repair_plan` provides a sequence of named steps that agents should apply in order.
Both fields are optional and omitted when not applicable.
