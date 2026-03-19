# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.0] — 2026-03-19

### Fixed

- **Generic function dispatch**: `identity<T>` called with multiple types (Int, String, Bool) now correctly resolves each monomorphized variant instead of picking the first one randomly from the HashMap
- **Generic List/Map parameters**: `fn first<T>(items: List<T>) -> T` now correctly unifies `List<T>` with `List<Int>` — type parameter inference recurses into generic type arguments
- **Enum monomorphization dispatch**: `Option<Int>` + `Option<String>` in the same module no longer causes segfault — enum variant dispatch now uses type-specific mangled names
- **Function types are Copy**: Closures and function references (`(Int) -> Int`) can now be passed to multiple functions without use-after-move errors — function types are treated as Copy (they are function pointers)
- **f-string Bool interpolation**: `f"{is_positive(42)}"` no longer crashes — Bool (i8) is widened to i64 before calling `kodo_bool_to_string`

### Changed

- Test count: 2228 → 2249

## [0.4.2] — 2026-03-18

### Fixed

- **Generic function dispatch**: `identity<T>` called with multiple types (Int, String, Bool) now correctly resolves each monomorphized variant instead of picking the first one randomly from the HashMap
- **Generic List/Map parameters**: `fn first<T>(items: List<T>) -> T` now correctly unifies `List<T>` with `List<Int>` — type parameter inference recurses into generic type arguments
- **f-string Bool interpolation**: `f"{is_positive(42)}"` no longer crashes — Bool (i8) is widened to i64 before calling `kodo_bool_to_string`

### Added

- 10 new e2e tests for generics: Bool, all primitives, struct, Pair<T,U>, List<T>, Map<K,V>

## [0.4.1] — 2026-03-18

### Fixed

- **f-string Bool interpolation**: `f"{is_positive(42)}"` crashed with Cranelift verifier error — Bool (`i8`) values were not widened to `i64` before calling `kodo_bool_to_string` in `emit_string_returning_call`

## [0.4.0] — 2026-03-18

### Added

- **Closure ownership analysis**: Type checker now tracks closure captures and detects use-after-move through closures (E0281, E0282, E0283)
- **Unicode-aware strings**: `substring()` and `length()` now operate on Unicode code points, not bytes. New methods: `byte_length()`, `char_count()`
- **Atomic RC**: Runtime allocator is now thread-safe with `AtomicI64` refcounts and `RwLock` registry — enables safe `parallel {}` blocks
- **String RC tracking**: Strings from `concat`, `replace`, `repeat`, `to_upper`, `to_lower`, `int_to_string`, etc. are now RC-managed and properly freed
- **Parser error recovery**: Malformed `requires`/`ensures` clauses no longer abort parsing — compiler reports the error and continues (E0104)
- **LSP improvements**: `include_declaration` support in find-references, 57 new snapshot/unit tests for hover, completion, diagnostics, symbols
- **VSCode extension**: Now connects to `kodoc lsp` via stdio for diagnostics, hover, and completions
- **Codegen benchmarks**: 5 criterion benchmarks (simple, medium, large, structs, optimized)
- **Fuzzing in CI**: Lexer and parser fuzz targets now run in CI (nightly, 60s each)
- **Contract recovery e2e tests**: 5 new tests covering multiple violations, nested calls, strict vs recoverable comparison
- **Security audit**: `UNSAFE_AUDIT.md` documenting all unsafe blocks (0 bugs, 5 risks, 8 reviews)

### Fixed

- **Memory leak (CRITICAL)**: String operations no longer leak memory — migrated from `Box::into_raw` to RC allocator (`alloc_string`)
- **Undefined behavior**: `dealloc` now passes correct Layout with real allocation size instead of `Layout(1, 8)`
- **Dead code**: Removed unused `CodegenOptions::recoverable_contracts` field
- **Thread safety**: RC registry and refcounts are now atomic, fixing potential data races in `parallel {}` blocks

### Changed

- **Resolver refactored**: `lib.rs` reduced from 2347 → 169 LOC, logic split into 12 strategy modules
- **Type checker tests split**: 7083-line monolithic `tests.rs` reorganized into 12 feature-area modules
- **Makefile**: `bench` target now runs workspace-wide benchmarks; added `fuzz`, `fuzz-lexer`, `fuzz-parser` targets
- **CI**: 9 jobs (was 8) — added fuzz job with nightly toolchain
- **CLAUDE.md**: Updated collections section to reflect they are fully wired since v0.3.0
- **KNOWN_LIMITATIONS.md**: Added closure ownership limitation, updated string operations to reflect Unicode support, updated memory section
- Test count: 2118 → 2228

## [0.3.0] — 2026-03-18

### Added

- `scripts/validate-doc-examples.sh` — validates all documentation examples compile, run, and produce correct output
- `make validate-docs` target for documentation example verification
- `examples/closure_capture_test.ko` — closure capture test example
- `tests/fixtures/valid/generic_struct_lit.ko` — generic struct literal test fixture

### Fixed

**17 compiler bugs fixed:**

- **Type inference**: `return Option::None` and `Result::Err(...)` now correctly infer generic type from function return type
- **Closures**: inline closures with `return` in custom higher-order functions no longer typed as `()`
- **Closures**: heap-allocated closure handles fix captured variable lifetime across function boundaries
- **List\<String\>**: `list_new()` now accepts `List<String>` type annotation
- **List\<String\>**: `println(list_get(names, 0))` codegen now handles string composite type correctly
- **Map iteration**: `for key in map` now returns actual keys instead of zeros
- **Generic structs**: `Pair { first: 1, second: 2 }` literal construction works via type inference
- **Generic structs**: field access on monomorphized generics returns correct values
- **dyn Trait**: Cranelift verifier error fixed (correct virtual call signature + sret handling)
- **dyn Trait**: runtime segfault fixed (MakeDynTrait coercion + fat pointer copy)
- **dyn Trait**: String return through virtual dispatch works (mangled name parsing fix)
- **Static methods**: `Counter.new()` syntax now supported
- **String methods**: `substring()`, `trim()` no longer SIGABRT on exit (prevent double-free)
- **String builtins**: `char_at`, `repeat`, `join` codegen argument passing fixed
- **JSON builtins**: `json_get_bool/float/array/object`, `json_set_float` argument passing fixed
- **Result/Option**: String payload extraction from match arms works correctly
- **`kodoc test`**: runtime symbols now link correctly

### Changed

- `CLAUDE.md` task completion checklist now includes `make validate-docs`
- Test count: 1348 → 2118

## [0.1.0-alpha] — 2026-03-12

### Added

#### Language Features
- Complete type system: `Int`, `Int8`–`Int64`, `Uint`–`Uint64`, `Float32`, `Float64`, `Bool`, `String`, `Byte`
- Struct types with field access, construction, and pattern matching
- Enum types with variants (unit, tuple, struct) and exhaustive `match`
- Generics with trait bounds (`<T: Ord>`)
- Trait definitions and static dispatch (`impl Trait for Type`)
- Inherent methods (`impl Type { ... }`)
- Dynamic dispatch via trait objects
- Closures with variable capture and lambda lifting
- Iterators with `for-in` loops over `List`, `Map`, and `String`
- Functional combinators: `map`, `filter`, `fold`, `count`, `any`, `all`, `reduce`
- Tuple types with indexing and destructuring
- Type inference for `let` bindings
- String interpolation with f-strings
- `break` and `continue` in loops
- Linear ownership with `own`, `ref`, `mut` qualifiers
- Associated types in traits
- Module system with `import` and selective imports

#### Contracts
- `requires` (preconditions) and `ensures` (postconditions) as first-class syntax
- Runtime contract checking with abort on violation
- Recoverable contract mode (`--contracts=recoverable`)
- Static verification via Z3 SMT solver (feature-gated with `--features smt`)
- Compilation certificates recording verification status

#### Agent Traceability
- `@authored_by`, `@confidence`, `@reviewed_by` annotations
- Trust policy enforcement (code below threshold requires review)
- Confidence propagation across function calls
- `kodoc audit` command for combined confidence + contract reporting

#### Standard Library
- `Option<T>` with `Some`/`None` and `unwrap`/`is_some`/`is_none`
- `Result<T, E>` with `Ok`/`Err` and `unwrap`/`is_ok`/`is_err`
- `List<T>`: push, pop, get, set, length, contains, remove, slice, reverse, sort, is_empty, join, iterator
- `Map<K, V>`: insert, get, remove, length, contains_key, is_empty, keys/values iterators
- `String`: length, substring, contains, starts_with, ends_with, to_upper, to_lower, trim, split, replace, index_of, char_at, concat (`+` operator), interpolation
- Math: abs, min, max, pow, sqrt, floor, ceil
- File I/O: read_file, write_file, file_exists
- Time: now, now_ms, format, elapsed_ms
- Environment: env_get, env_set
- JSON: parse, stringify, get_string, get_int, get_bool, get_float, get_array
- HTTP client: GET requests with response parsing

#### Concurrency
- `spawn` with captured variable support (sequential execution in v1)
- `parallel {}` blocks with real OS threads
- Typed channels (`Channel<Int>`, `Channel<Bool>`, `Channel<String>`)
- Actor model with state and message passing

#### Intent System
- `intent` blocks for declarative programming
- Built-in intent resolvers: HTTP routes, database, JSON API, cache, queue, math
- `kodoc intent-explain` for intent inspection

#### Compiler Tooling
- `kodoc build` — compile to native binary via Cranelift
- `kodoc check` — type check without codegen
- `kodoc lex` — show token stream
- `kodoc parse` — show AST
- `kodoc explain` — explain error codes
- `kodoc fix` — auto-apply fix patches
- `kodoc audit` — confidence and contract audit report
- `kodoc describe` — module metadata inspection
- `kodoc fmt` — code formatter
- `kodoc repl` — interactive REPL
- `kodoc lsp` — Language Server Protocol support
- `kodoc confidence-report` — confidence score analysis
- `--json-errors` flag for structured error output

#### Error Messages
- 74 unique error codes (E0001–E0699) with source locations
- Machine-parseable JSON diagnostics
- Fix patches with byte offsets for automated repair
- Levenshtein-based "did you mean?" suggestions

#### Infrastructure
- 1223 tests (unit, snapshot with insta, property-based with proptest, integration, e2e)
- 89 compilable `.ko` examples
- 18 documentation guides
- Criterion benchmarks for lexer
- GitHub Actions CI (format, clippy, test, MSRV, docs, deny, bench)

### Known Limitations

See [docs/KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md) for the full list of known limitations in this alpha release.
