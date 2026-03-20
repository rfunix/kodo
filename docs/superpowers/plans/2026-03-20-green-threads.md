# Green Threads & Concurrency — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Kōdo's sequential task scheduler with a green thread runtime featuring M:N scheduling, work-stealing, compiler-inserted yield points, real async/await, and generic channels.

**Architecture:** The runtime gets a new `green.rs` module with context-switch assembly (x86_64 + aarch64), a work-stealing scheduler using `crossbeam-deque`, and 64KB fixed stacks. The MIR gains a `Yield` instruction inserted automatically at loop back-edges and function calls. `spawn` creates green threads instead of enqueueing to FIFO. `async fn` returns `Future<T>`, `.await` suspends the green thread. Channels become type-erased to support any type.

**Tech Stack:** Rust, Cranelift, crossbeam-deque, num_cpus, libc (mmap), inline assembly (x86_64/aarch64)

**Spec:** `docs/superpowers/specs/2026-03-20-green-threads-design.md`

---

## File Map

### New Files
- `crates/kodo_runtime/src/green.rs` — green thread engine: GreenThread struct, stack allocation, scheduler, work-stealing
- `crates/kodo_runtime/src/context.rs` — context switch: save/restore registers, platform-specific assembly
- `crates/kodo_runtime/src/channel_generic.rs` — type-erased generic channels

### Modified Files
- `Cargo.toml` — add `crossbeam-deque`, `num_cpus`, `libc` to workspace dependencies
- `crates/kodo_runtime/Cargo.toml` — add new dependencies
- `crates/kodo_runtime/src/lib.rs` — replace scheduler entry, export new modules
- `crates/kodo_runtime/src/scheduler.rs` — keep parallel/channel/actor code, remove FIFO task queue (replaced by green.rs)
- `crates/kodo_mir/src/lib.rs` — add `Instruction::Yield`
- `crates/kodo_mir/src/optimize.rs` — yield point insertion pass
- `crates/kodo_mir/src/lowering/stmt.rs` — rewrite `lower_spawn_stmt()` to create green threads
- `crates/kodo_mir/src/lowering/expr.rs` — rewrite `Expr::Await` to suspend green thread
- `crates/kodo_codegen/src/builtins.rs` — declare green thread runtime functions
- `crates/kodo_codegen/src/instruction.rs` — translate `Instruction::Yield`
- `crates/kodo_types/src/lib.rs` — add `Type::Future(Box<Type>)`
- `crates/kodo_types/src/builtins.rs` — register green thread builtins
- `crates/kodo_types/src/expr.rs` — type-check await expressions properly
- `crates/kodo_mir/src/lowering/registry.rs` — register green thread return types
- `crates/kodoc/src/main.rs` — add `--threads=N` and `--no-green-threads` flags
- `docs/guide/concurrency.md` — rewrite with green threads
- `docs/guide/actors.md` — update for green thread scheduler
- `docs/KNOWN_LIMITATIONS.md` — remove resolved limitations
- `README.md` — update features table
- Website: `~/dev/kodo-website/src/content/docs/guide/concurrency.md`
- Website: `~/dev/kodo-website/src/content/docs/guide/actors.md`
- Website: `~/dev/kodo-website/public/llms.txt`

---

## Phase 1: Green Thread Runtime Foundation

### Task 1: Dependencies and Context Switch

**Files:**
- Modify: `Cargo.toml` (workspace deps)
- Modify: `crates/kodo_runtime/Cargo.toml`
- Create: `crates/kodo_runtime/src/context.rs`

- [ ] **Step 1: Add dependencies to workspace Cargo.toml**

Add to `[workspace.dependencies]`:
```toml
crossbeam-deque = "0.8"
num_cpus = "1.16"
libc = "0.2"
```

Add to `crates/kodo_runtime/Cargo.toml` `[dependencies]`:
```toml
crossbeam-deque = { workspace = true }
num_cpus = { workspace = true }
libc = { workspace = true }
```

- [ ] **Step 2: Create context.rs with context switch**

Create `crates/kodo_runtime/src/context.rs` with:

```rust
//! CPU context save/restore for green thread switching.
//!
//! Provides platform-specific register save/restore using inline assembly.
//! Supports x86_64 (System V ABI) and aarch64 (AAPCS64).

/// Saved CPU context for a suspended green thread.
#[repr(C)]
#[derive(Debug, Default)]
pub struct Context {
    // x86_64: RSP, RBP, RBX, R12, R13, R14, R15, RIP (8 registers)
    // aarch64: X19-X29, X30(LR), SP (13 registers)
    pub regs: [u64; 16],  // enough for both platforms
}

/// Switches from `old` context to `new` context.
/// Saves current registers into `old`, loads registers from `new`.
///
/// # Safety
/// Both pointers must be valid Context structs with proper stack pointers.
#[inline(never)]
pub unsafe fn switch_context(old: *mut Context, new: *const Context) {
    #[cfg(target_arch = "x86_64")]
    switch_context_x86_64(old, new);

    #[cfg(target_arch = "aarch64")]
    switch_context_aarch64(old, new);
}

/// Initializes a context to start execution at `entry` with the given stack.
///
/// # Safety
/// `stack_top` must point to the top of a valid, aligned stack allocation.
pub unsafe fn init_context(
    ctx: &mut Context,
    stack_top: *mut u8,
    entry: extern "C" fn(usize),
    arg: usize,
) {
    // Platform-specific stack frame setup
    #[cfg(target_arch = "x86_64")]
    {
        let sp = (stack_top as usize & !0xF) - 8; // 16-byte align, space for return addr
        // regs[0] = RSP, regs[1] = RBP, regs[7] = RIP (entry point)
        ctx.regs[0] = sp as u64;           // RSP
        ctx.regs[1] = sp as u64;           // RBP
        ctx.regs[7] = entry as u64;        // RIP (return address)
        ctx.regs[2] = arg as u64;          // RBX (first arg via register)
        // Write return address on stack
        *(sp as *mut u64) = trampoline as u64;
    }
    #[cfg(target_arch = "aarch64")]
    {
        let sp = stack_top as usize & !0xF; // 16-byte align
        ctx.regs[12] = sp as u64;           // SP
        ctx.regs[11] = entry as u64;        // X30 (LR)
        ctx.regs[0] = arg as u64;           // X19 (arg)
    }
}
```

Implement `switch_context_x86_64` and `switch_context_aarch64` using `core::arch::asm!`:

```rust
#[cfg(target_arch = "x86_64")]
#[inline(never)]
unsafe fn switch_context_x86_64(old: *mut Context, new: *const Context) {
    core::arch::asm!(
        // Save callee-saved registers to old context
        "mov [rdi + 0x00], rsp",
        "mov [rdi + 0x08], rbp",
        "mov [rdi + 0x10], rbx",
        "mov [rdi + 0x18], r12",
        "mov [rdi + 0x20], r13",
        "mov [rdi + 0x28], r14",
        "mov [rdi + 0x30], r15",
        // Load callee-saved registers from new context
        "mov rsp, [rsi + 0x00]",
        "mov rbp, [rsi + 0x08]",
        "mov rbx, [rsi + 0x10]",
        "mov r12, [rsi + 0x18]",
        "mov r13, [rsi + 0x20]",
        "mov r14, [rsi + 0x28]",
        "mov r15, [rsi + 0x30]",
        "ret",  // jump to new context's return address
        in("rdi") old,
        in("rsi") new,
        options(noreturn)
    );
}
```

Similar for aarch64 with X19-X29, X30, SP.

- [ ] **Step 3: Write tests for context switch**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn context_struct_is_default() {
        let ctx = super::Context::default();
        assert!(ctx.regs.iter().all(|&r| r == 0));
    }

    #[test]
    fn switch_context_roundtrip() {
        // Create two contexts, switch between them, verify state
        unsafe {
            let mut ctx_a = super::Context::default();
            let mut ctx_b = super::Context::default();
            // Setup ctx_b with a simple function that sets a flag
            // Switch A→B, B runs and switches back to A
            // Verify flag was set
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p kodo_runtime context`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/kodo_runtime/
git commit -m "runtime: add context switch assembly for green threads (x86_64 + aarch64)"
```

---

### Task 2: Green Thread Engine — Core Data Structures

**Files:**
- Create: `crates/kodo_runtime/src/green.rs`
- Modify: `crates/kodo_runtime/src/lib.rs`

- [ ] **Step 1: Create green.rs with GreenThread and stack allocation**

```rust
//! Green thread engine — lightweight cooperative threads with M:N scheduling.

use crate::context::Context;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};

const STACK_SIZE: usize = 64 * 1024;  // 64KB per green thread

/// Unique identifier for a green thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GreenThreadId(u64);

static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);

impl GreenThreadId {
    fn next() -> Self {
        Self(NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Status of a green thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    Ready,
    Running,
    Blocked,
    Dead,
}

/// A green thread with its own stack and execution context.
pub struct GreenThread {
    pub id: GreenThreadId,
    pub status: ThreadStatus,
    pub context: Context,
    stack: *mut u8,           // mmap'd stack base
    stack_size: usize,
    pub future_id: Option<u64>,  // if this thread backs a Future
    pub result: Option<i64>,     // return value for Future
}
```

- [ ] **Step 2: Implement stack allocation via mmap**

```rust
/// Allocates a fixed-size stack using mmap.
unsafe fn alloc_stack(size: usize) -> *mut u8 {
    let ptr = libc::mmap(
        std::ptr::null_mut(),
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        -1,
        0,
    );
    if ptr == libc::MAP_FAILED {
        panic!("failed to allocate green thread stack");
    }
    ptr as *mut u8
}

/// Frees a stack allocated by alloc_stack.
unsafe fn free_stack(ptr: *mut u8, size: usize) {
    libc::munmap(ptr as *mut libc::c_void, size);
}
```

- [ ] **Step 3: Implement GreenThread::new()**

```rust
impl GreenThread {
    /// Creates a new green thread that will execute `entry(arg)`.
    pub unsafe fn new(entry: extern "C" fn(usize), arg: usize) -> Self {
        let stack = alloc_stack(STACK_SIZE);
        let stack_top = stack.add(STACK_SIZE);
        let mut ctx = Context::default();
        crate::context::init_context(&mut ctx, stack_top, entry, arg);
        Self {
            id: GreenThreadId::next(),
            status: ThreadStatus::Ready,
            context: ctx,
            stack,
            stack_size: STACK_SIZE,
            future_id: None,
            result: None,
        }
    }
}

impl Drop for GreenThread {
    fn drop(&mut self) {
        unsafe { free_stack(self.stack, self.stack_size); }
    }
}
```

- [ ] **Step 4: Export in lib.rs**

Add `pub mod green;` and `pub mod context;` to `crates/kodo_runtime/src/lib.rs`.

- [ ] **Step 5: Write tests**

Test stack allocation/deallocation, GreenThread creation, ID uniqueness.

- [ ] **Step 6: Run tests and commit**

```bash
git commit -m "runtime: add GreenThread struct with 64KB mmap stacks"
```

---

### Task 3: Work-Stealing Scheduler

**Files:**
- Modify: `crates/kodo_runtime/src/green.rs`

- [ ] **Step 1: Implement Worker and Scheduler structs**

```rust
use crossbeam_deque::{self as deque, Steal};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

struct Worker {
    id: usize,
    local: deque::Worker<GreenThreadId>,
    stealer: deque::Stealer<GreenThreadId>,  // shared with other workers
}

pub struct Scheduler {
    workers: Vec<Worker>,
    stealers: Vec<deque::Stealer<GreenThreadId>>,  // all stealers for stealing
    global_queue: Mutex<std::collections::VecDeque<GreenThreadId>>,
    threads: HashMap<GreenThreadId, GreenThread>,
    num_workers: usize,
}
```

- [ ] **Step 2: Implement scheduler spawn and run**

```rust
impl Scheduler {
    pub fn new(num_workers: usize) -> Self { ... }

    /// Spawns a new green thread on the current worker's local queue.
    pub fn spawn(&mut self, entry: extern "C" fn(usize), arg: usize) -> GreenThreadId { ... }

    /// Main scheduler loop — starts worker threads, runs until all green threads complete.
    pub fn run(&mut self) { ... }
}
```

The run loop:
1. Create `num_workers` OS threads
2. Each worker thread runs a loop: pop local → steal → pop global → park
3. On resume: switch_context to green thread
4. On yield: switch_context back to worker, push to local queue
5. On block: don't enqueue, add to waiter list
6. On return: mark Dead, wake waiters, free stack

- [ ] **Step 3: Implement work-stealing**

```rust
fn try_steal(&self, worker_id: usize) -> Option<GreenThreadId> {
    let mut rng = fastrand::Rng::new();
    let n = self.stealers.len();
    for _ in 0..n {
        let victim = rng.usize(..n);
        if victim == worker_id { continue; }
        match self.stealers[victim].steal() {
            Steal::Success(id) => return Some(id),
            _ => continue,
        }
    }
    None
}
```

- [ ] **Step 4: Implement the yield point runtime function**

```rust
/// Called by compiled code at yield points.
/// Checks if the current green thread should yield.
#[no_mangle]
pub unsafe extern "C" fn kodo_green_maybe_yield() {
    // Check thread-local should_yield flag
    // If true: save context, switch to scheduler
    // If false: return immediately (fast path ~1ns)
}
```

- [ ] **Step 5: Write tests**

Test scheduler with multiple green threads, work-stealing, yield behavior.

- [ ] **Step 6: Run tests and commit**

```bash
git commit -m "runtime: implement M:N work-stealing scheduler with green threads"
```

---

### Task 4: Future Type and Await

**Files:**
- Modify: `crates/kodo_runtime/src/green.rs`
- Modify: `crates/kodo_types/src/lib.rs`
- Modify: `crates/kodo_types/src/builtins.rs`
- Modify: `crates/kodo_types/src/expr.rs`

- [ ] **Step 1: Add Future<T> to the type system**

In `crates/kodo_types/src/lib.rs`, add to `Type` enum:
```rust
/// A future value that will be available when a green thread completes.
Future(Box<Type>),
```

Update `Display` impl, `is_copy()`, and any exhaustive matches on `Type`.

- [ ] **Step 2: Type-check await expressions**

In `crates/kodo_types/src/expr.rs`, find the `Expr::Await` handler and implement:
```rust
Expr::Await { operand, span } => {
    let operand_ty = self.infer_expr(operand)?;
    match operand_ty {
        Type::Future(inner) => Ok(*inner),
        _ => Err(TypeError::AwaitOnNonFuture { ty: operand_ty, span: *span }),
    }
}
```

- [ ] **Step 3: Type-check async fn return type**

When a function is marked `is_async`, its return type `T` becomes `Future<T>` at call sites.

- [ ] **Step 4: Add FutureEntry to runtime**

```rust
pub struct FutureEntry {
    pub completed: AtomicBool,
    pub result: Mutex<Option<i64>>,
    pub waiters: Mutex<Vec<GreenThreadId>>,
}

/// Creates a new future, returns handle.
#[no_mangle]
pub unsafe extern "C" fn kodo_future_new() -> i64 { ... }

/// Completes a future with a result, wakes waiters.
#[no_mangle]
pub unsafe extern "C" fn kodo_future_complete(handle: i64, result: i64) { ... }

/// Awaits a future — suspends green thread if not ready.
#[no_mangle]
pub unsafe extern "C" fn kodo_future_await(handle: i64) -> i64 { ... }
```

- [ ] **Step 5: Register builtins**

Add `Future<T>` related builtins in type checker and MIR registry.

- [ ] **Step 6: Run tests and commit**

```bash
git commit -m "types: add Future<T> type, implement await type checking"
```

---

### Task 5: MIR — Yield Instruction and Insertion Pass

**Files:**
- Modify: `crates/kodo_mir/src/lib.rs`
- Modify: `crates/kodo_mir/src/optimize.rs`

- [ ] **Step 1: Add Yield instruction to MIR**

In `crates/kodo_mir/src/lib.rs`, add to `Instruction` enum after `DecRef`:
```rust
/// Yield control to another green thread.
/// Compiled to `kodo_green_maybe_yield()` call.
Yield,
```

- [ ] **Step 2: Implement yield point insertion pass**

In `crates/kodo_mir/src/optimize.rs`, add a pass that inserts `Instruction::Yield` at:
- Start of each loop body (back-edges of `while`, `for`, `for-in`)
- Before each `Instruction::Call` (except trivial builtins like `print_int`, `assert_eq`)

```rust
/// Inserts yield points at loop back-edges and function calls.
pub fn insert_yield_points(functions: &mut [MirFunction]) {
    for func in functions {
        for block in &mut func.blocks {
            let mut new_instructions = Vec::new();
            for inst in &block.instructions {
                // Insert yield before Call (except trivial builtins)
                if let Instruction::Call { callee, .. } = inst {
                    if !is_trivial_builtin(callee) {
                        new_instructions.push(Instruction::Yield);
                    }
                }
                new_instructions.push(inst.clone());
            }
            block.instructions = new_instructions;
        }
        // Insert yield at loop back-edges (Goto targets that point backward)
        // ... (detect back-edges in CFG)
    }
}
```

- [ ] **Step 3: Add --no-green-threads flag to skip insertion**

Read the flag from kodoc and conditionally skip the pass.

- [ ] **Step 4: Write tests**

Test that yield points are inserted at correct locations in MIR.

- [ ] **Step 5: Run tests and commit**

```bash
git commit -m "mir: add Yield instruction and yield point insertion pass"
```

---

### Task 6: MIR — Rewrite spawn and async/await lowering

**Files:**
- Modify: `crates/kodo_mir/src/lowering/stmt.rs`
- Modify: `crates/kodo_mir/src/lowering/expr.rs`

- [ ] **Step 1: Rewrite lower_spawn_stmt()**

Change `kodo_spawn_task` / `kodo_spawn_task_with_env` calls to `kodo_green_spawn` / `kodo_green_spawn_with_env`:

```rust
fn lower_spawn_stmt(&mut self, body: &Block) -> Result<Value> {
    // Same lambda-lifting as before
    // But emit kodo_green_spawn instead of kodo_spawn_task
    // This creates a green thread instead of enqueueing to FIFO
}
```

- [ ] **Step 2: Rewrite Await lowering**

Replace the stub at line 152 with:
```rust
Expr::Await { operand, .. } => {
    let future_val = self.lower_expr(operand)?;
    let result_local = self.alloc_local(/* inferred return type */, false);
    self.emit(Instruction::Call {
        dest: result_local,
        callee: "kodo_future_await".to_string(),
        args: vec![future_val],
    });
    Ok(Value::Local(result_local))
}
```

- [ ] **Step 3: Handle async fn calls**

When calling an `async fn`, the lowering should:
1. Create a future: `kodo_future_new()`
2. Spawn a green thread that runs the function body and completes the future
3. Return the future handle

- [ ] **Step 4: Run tests and commit**

```bash
git commit -m "mir: rewrite spawn/await lowering for green threads"
```

---

### Task 7: Codegen — Yield and Green Thread Builtins

**Files:**
- Modify: `crates/kodo_codegen/src/builtins.rs`
- Modify: `crates/kodo_codegen/src/instruction.rs`
- Modify: `crates/kodo_mir/src/lowering/registry.rs`

- [ ] **Step 1: Declare green thread builtins**

In `crates/kodo_codegen/src/builtins.rs`, add `declare_green_thread_builtins()`:
```rust
fn declare_green_thread_builtins(...) {
    decl_void!("kodo_green_maybe_yield", "green_yield", []);
    decl_void!("kodo_green_spawn", "green_spawn", [types::I64]);
    decl_void!("kodo_green_spawn_with_env", "green_spawn_env", [types::I64, types::I64, types::I64]);
    decl_ret!("kodo_future_new", "future_new", [], types::I64);
    decl_void!("kodo_future_complete", "future_complete", [types::I64, types::I64]);
    decl_ret!("kodo_future_await", "future_await", [types::I64], types::I64);
}
```

- [ ] **Step 2: Handle Yield instruction in codegen**

In `crates/kodo_codegen/src/instruction.rs`, add handler for `Instruction::Yield`:
```rust
Instruction::Yield => {
    // Emit call to kodo_green_maybe_yield()
    let yield_func = builtins.get("kodo_green_maybe_yield").unwrap();
    let fref = module.declare_func_in_func(yield_func.func_id, builder.func);
    builder.ins().call(fref, &[]);
}
```

- [ ] **Step 3: Register return types in MIR registry**

- [ ] **Step 4: Run tests and commit**

```bash
git commit -m "codegen: add green thread builtins and Yield instruction translation"
```

---

### Task 8: Integration — Wire Everything Together

**Files:**
- Modify: `crates/kodo_runtime/src/lib.rs`
- Modify: `crates/kodo_runtime/src/scheduler.rs`
- Modify: `crates/kodoc/src/main.rs`
- Modify: `crates/kodoc/src/commands/build.rs`

- [ ] **Step 1: Replace entry point**

In `crates/kodo_runtime/src/lib.rs`, change `main()`:
```rust
// Before: sequential
kodo_main();
scheduler::kodo_run_scheduler();

// After: green thread based
green::init_scheduler(num_threads);
green::spawn_main(kodo_main);  // run main() as first green thread
green::run_scheduler();         // blocks until all green threads complete
```

- [ ] **Step 2: Keep parallel {} backward compatible**

`parallel {}` continues using `std::thread::scope` — no changes needed. The existing `kodo_parallel_begin/spawn/join` functions remain.

- [ ] **Step 3: Add CLI flags**

In `kodoc/src/main.rs`, add:
- `--threads=N` — passed to runtime via environment variable or metadata
- `--no-green-threads` — skips yield point insertion, uses legacy scheduler

- [ ] **Step 4: Add yield insertion to build pipeline**

In `kodoc/src/commands/build.rs`, after MIR lowering and before codegen:
```rust
if !no_green_threads {
    kodo_mir::insert_yield_points(&mut all_mir_functions);
}
```

- [ ] **Step 5: End-to-end test**

Create `examples/green_threads.ko`:
```kodo
module green_threads {
    meta { purpose: "Green threads demo" version: "0.1.0" }

    fn main() -> Int {
        spawn { print_int(1) }
        spawn { print_int(2) }
        spawn { print_int(3) }
        print_int(0)
        return 0
    }
}
```

Run: `cargo run -p kodoc -- build examples/green_threads.ko && ./examples/green_threads`
Expected: prints 0, 1, 2, 3 (order may vary — they're on green threads now).

- [ ] **Step 6: Run full test suite**

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
make ui-test
```

- [ ] **Step 7: Commit**

```bash
git commit -m "cli: wire green thread runtime into build pipeline"
```

---

## Phase 2: Generic Channels

### Task 9: Type-Erased Channel Runtime

**Files:**
- Create: `crates/kodo_runtime/src/channel_generic.rs`

- [ ] **Step 1: Implement type-erased channel**

```rust
//! Generic channels — type-erased binary serialization for any Kōdo type.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

struct GenericChannel {
    sender: mpsc::Sender<Vec<u8>>,
    receiver: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
    value_size: usize,
}

#[no_mangle]
pub unsafe extern "C" fn kodo_channel_generic_new(value_size: i64) -> i64 { ... }

#[no_mangle]
pub unsafe extern "C" fn kodo_channel_generic_send(handle: i64, data_ptr: i64, data_size: i64) { ... }

#[no_mangle]
pub unsafe extern "C" fn kodo_channel_generic_recv(handle: i64, out_ptr: i64, data_size: i64) { ... }
```

- [ ] **Step 2: Integrate with green thread scheduler**

`recv` should yield the green thread instead of blocking the OS thread:
```rust
pub unsafe extern "C" fn kodo_channel_generic_recv(handle: i64, out_ptr: i64, data_size: i64) {
    loop {
        match try_recv(handle) {
            Some(data) => { copy to out_ptr; return; }
            None => { kodo_green_maybe_yield(); }  // yield and retry
        }
    }
}
```

- [ ] **Step 3: Update codegen for generic channels**

The compiler generates `kodo_channel_generic_send/recv` with the type's layout size.

- [ ] **Step 4: Write tests and commit**

```bash
git commit -m "runtime: add generic type-erased channels with green thread integration"
```

---

### Task 10: Channel Type Checking and Codegen

**Files:**
- Modify: `crates/kodo_types/src/builtins.rs`
- Modify: `crates/kodo_codegen/src/builtins.rs`
- Modify: `crates/kodo_codegen/src/instruction.rs`

- [ ] **Step 1: Update Channel type checking**

Allow `Channel<T>` for any type T, not just Int/Bool/String.

- [ ] **Step 2: Update codegen**

When compiling `channel_send(ch, value)` and `channel_recv(ch)`:
- Look up the layout size of the channel's type parameter
- Emit `kodo_channel_generic_send(handle, &value, size)` / `kodo_channel_generic_recv(handle, &out, size)`

- [ ] **Step 3: Write test with struct channel**

```kodo
module channel_struct_test {
    meta { purpose: "Test generic channels" version: "0.1.0" }
    struct Point { x: Int, y: Int }

    fn main() -> Int {
        let ch: Channel<Point> = Channel::new()
        spawn {
            ch.send(Point { x: 1, y: 2 })
        }
        let p: Point = ch.recv()
        print_int(p.x)
        return 0
    }
}
```

- [ ] **Step 4: Run tests and commit**

```bash
git commit -m "codegen: support generic channels for any type"
```

---

## Phase 3: Testing, Documentation, Release

### Task 11: Examples and UI Tests

**Files:**
- Create: `examples/green_threads.ko`
- Create: `examples/async_await.ko`
- Create: `examples/channel_generic.ko`
- Create: `tests/ui/concurrency/green_spawn.ko`
- Create: `tests/ui/concurrency/async_await.ko`
- Create: `tests/ui/concurrency/channel_generic.ko`

- [ ] **Step 1: Create examples**

`examples/async_await.ko`:
```kodo
module async_demo {
    meta { purpose: "Async/await demo" version: "0.1.0" }

    async fn compute(x: Int) -> Int {
        return x * x
    }

    fn main() -> Int {
        let f1: Future<Int> = compute(5)
        let f2: Future<Int> = compute(10)
        let a: Int = f1.await
        let b: Int = f2.await
        print_int(a + b)
        return 0
    }
}
```

- [ ] **Step 2: Create UI tests with `//@ check-pass`**

- [ ] **Step 3: Run all tests**

```bash
cargo test --workspace && make ui-test
```

- [ ] **Step 4: Commit**

```bash
git commit -m "test: add green threads, async/await, and generic channel examples"
```

---

### Task 12: Documentation

**Files:**
- Modify: `docs/guide/concurrency.md`
- Modify: `docs/guide/actors.md`
- Modify: `docs/KNOWN_LIMITATIONS.md`
- Modify: `README.md`
- Modify: `~/dev/kodo-website/src/content/docs/guide/concurrency.md`
- Modify: `~/dev/kodo-website/src/content/docs/guide/actors.md`
- Modify: `~/dev/kodo-website/public/llms.txt`

- [ ] **Step 1: Rewrite concurrency guide**

Replace sequential spawn docs with green thread docs. Document:
- `spawn {}` creates green threads (not sequential tasks)
- `async fn` and `.await`
- `Future<T>` type
- `parallel {}` still uses OS threads
- `Channel<T>` for any type
- `--threads=N` flag
- `--no-green-threads` flag

- [ ] **Step 2: Update actors guide**

Actors now run handlers on green threads instead of FIFO scheduler.

- [ ] **Step 3: Update KNOWN_LIMITATIONS.md**

Remove:
- "Sequential spawn/async/await" — now uses green threads
- "Limited channel types" — now generic

- [ ] **Step 4: Update README features table**

Change Concurrency row to reflect green threads, work-stealing, async/await.

- [ ] **Step 5: Update website**

Mirror all changes to `~/dev/kodo-website/`.

- [ ] **Step 6: Commit and push both repos**

```bash
git commit -m "docs: rewrite concurrency guide for green threads"
git push
cd ~/dev/kodo-website && git add -A && git commit -m "docs: update concurrency for green threads" && git push
```

---

### Task 13: Final Verification

- [ ] **Step 1: Full verification suite**

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
make ui-test
```

- [ ] **Step 2: End-to-end tests**

```bash
cargo run -p kodoc -- build examples/green_threads.ko && ./examples/green_threads
cargo run -p kodoc -- build examples/async_await.ko && ./examples/async_await
cargo run -p kodoc -- build examples/channel_generic.ko && ./examples/channel_generic
```

- [ ] **Step 3: Performance baseline**

Run existing benchmarks to verify yield point overhead is < 5%:
```bash
cargo bench -p kodo_lexer
```

- [ ] **Step 4: Push and prepare release**

```bash
git push
```
