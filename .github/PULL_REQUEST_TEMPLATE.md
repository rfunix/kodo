## Description

<!-- What does this PR do? Why is this change needed? -->

Fixes # <!-- issue number, if applicable -->

## Type of Change

<!-- Check all that apply -->

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing behavior to change)
- [ ] Refactor (no functional change)
- [ ] Documentation update
- [ ] Test improvement
- [ ] CI / tooling change

## Compiler Phase(s) Affected

<!-- Check all that apply -->

- [ ] Lexer (`kodo_lexer`)
- [ ] Parser (`kodo_parser`)
- [ ] AST (`kodo_ast`)
- [ ] Type checker (`kodo_types`)
- [ ] Contracts (`kodo_contracts`)
- [ ] Resolver (`kodo_resolver`)
- [ ] MIR (`kodo_mir`)
- [ ] Codegen (`kodo_codegen`)
- [ ] Standard library (`kodo_std`)
- [ ] CLI / driver (`kodoc`)
- [ ] Test harness (`kotest`)
- [ ] Documentation / examples only

## Checklist — ALL boxes must be checked before merging

### Code quality

- [ ] `cargo fmt --all -- --check` passes (run `cargo fmt --all` to fix)
- [ ] `cargo clippy --workspace -- -D warnings` passes with zero warnings
- [ ] No `unwrap()` or `expect()` added to library code (only allowed in `main()` or test code)
- [ ] All public items added/changed have `///` doc comments

### Tests

- [ ] `cargo test --workspace` passes — all tests green
- [ ] `make ui-test` passes — UI tests green
- [ ] New unit tests added for every new function or changed behavior
- [ ] If a new language feature was added: UI test in `tests/ui/` with `//@ ` directives
- [ ] If a new error code was added: entry added to `docs/error_index.md`
- [ ] If a new error was added: `fix_patch()` implemented alongside the diagnostic

### Documentation

- [ ] `cargo doc --workspace --no-deps` compiles without warnings (`RUSTDOCFLAGS="-D warnings"`)
- [ ] `docs/guide/` updated if any user-facing feature was added or changed
- [ ] `README.md` updated if "What Works Today" section is affected
- [ ] `docs/error_index.md` updated if new error codes were introduced
- [ ] New `.ko` example added to `examples/` if a significant feature was added

### Commit format

- [ ] Commit message follows `<phase>: <description>` convention (e.g., `parser: add intent block support`)

## How to Test

<!-- Step-by-step instructions for a reviewer to verify this change works correctly. -->

```bash
# Example:
cargo run -p kodoc -- check examples/my_new_feature.ko
```

## Notes for Reviewers

<!-- Anything the reviewer should pay special attention to, trade-offs made, or known limitations. -->
