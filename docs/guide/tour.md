# A Tour of Kōdo

This is a quick walkthrough of Kōdo's key features. By the end, you'll have a good feel for the language and what makes it different.

We recommend that you try the examples as you go. To compile and run any program:

```bash
cargo run -p kodoc -- build program.ko -o program
./program
```

## Self-Describing Modules

Every Kōdo program lives inside a module with a mandatory `meta` block:

```rust
module greeter {
    meta {
        purpose: "Greet users by name"
        version: "0.1.0"
    }

    fn main() {
        println("Hello from Kōdo!")
    }
}
```

The `meta` block isn't optional decoration — the compiler **rejects** modules without it. This is intentional: every module must declare its purpose so that any reader (human or AI agent) can immediately understand what it does.

## Functions and Types

Functions have explicit type annotations on every parameter and return value. There is no type inference across function boundaries — every signature is self-documenting:

```rust
fn add(a: Int, b: Int) -> Int {
    return a + b
}

fn is_positive(n: Int) -> Bool {
    return n > 0
}

fn greet(name: String) {
    println(name)
}
```

Kōdo supports several primitive types including `Int`, `Float64`, `Bool`, and `String`. Additional numeric types are available: `Int8`–`Int64`, `Uint`–`Uint64`, `Float32`, and `Byte`.

## Variables

Variables are immutable by default. Use `let mut` when you need to reassign:

```rust
let x: Int = 42          // immutable
let mut counter: Int = 0  // mutable
counter = counter + 1     // reassignment
```

## Control Flow

Kōdo has `if`/`else` and `while` loops:

```rust
fn abs(x: Int) -> Int {
    if x < 0 {
        return -x
    }
    return x
}

fn countdown(n: Int) {
    let mut i: Int = n
    while i > 0 {
        print_int(i)
        i = i - 1
    }
    println("Liftoff!")
}
```

## Contracts

This is where Kōdo gets interesting. Functions can declare `requires` (preconditions) and `ensures` (postconditions) that are checked at runtime:

```rust
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
    ensures { result * b <= a }
{
    return a / b
}
```

`requires` runs before the function body — if violated, the program aborts immediately. `ensures` runs before every `return` — the special name `result` refers to the return value.

Contracts make assumptions explicit. Instead of a comment saying "b must not be zero," the compiler enforces it.

## Structs

Structs group related data:

```rust
struct Point {
    x: Int,
    y: Int
}

fn distance_from_origin(p: Point) -> Int {
    return p.x * p.x + p.y * p.y
}

fn main() {
    let p: Point = Point { x: 3, y: 4 }
    print_int(distance_from_origin(p))
}
```

Structs can be passed to functions and returned from functions. They are passed by pointer internally, so there's no copy overhead.

## Enums and Pattern Matching

Enums represent values that can be one of several variants. Each variant can carry data:

```rust
enum Shape {
    Circle(Int),
    Rectangle(Int, Int)
}

fn area(s: Shape) -> Int {
    match s {
        Shape::Circle(r) => {
            return r * r * 3
        }
        Shape::Rectangle(w, h) => {
            return w * h
        }
    }
}
```

`match` expressions destructure enum values and bind the contained data to variables. Every variant must be handled — the compiler ensures exhaustiveness.

## Generics

Types and functions can be parameterized with type variables:

```rust
enum Option<T> {
    Some(T),
    None
}

fn identity<T>(x: T) -> T {
    return x
}

fn main() {
    let a: Int = identity(42)
    let b: Int = identity(99)
    print_int(a)
    print_int(b)
}
```

Generics are compiled via monomorphization — `Option<Int>` becomes a concrete type at compile time, with zero runtime overhead.

## Standard Library

Kōdo includes `Option<T>` and `Result<T, E>` in its standard library prelude — they're available in every program without an explicit import:

```rust
fn find_first_positive(a: Int, b: Int) -> Option<Int> {
    if a > 0 {
        return Option::Some(a)
    }
    if b > 0 {
        return Option::Some(b)
    }
    return Option::None
}

fn main() {
    let result: Option<Int> = find_first_positive(42, -1)
    match result {
        Option::Some(v) => { print_int(v) }
        Option::None => { println("none") }
    }
}
```

## Multi-File Programs

Programs can be split across multiple files using `import`:

```rust
// math.ko
module math {
    meta { purpose: "Math utilities", version: "0.1.0" }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}
```

```rust
// main.ko
module main {
    meta { purpose: "Main program", version: "0.1.0" }

    import math

    fn main() {
        let result: Int = add(3, 4)
        print_int(result)
    }
}
```

Compile with:

```bash
cargo run -p kodoc -- build main.ko -o main
```

The compiler automatically resolves and compiles imported modules.

## Compilation Certificates

When Kōdo compiles a program, it emits a `.ko.cert.json` file alongside the binary. This certificate contains:

- The module's name, purpose, and version
- A SHA-256 hash of the source code
- The number of `requires` and `ensures` contracts
- A list of all functions

This makes every compiled artifact traceable and auditable — an AI agent can verify what it compiled and why.

## Higher-Order Functions

Kōdo supports closures as values and passing functions as arguments:

```rust
fn double(x: Int) -> Int {
    return x * 2
}

fn apply(f: (Int) -> Int, x: Int) -> Int {
    return f(x)
}

fn main() {
    let inc = |x: Int| -> Int { x + 1 }
    print_int(inc(41))        // 42
    print_int(apply(double, 21)) // 42
}
```

Closures can capture variables from their enclosing scope. The compiler uses lambda lifting to compile them to top-level functions.

## Standard Library Math

The prelude includes math functions with contracts:

```rust
fn main() {
    print_int(abs(-42))       // 42
    print_int(min(10, 20))    // 10
    print_int(max(10, 20))    // 20
    print_int(clamp(50, 0, 25)) // 25
}
```

## Intent-Driven Programming

Kōdo's `intent` system lets you declare WHAT you want, and the compiler's resolvers generate the HOW:

```rust
module my_app {
    meta {
        purpose: "Intent-driven demo",
        version: "0.1.0"
    }

    intent console_app {
        greeting: "Hello from intent!"
    }
}
```

The `console_app` resolver generates a `kodo_main()` function that prints the greeting. Use `kodoc intent-explain` to see the generated code.

Multiple intents can be composed in the same module, and generated code is verified by the type checker.

## Agent Traceability

Kōdo tracks WHO wrote each piece of code — human or AI:

```rust
@authored_by(agent: "claude-3.5")
@confidence(0.95)
fn safe_add(a: Int, b: Int) -> Int {
    return a + b
}

@authored_by(agent: "gpt-4")
@confidence(0.5)
@reviewed_by(human: "rafael")
fn risky_fn() -> Int {
    return 42
}
```

**Trust policies** are enforced by the compiler:
- `@confidence` below 0.8 requires `@reviewed_by(human: "...")`
- `@security_sensitive` functions must have `requires`/`ensures` contracts

## Cooperative Concurrency

`spawn` creates tasks that run after the main function:

```rust
fn main() {
    print_int(1)
    spawn { print_int(2) }
    spawn { print_int(3) }
    print_int(4)
}
// Output: 1, 4, 2, 3
```

Spawned tasks are deferred — they execute cooperatively after `kodo_main` returns.

## LSP Server

Kōdo includes a Language Server Protocol server for real-time integration with editors and AI agents:

```bash
kodoc lsp
```

Features:
- Real-time diagnostics (lex → parse → type check errors as you type)
- Hover information (function signatures, contracts, annotations)

## What's Next?

Dive deeper into specific topics:

- [Language Basics](language-basics.md) — detailed coverage of types, variables, and control flow
- [Data Types and Pattern Matching](data-types.md) — structs, enums, and `match`
- [Generics](generics.md) — generic types and generic functions
- [Error Handling](error-handling.md) — using `Option<T>` and `Result<T, E>`
- [Contracts](contracts.md) — `requires`, `ensures`, and contract philosophy
- [Modules and Imports](modules-and-imports.md) — multi-file programs and standard library
- [CLI Reference](cli-reference.md) — all `kodoc` commands and flags
