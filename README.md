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
  <img src="https://img.shields.io/badge/tests-374%20passing-brightgreen" alt="Tests: 374 passing">
</p>

---

## Why Kōdo?

AI agents are writing more and more production code. But the languages they write in were designed decades ago, for humans typing at keyboards. The result: ambiguous code, no correctness guarantees, no way to trace which agent wrote what, and no enforcement that safety-critical code was actually reviewed.

Kōdo is the first compiled language **designed from scratch for AI agents to write, reason about, and maintain software** — while remaining fully transparent and auditable by humans. It's not a framework, a linter, or a set of conventions bolted onto an existing language. It's a new language where the problems of AI-generated code are solved **at the grammar level**.

If your team uses AI agents to generate or maintain code, Kōdo gives you what no other language does: **mathematical proof that the code is correct, traceability of who wrote every function, and a compiler that understands intent**.

### The Problem with AI + Existing Languages

| Problem | What happens today | What Kōdo does |
|---------|-------------------|----------------|
| **No correctness guarantees** | AI generates code that "looks right" but has subtle bugs | Contracts (`requires`/`ensures`) verified by Z3 SMT solver at compile time |
| **No authorship tracking** | You can't tell which agent wrote which function | `@authored_by`, `@confidence`, `@reviewed_by` — compiler-enforced |
| **No trust enforcement** | Low-quality AI code ships without review | Code below 0.8 confidence **won't compile** without human sign-off |
| **Ambiguous semantics** | AI hallucinates APIs, misunderstands ownership | Zero-ambiguity LL(1) grammar, linear ownership (`own`/`ref`), no implicit conversions |
| **Boilerplate generation** | AI generates repetitive glue code that rots | Intent blocks: declare **what**, compiler generates **how** |
| **Opaque modules** | AI generates code without explaining purpose | Mandatory `meta` blocks — every module is self-describing |
| **Useless error messages** | Compiler errors are for humans, not agents | Structured JSON errors with machine-applicable fix patches, Levenshtein suggestions |
| **No repair loop** | Agent gets an error, guesses the fix | `kodoc fix` applies patches automatically — agent reads error, applies fix, recompiles |

---

## What Makes Kōdo Different

### 1. Compiler-as-Reviewer: Trust Policies for AI Code

No other language tracks AI vs. human authorship **with compiler enforcement**. In Kōdo, every function can declare who wrote it, how confident the agent was, and whether a human reviewed it. The compiler uses this to **reject unsafe AI code before it ships**.

```rust
// High confidence — compiles without review
@authored_by(agent: "claude")
@confidence(0.95)
fn add(a: Int, b: Int) -> Int {
    return a + b
}

// Low confidence — won't compile without human sign-off
@authored_by(agent: "claude")
@confidence(0.5)
@reviewed_by(human: "rafael")
fn experimental(a: Int, b: Int) -> Int {
    return a * b
}

// Security-sensitive — won't compile without contracts
@security_sensitive
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
{
    return a / b
}
```

**Confidence propagation is transitive:** if function A (confidence 0.95) calls function B (confidence 0.5), A's effective confidence drops to 0.5. You can't hide low-quality code behind a high-confidence wrapper. `kodoc confidence-report` visualizes the entire call graph with effective confidence scores.

**Enforced policies:**
- `@confidence` below 0.8 → `@reviewed_by(human: "...")` required, or **compilation fails** (E0260)
- `@security_sensitive` → at least one `requires`/`ensures` contract required (E0262)
- `min_confidence` in module meta → all functions must meet the threshold (E0261)

### 2. Contracts as Grammar — Verified by Z3 Before Code Runs

Contracts aren't comments, decorators, or type annotations — they're **part of the language syntax**, verified by the **Z3 SMT solver** at compile time. The compiler doesn't just catch type errors; it catches **logical errors**.

```rust
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

**Why this matters for AI agents:** an agent doesn't just generate code — it generates code with **mathematical proof of correctness**. When an agent writes `requires { b != 0 }`, the compiler verifies it at every call site. Bugs that would survive code review in other languages are caught at compile time.

### 3. Error Messages Designed for Machines (and Humans)

In most languages, error messages are formatted for humans. AI agents have to parse free-text output, guess what went wrong, and hope their fix is right. Kōdo's compiler is designed for **agent consumption**:

```bash
# Structured JSON with machine-applicable patches
kodoc check file.ko --json-errors
```

```json
{
  "code": "E0201",
  "message": "undefined type `conter`",
  "suggestion": "did you mean `counter`?",
  "fix_patch": {
    "description": "rename to closest match",
    "start_offset": 42,
    "end_offset": 48,
    "replacement": "counter"
  }
}
```

```bash
# Automatic error correction — no guessing
kodoc fix file.ko
```

**What the compiler gives agents:**
- **Unique error codes** (E0001–E0699) with structured JSON — no regex parsing needed
- **Levenshtein-based suggestions** — "did you mean `counter`?" for typos (E0201)
- **Machine-applicable fix patches** — byte offsets + replacement text, apply without interpreting prose
- **`kodoc fix`** — applies all patches automatically, creating a compile→fix→recompile loop
- **Complete explanations** — `kodoc explain E0240` gives full context for any error code

This creates a **closed-loop repair cycle**: agent writes code → compiler returns structured error → agent applies fix patch → recompile. No ambiguity, no guessing.

### 4. Intent-Driven Programming

Declare **what** you want. The compiler generates **how**.

```rust
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

No `main()`, no boilerplate. The `intent` block is a **declaration of intent** — the compiler expands it into concrete code at the AST level with full type checking. Think of it as a semantic macro, not a text substitution.

**Why this matters for AI agents:** less generated code = fewer bugs. The agent declares intent (`"I want a console app that prints X"`) and the compiler guarantees the implementation is correct. Inspect what any intent generates with `kodoc intent-explain`.

### 5. Linear Ownership — Correct by Construction

Kōdo enforces linear ownership at the type level. Every value has exactly one owner. When a value is moved, it cannot be used again. Borrow with `ref` to share without transferring ownership.

```rust
fn consume(own s: String) {
    println(s)
}

fn borrow(ref s: String) {
    println(s)
}

fn main() {
    let msg: String = "hello"
    borrow(msg)       // OK — msg is borrowed, still usable
    borrow(msg)       // OK — msg is still owned
    consume(msg)      // OK — msg is moved to consume
    // println(msg)   // E0240: use-after-move
}
```

**Copy semantics for primitives:** `Int`, `Bool`, `Float`, `Byte` are implicitly copied — only compound types (structs, strings) enforce move semantics. This eliminates false positives without sacrificing safety.

**Why this matters for AI agents:** agents frequently generate code with aliasing bugs, dangling references, and use-after-free. Kōdo catches these at compile time with clear, structured error messages that the agent can fix automatically.

### 6. Self-Describing Modules

Every Kōdo module **must** include a `meta` block. The compiler rejects code without it.

```rust
module payments {
    meta {
        purpose: "Process recurring subscription payments",
        version: "2.1.0"
    }
    // ...
}
```

AI agents and humans can understand any module's purpose without reading the implementation. Self-documentation isn't optional — it's enforced at the grammar level.

---

## A Complete Language

Kōdo isn't just annotations on top of another language — it's a **full compiled language** with native binary output via Cranelift:

| Category | Features |
|----------|----------|
| **Type system** | `Int`, `Bool`, `String`, structs, enums, generics with monomorphization, no implicit conversions |
| **Pattern matching** | Exhaustive `match` on enums with destructuring |
| **Closures** | Lambda lifting, capture analysis, higher-order functions, `(Int) -> Int` types |
| **Ownership** | Linear ownership (`own`/`ref`), Copy semantics for primitives, use-after-move (E0240), borrow-escapes-scope (E0241), move-while-borrowed (E0242) |
| **Contracts** | `requires`/`ensures` verified by Z3 SMT solver, runtime fallback |
| **Agent traceability** | `@authored_by`, `@confidence`, `@reviewed_by`, transitive confidence propagation, `min_confidence` threshold |
| **Error repair** | Machine-applicable `FixPatch` in JSON, `kodoc fix` for auto-correction, Levenshtein suggestions for typos |
| **Error handling** | `Option<T>` and `Result<T, E>` in the prelude — no null, no exceptions |
| **Standard library** | `abs`, `min`, `max`, `clamp`, `string_length` |
| **Multi-file** | `import module_name` across `.ko` files |
| **Concurrency** | Cooperative `spawn` with deferred task execution |
| **Developer tools** | LSP server with real-time diagnostics and hover, JSON error output, `kodoc explain` for any error code |
| **Build artifacts** | Compilation certificates (`.ko.cert.json`) with SHA-256 hashes |

---

## Quick Start

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (1.91+)
- A C linker (`cc`) — Xcode Command Line Tools on macOS or `build-essential` on Linux

### Build

```bash
git clone https://github.com/kodo-lang/kodo.git
cd kodo
cargo build --workspace
```

### Hello World

Create `hello.ko`:

```rust
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
| [`ownership.ko`](examples/ownership.ko) | Linear ownership with `own`/`ref` qualifiers |
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
- [Ownership](docs/guide/ownership.md) — linear ownership with `own`/`ref`
- [Agent Traceability](docs/guide/agent-traceability.md) — confidence propagation and trust policies
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
