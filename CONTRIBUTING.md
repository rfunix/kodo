# Contributing to Kōdo

Thank you for your interest in contributing to Kōdo! Whether you are a human
developer or an AI agent, this guide covers everything you need to get started.

> Before making any change, read `CLAUDE.md` in full. It contains the project
> rules and conventions that every contributor must follow. Changes that violate
> those rules will be rejected.

---

## Table of Contents

1. [Building from Source](#building-from-source)
2. [Running Tests](#running-tests)
3. [Code Style](#code-style)
4. [Commit Conventions](#commit-conventions)
5. [Adding a New Language Feature](#adding-a-new-language-feature)
6. [Writing Tests](#writing-tests)
7. [PR Process](#pr-process)
8. [Academic References](#academic-references)

---

## Building from Source

### Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust (stable) | 1.91+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Z3 | any recent | macOS: `brew install z3` · Ubuntu: `sudo apt install libz3-dev` |

Z3 is required for `--contracts=static` mode. Without it, the compiler builds
and runs correctly in `--contracts=runtime` mode (the default).

### Clone and Build

```bash
git clone https://github.com/rfunix/kodo.git
cd kodo
cargo build --workspace
```

### Verify the full pipeline works

```bash
cargo run -p kodoc -- check examples/hello.ko
cargo run -p kodoc -- build examples/hello.ko
./examples/hello   # should print "Hello, world!"
```

---

## Running Tests

Run all checks in one shot:

```bash
make ci
```

Or individually:

```bash
# Unit and integration tests
cargo test --workspace

# UI tests (kotest harness — runs .ko files in tests/ui/)
make ui-test

# Formatting check
cargo fmt --all -- --check

# Lints (zero warnings enforced)
cargo clippy --workspace -- -D warnings

# Documentation
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Dependency audit
cargo deny check

# Fuzzing (requires nightly)
cargo +nightly fuzz run fuzz_lexer -- -max_total_time=60
cargo +nightly fuzz run fuzz_parser -- -max_total_time=60

# Benchmarks
cargo bench -p kodo_lexer
```

All commands must pass with **zero errors and zero warnings** before a PR can
be merged.

### Re-baselining snapshots

Snapshot tests use [`insta`](https://insta.rs). After intentional output
changes, update the baselines with:

```bash
cargo insta review
```

### Re-baselining UI tests

```bash
make ui-bless
```

---

## Code Style

### Formatting

The project uses `rustfmt` with custom settings in `rustfmt.toml`. Always run:

```bash
cargo fmt --all
```

before committing. The CI will reject PRs where formatting differs.

### Lints

All clippy lints are enforced at the error level:

```bash
cargo clippy --workspace -- -D warnings
```

Every `lib.rs` must include:

```rust
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]
```

### Error Handling

| Context | Rule |
|---------|------|
| Library crates | Zero `unwrap()` / `expect()`. Use `thiserror` enums. |
| Binary crates (`kodoc`, `ko`) | `unwrap()` / `expect()` only in `main()`. |
| Test code | `unwrap()` / `expect()` are fine. |

Every crate defines its own `Error` enum and `type Result<T>` alias.

### Documentation

- Every public item must have a `///` doc comment explaining **why**, not just what.
- Every module must have a `//!` doc comment.
- Run `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` to verify.

---

## Commit Conventions

Format: `<phase>: <description>`

```
lexer: add support for float literals
parser: implement intent block parsing
types: add generic type resolution
contracts: integrate Z3 for static precondition verification
mir: add dead-code elimination pass
codegen: fix register allocation for nested calls
stdlib: add List.sort()
cli: add --json-errors flag for agent consumption
test: add property tests for lexer edge cases
docs: document the MIR optimization pipeline
chore: release v0.8.0
ci: pin Z3 to 4.13 for reproducibility
```

Valid prefixes: `lexer`, `parser`, `ast`, `types`, `contracts`, `resolver`,
`mir`, `codegen`, `stdlib`, `cli`, `docs`, `test`, `bench`, `ci`, `chore`.

Keep the subject line under 72 characters. Use the body for context and
motivation when needed.

---

## Adding a New Language Feature

A language feature typically touches every layer of the compiler pipeline.
Work top-down and gate each phase on the previous one passing tests.

### Step 0 — Spec and grammar

1. Open `docs/DESIGN.md` and add the feature to the language spec.
2. Open `docs/grammar.ebnf` and add / update the grammar rule.
3. Confirm the grammar remains **LL(1)**: no left recursion, unambiguous
   FIRST/FOLLOW sets for every alternative.

### Step 1 — Lexer (`crates/kodo_lexer`)

Add any new tokens to `src/token.rs` using the `logos` derive macro.
Add snapshot tests in `src/tests.rs`:

```rust
#[test]
fn lex_new_keyword() {
    let src = "new_keyword";
    let tokens = lex_all(src);
    insta::assert_debug_snapshot!(tokens);
}
```

Run `cargo insta review` to accept the baseline.

### Step 2 — Parser (`crates/kodo_parser`)

Kōdo uses a hand-written recursive-descent LL(1) parser. **No parser
generators** (no LALRPOP, no pest).

1. Add the new AST node(s) to `crates/kodo_ast/src/`.
2. Add a `parse_<feature>` method to `crates/kodo_parser/src/parser.rs`.
3. Handle parse errors with a structured `ParseError` variant (never panic).
4. Add snapshot tests for the resulting AST.

### Step 3 — Type checker (`crates/kodo_types`)

1. Add type-checking logic in `src/checker.rs`.
2. If the feature introduces new type errors, assign an error code in the
   `E0200–E0299` range and add it to `docs/error_index.md`.
3. Implement `fix_patch()` alongside every new diagnostic — a suggestion
   without a machine-applicable patch is a half-finished feature.

### Step 4 — Contracts (`crates/kodo_contracts`) — if applicable

If the feature interacts with `requires`/`ensures`, update the SMT encoding
in `src/smt.rs`. Run `cargo test -p kodo_contracts` with Z3 installed.

### Step 5 — MIR (`crates/kodo_mir`)

Lower the new AST node to MIR basic blocks in `src/lower.rs`. Verify the
CFG is well-formed with `src/verify.rs`.

### Step 6 — Codegen (`crates/kodo_codegen`)

Emit Cranelift IR for the new MIR construct in `src/emit.rs`. Add an
integration test in `crates/kodoc/tests/` that compiles and runs a `.ko`
program using the feature end-to-end.

### Step 7 — Runtime (`stdlib/`) — if applicable

If the feature requires runtime support (e.g., a new built-in method), add
it to the relevant runtime module and wire it up through codegen.

### Step 8 — Documentation and examples

1. Add a `.ko` example in `examples/`.
2. Update `docs/guide/` with a user-facing explanation.
3. Update `README.md` if the "What Works Today" section is affected.

### Step 9 — UI test

Add a UI test file in `tests/ui/<category>/your_feature.ko`:

```ko
//@ check-pass
module my_feature {
    meta { version: "0.1.0" }

    fn main() -> Int {
        // exercise the new feature
        return 0
    }
}
```

Run `make ui-test` to confirm it passes.

---

## Writing Tests

Kōdo uses four complementary test strategies.

### Unit tests

Co-located with source code (`#[cfg(test)]` modules). Test every significant
function in isolation. Use `insta` for snapshot assertions on structured output:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_let_binding() {
        let ast = parse("let x: Int = 42").unwrap();
        insta::assert_debug_snapshot!(ast);
    }
}
```

### Snapshot tests

Use `insta` for lexer token streams, parser ASTs, and error message output.
Run `cargo insta review` to accept new baselines. Snapshots are committed to
the repository.

### Property tests

Use `proptest` to fuzz-test the lexer and parser with generated inputs.
Property tests live alongside unit tests:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn lex_never_panics(s in "\\PC*") {
        // The lexer must never panic, regardless of input.
        let _ = kodo_lexer::lex(&s);
    }
}
```

### UI tests

UI tests are `.ko` source files in `tests/ui/` that the `kotest` harness
compiles and optionally runs. They verify end-to-end compiler behavior.

**Directives** (placed at the top of the file, one per line):

| Directive | Meaning |
|-----------|---------|
| `//@ check-pass` | Must compile without errors |
| `//@ compile-fail` | Must fail compilation |
| `//@ run-pass` | Must compile and run successfully (exit 0) |
| `//@ run-fail` | Must compile but fail at runtime (exit != 0) |
| `//@ error-code: E0200` | Expected error code in the output |
| `//@ compile-flags: --contracts=static` | Extra flags passed to `kodoc` |

**Inline error annotations** (bidirectional — position must match):

```ko
//@ compile-fail
//@ error-code: E0201

module bad {
    meta { version: "0.1.0" }

    fn foo() -> Int {
        return "not an int"  //~ ERROR E0201
    }
}
```

Run UI tests with:

```bash
make ui-test
# or:
cargo run -p kotest -- tests/ui/
```

### Integration tests

Full-pipeline tests live in `crates/kodoc/tests/`. They compile `.ko` files
and assert on exit code, stdout, or generated artifacts. Use these to catch
regressions across the entire compiler pipeline.

---

## PR Process

1. **Fork** the repository and create a branch named `<phase>/<short-description>`:

   ```bash
   git checkout -b parser/add-match-expressions
   ```

2. **Make your changes** following the rules in `CLAUDE.md`.

3. **Run the full check suite** and confirm everything passes:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   make ui-test
   RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
   ```

4. **Open a pull request** against the `main` branch. Fill in the PR template
   completely — the checklist exists for a reason.

5. **Address review feedback.** All CI checks must be green before merge.

### What reviewers look for

- Correctness first — the compiler must never produce wrong output.
- Error quality — every new diagnostic must be machine-parseable with a fix patch.
- Zero warnings — in code, lints, and docs.
- Test coverage — new behavior without tests will be sent back.
- Commit hygiene — clean history following the commit convention.

---

## Academic References

Kōdo's design is grounded in established compiler and programming language
theory. Before making architectural decisions, consult `docs/REFERENCES.md`
for chapter-by-chapter mappings of academic literature to each compiler phase.

The quick-reference table in `CLAUDE.md` ("Academic Foundations") maps each
decision area to the relevant book and chapter.

---

## Questions?

- Open an issue on [GitHub](https://github.com/rfunix/kodo/issues).
- Read `docs/DESIGN.md` for the full language specification.
- Read `docs/error_index.md` for the error code catalog.
