# Kōdo Compiler — Technical Audit Report

**Date**: 2026-03-17
**Version**: 0.3.0
**Auditor**: Claude Opus 4.6 (automated, evidence-based)

---

## 1. Executive Summary

| Metric | Value |
|--------|-------|
| Total Rust LOC | ~85,700 |
| Crates | 14 workspace crates + root |
| Test count | 2,118 (all passing) |
| `.ko` examples | 120 |
| Pipeline | lexer (logos) → parser (LL(1) recursive descent) → type checker → contracts (Z3 optional) → MIR → codegen (Cranelift) |

**Maturity assessment: alpha sólido.** The compiler has a functional end-to-end pipeline producing native binaries via Cranelift. The foundation — crate separation, error handling discipline, testing infrastructure — is well above typical alpha quality. Specific gaps exist in runtime memory management, closure ownership analysis, and LSP completeness.

**Key strengths**: Zero `unwrap` in library code, excellent error messages with fix patches, unique agent-first features (confidence scores, intent system, contract verification), comprehensive CI with 8 jobs.

**Key risks**: String RC is a no-op (memory leak in long-running programs), `dealloc` uses minimal Layout (technically UB), closure captures are not ownership-checked.

---

## 2. Scorecard

| Dimension | Score (0-10) | Rationale |
|-----------|:---:|-----------|
| Compiler Architecture | 9 | Clean pipeline, zero circular deps, well-separated crates |
| Rust Code Quality | 8 | `#![deny(missing_docs)]` + `#![deny(clippy::unwrap_used)]` in all `lib.rs`, all `unsafe` blocks have `// SAFETY:` comments |
| Feature Completeness | 7 | Rich language (generics, closures, contracts, concurrency syntax), but ownership/closures incomplete |
| Tests & Coverage | 8 | 2,118 tests, insta snapshots, proptest, validate-docs script; LSP/MCP lack unit tests |
| Documentation | 8 | CLAUDE.md exemplary, doc comments mandatory, 120 examples, DESIGN.md spec |
| Developer Experience | 7 | REPL, LSP (partial), excellent error messages; VSCode extension is syntax-only |
| Error Messages | 9 | Error codes (E0001–E0699), spans, fix patches with byte offsets, JSON output, Levenshtein suggestions |
| Onboarding | 8 | Rich README, getting-started, CONTRIBUTING.md, Makefile shortcuts |
| CI/CD & Automation | 8 | 8-job CI, cross-platform, automated releases, coverage, cargo-deny; no fuzzing in CI |
| Open Source Readiness | 8 | CoC, SECURITY.md, CONTRIBUTING.md, MIT license, good structure |

**Overall: 8.0/10** — Strong foundation with well-defined gaps.

---

## 3. Bugs Found

### BUG-001: String RC is a no-op — memory leak (CRITICAL)

- **File**: `crates/kodo_runtime/src/memory.rs:207-218`
- **Description**: `kodo_rc_inc_string` and `kodo_rc_dec_string` are explicitly no-ops. Strings allocated by runtime operations (`kodo_string_concat`, `kodo_string_split`, `kodo_string_substring`) via `Box::into_raw` are never freed.
- **Documentation**: Well-documented with `/// Alpha limitation` doc comment explaining the rationale and migration path.
- **Impact**: Memory growth in long-running services. Acceptable for CLIs/scripts.
- **Fix**: Migrate string allocation to `kodo_alloc` with RC headers (non-trivial refactor).

### BUG-002: `dealloc` with minimal Layout (MEDIUM)

- **File**: `crates/kodo_runtime/src/memory.rs:150`
- **Description**: `std::alloc::dealloc(raw, Layout::from_size_align_unchecked(1, 8))` — the size=1 does not reflect the actual allocation size.
- **Mitigation**: The system allocator tracks sizes internally, so this works in practice on all major platforms.
- **Technical risk**: Per the Rust `dealloc` contract, the Layout must match the one used for allocation. Passing a different size is technically undefined behavior.
- **Has SAFETY comment**: Yes, explains the rationale.
- **Fix**: Store allocation size in the RC header or alongside the pointer; pass correct Layout to `dealloc`.

### BUG-003: `CodegenOptions::recoverable_contracts` is dead code (LOW)

- **File**: `crates/kodo_codegen/src/lib.rs:81`
- **Description**: The `recoverable_contracts` field is defined, defaulted, and tested for existence — but never read by the actual code generation logic. The recoverable contracts transformation happens in MIR, not codegen.
- **Impact**: No functional bug. Maintenance confusion only.
- **Fix**: Remove the field from `CodegenOptions` or wire it into codegen if intended.

---

## 4. Inconsistencies: Documentation vs Implementation

### INC-001: CLAUDE.md says collections "need wiring" — they're already wired

- **Location**: CLAUDE.md, section "Agent-First Design Principles", item 5
- **Claim**: "The runtime already has many of these — they just need to be wired through type checker → codegen"
- **Reality**: `list_pop`, `list_remove`, `list_set`, `list_slice`, `list_reverse`, `list_is_empty`, `map_remove`, `map_keys`, `map_values`, `map_is_empty` ARE present in the type checker AND codegen.
- **Action**: Update CLAUDE.md to reflect current state.

### INC-002: Ownership checker does not cover closures — undocumented limitation

- **Location**: `crates/kodo_types/src/ownership.rs` (108 LOC)
- **Description**: The ownership checker handles variable moves, borrows, and scope tracking, but has zero mention of closures. The runtime has `kodo_closure_new`/`kodo_closure_func`/`kodo_closure_env` but the type checker does not verify what closures capture or whether captures violate ownership rules.
- **Not listed in**: `docs/KNOWN_LIMITATIONS.md` (should be).
- **Action**: Document as a known limitation, or implement capture analysis.

---

## 5. Code Smells & Tech Debt

### TD-001: Large functions in compiler core

| File | LOC | Nature |
|------|-----|--------|
| `crates/kodo_resolver/src/lib.rs` | 2,347 | IntentResolver — large match/dispatch |
| `crates/kodo_codegen/src/lib.rs` | 1,874 | Code generation — large match over AST nodes |
| `crates/kodo_desugar/src/lib.rs` | 1,140 | Desugaring — match/dispatch |

These follow the common compiler pattern of large match arms. They work but hinder navigation and incremental review.

### TD-002: Type checker tests are monolithic

- **File**: `crates/kodo_types/src/tests.rs` — **7,083 lines** in a single file.
- **Recommendation**: Split by feature area (generics, ownership, closures, contracts, collections, etc.).

### TD-003: Parser lacks error recovery for contract clauses

- **File**: `crates/kodo_parser/src/decl.rs`
- **Description**: Contract parsing (`requires`/`ensures` blocks) uses `?` propagation without attempting to recover and continue parsing the rest of the function.
- **Impact**: A syntax error in a contract clause aborts parsing of the entire function, reducing error density per compilation.

### TD-004: LSP and MCP lack unit tests

- **Crates**: `crates/kodo_lsp/`, `crates/kodo_mcp/`
- **Current coverage**: Integration tests only.
- **Recommendation**: Add snapshot tests for hover responses, completion lists, and MCP tool outputs.

### TD-005: Clone frequency in type checker

- **File**: `crates/kodo_types/src/checker.rs` — ~112 `.clone()` calls
- **File**: `crates/kodo_types/src/expr.rs` — ~56 `.clone()` calls
- **Assessment**: Most are legitimate (AST walking requires owned values in recursive descent). Some may be reducible with `Cow` or reference-counted AST nodes, but this is a low-priority optimization.

### TD-006: VSCode extension does not use the LSP

- **Directory**: `editors/vscode/`
- **Current state**: Syntax highlighting only (TextMate grammar).
- **LSP crate**: `crates/kodo_lsp/` exists with hover, completion, and diagnostics support.
- **Gap**: The extension does not launch or connect to `kodo_lsp`.

### TD-007: Fuzzing not integrated in CI

- **Directory**: `fuzz/` — has lexer and parser fuzz targets.
- **CI**: Does not run fuzzing.
- **Benchmarks**: Only for lexer (`crates/kodo_lexer/benches/`). No parser or codegen benchmarks.

---

## 6. Prioritized Recommendations

### Quick Wins (< 1 day each)

| # | Action | Ref |
|---|--------|-----|
| 1 | Update CLAUDE.md item 5 — collections are already wired | INC-001 |
| 2 | Remove or wire `CodegenOptions::recoverable_contracts` | BUG-003 |
| 3 | Document closure ownership as known limitation | INC-002 |
| 4 | Add fuzzing to CI with `cargo-fuzz` (short timeout, nightly) | TD-007 |

### Structural Improvements (1-5 days each)

| # | Action | Ref |
|---|--------|-----|
| 5 | Split `tests.rs` into feature-area modules | TD-002 |
| 6 | Add error recovery in parser for contract clauses | TD-003 |
| 7 | Add snapshot tests for LSP hover/completion | TD-004 |
| 8 | Wire VSCode extension to `kodo_lsp` | TD-006 |
| 9 | Add benchmarks for parser and codegen | TD-007 |
| 10 | Fix `dealloc` Layout to use real allocation size | BUG-002 |

### Strategic Investments (1+ weeks)

| # | Action | Ref |
|---|--------|-----|
| 11 | String RC migration — allocate strings via `kodo_alloc` with RC headers | BUG-001 |
| 12 | Closure ownership analysis — capture tracking in type checker | INC-002 |
| 13 | Refactor large files in resolver/codegen/desugar | TD-001 |
| 14 | Atomic RC for real concurrency support | — |

---

## 7. Roadmap to v1.0

### Alpha → Beta Blockers

- [ ] String RC migration (BUG-001)
- [ ] Closure ownership analysis (INC-002)
- [ ] `dealloc` Layout fix (BUG-002)
- [ ] Contract recovery mode tested end-to-end

### Beta → RC Blockers

- [ ] Atomic RC for safe concurrency
- [ ] Unicode-aware string operations
- [ ] LSP complete (goto-definition, find-references functional)
- [ ] Security audit of all `unsafe` blocks

### RC → v1.0

- [ ] Published performance benchmarks
- [ ] Public coverage report
- [ ] Persistent contract verification cache
- [ ] Ecosystem: package manager, mature formatter

---

## 8. Competitive Analysis

| Feature | Kōdo | Dafny | Austral | Carbon/Mojo |
|---------|:----:|:-----:|:-------:|:-----------:|
| Contracts (Z3) | runtime + static | static only | — | — |
| Linear ownership | partial | — | full | — |
| Agent traceability | **unique** | — | — | — |
| JSON error output | + fix patches | — | — | — |
| Confidence scores | **unique** | — | — | — |
| Intent system | **unique** | — | — | — |
| Native codegen | Cranelift | .NET/JVM | C backend | LLVM/Mojo |
| Maturity | alpha | production | alpha | alpha/beta |

**Kōdo occupies a unique niche**: agent-first + contracts + traceability. No competitor combines all three.

---

## 9. Critical Files Reference

| File | Finding |
|------|---------|
| `crates/kodo_runtime/src/memory.rs:207-218` | BUG-001: String RC no-op |
| `crates/kodo_runtime/src/memory.rs:150` | BUG-002: `dealloc` minimal Layout |
| `crates/kodo_codegen/src/lib.rs:81` | BUG-003: dead code field |
| `crates/kodo_types/src/ownership.rs` | 108 LOC, no closure handling |
| `crates/kodo_types/src/tests.rs` | 7,083 LOC monolithic test file |
| `crates/kodo_parser/src/decl.rs` | Contract parsing without recovery |
| `CLAUDE.md` | Collections claim outdated (INC-001) |
| `editors/vscode/` | Syntax only, LSP not wired |
| `fuzz/` | Exists but not in CI |

---

*Generated by automated audit. All claims verified against source code.*
