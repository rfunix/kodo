# Language Basics

This guide covers the core features of Kōdo that are available today.

## Module Structure

Every `.ko` file contains exactly one module. A module has a name, a `meta` block, and one or more functions:

```
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

```
fn add(a: Int, b: Int) -> Int {
    return a + b
}
```

- Parameters must have explicit type annotations
- Return type is declared with `->` after the parameter list
- Functions without a return type return nothing
- The `main` function is the program's entry point

### Calling Functions

```
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

```
fn factorial(n: Int) -> Int {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}
```

## Types

Kōdo currently supports three primitive types:

| Type | Description | Example |
|------|-------------|---------|
| `Int` | Integer numbers | `42`, `-7`, `0` |
| `Bool` | Boolean values | `true`, `false` |
| `String` | String literals | `"hello"` |

All variables must have explicit type annotations:

```
let x: Int = 42
let name: String = "Kōdo"
let active: Bool = true
```

## Variables

### Immutable Variables

By default, variables are immutable:

```
let x: Int = 10
// x = 20  — this would be an error
```

### Mutable Variables

Use `let mut` to create a mutable variable:

```
let mut counter: Int = 0
counter = counter + 1
```

## Operators

### Arithmetic

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition | `a + b` |
| `-` | Subtraction | `a - b` |
| `*` | Multiplication | `a * b` |
| `/` | Division | `a / b` |
| `%` | Modulo | `a % b` |
| `-` | Negation (unary) | `-x` |

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

```
if x > 0 {
    println("positive")
} else {
    println("non-positive")
}
```

`if`/`else` blocks can be nested:

```
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

### return

Use `return` to exit a function with a value:

```
fn abs(x: Int) -> Int {
    if x < 0 {
        return -x
    }
    return x
}
```

## Builtin Functions

Kōdo provides three builtin functions for output:

| Function | Parameter | Description |
|----------|-----------|-------------|
| `println(s)` | `String` | Print a string followed by a newline |
| `print(s)` | `String` | Print a string without a newline |
| `print_int(n)` | `Int` | Print an integer followed by a newline |

```
fn main() {
    println("The answer is:")
    print_int(42)
}
```

## Complete Example

Here's a program that combines everything covered in this guide:

```
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

- [Contracts](contracts.md) — add runtime preconditions to your functions
- [CLI Reference](cli-reference.md) — all available commands and flags
