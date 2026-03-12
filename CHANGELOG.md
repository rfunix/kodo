# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
