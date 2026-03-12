# Language Basics

This guide covers the core features of Kōdo that are available today.

## Module Structure

Every `.ko` file contains exactly one module. A module has a name, a `meta` block, and one or more functions:

```rust
module my_program {
    meta {
        purpose: "What this module does",
        version: "0.1.0",
        author: "Your Name"
    }

    fn main() {
        println("Hello!")
    }
}
```

The `meta` block is mandatory. It makes every module self-describing — any reader (human or AI) can immediately understand the module's purpose.

### Meta Fields

| Field | Description |
|-------|-------------|
| `purpose` | What this module does (required) |
| `version` | Semantic version string |
| `author` | Who wrote it |

## Functions

Functions are declared with `fn`, followed by a name, parameters, and an optional return type:

```rust
fn add(a: Int, b: Int) -> Int {
    return a + b
}
```

- Parameters must have explicit type annotations
- Return type is declared with `->` after the parameter list
- Functions without a return type return nothing
- The `main` function is the program's entry point

### Calling Functions

```rust
fn double(x: Int) -> Int {
    return x * 2
}

fn main() {
    let result: Int = double(21)
    print_int(result)
}
```

### Recursion

Functions can call themselves:

```rust
fn factorial(n: Int) -> Int {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}
```

## Types

Kōdo supports the following primitive types:

| Type | Description | Example |
|------|-------------|---------|
| `Int` | 64-bit integer | `42`, `-7`, `0` |
| `Float64` | 64-bit floating point | `3.14`, `-0.5`, `1.0` |
| `Bool` | Boolean values | `true`, `false` |
| `String` | String literals | `"hello"` |

The full type system also includes `Int8`, `Int16`, `Int32`, `Int64`, `Float32`, and `Byte` — see the [Language Specification](../DESIGN.md) for details.

Variables can have explicit type annotations or use **local type inference**:

```rust
// Explicit type annotations
let x: Int = 42
let pi: Float64 = 3.14159
let name: String = "Kōdo"
let active: Bool = true

// Type inference — the compiler infers the type from the initializer
let y = 42           // inferred as Int
let greeting = "hi"  // inferred as String
let flag = true      // inferred as Bool
let tau = 6.28       // inferred as Float64
```

> **Note:** Function signatures always require explicit type annotations. Type inference is local to `let` bindings only.

## Variables

### Immutable Variables

By default, variables are immutable:

```rust
let x: Int = 10
// x = 20  — this would be an error
```

### Mutable Variables

Use `let mut` to create a mutable variable. Mutable variables can be reassigned using `=`:

```rust
let mut counter: Int = 0
counter = counter + 1
```

Reassignment requires the variable to have been declared with `mut`. The new value must match the variable's type. Only simple variable names can be reassigned — field assignment is not yet supported.

## Operators

### Arithmetic

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition (Int, Float64, or String concatenation) | `a + b`, `"hi" + name` |
| `-` | Subtraction | `a - b` |
| `*` | Multiplication | `a * b` |
| `/` | Division | `a / b` |
| `%` | Modulo | `a % b` |
| `-` | Negation (unary) | `-x` |

Arithmetic operators work on both `Int` and `Float64` values. The `+` operator also supports String concatenation — `"hello " + "world"` produces `"hello world"`.

### Comparison

| Operator | Description | Example |
|----------|-------------|---------|
| `==` | Equal | `a == b` |
| `!=` | Not equal | `a != b` |
| `<` | Less than | `a < b` |
| `>` | Greater than | `a > b` |
| `<=` | Less or equal | `a <= b` |
| `>=` | Greater or equal | `a >= b` |

### Logical

| Operator | Description | Example |
|----------|-------------|---------|
| `&&` | Logical AND | `a && b` |
| `\|\|` | Logical OR | `a \|\| b` |
| `!` | Logical NOT | `!a` |

## Control Flow

### if/else

```rust
if x > 0 {
    println("positive")
} else {
    println("non-positive")
}
```

`if`/`else` blocks can be nested:

```rust
if x > 100 {
    println("large")
} else {
    if x > 0 {
        println("small positive")
    } else {
        println("non-positive")
    }
}
```

### while

Use `while` to repeat a block while a condition is true:

```rust
let mut i: Int = 5
while i > 0 {
    print_int(i)
    i = i - 1
}
```

The condition must be a `Bool` expression. The loop body executes repeatedly until the condition becomes `false`.

Here's a complete example with a countdown function:

```rust
fn countdown(n: Int) {
    let mut i: Int = n
    while i > 0 {
        print_int(i)
        i = i - 1
    }
    println("Liftoff!")
}
```

### return

Use `return` to exit a function with a value:

```rust
fn abs(x: Int) -> Int {
    if x < 0 {
        return -x
    }
    return x
}
```

## Builtin Functions

Kōdo provides builtin functions for output:

| Function | Parameter | Description |
|----------|-----------|-------------|
| `println(s)` | `String` | Print a string followed by a newline |
| `print(s)` | `String` | Print a string without a newline |
| `print_int(n)` | `Int` | Print an integer followed by a newline |
| `print_float(f)` | `Float64` | Print a float without a newline |
| `println_float(f)` | `Float64` | Print a float followed by a newline |

```rust
fn main() {
    println("The answer is:")
    print_int(42)
    println_float(3.14)
}
```

## Complete Example

Here's a program that combines everything covered in this guide:

```rust
module demo {
    meta {
        purpose: "Demonstrate Kōdo language basics",
        version: "0.1.0",
        author: "Kōdo Team"
    }

    fn is_even(n: Int) -> Bool {
        return n % 2 == 0
    }

    fn fizzbuzz_single(n: Int) {
        if n % 15 == 0 {
            println("FizzBuzz")
        } else {
            if n % 3 == 0 {
                println("Fizz")
            } else {
                if n % 5 == 0 {
                    println("Buzz")
                } else {
                    print_int(n)
                }
            }
        }
    }

    fn main() {
        let x: Int = 42
        let mut counter: Int = 1

        if is_even(x) {
            println("42 is even")
        }

        fizzbuzz_single(3)
        fizzbuzz_single(5)
        fizzbuzz_single(15)
        fizzbuzz_single(7)
    }
}
```

## Next Steps

- [Data Types and Pattern Matching](data-types.md) — structs, enums, and `match` expressions
- [Closures](closures.md) — closures, lambda lifting, and higher-order functions
- [Generics](generics.md) — generic types and generic functions
- [Error Handling](error-handling.md) — using `Option<T>` and `Result<T, E>`
- [Contracts](contracts.md) — add runtime verification to your functions
- [Modules and Imports](modules-and-imports.md) — multi-file programs and the standard library
- [CLI Reference](cli-reference.md) — all available commands and flags
