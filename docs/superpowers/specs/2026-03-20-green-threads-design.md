# Green Threads & Concurrency — Design Spec

**Date**: 2026-03-20
**Status**: Approved
**Scope**: Phase 1 (green threads, async/await, spawn parallel) + Phase 2 (generic channels)

## Context

Kōdo v0.6.0 has concurrency primitives that are partially implemented:
- `parallel {}` — real OS threads via `std::thread::scope` (works)
- `spawn {}` — syntax works, but executes sequentially (FIFO scheduler)
- `async`/`await` — parser accepts, runtime thread pool exists, but never connected end-to-end
- Channels — only `Int`, `Bool`, `String`
- Actors — heap state + message passing, but sequential scheduler

This spec replaces the sequential scheduler with a green thread runtime featuring M:N scheduling, work-stealing, and cooperative yielding with compiler-inserted yield points.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Kōdo Source                                        │
│  spawn { task() }    async fn fetch() -> String     │
│  parallel { ... }    let r = fetch().await           │
└──────────────────────┬──────────────────────────────┘
                       │ compiles
┌──────────────────────▼──────────────────────────────┐
│  MIR: Yield Point Insertion                         │
│  - Back-edges of loops (while, for, for-in)         │
│  - Function calls                                   │
│  - I/O builtins (http_get, file_read, etc.)         │
└──────────────────────┬──────────────────────────────┘
                       │ codegen
┌──────────────────────▼──────────────────────────────┐
│  Runtime: Green Thread Engine                       │
│                                                     │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐  │
│  │Worker 0 │ │Worker 1 │ │Worker 2 │ │Worker 3 │  │
│  │(OS thd) │ │(OS thd) │ │(OS thd) │ │(OS thd) │  │
│  ├─────────┤ ├─────────┤ ├─────────┤ ├─────────┤  │
│  │local Q  │ │local Q  │ │local Q  │ │local Q  │  │
│  └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘  │
│       │    steal   │    steal  │    steal   │       │
│       └────────────┴──────────┴────────────┘       │
│                                                     │
│         Green Threads (64KB fixed stacks)           │
└─────────────────────────────────────────────────────┘
```

## Phase 1: Green Threads + async/await

### Green Thread Lifecycle

**Creation** (`spawn` or `async fn` call):
1. Allocate 64KB stack via `mmap` (or stack pool)
2. Create `GreenThread { id, stack_ptr, stack_base, status, context }`
3. Initialize stack frame pointing to lambda-lifted function
4. Enqueue on current worker's local queue

**States:**
```
         spawn
  ┌──────────────┐
  ▼              │
Ready ──run──▶ Running ──yield──▶ Ready
                  │                  ▲
                  │──await──▶ Blocked ─┘ (when future completes)
                  │
                  └──return──▶ Dead (stack freed)
```

**Context Switch:**
1. Save registers (RSP, RBP, RBX, R12-R15 on x86_64; X19-X30, SP on aarch64) to current thread's `context`
2. Scheduler picks next green thread from local queue (or steal)
3. Restore registers from new thread's `context`
4. Jump to saved execution point

Implementation via assembly inline (~20 lines per architecture).

**Destruction:**
1. Status → `Dead`
2. Stack returned to pool or `munmap`'d
3. If someone is `await`ing, propagate result and move waiter to `Ready`

### Yield Points (Compiler-Inserted)

The MIR gains a new instruction: `Instruction::Yield`.

**Where yields are inserted:**
- Back-edges of loops (`while`, `for`, `for-in`) — at loop start
- Function calls — before each `Call` instruction (except trivial builtins)
- I/O builtins — `http_get`, `http_post`, `file_read`, `file_write`, `channel_recv`

**What `Yield` compiles to:**
```
call kodo_green_maybe_yield()
```

`kodo_green_maybe_yield()` in runtime:
- Check thread-local `should_yield` flag
- If false → return immediately (~1ns, single branch)
- If true → save context, switch to next green thread

The `should_yield` flag is set by the scheduler periodically (~100us via timer) or on work-stealing requests.

**Do NOT insert yield in:**
- `assert`, `assert_eq` and test builtins
- Pure arithmetic operations
- Local variable access
- Inline runtime functions (string_length, list_get, etc.)

**Performance impact** for non-concurrent code: ~2-5% overhead in tight loops. Disable with `kodoc build --no-green-threads` (compiles without yield points, uses legacy FIFO scheduler).

### async/await Semantics

**`async fn`:**
```kodo
async fn fetch_data(url: String) -> String {
    let response: String = http_get(url)
    return response
}
```
- Identical to a normal function, except calling it creates a green thread
- Returns `Future<T>` immediately
- Body executes on green thread with normal yield points

**`await`:**
```kodo
let data: String = fetch_data("http://api.example.com").await
```
1. Check if Future already completed → return value immediately
2. If not → mark current green thread as `Blocked`, associate with Future
3. Yield to scheduler
4. When async fn returns → store result in Future → move blocked thread to `Ready`

**`Future<T>` type:**
```kodo
let f: Future<String> = fetch_data(url)  // does not block
// ... do other work ...
let result: String = f.await             // blocks here
```

Opaque type, no methods beyond `.await`. Runtime representation:
```rust
struct FutureEntry {
    status: FutureStatus,  // Pending | Completed(i64) | Failed
    waiters: Vec<GreenThreadId>,
}
```

**`spawn` vs `async`:**

| | `spawn { }` | `async fn()` |
|---|---|---|
| Returns | `Unit` (fire-and-forget) | `Future<T>` (awaitable) |
| Result | Cannot retrieve | Retrieved via `.await` |
| Use case | Side effects, logging | Computation with result |

### Work-Stealing Scheduler

**Structure:**
```rust
struct Scheduler {
    workers: Vec<Worker>,
    global_queue: Mutex<VecDeque<GreenThreadId>>,
    thread_pool: Vec<JoinHandle<()>>,
}

struct Worker {
    id: usize,
    local_queue: crossbeam::deque::Worker<GreenThreadId>,
    current: Option<GreenThreadId>,
    rng: FastRng,
}
```

**Worker loop:**
```
loop {
    1. Pop from local queue
    2. If empty → steal from random worker
    3. If steal failed → pop from global queue
    4. If all empty → park (sleep until notified)

    5. Resume chosen green thread
    6. On yield → push to local queue
    7. On block (await) → don't enqueue (waiter list)
    8. On return → free stack, wake waiters
}
```

**Worker count:** default = `num_cpus`, configurable via `--threads=N`.

**crossbeam-deque:** Each worker has a `crossbeam::deque::Worker` (LIFO push/pop for owner, FIFO steal for others). Good cache locality.

**Shutdown:**
1. `main()` returns → scheduler drains all queues
2. Wait for all green threads to finish (or 5s timeout)
3. Workers park → unpark with shutdown flag → OS threads exit

### Compatibility

- `parallel {}` continues using `std::thread::scope` (real OS threads) — unchanged
- Existing sequential `spawn` code works the same (but now runs on green threads)
- `--no-green-threads` flag falls back to legacy FIFO scheduler

## Phase 2: Generic Channels

### Current (limited)
```kodo
let ch: Channel<Int> = channel_new()
channel_send(ch, 42)
let val: Int = channel_recv(ch)
```
Only `Int`, `Bool`, `String`. Separate runtime functions per type.

### New (generic)
```kodo
let ch: Channel<MyStruct> = Channel::new()
ch.send(MyStruct { x: 1, y: 2 })
let val: MyStruct = ch.recv()
```

### Implementation

Type-erased channels with binary serialization:
```rust
struct Channel {
    sender: Sender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
    value_size: usize,
}
```

- `send(value)` → copy `value_size` bytes from stack to `Vec<u8>` → send
- `recv()` → receive `Vec<u8>` → copy back to receiver's stack

The compiler generates copy code based on type layout (existing `struct_layouts` and `enum_layouts` in codegen).

### Green Thread Integration

`channel_recv()` now yields instead of blocking the OS thread:
1. Try `try_recv()` — if value available, return
2. If empty → mark green thread as `Blocked(ChannelRecv(handle))`
3. Yield to scheduler
4. When `send()` detects waiters → move first waiter to `Ready`

This is the key advantage — thousands of goroutines can wait on channels without blocking OS threads.

## Changes by Crate

### New files
- `crates/kodo_runtime/src/green.rs` — green thread engine (stacks, context switch, scheduler)
- `crates/kodo_runtime/src/green/context_x86_64.s` — context switch assembly (x86_64)
- `crates/kodo_runtime/src/green/context_aarch64.s` — context switch assembly (aarch64)

### Modified files
- `crates/kodo_mir/src/lib.rs` — add `Instruction::Yield`
- `crates/kodo_mir/src/lowering/stmt.rs` — rewrite `lower_spawn_stmt()`, `lower_parallel_stmt()`
- `crates/kodo_mir/src/lowering/expr.rs` — async fn call → spawn green thread + return Future
- `crates/kodo_mir/src/optimize.rs` — yield point insertion pass
- `crates/kodo_codegen/src/instruction.rs` — emit `kodo_green_maybe_yield()` for Yield instruction
- `crates/kodo_codegen/src/builtins.rs` — register green thread runtime functions
- `crates/kodo_runtime/src/scheduler.rs` — replace FIFO with M:N work-stealing scheduler
- `crates/kodo_runtime/src/lib.rs` — new entry point using scheduler
- `crates/kodo_types/src/builtins.rs` — register `Future<T>` type
- `crates/kodo_types/src/lib.rs` — add `Type::Future(Box<Type>)`
- `Cargo.toml` — add `crossbeam-deque`, `num_cpus` dependencies

## New Dependencies
- `crossbeam-deque` — lock-free work-stealing deques
- `num_cpus` — detect number of CPU cores
- `libc` — for `mmap`/`munmap` stack allocation

## Non-Goals (v1)
- Preemptive scheduling (signal-based)
- Growable stacks (stack copying)
- `select` on multiple channels
- Distributed actors (cross-process)
- Structured concurrency enforcement (nurseries/scopes)
- Green thread pinning to specific workers
