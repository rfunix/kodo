# Design: Growable Stacks for Green Threads

## Status: Draft

## Problem

Green threads currently use fixed 64KB stacks allocated via `mmap`. This limits:
- Deep recursion (e.g., `fib(40)` in a green thread)
- Functions with large local variables
- Stack-heavy patterns like recursive descent parsing

## Current Implementation

File: `crates/kodo_runtime/src/green.rs`

```rust
fn get_stack_size() -> usize {
    let ps = page_size();
    ps * 16  // 16 pages = 64KB on most systems
}

unsafe fn alloc_stack(size: usize) -> *mut u8 {
    let ptr = libc::mmap(..., size, PROT_READ | PROT_WRITE, ...);
    libc::mprotect(ptr, guard_size, PROT_NONE);  // guard page
    ptr
}
```

Each green thread gets:
- 1 guard page (PROT_NONE) at the bottom
- 15 usable pages (60KB) above

Stack overflow hits the guard page → SIGSEGV → process abort.

## Options

### Option A: Signal-based Stack Growth

When SIGSEGV hits the guard page, instead of aborting:
1. Allocate a new, larger stack (2x current)
2. Copy stack contents to new location
3. Fix up stack pointers (frame pointer, saved registers)
4. Resume execution

**Pros**: Transparent to user code, stacks grow on demand
**Cons**: Complex pointer fixup, platform-specific signal handling, potential issues with saved register pointers

### Option B: Segmented Stacks (like Go pre-1.4)

At function entry, check remaining stack space. If insufficient:
1. Allocate a new stack segment
2. Set up a "stack link" to return to the previous segment
3. Execute function on new segment

**Pros**: No pointer fixup needed, deterministic
**Cons**: "Hot split" problem (function at boundary thrashes), requires compiler cooperation (MIR changes)

### Option C: Contiguous Stack Copying (like Go 1.4+)

Combine options A and B:
1. Detect stack overflow via guard page
2. Allocate a 2x larger contiguous stack
3. Copy entire stack
4. Walk the stack and fix all pointers

**Pros**: Best performance (no hot split), used by Go
**Cons**: Most complex to implement, requires stack walking

## Recommendation

**Option A (signal-based growth)** for v1.10.0 — simplest to implement, and our green threads already have guard pages.

Implementation steps:
1. Register SIGSEGV handler in `kodo_green_init`
2. In handler: check if fault address is in a guard page
3. If yes: `mprotect` the guard page to PROT_READ|PROT_WRITE, allocate new guard below
4. If stack reaches maximum (e.g., 8MB), abort with clear error message

This is a "grow downward" approach — each growth step makes the guard page writable and adds a new guard page below the current allocation. Requires re-mmapping with a larger region.

## Files to Modify

| File | Change |
|------|--------|
| `crates/kodo_runtime/src/green.rs` | Signal handler, stack growth logic |
| `crates/kodo_runtime/src/context.rs` | Stack bounds tracking |
| `crates/kodo_mir/src/yield_insertion.rs` | Optional: stack check at function entry |

## Testing

- Unit test: recursive function exceeding 64KB → grows instead of crashing
- Benchmark: fib(40) in green thread vs main thread
- Stress test: 1000 green threads each using 256KB of stack

## Timeline

Estimated: 7-10 days for Option A implementation.
