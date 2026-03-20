# Testing Framework Enhancement — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enhance Kōdo's existing test framework with `describe` grouping, setup/teardown lifecycle, skip/todo/timeout annotations, property-based testing with `@property` + `forall`, basic shrinking, and contract-driven test stub generation.

**Architecture:** All new syntax (`describe`, `setup`, `teardown`, `forall`) is parsed into AST nodes and desugared to existing constructs in `kodoc test`. The property testing engine and shrinking live in `kodo_runtime`. Stub generation is a new `kodoc generate-tests` command that reads AST contracts and emits `.ko` source.

**Tech Stack:** Rust, Cranelift (existing), `rand` crate (new, for property testing RNG)

**Spec:** `docs/superpowers/specs/2026-03-19-testing-framework-design.md`

---

## File Map

### New Files
- `crates/kodo_runtime/src/prop_ops.rs` — property testing engine (generators, shrinking)
- `crates/kodo_runtime/src/timeout_ops.rs` — test timeout via timer thread
- `crates/kodoc/src/commands/generate_tests.rs` — `kodoc generate-tests` command

### Modified Files
- `crates/kodo_lexer/src/lib.rs` — add `describe`, `setup`, `teardown`, `forall` keywords
- `crates/kodo_ast/src/lib.rs` — add `DescribeDecl`, `ForallStmt` nodes; extend `Module`
- `crates/kodo_parser/src/module.rs` — dispatch `describe` in module parsing loop
- `crates/kodo_parser/src/decl.rs` — add `parse_describe_decl()`, `parse_forall_stmt()`
- `crates/kodo_parser/src/stmt.rs` — add `forall` statement parsing
- `crates/kodo_types/src/builtins.rs` — register property testing builtins
- `crates/kodo_types/src/confidence.rs` — validate `@skip`, `@todo`, `@timeout`, `@property` annotations
- `crates/kodo_mir/src/lowering/registry.rs` — register property testing return types
- `crates/kodo_runtime/src/lib.rs` — export new modules
- `crates/kodo_runtime/src/test_ops.rs` — add isolation functions
- `crates/kodoc/src/commands/test.rs` — desugar describe/setup/teardown, @property/forall, @skip/@todo/@timeout, expanded JSON output
- `crates/kodoc/src/commands/mod.rs` — register `generate-tests` subcommand
- `crates/kodoc/src/main.rs` — wire `generate-tests` subcommand
- `docs/grammar.ebnf` — add `describe_decl`, `setup_block`, `teardown_block`, `forall_stmt`
- `Cargo.toml` (workspace) — add `rand` dependency for kodo_runtime

---

## Task 1: Lexer — Add New Keywords

**Files:**
- Modify: `crates/kodo_lexer/src/lib.rs:165-167`
- Test: `crates/kodo_lexer/src/lib.rs` (existing test module)

- [ ] **Step 1: Write failing tests for new keywords**

Add to the `#[cfg(test)]` module in `kodo_lexer/src/lib.rs`:

```rust
#[test]
fn tokenize_describe_keyword() {
    let tokens = tokenize("describe").unwrap();
    assert_eq!(tokens[0].kind, TokenKind::Describe);
}

#[test]
fn tokenize_setup_keyword() {
    let tokens = tokenize("setup").unwrap();
    assert_eq!(tokens[0].kind, TokenKind::Setup);
}

#[test]
fn tokenize_teardown_keyword() {
    let tokens = tokenize("teardown").unwrap();
    assert_eq!(tokens[0].kind, TokenKind::Teardown);
}

#[test]
fn tokenize_forall_keyword() {
    let tokens = tokenize("forall").unwrap();
    assert_eq!(tokens[0].kind, TokenKind::Forall);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kodo_lexer tokenize_describe`
Expected: FAIL — `Describe` variant does not exist

- [ ] **Step 3: Add keyword variants to TokenKind**

In `crates/kodo_lexer/src/lib.rs`, after the `Test` variant (line ~167), add:

```rust
/// The `describe` keyword — groups related tests.
#[token("describe")]
Describe,

/// The `setup` keyword — runs before each test in a describe block.
#[token("setup")]
Setup,

/// The `teardown` keyword — runs after each test in a describe block.
#[token("teardown")]
Teardown,

/// The `forall` keyword — introduces universally quantified variables in property tests.
#[token("forall")]
Forall,
```

Also add display cases in the `Display` impl for `TokenKind`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kodo_lexer`
Expected: ALL PASS

- [ ] **Step 5: Run clippy and format**

Run: `cargo fmt --all -- --check && cargo clippy -p kodo_lexer -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add crates/kodo_lexer/src/lib.rs
git commit -m "lexer: add describe, setup, teardown, forall keywords"
```

---

## Task 2: AST — Add New Nodes

**Files:**
- Modify: `crates/kodo_ast/src/lib.rs:149-195`

- [ ] **Step 1: Add `DescribeDecl` struct**

After `TestDecl` (line ~160), add:

```rust
/// A `describe` block groups related tests with optional setup/teardown.
#[derive(Debug, Clone, PartialEq)]
pub struct DescribeDecl {
    /// Unique node identifier.
    pub id: NodeId,
    /// Source span of the entire describe block.
    pub span: Span,
    /// Name of the test group (from string literal).
    pub name: String,
    /// Annotations on the describe block.
    pub annotations: Vec<Annotation>,
    /// Setup block executed before each test.
    pub setup: Option<Block>,
    /// Teardown block executed after each test.
    pub teardown: Option<Block>,
    /// Test declarations within this describe block.
    pub tests: Vec<TestDecl>,
    /// Nested describe blocks.
    pub describes: Vec<DescribeDecl>,
}
```

- [ ] **Step 2: Add `ForallStmt` to `Stmt` enum**

In the `Stmt` enum, add a new variant:

```rust
/// A `forall` statement in property-based tests.
ForAll {
    /// Source span.
    span: Span,
    /// Variable bindings with their types: `(name, type_expr)`.
    bindings: Vec<(String, TypeExpr)>,
    /// Body to execute for each generated input.
    body: Block,
},
```

- [ ] **Step 3: Extend `Module` struct**

Add after `test_decls` field (line ~194):

```rust
/// Describe blocks grouping related tests.
pub describe_decls: Vec<DescribeDecl>,
```

- [ ] **Step 4: Update all `Module` constructors**

Search for places that construct `Module` (parser, tests) and add `describe_decls: vec![]`.

- [ ] **Step 5: Verify compilation**

Run: `cargo check --workspace`
Expected: compiles (with warnings about unused fields, which is fine)

- [ ] **Step 6: Commit**

```bash
git add crates/kodo_ast/src/lib.rs
git commit -m "ast: add DescribeDecl, ForAll statement, extend Module"
```

---

## Task 3: Parser — Parse `describe`, `setup`, `teardown`

**Files:**
- Modify: `crates/kodo_parser/src/module.rs:85-138`
- Modify: `crates/kodo_parser/src/decl.rs:776-800`
- Test: `crates/kodo_parser/src/decl.rs` (test module)

- [ ] **Step 1: Write failing parser test for describe**

```rust
#[test]
fn parse_describe_block() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        describe "math" {
            test "add" { assert(true) }
        }
    }"#;
    let module = parse(source).unwrap();
    assert_eq!(module.describe_decls.len(), 1);
    assert_eq!(module.describe_decls[0].name, "math");
    assert_eq!(module.describe_decls[0].tests.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kodo_parser parse_describe_block`
Expected: FAIL

- [ ] **Step 3: Implement `parse_describe_decl()`**

In `crates/kodo_parser/src/decl.rs`, add after `parse_test_decl()`:

```rust
pub(crate) fn parse_describe_decl(
    &mut self,
    annotations: Vec<Annotation>,
) -> Result<DescribeDecl> {
    let start = self.expect(&TokenKind::Describe)?.span;
    let name = self.parse_ident_or_string_literal()?;
    self.expect(&TokenKind::LBrace)?;

    let mut setup = None;
    let mut teardown = None;
    let mut tests = Vec::new();
    let mut describes = Vec::new();

    while !self.check(&TokenKind::RBrace) {
        let inner_annotations = if self.check(&TokenKind::At) {
            self.parse_annotations()?
        } else {
            vec![]
        };

        if self.check(&TokenKind::Setup) {
            self.advance();
            setup = Some(self.parse_block()?);
        } else if self.check(&TokenKind::Teardown) {
            self.advance();
            teardown = Some(self.parse_block()?);
        } else if self.check(&TokenKind::Test) {
            tests.push(self.parse_test_decl(inner_annotations)?);
        } else if self.check(&TokenKind::Describe) {
            describes.push(self.parse_describe_decl(inner_annotations)?);
        } else {
            return Err(self.unexpected_token("setup, teardown, test, or describe"));
        }
    }

    let end = self.expect(&TokenKind::RBrace)?.span;
    Ok(DescribeDecl {
        id: self.next_id(),
        span: start.merge(end),
        name,
        annotations,
        setup,
        teardown,
        tests,
        describes,
    })
}
```

- [ ] **Step 4: Add describe dispatch in module parsing**

In `crates/kodo_parser/src/module.rs`, in the declaration loop (~line 99), add:

```rust
} else if self.check(&TokenKind::Describe) {
    describe_decls.push(self.parse_describe_decl(vec![])?);
```

And in the annotation dispatch (~line 129):

```rust
} else if self.check(&TokenKind::Describe) {
    describe_decls.push(self.parse_describe_decl(annotations)?);
```

- [ ] **Step 5: Write test for setup/teardown**

```rust
#[test]
fn parse_describe_with_setup_teardown() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        describe "group" {
            setup { let x: Int = 1 }
            teardown { let y: Int = 2 }
            test "a" { assert(true) }
        }
    }"#;
    let module = parse(source).unwrap();
    assert!(module.describe_decls[0].setup.is_some());
    assert!(module.describe_decls[0].teardown.is_some());
}
```

- [ ] **Step 6: Run all parser tests**

Run: `cargo test -p kodo_parser`
Expected: ALL PASS

- [ ] **Step 7: Commit**

```bash
git add crates/kodo_parser/src/module.rs crates/kodo_parser/src/decl.rs
git commit -m "parser: add describe, setup, teardown parsing"
```

---

## Task 4: Parser — Parse `forall` Statement

**Files:**
- Modify: `crates/kodo_parser/src/stmt.rs`
- Test: `crates/kodo_parser/src/stmt.rs` (test module)

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn parse_forall_statement() {
    let source = r#"module test {
        meta { purpose: "test" version: "0.1.0" }
        test "prop" {
            forall x: Int, y: Int {
                assert(true)
            }
        }
    }"#;
    let module = parse(source).unwrap();
    let body = &module.test_decls[0].body;
    assert!(matches!(&body.stmts[0], Stmt::ForAll { bindings, .. } if bindings.len() == 2));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kodo_parser parse_forall`
Expected: FAIL

- [ ] **Step 3: Implement forall parsing**

In `crates/kodo_parser/src/stmt.rs`, in the statement dispatch, add a branch for `TokenKind::Forall`:

```rust
if self.check(&TokenKind::Forall) {
    return self.parse_forall_stmt();
}
```

And implement:

```rust
fn parse_forall_stmt(&mut self) -> Result<Stmt> {
    let start = self.expect(&TokenKind::Forall)?.span;
    let mut bindings = Vec::new();
    loop {
        let name = self.parse_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type_expr()?;
        bindings.push((name, ty));
        if !self.check(&TokenKind::Comma) {
            break;
        }
        self.advance();
    }
    let body = self.parse_block()?;
    let end = self.prev_span();
    Ok(Stmt::ForAll {
        span: start.merge(end),
        bindings,
        body,
    })
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p kodo_parser`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kodo_parser/src/stmt.rs
git commit -m "parser: add forall statement parsing for property tests"
```

---

## Task 5: Grammar — Update EBNF

**Files:**
- Modify: `docs/grammar.ebnf:25-34,94-101`

- [ ] **Step 1: Add new productions**

After `test_decl` (line ~101), add:

```ebnf
describe_decl   = annotation* "describe" STRING_LIT "{"
                    setup_block? teardown_block?
                    (test_decl | describe_decl)*
                  "}" ;
setup_block     = "setup" block ;
teardown_block  = "teardown" block ;
forall_stmt     = "forall" IDENT ":" type_expr ("," IDENT ":" type_expr)* block ;
```

Update the `declaration` production to include `describe_decl`.

- [ ] **Step 2: Commit**

```bash
git add docs/grammar.ebnf
git commit -m "docs: update grammar with describe, setup, teardown, forall"
```

---

## Task 6: Type Checker — Validate New Annotations + ForAll

**Files:**
- Modify: `crates/kodo_types/src/builtins.rs:104-131`
- Modify: `crates/kodo_types/src/confidence.rs:153-195`
- Modify: `crates/kodo_types/src/stmt.rs` (add ForAll type checking)

- [ ] **Step 1: Register property testing builtins**

In `builtins.rs`, after test lifecycle builtins (~line 131), add:

```rust
// Property testing builtins
self.env.insert("kodo_prop_start".to_string(),
    Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Unit)));
self.env.insert("kodo_prop_gen_int".to_string(),
    Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)));
self.env.insert("kodo_prop_gen_bool".to_string(),
    Type::Function(vec![], Box::new(Type::Bool)));
self.env.insert("kodo_prop_gen_float".to_string(),
    Type::Function(vec![Type::Float64, Type::Float64], Box::new(Type::Float64)));
self.env.insert("kodo_prop_gen_string".to_string(),
    Type::Function(vec![Type::Int], Box::new(Type::String)));
// Timeout
self.env.insert("kodo_test_set_timeout".to_string(),
    Type::Function(vec![Type::Int], Box::new(Type::Unit)));
self.env.insert("kodo_test_clear_timeout".to_string(),
    Type::Function(vec![], Box::new(Type::Unit)));
// Isolation
self.env.insert("kodo_test_isolate_start".to_string(),
    Type::Function(vec![], Box::new(Type::Unit)));
self.env.insert("kodo_test_isolate_end".to_string(),
    Type::Function(vec![], Box::new(Type::Unit)));
```

- [ ] **Step 2: Add ForAll type checking in stmt.rs**

Add a new branch in `check_stmt()` for `Stmt::ForAll`:

```rust
Stmt::ForAll { bindings, body, .. } => {
    let scope = self.env.scope_level();
    for (name, type_expr) in bindings {
        let ty = self.resolve_type(type_expr, *span)?;
        self.env.insert(name.clone(), ty);
    }
    self.check_block(body)?;
    self.env.truncate(scope);
    Ok(())
}
```

- [ ] **Step 3: Validate @skip, @todo, @timeout, @property annotations**

In `confidence.rs`, extend annotation validation to accept these names without error. They will be processed during desugaring, not type checking.

- [ ] **Step 4: Run tests**

Run: `cargo test -p kodo_types`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kodo_types/src/builtins.rs crates/kodo_types/src/confidence.rs crates/kodo_types/src/stmt.rs
git commit -m "types: register property testing builtins, validate forall and annotations"
```

---

## Task 7: MIR — Register New Return Types

**Files:**
- Modify: `crates/kodo_mir/src/lowering/registry.rs:305-337`
- Modify: `crates/kodo_mir/src/lowering/stmt.rs` (add ForAll lowering)

- [ ] **Step 1: Add return types for new builtins**

In `register_test_return_types()`, add:

```rust
for name in &[
    "kodo_prop_start", "kodo_test_set_timeout",
    "kodo_test_clear_timeout", "kodo_test_isolate_start",
    "kodo_test_isolate_end",
] {
    fn_return_types.entry((*name).to_string()).or_insert(Type::Unit);
}
fn_return_types.entry("kodo_prop_gen_int".to_string()).or_insert(Type::Int);
fn_return_types.entry("kodo_prop_gen_bool".to_string()).or_insert(Type::Bool);
fn_return_types.entry("kodo_prop_gen_float".to_string()).or_insert(Type::Float64);
fn_return_types.entry("kodo_prop_gen_string".to_string()).or_insert(Type::String);
```

- [ ] **Step 2: Add ForAll lowering stub**

In `stmt.rs`, add a case for `Stmt::ForAll` that passes through to lowering the body (the actual desugaring to a while loop happens in `kodoc test`, not MIR):

```rust
Stmt::ForAll { body, .. } => {
    self.lower_block(body)?;
    Ok(Value::Unit)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p kodo_mir`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/kodo_mir/src/lowering/registry.rs crates/kodo_mir/src/lowering/stmt.rs
git commit -m "mir: register property testing return types, add forall lowering"
```

---

## Task 8: Runtime — Property Testing Engine

**Files:**
- Create: `crates/kodo_runtime/src/prop_ops.rs`
- Modify: `crates/kodo_runtime/src/lib.rs`
- Modify: `crates/kodo_runtime/Cargo.toml` (add `rand` dependency)

- [ ] **Step 1: Add `rand` dependency**

In workspace `Cargo.toml`, add `rand = "0.8"` to `[workspace.dependencies]`.
In `crates/kodo_runtime/Cargo.toml`, add `rand = { workspace = true }`.

- [ ] **Step 2: Create `prop_ops.rs` with generators**

Create `crates/kodo_runtime/src/prop_ops.rs`:

```rust
//! Property-based testing engine — random value generation and basic shrinking.

use rand::Rng;
use std::cell::RefCell;

thread_local! {
    static RNG: RefCell<Option<rand::rngs::StdRng>> = RefCell::new(None);
}

/// Initializes the property testing engine with iteration count and seed.
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_start(iterations: i64, seed: i64) {
    use rand::SeedableRng;
    let rng = if seed == 0 {
        rand::rngs::StdRng::from_entropy()
    } else {
        rand::rngs::StdRng::seed_from_u64(seed as u64)
    };
    RNG.with(|r| *r.borrow_mut() = Some(rng));
    let _ = iterations; // used by caller loop
}

/// Generates a random Int in [min, max].
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_gen_int(min: i64, max: i64) -> i64 {
    RNG.with(|r| {
        r.borrow_mut()
            .as_mut()
            .map_or(0, |rng| rng.gen_range(min..=max))
    })
}

/// Generates a random Bool (0 or 1).
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_gen_bool() -> i64 {
    RNG.with(|r| {
        r.borrow_mut()
            .as_mut()
            .map_or(0, |rng| i64::from(rng.gen_bool(0.5)))
    })
}

/// Generates a random Float64 in [min, max].
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_gen_float(min: f64, max: f64) -> f64 {
    RNG.with(|r| {
        r.borrow_mut()
            .as_mut()
            .map_or(0.0, |rng| rng.gen_range(min..=max))
    })
}

// String and collection generators follow the same pattern,
// allocating via kodo_runtime helpers and returning slot pointers.
```

Add generators for `String`, `List<Int>`, `List<String>`, `Option<Int>`, `Result<Int, String>`, `Map<K,V>` following the same pattern.

- [ ] **Step 3: Add basic shrinking functions**

```rust
/// Shrinks an Int toward 0.
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_shrink_int(value: i64) -> i64 {
    match value {
        0 => 0,
        v if v > 0 => v / 2,
        v => v / 2,
    }
}

/// Shrinks a Bool to false.
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_shrink_bool(_value: i64) -> i64 {
    0
}
```

- [ ] **Step 4: Export module in lib.rs**

In `crates/kodo_runtime/src/lib.rs`, add:
```rust
pub mod prop_ops;
```

- [ ] **Step 5: Write unit tests**

Test each generator produces values in range, test shrinking convergence.

- [ ] **Step 6: Run tests**

Run: `cargo test -p kodo_runtime`
Expected: ALL PASS

- [ ] **Step 7: Commit**

```bash
git add crates/kodo_runtime/ Cargo.toml
git commit -m "runtime: add property testing engine with generators and basic shrinking"
```

---

## Task 9: Runtime — Timeout and Isolation

**Files:**
- Create: `crates/kodo_runtime/src/timeout_ops.rs`
- Modify: `crates/kodo_runtime/src/test_ops.rs`
- Modify: `crates/kodo_runtime/src/lib.rs`

- [ ] **Step 1: Create `timeout_ops.rs`**

```rust
//! Test timeout support via timer thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static mut TIMEOUT_FLAG: AtomicBool = AtomicBool::new(false);
static mut TIMEOUT_HANDLE: Option<thread::JoinHandle<()>> = None;

/// Sets a timeout for the current test (milliseconds).
#[no_mangle]
pub unsafe extern "C" fn kodo_test_set_timeout(ms: i64) {
    TIMEOUT_FLAG.store(false, Ordering::SeqCst);
    let duration = Duration::from_millis(ms as u64);
    TIMEOUT_HANDLE = Some(thread::spawn(move || {
        thread::sleep(duration);
        TIMEOUT_FLAG.store(true, Ordering::SeqCst);
        eprintln!("test timeout: exceeded {}ms", ms);
        std::process::abort();
    }));
}

/// Clears the current timeout.
#[no_mangle]
pub unsafe extern "C" fn kodo_test_clear_timeout() {
    // Timer thread will complete harmlessly after test ends
    TIMEOUT_FLAG.store(false, Ordering::SeqCst);
    TIMEOUT_HANDLE = None;
}
```

- [ ] **Step 2: Add isolation functions to `test_ops.rs`**

```rust
/// Marks the start of test isolation (currently a no-op placeholder).
#[no_mangle]
pub unsafe extern "C" fn kodo_test_isolate_start() {
    // Future: snapshot global state
}

/// Marks the end of test isolation (currently a no-op placeholder).
#[no_mangle]
pub unsafe extern "C" fn kodo_test_isolate_end() {
    // Future: restore global state, free allocations
}
```

- [ ] **Step 3: Export in lib.rs**

- [ ] **Step 4: Run tests and commit**

```bash
git commit -m "runtime: add timeout and isolation support for tests"
```

---

## Task 10: Kodoc Test — Desugar `describe`, `@skip`, `@todo`, `@timeout`

**Files:**
- Modify: `crates/kodoc/src/commands/test.rs:21-214`

This is the largest task. The `run_test()` function needs to:

- [ ] **Step 1: Flatten describe blocks into test list**

Write a helper `flatten_describes()` that converts nested `DescribeDecl` into flat `TestDecl` list with hierarchical names and injected setup/teardown:

```rust
fn flatten_describes(
    describes: &[DescribeDecl],
    prefix: &str,
    parent_setup: Option<&Block>,
    parent_teardown: Option<&Block>,
) -> Vec<(String, Vec<Annotation>, Block)> {
    // For each describe:
    //   For each test:
    //     name = "prefix > describe_name > test_name"
    //     body = setup_stmts + test_body + teardown_stmts
    //   Recurse into nested describes
}
```

- [ ] **Step 2: Process annotations in desugaring**

Before converting `TestDecl` to `__test_N` functions:
- `@skip(reason)` → skip test, add to skipped count
- `@todo(reason)` → skip test, add to todo count
- `@timeout(ms)` → wrap function body with `kodo_test_set_timeout(ms)` / `kodo_test_clear_timeout()`

- [ ] **Step 3: Expand JSON output**

Add fields for `skipped`, `todo`, `group`, `duration_ms`, `property`, `failing_input`, `shrunk_input` to the JSON output.

- [ ] **Step 4: Write integration tests**

Create test `.ko` files exercising describe, skip, todo, timeout.

- [ ] **Step 5: Run tests**

Run: `cargo test -p kodoc`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git commit -m "cli: desugar describe/setup/teardown, handle @skip/@todo/@timeout annotations"
```

---

## Task 11: Kodoc Test — Desugar `@property` + `forall`

**Files:**
- Modify: `crates/kodoc/src/commands/test.rs`

- [ ] **Step 1: Detect `@property` annotation on test**

When a test has `@property(iterations: N)`, transform the body differently.

- [ ] **Step 2: Desugar forall into while loop**

Transform:
```kodo
forall a: Int, b: Int { body }
```
into:
```kodo
kodo_prop_start(iterations, seed)
let __iter: Int = 0
while __iter < iterations {
    let a: Int = kodo_prop_gen_int(min, max)
    let b: Int = kodo_prop_gen_int(min, max)
    body
    __iter = __iter + 1
}
```

Use annotation params for ranges: `int_range`, `float_range`, `max_string_len`, `max_list_len`, `seed`.

- [ ] **Step 3: Add shrinking on failure**

When a property test assertion fails, re-run with shrunk inputs and report both original and shrunk failing inputs.

- [ ] **Step 4: Write integration tests**

Create `examples/test_property.ko` with property-based tests.

- [ ] **Step 5: Run tests**

Run: `cargo test -p kodoc && cargo run -p kodoc -- test examples/test_property.ko`

- [ ] **Step 6: Commit**

```bash
git commit -m "cli: desugar @property + forall into property-based test loops"
```

---

## Task 12: Kodoc — `generate-tests` Command

**Files:**
- Create: `crates/kodoc/src/commands/generate_tests.rs`
- Modify: `crates/kodoc/src/commands/mod.rs`
- Modify: `crates/kodoc/src/main.rs`

- [ ] **Step 1: Add clap subcommand**

In `main.rs`, add `generate-tests` subcommand with args: `file`, `--inline`, `--stdout`, `--json`.

- [ ] **Step 2: Implement stub generation**

In `generate_tests.rs`:
1. Parse the source file
2. For each public function with contracts:
   - `requires` → generate test with valid input + boundary test
   - `ensures` → generate `@property` test verifying postcondition
3. For functions without contracts → generate skeleton `// TODO` test
4. Write output to `{name}_test.ko` or inline or stdout

- [ ] **Step 3: Write integration tests**

Test with `examples/test_contracts.ko` — verify generated stubs compile.

- [ ] **Step 4: Run tests and commit**

```bash
git commit -m "cli: add kodoc generate-tests command for contract-driven stub generation"
```

---

## Task 13: UI Tests + Examples

**Files:**
- Create: `tests/ui/testing/describe_basic.ko`
- Create: `tests/ui/testing/property_basic.ko`
- Create: `tests/ui/testing/skip_todo.ko`
- Create: `examples/test_property.ko`
- Create: `examples/test_describe.ko`

- [ ] **Step 1: Write UI tests**

```kodo
//@ check-pass
// Basic describe block with setup/teardown.
module describe_basic {
    meta { purpose: "Test describe" version: "0.1.0" }

    describe "math" {
        setup { let base: Int = 100 }
        test "add" { assert_eq(base + 1, 101) }
    }
}
```

- [ ] **Step 2: Write examples**

Create `examples/test_property.ko` and `examples/test_describe.ko` showcasing the new features.

- [ ] **Step 3: Run all UI tests**

Run: `make ui-test`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git commit -m "test: add UI tests and examples for testing framework enhancements"
```

---

## Task 14: Documentation

**Files:**
- Modify: `docs/guide/testing.md` (create if not exists)
- Modify: `README.md`
- Modify: website `~/dev/kodo-website/src/content/docs/guide/testing.md`

- [ ] **Step 1: Write testing guide**

Document: describe, setup/teardown, @skip/@todo/@timeout, @property + forall, generate-tests command.

- [ ] **Step 2: Update README**

Add testing framework to "What Works Today" section.

- [ ] **Step 3: Update website**

Mirror testing guide to website.

- [ ] **Step 4: Commit**

```bash
git commit -m "docs: add testing framework guide with describe, property testing, stub generation"
```

---

## Task 15: Final Verification

- [ ] **Step 1: Run full verification suite**

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
make ui-test
```

- [ ] **Step 2: Run test examples end-to-end**

```bash
cargo run -p kodoc -- test examples/testing.ko
cargo run -p kodoc -- test examples/test_describe.ko
cargo run -p kodoc -- test examples/test_property.ko
cargo run -p kodoc -- test examples/test_property.ko --json
cargo run -p kodoc -- generate-tests examples/test_contracts.ko --stdout
```

- [ ] **Step 3: Verify all test counts**

Run: `cargo test --workspace 2>&1 | grep "^test result:" | awk '{sum += $4} END {print sum}'`

- [ ] **Step 4: Final commit and push**

```bash
git push
```
