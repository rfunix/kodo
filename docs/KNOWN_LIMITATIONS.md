# Known Limitations — Kōdo v0.7.0

This document lists the known limitations of the current alpha release. These are deliberate trade-offs or features not yet fully implemented.

## Memory

**Atomic reference counting for memory management**: As of v0.4.0, all heap-allocated values (strings, closures, collections) use **atomic reference counting** (Atomic RC) for thread-safe memory management. String operations (concatenation, replace, to_upper, to_lower, int/float/bool to_string, etc.) allocate via the RC allocator (`kodo_alloc`). Strings are freed when their reference count drops to zero via `kodo_rc_dec_string`. The MIR's `IncRef`/`DecRef` instructions properly track string lifetimes. The atomic operations ensure correctness when values are shared across `parallel {}` blocks and `spawn` tasks.

- **Remaining gap**: Strings stored inside `List<String>` (e.g., from `split`, `lines`, `args`) use RC-managed memory for the string data, but the `[ptr, len]` pair metadata is still allocated via `Box::into_raw`. These pairs are freed when the list is freed.
- **Recommendation**: String memory management is now automatic for most use cases. Long-running services with heavy string operations should see significantly reduced memory growth compared to earlier releases.

## Concurrency

**Async execution**: In v1, `async fn` calls execute synchronously and return their result directly. The runtime infrastructure for true futures (create Future, spawn green thread, await later) exists but is not yet wired end-to-end in the MIR lowering.

- **Impact**: `async fn` works but doesn't provide true concurrency yet. `spawn {}` does run on green threads.
- **Recommendation**: Use `spawn {}` for fire-and-forget concurrency. Use `parallel {}` for structured parallelism.

**No channel select**: Cannot wait on multiple channels simultaneously.

- **Plan**: Go-style `select` statement in a future release.

**Fixed green thread stack**: Each green thread gets 64KB. Deep recursion may overflow.

- **Plan**: Growable stacks in a future release.

## Error Handling

**Result\<T, E\> error type**: While `Result<T, E>` is fully supported syntactically, the error type `E` is always `String` in practice. Custom error enums do not work end-to-end through the type checker and codegen.

- **Impact**: Error handling works, but you cannot define domain-specific error types.
- **Recommendation**: Use `Result<T, String>` with descriptive error messages.

**Result\<T, E\> unwrap methods**: `unwrap()` and `unwrap_err()` are available and return the correct polymorphic type (e.g., `String` from `Result<String, String>`). On the wrong variant, the program traps (abnormal termination) without a descriptive error message.

- **Impact**: Useful for prototyping, but production code should use `is_ok()`/`is_err()` checks.
- **Recommendation**: Guard `unwrap()` calls with an `is_ok()` check, or use `unwrap_or()` for safe defaults.

**Result pattern matching**: `match` on `Result` with `Ok(value)` / `Err(msg)` destructuring patterns is not yet supported. Use `is_ok()`/`is_err()` + `unwrap()`/`unwrap_err()` instead.

- **Plan**: Full pattern matching on enum variants in a future release.

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
