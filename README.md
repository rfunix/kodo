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
  <img src="https://img.shields.io/badge/tests-1224%20passing-brightgreen" alt="Tests: 1224 passing">
  <img src="https://img.shields.io/badge/coverage-pending-lightgrey" alt="Coverage: pending">
</p>

---

## Why Kōdo?

AI agents are writing more and more production code. But the languages they write in were designed decades ago, for humans typing at keyboards. The result: ambiguous code, no correctness guarantees, no way to trace which agent wrote what, and no enforcement that safety-critical code was actually reviewed.

Kōdo is the first compiled language **designed from scratch for AI agents to write, reason about, and maintain software** — while remaining fully transparent and auditable by humans. It's not a framework, a linter, or a set of conventions bolted onto an existing language. It's a new language where the problems of AI-generated code are solved **at the grammar level**.

If your team uses AI agents to generate or maintain code, Kōdo gives you what no other language does: **contract verification via SMT solver, compiler-enforced authorship tracking, and intent-driven code generation**.

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

Kōdo enforces linear ownership at the type level. Every value has exactly one owner. When a value is moved, it cannot be used again. Borrow with `ref` to share without transferring ownership. Use `mut` for exclusive mutable access — no other borrows may coexist.

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
| **Type system** | `Int`, `Float64`, `Bool`, `String`, structs, enums, tuples (`(Int, String)`), generics with monomorphization and trait bounds (`<T: Ord + Display>`), `dyn Trait` (dynamic dispatch with vtables), local type inference, no implicit conversions |
| **Pattern matching** | Exhaustive `match` on enums with destructuring |
| **Closures** | Lambda lifting, capture analysis, higher-order functions, `(Int) -> Int` types |
| **Ownership** | Linear ownership (`own`/`ref`/`mut`), Copy semantics for primitives, use-after-move (E0240), borrow-escapes-scope (E0241), move-while-borrowed (E0242), mut-borrow-while-ref-borrowed (E0245), ref-borrow-while-mut-borrowed (E0246), double-mut-borrow (E0247), assign-through-ref (E0248), functional reference counting (automatic deallocation) |
| **Contracts** | `requires`/`ensures` verified by Z3 SMT solver, runtime fallback, `recoverable` mode, module-level `invariant` blocks |
| **Agent traceability** | `@authored_by`, `@confidence`, `@reviewed_by`, transitive confidence propagation, `min_confidence` threshold |
| **Error repair** | Machine-applicable `FixPatch` in JSON, `kodoc fix` for auto-correction, Levenshtein suggestions for typos |
| **Error handling** | `Option<T>` and `Result<T, E>` in the prelude — no null, no exceptions |
| **String interpolation** | `f"Hello {name}!"` — f-strings desugar to concatenation with automatic `to_string` |
| **Inherent impl blocks** | `impl Point { fn distance(self) ... }` — methods on structs without requiring a trait |
| **Iterators & functional** | Iterator protocol for `List<T>`, `String`, `Map<K,V>`; functional combinators (`map`, `filter`, `fold`, `count`, `any`, `all`); functional pipelines |
| **Standard library** | `abs`, `min`, `max`, `clamp`, string methods (`length`, `contains`, `split`, `trim`, `to_upper`, `to_lower`, `substring`, `concat`, `index_of`, `replace`, `lines`, `parse_int`), `List<T>` (push, get, pop, remove, set, slice, sort, join), `Map<K,V>` (Int and String keys), methods on `Option<T>` and `Result<T,E>`, generic method dispatch, File I/O, HTTP client (`http_get`, `http_post`), JSON (`json_parse`, `json_get_string`, `json_get_int`, `json_free`) |
| **Multi-file** | `import module_name` across `.ko` files, qualified calls (`math.add(1, 2)`) |
| **Concurrency** | `spawn` with captured variables (works), `actor` with state and message passing (works), `async`/`await` (syntax-only, planned for v2) |
| **Developer tools** | Interactive REPL (`kodoc repl`) with full compile-and-execute pipeline and persistent history (`~/.kodo_history`); LSP server with diagnostics, hover (full annotations), goto-definition (functions, variables, params, structs, enums), find-references, contract-aware completions (31 builtins), and code actions from FixPatch; JSON error output; `kodoc explain` for any error code; `kodoc audit` for consolidated trust reports |
| **Build artifacts** | Compilation certificates (`.ko.cert.json`) with SHA-256 hashes, per-function confidence scores, and contract verification stats (static vs runtime) |

---

## Quick Start

### Option A: Download Pre-Built Binary

Download the latest release from the [Releases page](https://github.com/rfunix/kodo/releases):

```bash
# macOS (Apple Silicon)
curl -L https://github.com/rfunix/kodo/releases/latest/download/kodoc-macos-aarch64 -o kodoc

# macOS (Intel)
curl -L https://github.com/rfunix/kodo/releases/latest/download/kodoc-macos-x86_64 -o kodoc

# Linux (x86_64)
curl -L https://github.com/rfunix/kodo/releases/latest/download/kodoc-linux-x86_64 -o kodoc

chmod +x kodoc
sudo mv kodoc /usr/local/bin/
```

### Option B: Build from Source

**Prerequisites:** [Rust toolchain](https://rustup.rs/) (1.91+) and a C linker (`cc`).

```bash
git clone https://github.com/rfunix/kodo.git
cd kodo
make install
```

This builds in release mode and installs `kodoc` to `~/.kodo/bin/`. Add to your PATH:

```bash
echo 'export PATH="$HOME/.kodo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

Verify:

```bash
kodoc --version
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
kodoc build hello.ko -o hello
./hello
```

Or try the interactive REPL:

```bash
kodoc repl
# kōdo> println("Hello from the REPL!")
# kōdo> let x: Int = 2 + 3
# kōdo> fn double(n: Int) -> Int { return n * 2 }
# kōdo> double(x)
```

---

## Examples

The [`examples/`](examples/) directory contains 89 compilable programs:

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
| [`expressions.ko`](examples/expressions.ko) | Arithmetic and boolean expressions |
| [`for_loop.ko`](examples/for_loop.ko) | For loop iteration |
| [`optional_sugar.ko`](examples/optional_sugar.ko) | Optional syntactic sugar (`?.`, `??`) |
| [`break_continue.ko`](examples/break_continue.ko) | Break and continue in loops |
| [`type_errors.ko`](examples/type_errors.ko) | Demonstrates type error messages |
| [`type_inference.ko`](examples/type_inference.ko) | Local type inference for `let` bindings |

### Type System

| File | What it demonstrates |
|------|---------------------|
| [`generics.ko`](examples/generics.ko) | Generic enum types |
| [`generic_fn.ko`](examples/generic_fn.ko) | Generic functions with monomorphization |
| [`option_demo.ko`](examples/option_demo.ko) | `Option<T>` — no null values |
| [`result_demo.ko`](examples/result_demo.ko) | `Result<T, E>` — explicit error handling |
| [`flow_typing.ko`](examples/flow_typing.ko) | Flow-sensitive type narrowing |
| [`traits.ko`](examples/traits.ko) | Trait definitions and static dispatch |
| [`generic_bounds.ko`](examples/generic_bounds.ko) | Generic trait bounds (`<T: Ord>`) |
| [`sorted_list.ko`](examples/sorted_list.ko) | Bounded generics with sorted collections |
| [`advanced_traits.ko`](examples/advanced_traits.ko) | Advanced trait patterns |
| [`associated_types.ko`](examples/associated_types.ko) | Associated types in traits |
| [`methods.ko`](examples/methods.ko) | Inherent impl blocks — struct methods |
| [`tuples.ko`](examples/tuples.ko) | Tuple types, literals, indexing, and destructuring |

### Functions & Closures

| File | What it demonstrates |
|------|---------------------|
| [`closures.ko`](examples/closures.ko) | Closures and direct closure calls |
| [`closures_functional.ko`](examples/closures_functional.ko) | Higher-order functions and indirect calls |
| [`float_math.ko`](examples/float_math.ko) | Float64 arithmetic operations |
| [`string_concat_operator.ko`](examples/string_concat_operator.ko) | String concatenation with `+` operator |
| [`string_interpolation.ko`](examples/string_interpolation.ko) | F-string interpolation (`f"Hello {name}"`) |
| [`stdlib_demo.ko`](examples/stdlib_demo.ko) | Standard library: `abs`, `min`, `max`, `clamp` |

### Contracts, Ownership & AI Traceability

| File | What it demonstrates |
|------|---------------------|
| [`contracts.ko`](examples/contracts.ko) | Basic contract syntax |
| [`contracts_demo.ko`](examples/contracts_demo.ko) | Runtime contract checking (`requires`/`ensures`) |
| [`contracts_verified.ko`](examples/contracts_verified.ko) | Statically verified contracts via Z3 |
| [`contracts_smt_demo.ko`](examples/contracts_smt_demo.ko) | SMT solver contract verification demo |
| [`smt_verified.ko`](examples/smt_verified.ko) | SMT contract verification |
| [`ownership.ko`](examples/ownership.ko) | Linear ownership with `own`/`ref`, move semantics for structs |
| [`borrow_rules.ko`](examples/borrow_rules.ko) | Borrow rules: multiple `ref` borrows, `mut` exclusivity |
| [`move_semantics.ko`](examples/move_semantics.ko) | Move semantics, Copy vs non-Copy types |
| [`copy_semantics.ko`](examples/copy_semantics.ko) | Implicit Copy for primitives vs move for compounds |
| [`confidence_demo.ko`](examples/confidence_demo.ko) | Transitive confidence propagation through call graph |
| [`agent_traceability.ko`](examples/agent_traceability.ko) | `@authored_by`, `@confidence`, `@reviewed_by`, `@security_sensitive` |
| [`refinement_types.ko`](examples/refinement_types.ko) | Refinement types with `requires` constraints |
| [`refinement_smt.ko`](examples/refinement_smt.ko) | SMT-verified refinement types |
| [`struct_predicates.ko`](examples/struct_predicates.ko) | Struct field predicates in contracts |
| [`memory_management.ko`](examples/memory_management.ko) | Reference counting for heap-allocated values |
| [`module_invariant.ko`](examples/module_invariant.ko) | Module-level `invariant` blocks for global properties |

### Intent System

| File | What it demonstrates |
|------|---------------------|
| [`intent_demo.ko`](examples/intent_demo.ko) | Intent-driven programming (`console_app`) |
| [`intent_math.ko`](examples/intent_math.ko) | Math module intent resolver |
| [`intent_composed.ko`](examples/intent_composed.ko) | Multiple intents composed in one module |
| [`intent_http.ko`](examples/intent_http.ko) | HTTP intent resolver |
| [`intent_database.ko`](examples/intent_database.ko) | Database intent resolver |
| [`intent_json_api.ko`](examples/intent_json_api.ko) | JSON API intent resolver |
| [`intent_cache.ko`](examples/intent_cache.ko) | Cache intent resolver |
| [`intent_queue.ko`](examples/intent_queue.ko) | Queue intent resolver |

### Collections & I/O

| File | What it demonstrates |
|------|---------------------|
| [`list_demo.ko`](examples/list_demo.ko) | `List<T>` — `list_new`, `list_push`, `list_get`, `list_length`, `list_contains` |
| [`map_demo.ko`](examples/map_demo.ko) | `Map<K,V>` — `map_new`, `map_insert`, `map_get`, `map_contains_key`, `map_length` |
| [`string_demo.ko`](examples/string_demo.ko) | String methods including `split`, `trim`, `to_upper`, `substring` |
| [`for_in.ko`](examples/for_in.ko) | For-in loops over `List<T>` collections |
| [`file_io_demo.ko`](examples/file_io_demo.ko) | File I/O: `file_exists`, `file_read`, `file_write` |
| [`http_client.ko`](examples/http_client.ko) | HTTP GET and JSON parsing |
| [`time_env.ko`](examples/time_env.ko) | Time functions and environment variables |

### Iterators, Functional Combinators & Methods

| File | What it demonstrates |
|------|---------------------|
| [`iterator_basic.ko`](examples/iterator_basic.ko) | Basic iterator protocol |
| [`iterator_list.ko`](examples/iterator_list.ko) | Iterating over `List<T>` |
| [`iterator_string.ko`](examples/iterator_string.ko) | Iterating over `String` characters |
| [`iterator_map.ko`](examples/iterator_map.ko) | Iterating over `Map<K,V>` entries |
| [`iterator_map_filter.ko`](examples/iterator_map_filter.ko) | `map` and `filter` combinators on iterators |
| [`iterator_fold.ko`](examples/iterator_fold.ko) | `fold` combinator for aggregation |
| [`functional_pipeline.ko`](examples/functional_pipeline.ko) | Functional pipelines with chained combinators (`map`/`filter`/`fold`/`count`/`any`/`all`) |
| [`enum_methods.ko`](examples/enum_methods.ko) | Methods on `Option<T>` and `Result<T,E>` enum types |
| [`generic_method_dispatch.ko`](examples/generic_method_dispatch.ko) | Generic method dispatch on parameterized types |

### Real-World Examples

| File | What it demonstrates |
|------|---------------------|
| [`todo_app.ko`](examples/todo_app.ko) | Agent-built task manager with `@authored_by`, `@confidence`, forced `@reviewed_by`, contracts |
| [`config_validator.ko`](examples/config_validator.ko) | Config validation with refinement types (`Port`, `MaxConns`), `@security_sensitive`, module `invariant` |
| [`health_checker.ko`](examples/health_checker.ko) | HTTP health checker with `http_get`, `fold` aggregation, `--json-errors` for agent consumption |
| [`url_shortener.ko`](examples/url_shortener.ko) | URL shortener with `@security_sensitive`, contracts, `Map<Int,Int>` lookup |
| [`word_counter.ko`](examples/word_counter.ko) | Word counter demonstrating `ref` borrowing (E0240 prevention), `for-in` over `.split()` |

### Concurrency & Multi-File

> **Note:** `spawn` with captured variables, `actor` with state/message passing, `parallel` blocks, `channels`, and `async`/`await` with thread pool runtime are fully working.

| File | What it demonstrates |
|------|---------------------|
| [`async_demo.ko`](examples/async_demo.ko) | Async syntax preview (compiles synchronously) |
| [`async_real.ko`](examples/async_real.ko) | Cooperative `spawn` syntax preview |
| [`async_tasks.ko`](examples/async_tasks.ko) | Spawn with captured variables |
| [`concurrency_demo.ko`](examples/concurrency_demo.ko) | Concurrency patterns |
| [`actors.ko`](examples/actors.ko) | Actor state and message passing |
| [`actor_demo.ko`](examples/actor_demo.ko) | Actor demonstration |
| [`parallel_blocks.ko`](examples/parallel_blocks.ko) | Structured concurrency with `parallel` blocks |
| [`channels.ko`](examples/channels.ko) | Inter-thread communication with channels |
| [`channel_string.ko`](examples/channel_string.ko) | Generic typed channels |
| [`parallel_demo.ko`](examples/parallel_demo.ko) | Structured concurrency with `parallel {}` and async runtime |
| [`qualified_imports.ko`](examples/qualified_imports.ko) | Qualified imports (`math.add(1, 2)`) |
| [`selective_imports.ko`](examples/selective_imports.ko) | Selective imports (`import { add } from math`) |
| [`send_sync_demo.ko`](examples/send_sync_demo.ko) | `Send`/`Sync` bounds for thread safety |
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
┌─────────────────┐
│  MIR Optimizer   │  Constant folding, DCE, copy propagation
└──────┬──────────┘
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
- [Pattern Matching](docs/guide/pattern-matching.md) — exhaustive match on enums
- [Generics](docs/guide/generics.md) — generic types and functions
- [Traits](docs/guide/traits.md) — trait definitions and static dispatch
- [Inherent Methods](docs/guide/methods.md) — struct methods without traits
- [Error Handling](docs/guide/error-handling.md) — `Option<T>` and `Result<T, E>`
- [Contracts](docs/guide/contracts.md) — `requires` and `ensures`
- [Ownership](docs/guide/ownership.md) — linear ownership with `own`/`ref`
- [String Interpolation](docs/guide/string-interpolation.md) — f-strings with `{expression}` embedding
- [Agent Traceability](docs/guide/agent-traceability.md) — confidence propagation and trust policies
- [Closures](docs/guide/closures.md) — closures, lambda lifting, and higher-order functions
- [Modules and Imports](docs/guide/modules-and-imports.md) — multi-file programs and standard library
- [HTTP & JSON](docs/guide/http.md) — HTTP client and JSON parsing
- [Actors](docs/guide/actors.md) — actor model with state and message passing
- [Concurrency & Spawn](docs/guide/concurrency.md) — spawn with captured variables
- [Real-World Examples](docs/guide/real-world-examples.md) — complete programs showcasing agent features
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
