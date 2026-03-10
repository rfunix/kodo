<p align="center">
  <img src="assets/kodo.png" width="200" alt="Kōdo Logo">
</p>

<h1 align="center">Kōdo</h1>

<p align="center">
  <em>The programming language designed for AI agents — transparent for humans.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-alpha-orange" alt="Status: Alpha">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License: MIT">
  <img src="https://img.shields.io/badge/tests-363%20passing-brightgreen" alt="Tests: 363 passing">
</p>

---

## Why Kōdo?

AI agents are writing more and more production code. But the languages they write in were designed decades ago, for humans typing at keyboards. The result: ambiguous code, no correctness guarantees, no way to trace which agent wrote what, and no enforcement that safety-critical code was actually reviewed.

Kōdo is a compiled language where **correctness is built-in**, **intent is explicit**, and **authorship is tracked**. Contracts are part of the grammar, not bolted-on comments. Modules must describe themselves. Low-confidence AI code is rejected unless a human signs off. The compiler doesn't just catch bugs — it enforces a trust model between humans and AI agents.

If your team uses AI agents to generate or maintain code, Kōdo gives you what no other language does: **mathematical proof that the code is correct, traceability of who wrote every function, and a compiler that understands intent**.

---

## Features That No Other Language Has

### 1. Intent-Driven Programming

Declare **what** you want. The compiler generates **how**.

```
module intent_demo {
    meta {
        purpose: "Demonstrates intent-driven programming in Kōdo",
        version: "1.0.0"
    }

    intent console_app {
        greeting: "Hello from intent-driven Kōdo!"
    }
}
```

No `main()`, no boilerplate. The `console_app` intent resolver reads your config and generates the entire entry point. AI agents are good at declaring intentions — compilers are good at implementing them correctly.

Built-in resolvers: `console_app`, `math_module`. Intents are composable — use multiple in the same module.

### 2. Contracts Verified at Compile Time

Contracts aren't comments or decorators — they're part of the language grammar, verified by **Z3 SMT solver** before your code ever runs.

```
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
{
    return a / b
}

fn clamp(value: Int, min: Int, max: Int) -> Int
    requires { min <= max }
{
    if value < min { return min }
    if value > max { return max }
    return value
}
```

`requires` checks preconditions. `ensures` checks postconditions. With `--contracts static`, the compiler uses Z3 to **mathematically prove** your contracts hold — bugs are caught before the code ever runs, not in production at 3 AM.

### 3. Agent Traceability & Trust Policies

Every function can declare **who** wrote it, how **confident** the agent was, and whether a **human reviewed** it. The compiler enforces trust policies:

```
// High confidence — no review needed
@authored_by(agent: "claude")
@confidence(0.95)
fn add(a: Int, b: Int) -> Int {
    return a + b
}

// Low confidence — requires human review to compile
@authored_by(agent: "claude")
@confidence(0.5)
@reviewed_by(human: "rafael")
fn experimental_multiply(a: Int, b: Int) -> Int {
    return a * b
}

// Security-sensitive — requires contracts to compile
@security_sensitive
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
{
    return a / b
}
```

**Trust policies enforced by the compiler:**
- `@confidence` below 0.8 → `@reviewed_by` required, or the code **won't compile**
- `@security_sensitive` → at least one `requires` or `ensures` clause required

No other language tracks AI vs. human authorship with compiler enforcement.

### 4. Self-Describing Modules

Every Kōdo module **must** include a `meta` block. No exceptions.

```
module payments {
    meta {
        purpose: "Process recurring subscription payments",
        version: "2.1.0",
        author: "Billing Team"
    }

    // ...
}
```

AI agents and humans can understand any module's purpose without reading the implementation. The compiler rejects modules without `meta` — self-documentation isn't optional, it's enforced.

---

## A Complete Language

Kōdo isn't just annotations — it's a full compiled language with native binary output via Cranelift:

- **Type system** — `Int`, `Bool`, `String`, structs, enums, generics with monomorphization
- **Pattern matching** — exhaustive `match` on enums with destructuring
- **Closures** — lambda lifting, capture analysis, higher-order functions, `(Int) -> Int` types
- **Error handling** — `Option<T>` and `Result<T, E>` in the prelude, no null, no exceptions
- **Standard library** — `abs`, `min`, `max`, `clamp`, `string_length`
- **Multi-file imports** — `import module_name` across `.ko` files
- **Concurrency** — cooperative `spawn` with deferred task execution
- **Developer tools** — LSP server with real-time diagnostics and hover, JSON error output
- **Build artifacts** — compilation certificates (`.ko.cert.json`) with SHA-256 hashes

---

## Quick Start

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (1.75+)
- A C linker (`cc`) — Xcode Command Line Tools on macOS or `build-essential` on Linux

### Build

```bash
git clone https://github.com/kodo-lang/kodo.git
cd kodo
cargo build --workspace
```

### Hello World

Create `hello.ko`:

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

---

## Examples

The [`examples/`](examples/) directory contains 22+ compilable programs:

### Core Language

| File | What it demonstrates |
|------|---------------------|
| [`hello.ko`](examples/hello.ko) | Minimal hello world |
| [`fibonacci.ko`](examples/fibonacci.ko) | Recursive functions |
| [`while_loop.ko`](examples/while_loop.ko) | Loops and mutable variables |
| [`structs.ko`](examples/structs.ko) | Struct definition and field access |
| [`struct_params.ko`](examples/struct_params.ko) | Structs as function parameters and return values |
| [`enums.ko`](examples/enums.ko) | Enum types and pattern matching |
| [`enum_params.ko`](examples/enum_params.ko) | Enums as function parameters |

### Type System

| File | What it demonstrates |
|------|---------------------|
| [`generics.ko`](examples/generics.ko) | Generic enum types |
| [`generic_fn.ko`](examples/generic_fn.ko) | Generic functions with monomorphization |
| [`option_demo.ko`](examples/option_demo.ko) | `Option<T>` — no null values |
| [`result_demo.ko`](examples/result_demo.ko) | `Result<T, E>` — explicit error handling |
| [`flow_typing.ko`](examples/flow_typing.ko) | Flow-sensitive type narrowing |
| [`traits.ko`](examples/traits.ko) | Trait definitions and static dispatch |

### Functions & Closures

| File | What it demonstrates |
|------|---------------------|
| [`closures.ko`](examples/closures.ko) | Closures and direct closure calls |
| [`closures_functional.ko`](examples/closures_functional.ko) | Higher-order functions and indirect calls |
| [`stdlib_demo.ko`](examples/stdlib_demo.ko) | Standard library: `abs`, `min`, `max`, `clamp` |

### Contracts & Safety

| File | What it demonstrates |
|------|---------------------|
| [`contracts_demo.ko`](examples/contracts_demo.ko) | Runtime contract checking |
| [`agent_traceability.ko`](examples/agent_traceability.ko) | `@authored_by`, `@confidence`, `@reviewed_by` |

### Intent System

| File | What it demonstrates |
|------|---------------------|
| [`intent_demo.ko`](examples/intent_demo.ko) | Intent-driven programming (`console_app`) |
| [`intent_math.ko`](examples/intent_math.ko) | Math module intent resolver |
| [`intent_composed.ko`](examples/intent_composed.ko) | Multiple intents composed in one module |

### Concurrency & Multi-File

| File | What it demonstrates |
|------|---------------------|
| [`async_real.ko`](examples/async_real.ko) | Cooperative `spawn` with deferred execution |
| [`multi_file/`](examples/multi_file/) | Multi-file compilation with imports |

---

## Compiler Architecture

```
Source (.ko)
    │
    ▼
┌─────────────┐
│  kodo_lexer  │  Token stream (logos-based DFA)
└──────┬──────┘
       ▼
┌─────────────┐
│ kodo_parser  │  AST (hand-written recursive descent, LL(1))
└──────┬──────┘
       ▼
┌─────────────┐
│  kodo_types  │  Type checking (no inference across modules)
└──────┬──────┘
       ▼
┌──────────────────┐
│ kodo_contracts   │  Z3 SMT verification (static) + runtime fallback
└──────┬───────────┘
       ▼
┌────────────────┐
│ kodo_resolver  │  Intent expansion (intent blocks → concrete code)
└──────┬─────────┘
       ▼
┌──────────────┐
│  kodo_desugar │  Syntactic desugaring (for loops, optional sugar)
└──────┬───────┘
       ▼
┌─────────────┐
│  kodo_mir    │  Mid-level IR (CFG, basic blocks)
└──────┬──────┘
       ▼
┌──────────────┐
│ kodo_codegen │  Native binary (Cranelift)
└──────────────┘
```

13 crates in a Rust workspace. Zero circular dependencies. Zero clippy warnings.

---

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
- [Intent System](docs/intent_system.md) — intent-driven programming
- [CLI Reference](docs/guide/cli-reference.md) — all `kodoc` commands and flags
- [Language Design](docs/DESIGN.md) — full specification
- [Error Index](docs/error_index.md) — all error codes

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

```bash
cargo fmt --all          # Format
cargo clippy --workspace -- -D warnings  # Lint
cargo test --workspace   # Test
```

## License

MIT
