# Contributing to Kōdo

Thank you for your interest in contributing to Kōdo! Whether you're a human developer or an AI agent, this guide will help you get started.

## Setting Up the Development Environment

### Prerequisites

- **Rust** (stable, 1.75+): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Z3** (optional, for contract verification): `brew install z3` (macOS) or `apt install libz3-dev` (Ubuntu)

### Clone and Build

```bash
git clone https://github.com/kodo-lang/kodo.git
cd kodo
cargo build --workspace
```

### Verify Everything Works

```bash
cargo check --workspace        # Compilation
cargo test --workspace         # All tests
cargo clippy --workspace -- -D warnings  # Lints
cargo fmt --all -- --check     # Formatting
cargo doc --workspace --no-deps          # Documentation
```

All five commands must pass with zero errors and zero warnings.

## Making Changes

### 1. Read CLAUDE.md First
The `CLAUDE.md` file contains all project rules and conventions. Every contributor (human or AI) must follow them.

### 2. Create a Branch
```bash
git checkout -b <phase>/<short-description>
# Examples:
# git checkout -b lexer/add-float-literals
# git checkout -b parser/intent-block-support
# git checkout -b contracts/z3-integration
```

### 3. Write Tests First (TDD)
For every change:
1. Write a failing test that demonstrates the desired behavior
2. Implement the change
3. Verify the test passes
4. Add snapshot tests for any new output format

### 4. Follow Commit Conventions
```
<phase>: <description>

Examples:
  lexer: add support for float literals
  parser: implement intent block parsing
  types: add generic type resolution
  test: add property tests for lexer edge cases
```

### 5. Run the Full Check Suite
Before submitting:
```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

## Code Review Checklist

- [ ] All tests pass (`cargo test --workspace`)
- [ ] No clippy warnings (`cargo clippy --workspace -- -D warnings`)
- [ ] Code is formatted (`cargo fmt --all -- --check`)
- [ ] Documentation compiles (`cargo doc --workspace --no-deps`)
- [ ] No `unwrap()`/`expect()` in library code
- [ ] All public items have doc comments
- [ ] New error types follow the error code scheme (see `docs/error_index.md`)
- [ ] Commit message follows conventions

## Architecture Overview

See `CLAUDE.md` for the full architecture diagram. Key principle: **one crate per compiler phase**, with dependencies flowing strictly downward through the pipeline.

## Running Benchmarks

```bash
cargo bench -p kodo_lexer
```

Benchmark results are saved in `target/criterion/`. Do not commit these.

## Questions?

Open an issue on GitHub or check `docs/DESIGN.md` for language specification details.
