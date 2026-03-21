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
kotest           ← clap + serde_json (UI test harness, no internal deps)
```

## Academic Foundations

Kōdo's design is grounded in established compiler and PL theory. When making design decisions, consult the relevant references:

| Decision Area | Consult | Key Concept |
|---------------|---------|-------------|
| Lexer design | **[CI]** Ch.4, **[EC]** Ch.2 | DFA scanning, maximal munch, token design |
| Parser structure | **[CI]** Ch.6–8, **[EC]** Ch.3 | Recursive descent, LL(1), FIRST/FOLLOW |
| AST node design | **[CI]** Ch.5, **[EC]** Ch.4–5 | Spans, visitor pattern, IR taxonomy |
| Type safety | **[TAPL]** Ch.1–11 | Progress + preservation, no implicit conversions |
| Generics / System F | **[TAPL]** Ch.22–26, **[PLP]** Ch.7–8 | Bounded quantification, parametric polymorphism |
| Ownership (own/ref/mut) | **[ATAPL]** Ch.1 | Linear and affine type systems |
| Contracts (requires/ensures) | **[SF]** Vol.1–2, **[CC]** Ch.1–6 | Hoare logic, decision procedures |
| SMT verification | **[CC]** Ch.10–12 | Z3, satisfiability, automated proving |
| MIR / optimization | **[Tiger]** Ch.7–8, **[EC]** Ch.8–10 | CFG, SSA, basic blocks, data-flow |
| Code generation | **[Tiger]** Ch.9–11, **[EC]** Ch.11–13 | Instruction selection, register allocation |

**Abbreviations:**
- **[CI]** *Crafting Interpreters* (Nystrom) — **[EC]** *Engineering a Compiler* (Cooper & Torczon)
- **[TAPL]** *Types and Programming Languages* (Pierce) — **[ATAPL]** *Advanced Topics in Types and PL* (Pierce, ed.)
- **[SF]** *Software Foundations* (Pierce et al.) — **[CC]** *The Calculus of Computation* (Bradley & Manna)
- **[Tiger]** *Modern Compiler Implementation in ML* (Appel) — **[PLP]** *Programming Language Pragmatics* (Scott)

See `docs/REFERENCES.md` for the full bibliography with chapter-by-chapter mapping.

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
- **UI tests**: `kotest` harness with `.ko` files in `tests/ui/` (inspired by Rust's `compiletest`).
- **Benchmarks**: `criterion` in `crates/kodo_lexer/benches/`.
- Test fixtures live in `tests/fixtures/{valid,invalid}/`.
- UI test files live in `tests/ui/` organized by feature (basics, types, ownership, contracts, etc.).

#### UI Test Directives

UI tests use `//@ ` directives in `.ko` files:
- `//@ check-pass` — must compile without errors
- `//@ compile-fail` — must fail compilation
- `//@ run-pass` — must compile AND run successfully
- `//@ run-fail` — must compile but fail at runtime
- `//@ error-code: E0200` — expected error code
- `//@ compile-flags: --contracts=static` — extra kodoc flags
- `//~ ERROR E0200: message` — inline annotation (bidirectional verification)

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
| `docs/REFERENCES.md` | Academic references mapped to compiler phases |
| `rustfmt.toml` | Formatting rules |
| `clippy.toml` | Lint configuration |
| `deny.toml` | Dependency audit rules |
| `Makefile` | Build, test, and run shortcuts |
| `crates/kotest/` | UI test harness (compiletest-inspired) |
| `tests/ui/` | UI test files organized by feature |
| `scripts/validate-doc-examples.sh` | Validates every doc example compiles, runs, and produces correct output |
| `~/dev/kodo-website` | Kōdo language website (update when user-facing changes occur) |
| `~/dev/kodo-website/public/llms.txt` | llms.txt for AI agent discoverability (update when docs pages are added/removed/renamed) |

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

## Agent-First Design Principles

Kōdo exists for AI agents. Every feature, error message, and tool decision should be evaluated from the agent's perspective. These principles guide development priorities:

### 1. The Error→Fix→Recompile Loop is Everything

The #1 value proposition of Kōdo is the closed-loop repair cycle. Every compiler error should be:
- **Machine-parseable**: JSON with structured fields, not prose
- **Auto-fixable when possible**: `FixPatch` with byte offsets, not just suggestions
- **Classifiable**: agents need to know instantly if they can fix it alone (`auto`), need context (`assisted`), or need a human (`manual`)

**When adding new errors**: ALWAYS implement `fix_patch()` alongside the diagnostic. A suggestion without a patch is a half-finished feature. Target: >80% of errors should have machine-applicable patches.

**Key files**: `crates/kodo_types/src/errors.rs` (fix_patch), `crates/kodoc/src/diagnostics.rs` (JSON output), `crates/kodo_parser/src/error.rs` (parser patches).

### 2. Every CLI Output Must Have a `--json` Mode

Agents don't read prose — they parse structured data. Every `kodoc` subcommand that produces output should support `--json`:
- `kodoc check --json-errors` ✓ (done)
- `kodoc confidence-report --json` ✓ (done)
- `kodoc explain --json` ✗ (needed)
- `kodoc repl --json` ✗ (needed)
- `kodoc intent-explain --json` ✗ (needed)
- `kodoc describe --json` ✓ (done)

### 3. Contracts Are the Killer Feature — Double Down

Contracts (`requires`/`ensures`) verified by Z3 are what no other language offers agents. Prioritize:
- More errors with contract-aware fix patches (e.g., "add `requires { x > 0 }` to satisfy callee's precondition")
- Contract status in compilation certificates (`verified_static` vs `runtime_only`)
- Recoverable contract mode (`--contracts=recoverable`) so services don't crash on violations
- Contract-aware LSP completions (show `requires`/`ensures` in hover and autocomplete)

### 4. Confidence + Certificates = Automated Trust

The `@confidence` → `@reviewed_by` enforcement is unique. Make it operationally useful:
- Store transitive confidence scores in `.ko.cert.json` (currently computed but not persisted)
- Enable policy-based automation: "deploy if all functions > 0.9 confidence and all contracts statically verified"
- `kodoc audit` command combining confidence + contracts + annotations in one report

### 5. Collections Are Complete — Maintain and Extend

Collections are fully wired through type checker → codegen as of v0.3.0:
- **List**: push, get, length, contains, pop, remove, set, slice, reverse, is_empty ✓
- **Map**: insert, get, contains_key, length, remove, is_empty, keys()/values()/entries() + `for-in` ✓
- **JSON**: parse, stringify, get_string, get_int, get_bool, get_array, get_float ✓

Next priorities for collections:
- **List**: sort, filter, map, fold, reduce, count, any (higher-order collection methods) -- DONE
- **Map**: merge, filter -- DONE
- **Set<T>**: new collection type

### 6. LSP Is the Agent's Eyes

For agents operating via IDE/editor integration, LSP quality directly impacts productivity:
- Hover MUST show full annotations: `@confidence(0.85)`, `@authored_by(agent: "claude")`, not just `@confidence`
- Code actions should surface `FixPatch` as one-click fixes
- Goto definition should work (currently a stub)
- Completions should be contract-aware

### 7. Known Limitations to Communicate Clearly

When implementing features, be honest about current limitations in error messages and docs:
- **Concurrency**: `spawn`/`async`/`await` compile but execute sequentially in v1
- **Channels**: only `Int`, `Bool`, `String` — not generic
- **Result<T, E>**: `E` is always `String` in practice — custom error enums don't work end-to-end yet
- **String**: `substring` is byte-based, not Unicode-aware
- **Contract violation**: calls `abort()` — no recovery mechanism yet

## Task Completion Checklist — MANDATORY

After completing ANY task (feature, bugfix, refactor, etc.), you MUST execute ALL of the following steps before considering the task done. This is NON-NEGOTIABLE.

### 1. Tests

- Write unit tests for every new function or changed behavior.
- Write integration tests for new features (in `crates/kodoc/tests/`).
- If adding a new language feature, create a `.ko` example in `examples/`.
- Add UI tests in `tests/ui/` for new features or error messages (with `//@ ` directives).
- Run `cargo test --workspace` and confirm ALL tests pass.
- Run `make ui-test` and confirm all UI tests pass.

### 2. Linters and Formatting

- Run `cargo fmt --all -- --check` and fix any formatting issues.
- Run `cargo clippy --workspace -- -D warnings` and fix ALL warnings (zero tolerance).
- Never suppress warnings without a documented reason.

### 3. Documentation

- Update `docs/guide/` if any user-facing feature was added or changed.
- Update `docs/index.md` if new guide pages were created.
- Update `README.md` if the "What Works Today" section is affected.
- Update `docs/error_index.md` if new error codes were added.
- Ensure all new public items have `///` doc comments.
- Update examples table in `README.md` if new `.ko` examples were added.
- If any user-facing feature, documentation, or README content was changed, check if `~/dev/kodo-website` needs updates and update it accordingly.
- If doc pages were added, removed, or renamed on the website, update `~/dev/kodo-website/public/llms.txt` to keep the AI-facing sitemap in sync.

### 4. Verification

Run this exact sequence and confirm all pass:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
make ui-test
```

If any user-facing feature or codegen change was made, also run:

```bash
make validate-docs
```

This compiles, runs, and **verifies the output** of every code example from the `kodo-website` documentation against the real compiler. If any example fails, either fix the compiler or update the documentation before reporting the task as complete.

If any step fails, fix the issue before reporting the task as complete.

### 5. Summary

Report to the user:
- What was changed (files modified/created)
- Number of tests passing
- Any documentation updated
- Any new examples added

**Do NOT skip these steps. Do NOT report a task as done without running all checks.**

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

# Run UI tests (kotest harness)
make ui-test

# Auto-update UI test baselines
make ui-bless
```
