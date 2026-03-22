# Kōdo Playground — WASM-based Check + Errors

## Context

Kōdo needs a web playground at kodo-lang.dev/playground where users can write Kōdo code and see type checking results, errors, and diagnostics in real-time — without a backend server. The compiler frontend (lexer + parser + type checker) will be compiled to WebAssembly.

## Scope

**In scope:** Syntax checking, type checking, error diagnostics with codes/suggestions/fix patches, function signatures, contract validation (runtime mode only, no Z3 in WASM).

**Out of scope:** Code execution, codegen, runtime, I/O, concurrency, LLVM backend, Z3 SMT verification.

## Architecture

```
Browser (kodo-lang.dev/playground)
├── Monaco Editor (syntax highlighting + Kōdo grammar)
├── kodo_playground.wasm (~500KB gzipped)
│   ├── kodo_ast
│   ├── kodo_lexer
│   ├── kodo_parser
│   └── kodo_types
└── Error Panel (diagnostics, type info, suggestions)
```

## Components

### 1. `crates/kodo_playground/` (new crate)

**Cargo.toml:**
```toml
[package]
name = "kodo_playground"
version.workspace = true
edition.workspace = true

[lib]
crate-type = ["cdylib"]

[dependencies]
kodo_ast = { path = "../kodo_ast" }
kodo_lexer = { path = "../kodo_lexer" }
kodo_parser = { path = "../kodo_parser" }
kodo_types = { path = "../kodo_types" }
wasm-bindgen = "0.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

**API (lib.rs):**
```rust
#[wasm_bindgen]
pub fn check(source: &str) -> String {
    // Returns JSON with diagnostics, module info, type info
}

#[wasm_bindgen]
pub fn tokenize(source: &str) -> String {
    // Returns JSON token array for syntax highlighting
}
```

**Output format:**
```json
{
  "success": true,
  "module_name": "hello",
  "functions": [
    { "name": "main", "params": [], "return_type": "Int", "requires": [], "ensures": [] }
  ],
  "diagnostics": [],
  "token_count": 42
}
```

### 2. Website page (`~/dev/kodo-website/src/pages/playground.astro`)

- Full-width layout without sidebar
- Monaco editor with Kōdo syntax highlighting
- Split pane: editor left, diagnostics right
- Example dropdown with pre-loaded .ko snippets
- Debounced check on keystroke (300ms)
- WASM loaded async with spinner

### 3. Monaco grammar

Kōdo language definition for Monaco:
- Keywords: module, meta, fn, let, mut, if, else, while, for, in, return, match, struct, enum, trait, impl, pub, test, describe, requires, ensures, spawn, async, await, import, from
- Annotations: @authored_by, @confidence, @reviewed_by, @security_sensitive, @skip, @todo, @timeout, @property
- Types: Int, Float64, Bool, String, Unit, Option, Result, List, Map, Set, Channel
- Comments: // line comments
- Strings: "..." and f"...{expr}..."

## Build Pipeline

```bash
# Build WASM
cd crates/kodo_playground
wasm-pack build --target web --out-dir ../../kodo-website/public/wasm

# This produces:
# kodo-website/public/wasm/kodo_playground_bg.wasm
# kodo-website/public/wasm/kodo_playground.js
```

## Verification

1. `wasm-pack build` succeeds
2. WASM file < 3MB uncompressed, < 1MB gzipped
3. `check("module test { meta { purpose: \"t\", version: \"1\" } fn main() -> Int { return 42 } }")` returns `{ success: true }`
4. `check("module test { fn main() { let x: Int = true } }")` returns diagnostics with E0200
5. Page loads in < 2 seconds on fast connection
6. Editor responds to keystrokes with < 500ms latency for check results
