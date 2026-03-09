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

Kōdo embeds runtime contracts directly in the language. A `requires` block defines preconditions that are checked at runtime:

```
module math {
    meta {
        purpose: "Safe math operations",
        version: "0.1.0",
        author: "Your Name"
    }

    fn safe_divide(a: Int, b: Int) -> Int
        requires { b != 0 }
    {
        return a / b
    }

    fn main() {
        let result: Int = safe_divide(10, 2)
        print_int(result)
    }
}
```

If a contract is violated, the program aborts with a clear error message.

## Documentation

- [Getting Started](docs/guide/getting-started.md) — install, build, run your first program
- [Language Basics](docs/guide/language-basics.md) — modules, functions, types, variables, control flow
- [Contracts](docs/guide/contracts.md) — preconditions with `requires`
- [CLI Reference](docs/guide/cli-reference.md) — all `kodoc` commands and flags

## Examples

The [`examples/`](examples/) directory contains compilable programs:

| File | Description |
|------|-------------|
| `hello.ko` | Minimal hello world |
| `fibonacci.ko` | Recursive Fibonacci with `print_int` |
| `contracts_demo.ko` | Contracts passing and failing |
| `contracts.ko` | `requires` and `ensures` syntax |
| `expressions.ko` | Arithmetic, logic, and control flow |

## What Works Today

- Modules with `meta` blocks
- Functions with typed parameters and return types
- Types: `Int`, `Bool`, `String` (literals)
- Operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`, `!`, unary `-`
- `if`/`else`, `return`
- Variables: `let` (immutable), `let mut` (mutable)
- Recursion and function calls
- Runtime contracts: `requires { ... }`
- Builtins: `println(String)`, `print(String)`, `print_int(Int)`
- Compilation to native binaries via Cranelift

## Known Limitations

- No structs, enums, or user-defined types yet
- No generics
- No standard library beyond builtins
- No module imports (single-file compilation only)
- `ensures` clauses are parsed but do not inject runtime checks yet
- Error messages are functional but not yet polished with source spans

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

MIT
