# Kōdo

**Kōdo** (コード) is a compiled programming language designed for AI agents to write, reason about, and maintain software — while remaining fully transparent and auditable by humans. It features zero syntactic ambiguity, contracts as first-class citizens, and self-describing modules.

> **Status: Alpha** — core language features work, more are being added actively. Expect breaking changes.

## Quick Start

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (1.75+)
- A C linker (`cc`) — included with Xcode Command Line Tools on macOS or `build-essential` on Linux

### Install

```bash
git clone https://github.com/kodo-lang/kodo.git
cd kodo
cargo build --workspace
```

### Hello World

Create a file called `hello.ko`:

```
module hello {
    meta {
        purpose: "My first Kōdo program",
        version: "0.1.0",
        author: "Your Name"
    }

    fn main() {
        println("Hello, World!")
    }
}
```

Compile and run:

```bash
cargo run -p kodoc -- build hello.ko -o hello
./hello
```

### Contracts

Kōdo embeds runtime contracts directly in the language:

```
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
    ensures { result >= 0 }
{
    return a / b
}
```

`requires` checks preconditions at function entry. `ensures` checks postconditions at every return point. If a contract is violated, the program aborts with a clear error message.

### Pattern Matching

Define algebraic data types and destructure them with `match`:

```
enum Shape {
    Circle(Int),
    Rectangle(Int, Int)
}

fn area(s: Shape) -> Int {
    match s {
        Shape::Circle(r) => { return r * r * 3 }
        Shape::Rectangle(w, h) => { return w * h }
    }
}
```

### Generics

Types and functions can be parameterized:

```
fn identity<T>(x: T) -> T {
    return x
}

let a: Int = identity(42)
```

The standard library provides `Option<T>` and `Result<T, E>` in the prelude — available in every program without an import.

## Documentation

- **[Documentation Index](docs/index.md)** — start here
- [A Tour of Kōdo](docs/guide/tour.md) — quick walkthrough of all features
- [Getting Started](docs/guide/getting-started.md) — install, build, run
- [Language Basics](docs/guide/language-basics.md) — modules, functions, types, variables, control flow
- [Data Types and Pattern Matching](docs/guide/data-types.md) — structs, enums, and `match`
- [Generics](docs/guide/generics.md) — generic types and functions
- [Error Handling](docs/guide/error-handling.md) — `Option<T>` and `Result<T, E>`
- [Contracts](docs/guide/contracts.md) — `requires` and `ensures`
- [Modules and Imports](docs/guide/modules-and-imports.md) — multi-file programs and standard library
- [CLI Reference](docs/guide/cli-reference.md) — all `kodoc` commands and flags

## Examples

The [`examples/`](examples/) directory contains compilable programs:

| File | Description |
|------|-------------|
| `hello.ko` | Minimal hello world |
| `fibonacci.ko` | Recursive Fibonacci |
| `while_loop.ko` | Loops and mutable variables |
| `contracts_demo.ko` | Runtime contracts in action |
| `structs.ko` | Struct definition and field access |
| `struct_params.ko` | Structs as function parameters and return values |
| `enums.ko` | Enum types and pattern matching |
| `enum_params.ko` | Enums as function parameters |
| `generics.ko` | Generic enum types |
| `generic_fn.ko` | Generic functions |
| `option_demo.ko` | Standard library `Option<T>` |
| `result_demo.ko` | Standard library `Result<T, E>` |
| `multi_file/` | Multi-file compilation with imports |

## What Works Today

- Modules with mandatory `meta` blocks (self-describing)
- Functions with typed parameters and return types
- Types: `Int`, `Bool`, `String`, structs, enums
- Generics with monomorphization (types and functions)
- Pattern matching with `match` on enums
- Operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`, `!`, unary `-`
- Control flow: `if`/`else`, `while`, `return`
- Variables: `let` (immutable), `let mut` (mutable)
- Runtime contracts: `requires` (preconditions) and `ensures` (postconditions)
- Standard library prelude: `Option<T>`, `Result<T, E>`
- Multi-file compilation with `import`
- Compilation certificates (`.ko.cert.json`) with SHA-256 hashes
- Structured JSON error output (`--json-errors`) for AI agent consumption
- Compilation to native binaries via Cranelift

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

MIT
