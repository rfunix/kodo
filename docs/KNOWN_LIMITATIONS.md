# Known Limitations — Kōdo v0.1.0-alpha

This document lists the known limitations of the current alpha release. These are deliberate trade-offs or features not yet fully implemented.

## Memory

**Intermediate string leaks**: String operations (concatenation, split, substring, interpolation) allocate heap memory that is not automatically freed. Local string variables are cleaned up at function exit via `heap_locals`, but intermediate results (e.g., `"a" + "b" + "c"` produces a temporary `"ab"` that is never freed) accumulate until process exit.

- **Impact**: Memory usage grows over time in long-running programs with heavy string operations.
- **Recommendation**: Safe for CLIs, scripts, and short-lived processes. Avoid tight loops with string concatenation in long-running services without periodic restarts.
- **Plan**: Migrate string allocation to the RC allocator (`kodo_alloc`) in a future release.

## Concurrency

**Sequential spawn/async/await**: The `spawn`, `async`, and `await` keywords compile and execute, but spawned tasks run **sequentially** on the main thread, not in parallel. The `parallel {}` blocks use real OS threads.

- **Impact**: `spawn` provides deferred execution semantics but no actual parallelism.
- **Recommendation**: Use `parallel {}` blocks for real concurrency. Use `spawn` for structuring deferred work.
- **Plan**: True multi-threaded task scheduling in a future release.

## Channels

**Limited channel types**: Channels (`Channel<T>`) only support `Int`, `Bool`, and `String` payloads. Sending composite types (structs, enums, `List`, `Map`) through channels is not supported.

- **Impact**: Concurrent programs must serialize complex data to `String` or use shared state via actors.
- **Recommendation**: Use actors for complex message passing between concurrent components.

## Error Handling

**Result\<T, E\> error type**: While `Result<T, E>` is fully supported syntactically, the error type `E` is always `String` in practice. Custom error enums do not work end-to-end through the type checker and codegen.

- **Impact**: Error handling works, but you cannot define domain-specific error types.
- **Recommendation**: Use `Result<T, String>` with descriptive error messages.

## Strings

**Byte-based substring**: The `substring(start, end)` method operates on byte offsets, not Unicode code points. Multi-byte UTF-8 characters (e.g., emojis, CJK characters) may be split in the middle, producing invalid UTF-8.

- **Impact**: Programs processing non-ASCII text with `substring` may produce garbled output.
- **Recommendation**: Only use `substring` with ASCII text, or use `split` for safe Unicode handling.

## Contracts

**Default abort on violation**: When a `requires` or `ensures` contract fails at runtime, the program calls `abort()` and terminates immediately with no recovery.

- **Mitigation**: Use `--contracts=recoverable` to log violations and return default values instead of aborting. See the [Contracts guide](guide/contracts.md#recoverable-contract-mode).

## Static Verification (Z3/SMT)

**Feature-gated**: Static contract verification requires building with the `smt` feature flag and having Z3 4.8+ installed on the system.

```bash
# Build with static verification support
cargo build -p kodoc --features smt

# Or install Z3 first:
# macOS: brew install z3
# Ubuntu: sudo apt-get install libz3-dev
```

Without the `smt` feature, contracts are checked at runtime only.

## Cross-Compilation

**Host architecture only**: The compiler generates native binaries for the current host architecture only. Cross-compilation to other platforms is not supported.

- **Recommendation**: Build on the target platform, or use CI with multiple OS runners.

## Numeric Limits

**Integer overflow**: Integer arithmetic uses 64-bit signed integers (`i64`). Overflow wraps silently (two's complement behavior) without runtime detection.
