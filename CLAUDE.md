# CLAUDE.md — Kōdo Compiler Project Instructions

> Read this file completely before making ANY change.
> If you violate these rules, your change will be rejected.

## What is Kōdo?

Kōdo (コード) is a compiled, general-purpose programming language designed for **AI agents to write, reason about, and maintain software** — while remaining fully transparent and auditable by humans.

**Core thesis**: Remove ambiguity, make intent explicit, embed contracts into the grammar, make every module self-describing. AI agents produce software that is correct by construction.

**This is NOT a toy language.** Kōdo has: zero syntactic ambiguity (LL(1)), contracts as first-class citizens (`requires`/`ensures`), self-describing modules (mandatory `meta`), intent-driven programming (`intent` blocks), linear ownership (own/ref/mut), structured concurrency, and agent traceability annotations (`@authored_by`, `@confidence`).

See `docs/DESIGN.md` for the full language specification.

## Architecture

```
Source (.ko)
    │
    ▼
[kodo_lexer]    → Token stream (logos)
    │
    ▼
[kodo_parser]   → AST (hand-written recursive descent LL(1))
    │
    ▼
[kodo_types]    → Typed AST (type checking, no inference across modules)
    │
    ▼
[kodo_contracts]→ Verified AST (Z3 SMT for static, runtime fallback)
    │
    ▼
[kodo_resolver] → Expanded AST (intents → concrete code)
    │
    ▼
[kodo_mir]      → Mid-level IR (CFG, optimization, borrow check)
    │
    ▼
[kodo_codegen]  → Native binary (Cranelift)
```

### Crate Dependency Graph

```
kodo_ast         ← no internal deps (shared foundation)
kodo_lexer       ← kodo_ast
kodo_parser      ← kodo_ast, kodo_lexer
kodo_types       ← kodo_ast
kodo_contracts   ← kodo_ast, kodo_types
kodo_resolver    ← kodo_ast, kodo_types, kodo_contracts
kodo_mir         ← kodo_ast, kodo_types
kodo_codegen     ← kodo_mir
kodo_std         ← kodo_ast
kodoc            ← all crates + clap + tracing + ariadne
```

## Mandatory Rules

### Code Quality — NON-NEGOTIABLE

1. **`cargo fmt --all`** — Must pass. See `rustfmt.toml`.
2. **`cargo clippy --workspace -- -D warnings`** — Zero warnings.
3. **`cargo test --workspace`** — All tests pass. No skipped tests.
4. **`cargo doc --workspace --no-deps`** — Documentation compiles.

### Every `lib.rs` MUST Have

```rust
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]
```

### Error Handling

- **Library crates**: ZERO `unwrap()` or `expect()`. Use `thiserror` enums.
- **Binary crates** (kodoc, ko): `unwrap()`/`expect()` only in `main()` or test code.
- **Test code**: `unwrap()`/`expect()` is fine.
- Every crate defines its own `Error` enum and `type Result<T>` alias.

### Documentation

- Every public item has a `///` doc comment.
- Every module has a `//!` doc comment explaining its purpose.
- Doc comments explain WHY, not just WHAT.

### Testing

- **Unit tests**: Every crate, every significant function.
- **Snapshot tests**: `insta` for lexer output, parser AST, error messages.
- **Property tests**: `proptest` for fuzzing lexer and parser.
- **Integration tests**: `crates/kodoc/tests/` for full pipeline.
- **Benchmarks**: `criterion` in `crates/kodo_lexer/benches/`.
- Test fixtures live in `tests/fixtures/{valid,invalid}/`.

## Error Messages — Primary UX Surface

Error messages are how AI agents interact with the compiler. Every error MUST:

1. Have a **unique error code** (e.g., `E0042`). See `docs/error_index.md`.
2. Include **source location** (file, line, column, span).
3. Provide **structured JSON** alongside human-readable output (`--json-errors`).
4. **Suggest a fix** when possible.
5. Reference the **relevant spec section**.

Error code ranges:
- `E0001–E0099`: Lexer
- `E0100–E0199`: Parser
- `E0200–E0299`: Types
- `E0300–E0399`: Contracts
- `E0400–E0499`: Resolver
- `E0500–E0599`: MIR
- `E0600–E0699`: Codegen

## Commit Conventions

Format: `<phase>: <description>`

Prefixes: `lexer:`, `parser:`, `ast:`, `types:`, `contracts:`, `resolver:`, `mir:`, `codegen:`, `stdlib:`, `cli:`, `docs:`, `test:`, `bench:`, `ci:`, `chore:`

Examples:
- `parser: add support for intent blocks with route declarations`
- `types: implement generic type resolution`
- `contracts: integrate Z3 for static precondition verification`
- `cli: add --json-errors flag for agent consumption`

## Priority Order

1. **Correctness** — The compiler must NEVER produce wrong code.
2. **Error quality** — Bad errors make agents produce worse code.
3. **Performance** — Fast compilation enables tight agent feedback loops.
4. **Features** — Only add when the foundation is solid.

## What NOT To Do

- **NO parser generators** (no LALRPOP, no pest). Hand-written recursive descent only.
- **NO implicit type conversions** anywhere.
- **NO `unwrap()`/`expect()`** in library code.
- **NO circular dependencies** between crates.
- **NO `unsafe`** without a `// SAFETY:` comment and a damn good reason.
- **NO feature without tests**.
- **NO dead code** — if it's not used, delete it.
- **NO magic numbers** — use named constants.

## Key Files

| File | Purpose |
|------|---------|
| `Cargo.toml` | Workspace root with all dependency versions |
| `docs/DESIGN.md` | Full language specification |
| `docs/grammar.ebnf` | Formal grammar (LL(1)) |
| `docs/error_index.md` | Error code catalog |
| `docs/intent_system.md` | Intent resolver documentation |
| `rustfmt.toml` | Formatting rules |
| `clippy.toml` | Lint configuration |
| `deny.toml` | Dependency audit rules |

## Quick Language Reference

```
module name {
    meta { key: "value" }

    fn name(param: Type) -> ReturnType
        requires { precondition }
        ensures  { postcondition }
    {
        let x: Int = 42
        let s: String = "hello"
        if condition { ... } else { ... }
        return value
    }

    intent name {
        config_key: value
    }
}
```

Primitives: `Int`, `Int8`-`Int64`, `Uint`, `Uint8`-`Uint64`, `Float32`, `Float64`, `Bool`, `String`, `Byte`

No null. `Option<T>` only. No exceptions. `Result<T, E>` only.

## Development Workflow

```bash
# Check everything compiles
cargo check --workspace

# Run all tests
cargo test --workspace

# Lint
cargo clippy --workspace -- -D warnings

# Format
cargo fmt --all

# Build
cargo build --workspace

# Run benchmarks
cargo bench -p kodo_lexer

# Generate docs
cargo doc --workspace --no-deps --open

# Run the compiler
cargo run -p kodoc -- lex examples/hello.ko
cargo run -p kodoc -- parse examples/hello.ko
cargo run -p kodoc -- check examples/hello.ko
cargo run -p kodoc -- build examples/hello.ko
```
