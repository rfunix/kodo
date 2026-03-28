# KŌDO — A Language Built for AI Agents

> **Kōdo** (コード) — Japanese for "code", chosen to signal a new paradigm: code written *by* agents, *for* machines, readable by humans.

---

## Vision

Kōdo is a compiled, general-purpose programming language designed from the ground up for AI agents to write, reason about, and maintain software. While traditional languages optimize for human ergonomics — familiar syntax, flexible semantics, implicit behavior — Kōdo optimizes for **machine-first authorship** with human auditability.

The core thesis: if we remove ambiguity, make intent explicit, embed contracts into the grammar, and make every module self-describing, AI agents can produce software that is **correct by construction** rather than correct by testing.

---

## Design Principles

### 1. Zero Syntactic Ambiguity
- Every construct has exactly ONE valid parse.
- No operator precedence surprises — use explicit grouping or prefix notation for complex expressions.
- No implicit conversions, coercions, or promotions.
- Whitespace is structurally insignificant (blocks are delimited, not indented).
- No semicolons, no "gotcha" syntax — the grammar is context-free and LL(1)-parseable.

### 2. Contracts & Specs as First-Class Citizens
- Every function signature includes `requires` (preconditions) and `ensures` (postconditions).
- Module-level `invariant` blocks define properties that must always hold.
- Contracts are checked at compile time where possible (via SMT solver integration) and at runtime otherwise.
- Contracts are not comments — they are part of the type system and affect compilation.

### 3. Self-Describing Modules
- Every module has a mandatory `meta` block: purpose, version, author (human or agent), dependencies with intent.
- Type signatures are always explicit — no type inference across module boundaries.
- **Visibility**: declarations are private by default. Use `pub` to make functions, structs, and enums accessible from other modules. Selective imports: `import module { name1, name2 }`.
- Every public symbol must have a `doc` annotation (enforced by compiler).
- Module dependency graph is statically resolved — no dynamic imports, no circular dependencies.

### 4. Intent-Driven Programming
- Agent writes `intent` blocks describing WHAT should happen.
- Compiler has a built-in **resolver** that maps intents to concrete implementations.
- Standard library provides resolver strategies (e.g., "http_server", "json_parse", "file_io").
- Intents are verifiable: the resolver proves the implementation satisfies the intent's contracts.
- Escape hatch: agent can write concrete `impl` blocks when intent resolution isn't sufficient.

---

## Language Overview

### File Extension
`.ko`

### Basic Syntax

```rust
// Module declaration — every file is a module
module hello_server

meta {
  purpose: "A minimal HTTP server that returns a greeting"
  version: "0.1.0"
  author: agent("claude-code")
  license: "MIT"
}

// Type definitions
type Greeting {
  message: String
  timestamp: Time
}

// Function with contracts
fn create_greeting(name: String) -> Greeting
  requires { name.length > 0 }
  ensures  { result.message.contains(name) }
{
  Greeting {
    message: "Hello, {name}!",
    timestamp: Time.now(),
  }
}

// Intent-driven: agent declares WHAT, compiler resolves HOW
intent serve_http {
  port: 8080
  routes: [
    GET "/greet/:name" => handle_greet
  ]
}

fn handle_greet(ctx: HttpContext) -> HttpResponse
  requires { ctx.params.has("name") }
  ensures  { result.status == 200 }
{
  let greeting = create_greeting(ctx.params["name"])
  HttpResponse.json(greeting)
}
```

### Type System

```rust
// Primitive types
Int, Int8, Int16, Int32, Int64
Uint, Uint8, Uint16, Uint32, Uint64
Float32, Float64
Bool
String
Byte
Void

// Compound types
type Point { x: Float64, y: Float64 }

// Enums (algebraic data types)
enum Result<T, E> {
  Ok(T)
  Err(E)
}

enum Option<T> {
  Some(T)
  None
}

// No null. Ever. Option<T> is the only way.

// Traits (interfaces)
trait Serializable {
  fn serialize(self) -> Bytes
  fn deserialize(bytes: Bytes) -> Result<Self, SerError>
}

// Implement traits for types
impl Serializable for Greeting {
  fn serialize(self) -> Bytes { ... }
  fn deserialize(bytes: Bytes) -> Result<Greeting, SerError> { ... }
}
```

### Error Handling

```rust
// No exceptions. No panic (except in debug builds).
// Every error is explicit via Result<T, E>.

fn read_file(path: String) -> Result<String, IoError>
  requires { path.length > 0 }
{
  // The ? operator propagates errors (like Rust)
  let handle = File.open(path)?
  let content = handle.read_all()?
  Ok(content)
}

// Error types are enums
enum IoError {
  NotFound(String)
  PermissionDenied(String)
  Corrupted { path: String, reason: String }
}
```

### Ownership & Memory

```rust
// Linear ownership model (inspired by Rust, simplified for agents)
// Three modes:
//   own  — exclusive ownership (default, can be omitted)
//   ref  — immutable borrow
//   mut  — mutable borrow

fn process(data: own Buffer) -> Buffer {
  // data is consumed here, caller can't use it anymore
  transform(data)
}

fn inspect(data: ref Buffer) -> Int {
  // read-only access, caller retains ownership
  data.length
}

fn modify(data: mut Buffer) {
  // mutable access, caller retains ownership but can't use until this returns
  data.append(0xFF)
}

// No garbage collector. No manual free.
// Compiler inserts drops at scope boundaries.
// Lifetime annotations only when compiler can't infer (rare).
```

### Concurrency

> **V1 Status:** `spawn` with captured variables and `actor` with state/message passing are fully working in v1. `async`/`await` is fully wired to real green threads as of v1.12.0 — `async fn` spawns a green thread and returns a `Future<T>` handle; `await` blocks until the result is ready. Channels (`Channel<T>`), `parallel` blocks, and structured concurrency are planned for v2.

```rust
// Structured concurrency — no raw threads, no unstructured spawns
// Async by default, sync is the special case
// NOTE: This is the PLANNED v2 design. V1 compiles async/spawn synchronously.

fn fetch_all(urls: List<String>) -> List<Result<String, NetError>> {
  // parallel maps over urls, structured — waits for all to complete
  parallel urls.map(|url| {
    http.get(url)?.body_text()
  })
}

// Channels for communication (planned for v2)
fn pipeline() {
  let (tx, rx) = Channel<Int>.new(buffer: 100)

  parallel {
    // Producer
    for i in 0..1000 {
      tx.send(i)
    }
    tx.close()
  } and {
    // Consumer
    for value in rx {
      process(value)
    }
  }
}
```

### Intent System (Deep Dive)

```rust
// Intents are high-level declarations of WHAT the agent wants
// The compiler's resolver maps them to concrete implementations

intent database {
  engine: "sqlite"
  path: "./data.db"
  migrations: auto
}

intent api {
  protocol: "http"
  port: 8080
  middleware: [logging, cors, rate_limit(100)]
  routes: [
    GET    "/users"      => list_users,
    POST   "/users"      => create_user,
    GET    "/users/:id"  => get_user,
    DELETE "/users/:id"  => delete_user,
  ]
}

// The resolver generates connection pools, migration runners,
// route handlers, middleware chains — all verified against
// the contracts of list_users, create_user, etc.

// Custom resolvers can be defined for domain-specific intents
resolver my_resolver for intent cache {
  fn resolve(config: CacheConfig) -> impl CacheProvider {
    // concrete implementation
  }
}
```

### Agent Metadata & Traceability

```rust
// Every code change is traceable to an agent or human
@authored_by(agent("claude-code"), session: "abc123")
@reviewed_by(human("rafael"))
@confidence(0.95)  // agent's self-reported confidence
fn critical_calculation(input: Float64) -> Float64
  requires { input >= 0.0 }
  ensures  { result >= 0.0 }
{
  math.sqrt(input)
}

// The compiler can enforce policies:
// - Functions with @confidence < 0.8 require @reviewed_by(human(...))
// - Functions touching @security_sensitive data need extra contracts
```

---

## Compiler Architecture

### Pipeline

```rust
Source (.ko)
    │
    ▼
[1. Lexer] ──────────── Token stream
    │
    ▼
[2. Parser] ─────────── AST (concrete syntax tree)
    │                    LL(1), zero ambiguity
    ▼
[3. Semantic Analysis] ─ Typed AST
    │                    Type checking, ownership analysis
    ▼
[4. Contract Checker] ── Verified AST
    │                    SMT solver for static contracts
    ▼
[5. Intent Resolver] ── Expanded AST
    │                    Intents → concrete code
    ▼
[6. MIR Generation] ─── Mid-level IR
    │                    CFG, basic blocks, local variables
    ▼
[6.5 MIR Optimizer] ─── Optimized MIR
    │                    Constant folding, DCE, copy propagation
    ▼
[7. Code Generation] ── Binary
    │                    Via Cranelift (LLVM backend planned for v2)
    ▼
[8. Linker] ──────────── Executable
```

### MIR Optimization Passes

After MIR generation and before code generation, the compiler runs three optimization passes on every function:

1. **Constant Folding** — Evaluates expressions with constant operands at compile time. For example, `3 + 4` becomes `7`, `true && false` becomes `false`. Supports `Int`, `Float64`, and `Bool` operations. Float equality is intentionally not folded to avoid `clippy::float_cmp` issues.

2. **Dead Code Elimination (DCE)** — Removes `Assign` instructions whose destination local is never read by any subsequent instruction or terminator. `Call` instructions are preserved even if unused (they may have side effects).

3. **Copy Propagation** — When a local is assigned directly from another local (`_2 = _1`), subsequent uses of `_2` are replaced with `_1`. Resolves transitive copy chains (`_3 = _2`, `_2 = _1` → `_3` becomes `_1`).

These passes follow standard compiler optimization techniques from **[EC]** Ch. 8–10 and **[Tiger]** Ch. 8.

### Implementation Language
The Kōdo compiler (kodoc) is written in **Rust**.

### Build System
Kōdo uses a built-in build tool (`ko`) that reads `project.ko.toml`.

---

## Standard Library Modules

> **Note:** Modules marked with **(implemented)** are available in v1. Others are planned.

```rust
kodo::core        — Primitives, Result, Option, basic traits (implemented)
kodo::collections — List, Map (implemented); Set, Queue, Stack (planned)
kodo::string      — UTF-8 string operations (implemented)
kodo::io          — File I/O, stdin/stdout (implemented)
kodo::math        — abs, min, max, clamp (implemented)
kodo::net         — TCP/UDP, DNS (planned)
kodo::http        — HTTP client: http_get, http_post (implemented)
kodo::json        — JSON: json_parse, json_get_string, json_get_int, json_free (implemented)
kodo::db          — Database abstractions (planned)
kodo::crypto      — Hashing, encryption, signing (planned)
kodo::time        — Time, Duration, Timezone (planned)
kodo::concurrency — spawn with captures, actors with state/message passing (implemented); channels, parallel (planned for v2)
kodo::test        — Test framework, property-based testing (planned)
kodo::ffi         — C FFI for interop (planned)
```

---

## Development Roadmap

### Phase 1 — Foundation (MVP)
- [x] Formal grammar in EBNF
- [x] Lexer with full token set
- [x] Parser producing AST
- [x] Basic type checker (primitives, structs, enums)
- [x] Simple code generation (Cranelift → native binary)
- [x] "Hello World" compiles and runs
- [x] Basic standard library (core, io, string)

### Phase 2 — Type System & Ownership
- [x] Generics
- [x] Traits and trait implementations
- [x] Ownership and borrow checker
- [x] Pattern matching
- [x] Error handling with Result/Option

### Phase 3 — Contracts
- [x] `requires`/`ensures` parsing and AST representation
- [x] Runtime contract checking (`requires` + `ensures` inject runtime checks)
- [x] SMT solver integration (Z3) for static verification (feature-gated behind `smt`)
- [x] Module-level invariants
- [x] Contract-aware error messages (ariadne source-span rendering)

### Phase 4 — Intent System
- [x] Intent declaration parsing
- [x] Built-in resolvers (console_app, math_module, serve_http)
- [x] Custom resolver framework
- [x] Intent verification against contracts
- [x] Intent composition

### Phase 5 — Agent Features
- [x] `@authored_by`, `@confidence` annotations
- [x] Compiler policies (require review for low confidence)
- [x] Traceability report generation
- [x] LSP server for agent integration (diagnostics, hover, goto-definition, completions, signature help, document symbols)
- [x] Agent-friendly error messages (structured JSON errors with machine-applicable fix patches)

### Phase 6 — Production
- [ ] LLVM backend for optimized builds (currently Cranelift only)
- [x] Closures with full capture (lambda lifting + closure capture end-to-end)
- [x] Spawn with captured variables
- [x] Actor runtime with state and message passing
- [x] HTTP client and JSON stdlib (`http_get`, `http_post`, `json_parse`, `json_get_string`, `json_get_int`, `json_free`)
- [x] Full async/await runtime — `async fn` spawns green thread, `await` blocks until done (v1.12.0)
- [ ] Channels and parallel execution
- [ ] Cross-compilation targets
- [ ] Package registry
- [ ] Comprehensive standard library
- [ ] Benchmarks and performance tuning

---

## Why "Kōdo"?

The name carries three layers of meaning:
1. **コード (kōdo)** — "Code" in Japanese, signaling a fresh perspective on programming.
2. **Code** — What it fundamentally is: a language for writing code.
3. **道 (dō)** — "The way" / "The path" — like Bushidō or Judō, Kōdo is "the way of code."

---

## License

MIT — Open source from day one.
