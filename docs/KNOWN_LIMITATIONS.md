# Known Limitations — Kōdo v1.11.0

This document lists the known limitations of the current alpha release. These are deliberate trade-offs or features not yet fully implemented.

## Memory

**Atomic reference counting for memory management**: As of v0.4.0, all heap-allocated values (strings, closures, collections) use **atomic reference counting** (Atomic RC) for thread-safe memory management. String operations (concatenation, replace, to_upper, to_lower, int/float/bool to_string, etc.) allocate via the RC allocator (`kodo_alloc`). Strings are freed when their reference count drops to zero via `kodo_rc_dec_string`. The MIR's `IncRef`/`DecRef` instructions properly track string lifetimes. The atomic operations ensure correctness when values are shared across `parallel {}` blocks and `spawn` tasks.

- **Remaining gap**: Strings stored inside `List<String>` (e.g., from `split`, `lines`, `args`) use RC-managed memory for the string data, but the `[ptr, len]` pair metadata is still allocated via `Box::into_raw`. These pairs are freed when the list is freed.
- **Recommendation**: String memory management is now automatic for most use cases. Long-running services with heavy string operations should see significantly reduced memory growth compared to earlier releases.

## Concurrency

**Async execution**: In v1, `async fn` calls execute synchronously and return their result directly. The runtime infrastructure for true futures (create Future, spawn green thread, await later) exists but is not yet wired end-to-end in the MIR lowering.

- **Impact**: `async fn` works but doesn't provide true concurrency yet. `spawn {}` does run on green threads.
- **Recommendation**: Use `spawn {}` for fire-and-forget concurrency. Use `parallel {}` for structured parallelism.

**Channel select** (v1.9.0): `select {}` statement for Go-style channel multiplexing. Supports 2-3 channels.

- **Resolved** in v1.9.0.

**Generic Channel\<T\>** (v1.12.0): ✅ **Resolved in v1.12.0.** `channel_new()` is now a universal factory — the element type is inferred from the `let` binding's annotation. `channel_send(ch, val)` and `channel_recv(ch)` work for any `Channel<T>` (Int, Bool, String). The old type-specific variants (`channel_new_bool`, `channel_send_bool`, etc.) still work for backwards compatibility. See `examples/channel_generic.ko`.

**Growable green thread stacks** (v1.10.0): Each green thread starts with 1MB and grows automatically up to 8MB via SIGSEGV signal handler. Configurable via `KODO_STACK_SIZE` environment variable.

- **Resolved** in v1.10.0.

## Error Handling

**Result\<T, E\> error type**: ✅ **Resolved in v1.12.0.** Custom error enums work end-to-end through the type checker, MIR lowering, and codegen. You can write `Result<T, AppError>` where `AppError` is any enum you define.

**Result\<T, E\> unwrap methods**: `unwrap()` and `unwrap_err()` are available and return the correct polymorphic type (e.g., `String` from `Result<String, String>`). On the wrong variant, the program traps (abnormal termination) without a descriptive error message.

- **Impact**: Useful for prototyping, but production code should use `is_ok()`/`is_err()` checks.
- **Recommendation**: Guard `unwrap()` calls with an `is_ok()` check, or use `unwrap_or()` for safe defaults.

**Result pattern matching**: ✅ **Resolved in v1.12.0.** `match` on `Result` with `Ok(v)` / `Err(e)` and nested enum variant patterns (`Err(AppError::NotFound)`, `Err(AppError::BadRequest(msg))`) is fully supported. See `examples/result_enum_match.ko`.

## Strings

**Unicode-aware string operations**: As of v0.3.0, `substring(start, end)` and `length()` operate on Unicode code points (characters), not byte offsets. Multi-byte UTF-8 characters (accented letters, CJK, emoji) are handled correctly.

- `"héllo".length()` returns `5` (characters), not `6` (bytes).
- `"héllo".substring(0, 3)` returns `"hél"` (3 characters), not a corrupted byte slice.
- Use `byte_length()` if you need the raw UTF-8 byte count.
- Use `char_count()` as an explicit alias for `length()` when clarity is desired.

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

## Closures

**Ownership analysis for captures**: As of v0.4.0, closures are subject to full ownership analysis. The compiler tracks what each closure captures and enforces move/borrow rules (E0281, E0282, E0283). Capturing a moved variable, using a variable after it was captured by a closure, or having two closures capture the same non-Copy variable are all compile-time errors.

- **Remaining gap**: Closures that return captured references are not yet fully checked for lifetime escapes. The borrow-escapes-scope (E0241) check applies to function parameters but not yet to closure captures that escape via return values.
- **Recommendation**: Prefer capturing owned values or Copy types in closures. Avoid returning closures that capture borrowed references.

## Numeric Limits

**Integer overflow**: Integer arithmetic uses 64-bit signed integers (`i64`). Overflow wraps silently (two's complement behavior) without runtime detection.
