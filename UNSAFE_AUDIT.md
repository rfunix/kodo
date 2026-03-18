# Unsafe Audit Report -- Kodo Runtime

**Date**: 2026-03-18
**Auditor**: Claude Opus 4.6 (automated security audit)
**Scope**: All `unsafe` blocks in `crates/kodo_runtime/src/`
**Other crates**: `kodo_codegen`, `kodo_parser`, `kodo_lexer`, `kodo_ast`, `kodo_types`, `kodo_contracts`, `kodo_resolver`, `kodo_mir`, `kodo_std`, `kodoc` -- **zero unsafe blocks found** (clean)

## Executive Summary

All unsafe code in the Kodo compiler is concentrated in `crates/kodo_runtime/`, which is the FFI boundary between Cranelift-generated native code and Rust runtime functions. This is architecturally expected -- the runtime must accept raw pointers from compiled code.

**Total unsafe extern "C" functions**: 127
**Total unsafe fn (internal)**: 8
**Files with unsafe**: 11 (all in `kodo_runtime`)

**Findings**:
- **BUG**: 0
- **RISK**: 5
- **REVIEW**: 8
- **SAFE**: All remaining (~114)

No definite soundness bugs were found. However, several patterns carry inherent risk if caller invariants are violated, and some code could be made safer or better documented.

---

## Summary Table

| File | Line(s) | Classification | Issue |
|------|---------|---------------|-------|
| `test_ops.rs` | 15 | **RISK** | `static mut TEST_FAILED` -- data race if tests run with `--test-threads > 1` |
| `scheduler.rs` | 81, 90, 408, 548, 571 | **RISK** | `std::mem::transmute(fn_ptr)` -- casting i64 to function pointers; unsound if codegen emits wrong signature |
| `io_ops.rs` | 244 | **RISK** | `std::env::set_var` in unsafe block -- inherently unsafe in multi-threaded programs per Rust 1.66+ |
| `io_ops.rs` | 216, 239, 241, 402, 431, 460, 720, 722, 747, 775, 803, 833 | **RISK** | `from_utf8_unchecked` -- UB if codegen ever passes non-UTF-8 data; `from_utf8` would be safer |
| `memory.rs` | 155, 158, 162 | **RISK** | `kodo_rc_dec` uses `unwrap_or(RC_HEADER_SIZE)` for total size -- if handle is somehow corrupted but still in registry, wrong Layout could be passed to dealloc |
| `memory.rs` | 278-284, 293-296, 305-308 | **REVIEW** | `kodo_closure_*` functions have no `// SAFETY:` comment on individual unsafe blocks |
| `scheduler.rs` | 76-100 | **REVIEW** | `kodo_parallel_spawn` -- `fn_ptr` transmute lacks verification; missing `// SAFETY:` on `transmute` calls |
| `collections.rs` | 455-466 | **REVIEW** | `kodo_list_iterator_advance/value` -- not marked `unsafe extern "C"` despite dereferencing raw pointers; relies on caller providing valid handles |
| `collections.rs` | 491-497 | **REVIEW** | `kodo_list_iterator_free` -- not marked `unsafe extern "C"` despite calling `Box::from_raw` |
| `collections.rs` | 442-448 | **REVIEW** | `kodo_list_iter` -- not marked `unsafe extern "C"` despite storing raw pointer |
| `server.rs` | 112 | **REVIEW** | `kodo_http_request_body` casts to `&mut` from shared handle -- potential aliasing violation if called twice |
| `string_ops.rs` | 316-318, 404-407 | **REVIEW** | `kodo_string_trim` and `kodo_string_substring` return pointers into the original buffer without RC tracking -- dangling pointer if source string is freed |
| `scheduler.rs` | 346-358 | **REVIEW** | `kodo_channel_recv_string` allocates via `Box::into_raw` instead of `alloc_string` -- inconsistent with RC memory management |

---

## Detailed Findings

### RISK-1: `static mut TEST_FAILED` (test_ops.rs:15)

```rust
static mut TEST_FAILED: bool = false;
```

**Issue**: `static mut` is unsound if accessed from multiple threads. The comment says "single-threaded compiled Kodo test code," but Rust's test harness runs tests in parallel by default. The runtime's own tests (e.g., `assert_fails_on_zero`) access `TEST_FAILED` -- if two such tests run concurrently, this is a data race (UB).

**Mitigation**: For compiled Kodo test binaries, this is fine (they run in their own process). For the Rust unit tests in `test_ops.rs`, this is technically UB under Miri. The tests work in practice because `--test-threads=1` is common, but it is not enforced.

**Recommendation**: Replace with `AtomicBool` or `thread_local!`. The performance argument (avoiding atomic in hot path) is negligible -- assertion failure is the cold path.

### RISK-2: `std::mem::transmute(fn_ptr)` (scheduler.rs:81,90,408,548,571)

```rust
let func: extern "C" fn() = unsafe { std::mem::transmute(fn_ptr) };
```

**Issue**: These transmute calls convert an `i64` to a function pointer. If the codegen produces a function with a mismatched signature (e.g., `fn(i64) -> i64` where `fn()` is expected), this is instant UB. There is no runtime validation.

**Mitigation**: The codegen is the only producer of these values, so the invariant is maintained by construction. But any codegen bug would produce silent memory corruption rather than a diagnostic error.

**Recommendation**: Consider a debug-mode assertion that validates function pointer alignment and non-null. Also, all transmute calls should have `// SAFETY:` comments (some do, some don't).

### RISK-3: `std::env::set_var` (io_ops.rs:244)

```rust
unsafe { std::env::set_var(key, val) };
```

**Issue**: Since Rust 1.66, `std::env::set_var` is documented as unsafe in multi-threaded contexts (UB per POSIX `setenv`). The Kodo runtime uses threads for `kodo_parallel_join` and the async thread pool. If `env_set` is called concurrently with another thread reading env vars, this is a data race.

**Mitigation**: The comment says "The Kodo runtime serialises env access through the scheduler," but `kodo_env_set` is a direct FFI function callable from any spawned task or parallel thread.

**Recommendation**: Add a `Mutex` guard around `set_var`/`var` pairs, or document that `env_set` must not be called from parallel/async contexts.

### RISK-4: `from_utf8_unchecked` throughout io_ops.rs (12 occurrences)

**Issue**: `std::str::from_utf8_unchecked` produces UB if the input bytes are not valid UTF-8. While the Kodo compiler guarantees string literals are UTF-8, runtime-constructed strings (from HTTP responses, file reads, user input, etc.) might not be. Several call sites (JSON get_bool, get_float, get_array, set_string, set_int, set_bool, set_float, get_object) use `from_utf8_unchecked` for key parameters that could conceivably receive malformed data.

**Mitigation**: The Kodo type system ensures String values are UTF-8 at compile time, and the codegen only passes compiler-validated strings. In practice, this is safe.

**Recommendation**: Replace with `from_utf8` + error handling. The performance difference is negligible for hash map key lookups. This is defense-in-depth against potential future codegen bugs.

### RISK-5: `kodo_rc_dec` fallback Layout (memory.rs:155-162)

```rust
let total = rc_registry::total_size(handle).unwrap_or(RC_HEADER_SIZE);
```

**Issue**: If `total_size` returns `None` (which should never happen for a managed handle), the fallback of `RC_HEADER_SIZE` (8 bytes) would be passed to `dealloc` with a Layout that doesn't match the original allocation. This is UB per the `dealloc` contract.

**Mitigation**: The code only reaches this path for handles that pass the `is_managed()` check, so `total_size()` should always return `Some`. The `unwrap_or` is a safety net against registry corruption.

**Recommendation**: Change to a debug assertion: `debug_assert!(total.is_some())` before the `unwrap_or`, so registry corruption is caught in testing.

---

### REVIEW-1: Missing `// SAFETY:` on closure functions (memory.rs:278-308)

`kodo_closure_new`, `kodo_closure_func`, and `kodo_closure_env` have unsafe blocks without individual `// SAFETY:` comments. Per CLAUDE.md: "NO `unsafe` without a `// SAFETY:` comment."

### REVIEW-2: Iterator functions not marked `unsafe` (collections.rs:442-497)

`kodo_list_iter`, `kodo_list_iterator_advance`, `kodo_list_iterator_value`, and `kodo_list_iterator_free` are declared as `pub extern "C" fn` (safe) but internally dereference raw pointers via unchecked casts. This means safe Rust code could call these with arbitrary i64 values, leading to UB. They should be `pub unsafe extern "C" fn` for correctness.

### REVIEW-3: `kodo_http_request_body` aliasing (server.rs:112)

```rust
let request = unsafe { &mut *(req as *mut tiny_http::Request) };
```

This creates a mutable reference from a raw pointer. If `kodo_http_request_body` is called twice on the same request handle, two `&mut` references exist (undefined behavior). The tiny_http `as_reader()` consumes the body reader, so calling it twice is a logic error that could trigger UB via the aliasing violation.

### REVIEW-4: Trim/substring return borrowed pointers (string_ops.rs:316-318, 404-407)

`kodo_string_trim` and `kodo_string_substring` write pointers directly into the source string's buffer (no allocation, no RC). If the source string is freed before the trim/substring result is used, this becomes a dangling pointer. This is by design (performance), but it's a footgun that should be documented more prominently.

### REVIEW-5: Channel recv_string uses Box::into_raw (scheduler.rs:346-358)

`kodo_channel_recv_string` allocates the received string via `Box::into_raw` instead of `alloc_string`. This means the returned memory is NOT RC-managed, creating inconsistency with other string-producing functions. If the MIR emits `DecRef` for this string, `kodo_rc_dec` will be a no-op (safe), but the memory will leak.

---

## Patterns Verified as SAFE

The following patterns are used extensively and are correct:

1. **`std::slice::from_raw_parts(ptr, len)`**: Used in all string functions. Always guarded by the function-level `# Safety` contract. Correct.

2. **`Box::into_raw` / `Box::from_raw`**: Used for opaque handle management (JSON, DB, HTTP server, lists, maps). The pattern is sound as long as each handle is freed exactly once. Null checks prevent double-free on 0 handles.

3. **`std::alloc::alloc` / `dealloc` / `realloc`**: Used in `memory.rs` and `collections.rs`. Layouts are computed correctly; null checks on allocation results are present.

4. **`std::ptr::copy_nonoverlapping`**: Used in `alloc_string`, `kodo_spawn_task_with_env`, `kodo_parallel_spawn`. Source/dest don't overlap by construction.

5. **`extern "C" fn` declarations on main (lib.rs:66)**: Standard FFI entry point with proper SAFETY comments.

6. **RC registry pattern (memory.rs)**: Thread-local HashMap with `RefCell` is sound for single-threaded runtime. The `is_managed()` guard prevents operating on non-RC pointers.

---

## Metrics

| Category | Count |
|----------|-------|
| Total unsafe extern "C" fn | 127 |
| Total unsafe fn (internal) | 8 |
| Total `std::mem::transmute` | 5 |
| Total `from_utf8_unchecked` | 12 |
| Total `static mut` | 1 |
| Total `Box::into_raw` / `Box::from_raw` pairs | ~25 |
| Total `std::alloc::alloc/dealloc` calls | ~8 |
| Files with unsafe | 11 (all `kodo_runtime`) |
| Files without unsafe | All other crates (10 crates, ~50+ files) |

## Recommendations (Priority Order)

1. **Replace `static mut TEST_FAILED` with `AtomicBool`** -- eliminates data race risk in test suite (RISK-1)
2. **Replace `from_utf8_unchecked` with `from_utf8`** in io_ops.rs -- eliminates 12 potential UB sites for negligible performance cost (RISK-4)
3. **Mark iterator functions as `unsafe extern "C"`** in collections.rs -- prevents safe code from calling them with invalid handles (REVIEW-2)
4. **Add `// SAFETY:` comments to all closure functions** in memory.rs (REVIEW-1)
5. **Add debug_assert on RC registry total_size** in memory.rs:158 (RISK-5)
6. **Guard `env::set_var` with a Mutex** or document single-thread-only restriction (RISK-3)
7. **Use `alloc_string` in `kodo_channel_recv_string`** for RC consistency (REVIEW-5)
8. **Document trim/substring borrowing semantics** more prominently (REVIEW-4)

---

## Conclusion

The Kodo runtime's unsafe code is **well-structured and consistently documented**. Every `unsafe extern "C"` function has `# Safety` doc comments, and most inline `unsafe` blocks have `// SAFETY:` comments. No definite soundness bugs were found. The 5 RISK items are latent issues that depend on invariants maintained by the codegen; they represent defense-in-depth opportunities rather than exploitable vulnerabilities. The codebase demonstrates disciplined use of unsafe Rust for an FFI runtime.
