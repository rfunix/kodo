//! # Green Thread Data Structures and M:N Work-Stealing Scheduler
//!
//! Provides the [`GreenThread`] struct — a cooperatively-scheduled lightweight
//! thread with its own `mmap`'d stack and CPU context — and the
//! [`Scheduler`] that multiplexes green threads onto a pool of OS worker
//! threads using work-stealing deques (crossbeam).
//!
//! ## Memory layout
//!
//! Each green thread owns a contiguous region (default 1 MB, configurable via
//! `KODO_STACK_SIZE` env var) obtained via `mmap(MAP_PRIVATE | MAP_ANONYMOUS)`.
//! The bottom OS page is set to `PROT_NONE` as a guard page — any access to
//! it raises SIGSEGV, giving a clean crash on stack overflow rather than
//! silent corruption.  The stack pointer starts at the **top** (highest
//! address) of this region and grows downward, following the System V AMD64 /
//! AAPCS64 ABI convention.
//!
//! The entire region (including the guard page) is freed via `munmap` when
//! the [`GreenThread`] is dropped.
//!
//! ## Scheduler architecture
//!
//! ```text
//! Scheduler (global singleton via OnceLock)
//!   ├── Worker 0 (OS thread) — local crossbeam deque
//!   ├── Worker 1 (OS thread) — local crossbeam deque
//!   └── Worker N (OS thread) — local crossbeam deque
//!
//! Each worker runs a loop:
//!   1. Pop from local deque
//!   2. If empty → steal from random other worker
//!   3. If steal failed → pop from global queue
//!   4. If all empty → park (condvar wait)
//!   5. Resume chosen green thread (switch_context)
//!   6. On yield → push back to local deque
//!   7. On return → mark Dead, clean up
//! ```

use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};

use crate::context::{switch_context, Context};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default stack size per green thread (1 MB).
///
/// This can be overridden at runtime via the `KODO_STACK_SIZE` environment
/// variable (value in bytes).  A guard page is placed at the bottom of each
/// stack so that overflow causes a clean crash (SIGSEGV on the guard page)
/// instead of silent memory corruption.
pub const DEFAULT_STACK_SIZE: usize = 1024 * 1024;

/// Returns the effective stack size for green threads.
///
/// Reads the `KODO_STACK_SIZE` environment variable (value in bytes).
/// Falls back to [`DEFAULT_STACK_SIZE`] (1 MB) when the variable is absent
/// or not a valid `usize`.  The returned value is always at least one OS
/// page so the guard page logic remains sound.
#[must_use]
pub fn get_stack_size() -> usize {
    let size = std::env::var("KODO_STACK_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_STACK_SIZE);
    // Ensure at least two pages so there's room for both the guard and usable area.
    let page_size = page_size();
    size.max(page_size * 2)
}

/// Returns the OS page size (typically 4096 on most platforms).
fn page_size() -> usize {
    // SAFETY: _SC_PAGESIZE is a valid sysconf parameter on all POSIX systems.
    let ps = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if ps <= 0 {
        4096
    } else {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let result = ps as usize;
        result
    }
}

// ---------------------------------------------------------------------------
// GreenThreadId
// ---------------------------------------------------------------------------

/// Unique identifier for a green thread.
///
/// IDs are generated from a global monotonic counter and are never reused
/// within a process lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GreenThreadId(pub u64);

impl GreenThreadId {
    /// Allocates the next unique [`GreenThreadId`].
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

// ---------------------------------------------------------------------------
// ThreadStatus
// ---------------------------------------------------------------------------

/// Current execution status of a green thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    /// Ready to run, waiting in the run queue.
    Ready,
    /// Currently executing on a worker OS thread.
    Running,
    /// Blocked waiting for a future, channel, or I/O.
    Blocked,
    /// Finished execution; its stack can be freed.
    Dead,
}

// ---------------------------------------------------------------------------
// Stack helpers
// ---------------------------------------------------------------------------

/// Allocates a stack with a guard page at the bottom using `mmap`.
///
/// Returns a pointer to the **base** (lowest address) of the mapping.
/// The first OS page of the mapping is set to `PROT_NONE` (the guard page)
/// so that a stack overflow triggers a clean SIGSEGV rather than silently
/// corrupting adjacent memory.
///
/// The caller is responsible for:
/// - Computing the usable stack top as `base + size`.
/// - Passing `base` and `size` to [`free_stack`] when done.
/// - Starting the stack pointer at `base + size` (stacks grow downward).
///
/// The guard page is at `base..base+page_size`; usable stack space is
/// `base+page_size..base+size`.
///
/// # Safety
///
/// The returned pointer must eventually be passed to [`free_stack`] with the
/// same `size` to avoid a memory leak.  The mapping is readable and writable
/// (except the guard page) but not executable.
unsafe fn alloc_stack(size: usize) -> *mut u8 {
    // SAFETY: All arguments are valid mmap parameters.  MAP_ANONYMOUS means
    // no file descriptor is required (-1 is the conventional value).
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    assert!(
        ptr != libc::MAP_FAILED,
        "mmap failed to allocate green thread stack"
    );

    // SAFETY: Set the bottom page as a guard (PROT_NONE).  Any access to
    // this page will raise SIGSEGV, giving a clean crash on stack overflow
    // instead of silent corruption.  ptr is page-aligned (mmap guarantees
    // this) and page_size() bytes is within the mapped region.
    let guard_size = page_size();
    let ret = unsafe { libc::mprotect(ptr, guard_size, libc::PROT_NONE) };
    assert!(
        ret == 0,
        "mprotect failed to set guard page on green thread stack"
    );

    ptr.cast::<u8>()
}

/// Frees a stack previously allocated by [`alloc_stack`].
///
/// # Safety
///
/// `ptr` must be the exact pointer returned by [`alloc_stack`] for the same
/// `size`.  After this call the memory is unmapped and must not be accessed.
unsafe fn free_stack(ptr: *mut u8, size: usize) {
    // SAFETY: ptr was obtained from mmap with the same size; munmap is safe
    // when the arguments match the original mapping.
    unsafe {
        libc::munmap(ptr.cast::<libc::c_void>(), size);
    }
}

// ---------------------------------------------------------------------------
// GreenThread
// ---------------------------------------------------------------------------

/// A green thread with its own stack and saved CPU context.
///
/// Green threads are cooperatively scheduled: they run until they voluntarily
/// yield (or block) by calling into the scheduler, which calls
/// [`crate::context::switch_context`] to suspend the current thread and resume
/// another.
///
/// # Ownership
///
/// `GreenThread` owns the stack memory it was created with.  The stack is
/// freed automatically when the struct is dropped.
///
/// # Safety
///
/// Creating a `GreenThread` allocates OS memory and sets up raw CPU state.
/// See [`GreenThread::new`] for the safety requirements.
pub struct GreenThread {
    /// Unique thread identifier.
    pub id: GreenThreadId,
    /// Current execution status.
    pub status: ThreadStatus,
    /// Saved CPU context (callee-saved registers + stack pointer).
    pub context: Context,
    /// Base (lowest) address of the `mmap`'d stack region.
    stack: *mut u8,
    /// Total size of the stack region in bytes.
    stack_size: usize,
    /// If this thread is backing a `Future`, the future's numeric ID.
    pub future_id: Option<u64>,
    /// Return value written by the thread when it completes (for futures).
    pub result: Option<i64>,
}

// SAFETY: GreenThread owns its stack exclusively.  No shared mutable state
// is exposed through the raw pointer; only the owning GreenThread ever
// reads or writes it.
unsafe impl Send for GreenThread {}

impl GreenThread {
    /// Creates a new green thread that will execute `entry(arg)`.
    ///
    /// Allocates a stack via `mmap` (size determined by [`get_stack_size`])
    /// with a guard page at the bottom, and initialises the CPU context so
    /// that the first [`crate::context::switch_context`] into this thread
    /// begins executing `entry(arg)`.
    ///
    /// The new thread starts in the [`ThreadStatus::Ready`] state.
    ///
    /// # Safety
    ///
    /// - `entry` must be a valid function pointer for the lifetime of this
    ///   `GreenThread`.
    /// - The caller must ensure the `GreenThread` is switched to from a valid
    ///   scheduler context and that it is properly cleaned up (dropped) after
    ///   reaching [`ThreadStatus::Dead`].
    pub unsafe fn new(entry: crate::context::EntryFn, arg: usize) -> Self {
        let stack_size = get_stack_size();
        // SAFETY: alloc_stack returns a valid region of `stack_size` bytes
        // with a guard page at the bottom.  We compute the top as
        // base + size (stacks grow downward).
        let stack = unsafe { alloc_stack(stack_size) };
        // SAFETY: stack + stack_size is within the allocated region.
        let stack_top = unsafe { stack.add(stack_size) };

        let mut ctx = Context::default();
        // SAFETY: ctx is valid, stack_top points to the end of a live region,
        // and entry is a valid function pointer (caller guarantee).
        unsafe {
            crate::context::init_context(&raw mut ctx, stack_top, entry, arg);
        }

        Self {
            id: GreenThreadId::next(),
            status: ThreadStatus::Ready,
            context: ctx,
            stack,
            stack_size,
            future_id: None,
            result: None,
        }
    }
}

impl Drop for GreenThread {
    fn drop(&mut self) {
        // SAFETY: self.stack was allocated by alloc_stack with self.stack_size.
        // We are the sole owner of this mapping; no other reference exists.
        unsafe {
            free_stack(self.stack, self.stack_size);
        }
    }
}

// ===========================================================================
// M:N Work-Stealing Scheduler
// ===========================================================================

// ---------------------------------------------------------------------------
// Thread-local per-worker state
// ---------------------------------------------------------------------------

thread_local! {
    /// Index of the current worker OS thread (0-based).
    static WORKER_ID: Cell<usize> = const { Cell::new(0) };
    /// The worker's local crossbeam deque for its ready queue.
    static WORKER_DEQUE: RefCell<Option<crossbeam_deque::Worker<GreenThreadId>>> =
        const { RefCell::new(None) };
    /// The green thread currently executing on this worker (if any).
    static CURRENT_THREAD: Cell<Option<GreenThreadId>> = const { Cell::new(None) };
    /// Flag checked at yield points — when true the green thread should yield.
    static SHOULD_YIELD: Cell<bool> = const { Cell::new(false) };
    /// Saved CPU context for the scheduler loop on this worker thread.
    ///
    /// We use `UnsafeCell` instead of `RefCell` because `switch_context`
    /// suspends execution in the middle of a borrow — the scheduler's
    /// context is written by `switch_context` (saving the scheduler state)
    /// and then read later (to resume the scheduler).  A `RefCell` would
    /// panic because the mutable borrow from the first `switch_context`
    /// appears to still be held when the green thread finishes and calls
    /// `switch_context` back.  With cooperative scheduling, only one
    /// logical access happens at a time, so `UnsafeCell` is sound.
    static SCHEDULER_CONTEXT: UnsafeCell<Context> = const { UnsafeCell::new(Context {
        regs: [0; 13],
    }) };
}

// ---------------------------------------------------------------------------
// Global scheduler singleton
// ---------------------------------------------------------------------------

/// Global singleton holding the scheduler state.
static SCHEDULER: OnceLock<Scheduler> = OnceLock::new();

/// Count of currently alive (non-Dead) green threads across all workers.
static ALIVE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// The M:N work-stealing scheduler.
///
/// Manages all green threads and distributes them across a pool of OS
/// worker threads.  Each worker has a local crossbeam deque; idle workers
/// steal from other workers or fall back to the global overflow queue.
pub struct Scheduler {
    /// All green threads indexed by ID (protected by mutex for cross-thread access).
    threads: Mutex<HashMap<GreenThreadId, GreenThread>>,
    /// Global queue for overflow / new spawns from non-worker threads.
    global_queue: Mutex<VecDeque<GreenThreadId>>,
    /// Number of worker OS threads.
    num_workers: usize,
    /// Flag to signal shutdown.
    shutdown: AtomicBool,
    /// Condvar to wake parked workers.
    park_condvar: Condvar,
    /// Mutex paired with [`park_condvar`].
    park_mutex: Mutex<()>,
}

impl Scheduler {
    /// Creates a new scheduler with `num_workers` worker deques.
    ///
    /// Returns the scheduler and the worker-side deques (one per worker).
    fn new(num_workers: usize) -> Self {
        Self {
            threads: Mutex::new(HashMap::new()),
            global_queue: Mutex::new(VecDeque::new()),
            num_workers,
            shutdown: AtomicBool::new(false),
            park_condvar: Condvar::new(),
            park_mutex: Mutex::new(()),
        }
    }
}

/// Returns a reference to the global scheduler, panicking if not initialised.
fn get_scheduler() -> &'static Scheduler {
    // SAFETY: This is only called after kodo_green_init has been called.
    SCHEDULER.get().unwrap_or_else(|| {
        panic!("kodo_green_init must be called before using green threads");
    })
}

// ---------------------------------------------------------------------------
// Green thread entry wrapper
// ---------------------------------------------------------------------------

/// Wrapper entry function for green threads spawned without an environment.
///
/// The `arg` encodes the raw function pointer (as `usize`) of the user's
/// `extern "C" fn()`.  After calling it, we mark the thread as Dead and
/// switch back to the scheduler context.
///
/// # Safety
///
/// `arg` must be a valid function pointer cast to `usize`.
unsafe fn green_entry_no_env(arg: usize) {
    // SAFETY: arg was set to the function pointer by the spawn call.
    let func: extern "C" fn() = unsafe { std::mem::transmute(arg) };
    func();
    green_thread_finished();
}

/// Wrapper entry function for green threads spawned with a captured environment.
///
/// `arg` is a pointer to a heap-allocated [`EnvPayload`] struct containing
/// the function pointer and a copy of the environment bytes.
///
/// # Safety
///
/// `arg` must be a valid pointer to a boxed `EnvPayload`.
unsafe fn green_entry_with_env(arg: usize) {
    // SAFETY: arg is a pointer to a heap-allocated EnvPayload.
    let payload = unsafe { Box::from_raw(arg as *mut EnvPayload) };
    (payload.func)(payload.env.as_ptr() as i64);
    green_thread_finished();
}

/// Payload for green threads that carry a captured environment.
struct EnvPayload {
    /// The function pointer that accepts an environment pointer.
    func: extern "C" fn(i64),
    /// Owned copy of the environment bytes.
    env: Vec<u8>,
}

/// Called when a green thread's entry function returns.
///
/// Marks the current thread as [`ThreadStatus::Dead`], decrements the alive
/// counter, and switches back to the scheduler context on this worker.
fn green_thread_finished() {
    let thread_id = CURRENT_THREAD.get();
    if let Some(id) = thread_id {
        let sched = get_scheduler();
        if let Ok(mut threads) = sched.threads.lock() {
            if let Some(thread) = threads.get_mut(&id) {
                thread.status = ThreadStatus::Dead;
            }
        }
        ALIVE_COUNT.fetch_sub(1, Ordering::SeqCst);
        // Wake all workers in case they are parked — the alive count changed
        // and if it reached zero, they need to check for shutdown.
        sched.park_condvar.notify_all();
    }

    CURRENT_THREAD.set(None);

    // Switch back to the scheduler context.
    SCHEDULER_CONTEXT.with(|sched_ctx| {
        let mut dummy = Context::default();
        // SAFETY: sched_ctx was saved by the worker loop before switching to
        // this green thread.  We switch into it now to return control to the
        // worker loop.  Using UnsafeCell::get() is sound because only one
        // logical access occurs at a time (cooperative scheduling).
        unsafe {
            switch_context(&raw mut dummy, sched_ctx.get());
        }
    });
}

// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

/// Try to pop a green thread ID from the current worker's local deque.
fn try_pop_local() -> Option<GreenThreadId> {
    WORKER_DEQUE.with(|deque| {
        deque
            .borrow()
            .as_ref()
            .and_then(crossbeam_deque::Worker::pop)
    })
}

/// Try to pop a green thread ID from the global overflow queue.
fn try_pop_global() -> Option<GreenThreadId> {
    let sched = get_scheduler();
    if let Ok(mut queue) = sched.global_queue.lock() {
        queue.pop_front()
    } else {
        None
    }
}

/// Resumes execution of a green thread by switching from the scheduler
/// context to the green thread's saved context.
///
/// When the green thread yields or finishes, control returns here.
/// The function then inspects the thread's status and either re-enqueues
/// it (if Ready) or leaves it alone (if Dead).
fn resume_green_thread(id: GreenThreadId) {
    let sched = get_scheduler();

    // Extract the thread's context pointer and mark it Running.
    let ctx_ptr: *mut Context = {
        let Ok(mut threads) = sched.threads.lock() else {
            return;
        };
        let Some(thread) = threads.get_mut(&id) else {
            return;
        };
        if thread.status == ThreadStatus::Dead {
            return;
        }
        thread.status = ThreadStatus::Running;
        &raw mut thread.context
    };

    CURRENT_THREAD.set(Some(id));
    // Set the yield flag so the thread will yield at its next yield point.
    SHOULD_YIELD.set(true);

    // Switch from the scheduler context to the green thread.
    SCHEDULER_CONTEXT.with(|sched_ctx| {
        // SAFETY: sched_ctx is a valid thread-local Context.  ctx_ptr points
        // to the green thread's context inside the scheduler's HashMap, which
        // is not moved while we hold no lock (the thread owns its own context
        // memory via its stack allocation — the Context is stored inline).
        // The switch will save the current (scheduler) state into sched_ctx
        // and restore the green thread's registers from ctx_ptr.
        // SAFETY: UnsafeCell::get() returns a raw pointer.  Only one
        // logical access happens at a time due to cooperative scheduling.
        unsafe {
            switch_context(sched_ctx.get(), ctx_ptr);
        }
    });

    // We're back in the scheduler — the green thread yielded or finished.
    CURRENT_THREAD.set(None);

    // Check the thread status and re-enqueue if still alive.
    let Ok(mut threads) = sched.threads.lock() else {
        return;
    };
    if let Some(thread) = threads.get_mut(&id) {
        match thread.status {
            ThreadStatus::Running => {
                // Thread yielded — mark Ready and push back to local deque.
                thread.status = ThreadStatus::Ready;
                drop(threads); // release lock before touching deque
                WORKER_DEQUE.with(|deque| {
                    if let Some(w) = deque.borrow().as_ref() {
                        w.push(id);
                    }
                });
            }
            ThreadStatus::Dead => {
                // Thread finished — remove it and drop (frees the stack).
                threads.remove(&id);
            }
            ThreadStatus::Blocked | ThreadStatus::Ready => {
                // Blocked threads are not re-enqueued (a waker will do it).
                // Ready shouldn't happen here but is harmless.
            }
        }
    }
}

// ===========================================================================
// Public extern "C" API — called from compiled Kōdo code
// ===========================================================================

/// Initialises the green thread scheduler with `num_threads` worker threads.
///
/// Must be called once before any `kodo_green_spawn`.  If `num_threads` is 0
/// or negative, checks the `KODO_THREADS` environment variable first, then
/// defaults to the number of available CPU cores.
///
/// # Safety
///
/// Must be called exactly once. Calling it a second time is a no-op (the
/// existing scheduler is retained).
#[no_mangle]
pub unsafe extern "C" fn kodo_green_init(num_threads: i64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let n = if num_threads <= 0 {
        // Check KODO_THREADS environment variable for runtime override.
        std::env::var("KODO_THREADS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or_else(|| num_cpus::get().max(1))
    } else {
        num_threads as usize
    };

    // OnceLock ensures this only runs once.
    let _ = SCHEDULER.get_or_init(|| Scheduler::new(n));
}

/// Spawns a new green thread running `func_ptr()` (no captures).
///
/// Called by compiled `spawn {}` blocks that don't capture any variables.
///
/// # Safety
///
/// `func_ptr` must be a valid pointer to an `extern "C" fn()`.
#[no_mangle]
pub unsafe extern "C" fn kodo_green_spawn(func_ptr: i64) {
    let sched = get_scheduler();

    // SAFETY: green_entry_no_env is a valid entry function; func_ptr is
    // passed as the arg and will be transmuted back inside the entry.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let thread = unsafe { GreenThread::new(green_entry_no_env, func_ptr as usize) };
    let id = thread.id;

    if let Ok(mut threads) = sched.threads.lock() {
        threads.insert(id, thread);
    }
    ALIVE_COUNT.fetch_add(1, Ordering::SeqCst);

    if let Ok(mut queue) = sched.global_queue.lock() {
        queue.push_back(id);
    }
    sched.park_condvar.notify_one();
}

/// Spawns a new green thread running `func_ptr(env_ptr)` (with captures).
///
/// The runtime copies `env_size` bytes from `env_ptr` to a heap buffer that
/// stays alive for the green thread's lifetime.
///
/// # Safety
///
/// - `func_ptr` must be a valid pointer to an `extern "C" fn(i64)`.
/// - `env_ptr` must point to a readable buffer of at least `env_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_green_spawn_with_env(func_ptr: i64, env_ptr: i64, env_size: i64) {
    let sched = get_scheduler();

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = env_size as usize;

    let mut env = vec![0u8; size];
    if size > 0 {
        // SAFETY: caller guarantees env_ptr points to env_size readable bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(env_ptr as *const u8, env.as_mut_ptr(), size);
        }
    }

    // SAFETY: func_ptr is a valid function pointer from Cranelift codegen.
    let func: extern "C" fn(i64) = unsafe { std::mem::transmute(func_ptr) };

    let payload = Box::new(EnvPayload { func, env });
    let payload_ptr = Box::into_raw(payload) as usize;

    // SAFETY: green_entry_with_env is a valid entry function; payload_ptr
    // points to a heap-allocated EnvPayload that will be freed inside the
    // entry function.
    let thread = unsafe { GreenThread::new(green_entry_with_env, payload_ptr) };
    let id = thread.id;

    if let Ok(mut threads) = sched.threads.lock() {
        threads.insert(id, thread);
    }
    ALIVE_COUNT.fetch_add(1, Ordering::SeqCst);

    if let Ok(mut queue) = sched.global_queue.lock() {
        queue.push_back(id);
    }
    sched.park_condvar.notify_one();
}

/// Yield point — called by compiled code at loop back-edges and function calls.
///
/// **Fast path** (~1ns): checks a thread-local boolean flag.  If false,
/// returns immediately.
///
/// **Slow path**: saves the green thread's context and switches back to
/// the scheduler, which will pick the next thread to run.
///
/// # Safety
///
/// Must only be called from within a green thread context (i.e., after the
/// scheduler has switched to a green thread).
#[no_mangle]
pub unsafe extern "C" fn kodo_green_maybe_yield() {
    // Fast path: check thread-local flag.
    if !SHOULD_YIELD.get() {
        return;
    }
    SHOULD_YIELD.set(false);

    // Slow path: switch back to the scheduler.
    let thread_id = CURRENT_THREAD.get();
    let Some(id) = thread_id else { return };

    let sched = get_scheduler();

    // Get the thread's context pointer.
    let ctx_ptr: *mut Context = {
        let Ok(mut threads) = sched.threads.lock() else {
            return;
        };
        let Some(t) = threads.get_mut(&id) else {
            return;
        };
        &raw mut t.context
    };

    // Switch from the green thread back to the scheduler.
    SCHEDULER_CONTEXT.with(|sched_ctx| {
        // SAFETY: Both contexts are valid and pinned.  The green thread's
        // context lives in the HashMap (stable address while not moved).
        // The scheduler context is on this worker's thread-local storage.
        unsafe {
            switch_context(ctx_ptr, sched_ctx.get());
        }
    });

    // When we return here, the scheduler has switched back to us.
    // Reset the yield flag for the next yield point.
    SHOULD_YIELD.set(true);
}

/// Starts the scheduler — blocks until all green threads complete.
///
/// Spawns worker OS threads and runs the worker loop on each.  Returns
/// when all green threads have finished (status = Dead) or the scheduler
/// is shut down.
///
/// # Safety
///
/// Must be called after `kodo_green_init` and after spawning at least one
/// green thread.  Must be called from the main thread.
///
/// # Panics
///
/// Panics if `kodo_green_init` was not called beforehand.
#[no_mangle]
pub unsafe extern "C" fn kodo_green_run() {
    let sched = get_scheduler();

    // If there are no alive threads, return immediately.
    if ALIVE_COUNT.load(Ordering::SeqCst) == 0 {
        return;
    }

    let num = sched.num_workers;

    // Create per-worker deques and their corresponding stealers.  Workers
    // own their deque; stealers are shared (via Arc) so any worker can
    // steal from any other.
    let mut workers: Vec<crossbeam_deque::Worker<GreenThreadId>> = Vec::with_capacity(num);
    let mut local_stealers: Vec<crossbeam_deque::Stealer<GreenThreadId>> = Vec::with_capacity(num);
    for _ in 0..num {
        let w = crossbeam_deque::Worker::new_fifo();
        local_stealers.push(w.stealer());
        workers.push(w);
    }

    // Move initial global queue items into worker 0's deque to bootstrap.
    if let Some(w0) = workers.first() {
        if let Ok(mut queue) = sched.global_queue.lock() {
            for id in queue.drain(..) {
                w0.push(id);
            }
        }
    }

    let stealers = std::sync::Arc::new(local_stealers);

    let handles: Vec<_> = workers
        .into_iter()
        .enumerate()
        .map(|(i, worker)| {
            let stealers = std::sync::Arc::clone(&stealers);
            std::thread::spawn(move || {
                worker_loop(i, worker, &stealers);
            })
        })
        .collect();

    for h in handles {
        let _ = h.join();
    }

    sched.shutdown.store(true, Ordering::SeqCst);

    // Clean up any remaining dead threads.
    if let Ok(mut threads) = sched.threads.lock() {
        threads.retain(|_, t| t.status != ThreadStatus::Dead);
    }
}

/// The main loop executed by each worker OS thread.
///
/// Repeatedly finds a green thread to run (local deque → steal → global
/// queue), resumes it via `switch_context`, and handles the result when it
/// yields or finishes.  Parks on a condvar when no work is available.
fn worker_loop(
    worker_id: usize,
    worker: crossbeam_deque::Worker<GreenThreadId>,
    stealers: &[crossbeam_deque::Stealer<GreenThreadId>],
) {
    WORKER_ID.set(worker_id);
    WORKER_DEQUE.with(|deque| {
        *deque.borrow_mut() = Some(worker);
    });

    let sched = get_scheduler();

    loop {
        // Find next thread to run.
        let thread_id = try_pop_local()
            .or_else(|| try_steal_from(worker_id, stealers))
            .or_else(try_pop_global);

        if let Some(id) = thread_id {
            resume_green_thread(id);
        } else {
            if sched.shutdown.load(Ordering::SeqCst) {
                break;
            }
            if ALIVE_COUNT.load(Ordering::SeqCst) == 0 {
                break;
            }
            if let Ok(guard) = sched.park_mutex.lock() {
                if sched.shutdown.load(Ordering::SeqCst) || ALIVE_COUNT.load(Ordering::SeqCst) == 0
                {
                    break;
                }
                let _ = sched
                    .park_condvar
                    .wait_timeout(guard, std::time::Duration::from_millis(10));
            }
        }
    }
}

/// Try to steal from a given list of stealers (excluding self).
fn try_steal_from(
    worker_id: usize,
    stealers: &[crossbeam_deque::Stealer<GreenThreadId>],
) -> Option<GreenThreadId> {
    if stealers.is_empty() {
        return None;
    }
    let num = stealers.len();
    let start = fastrand::usize(..num);
    for i in 0..num {
        let idx = (start + i) % num;
        if idx == worker_id {
            continue;
        }
        if let crossbeam_deque::Steal::Success(id) = stealers[idx].steal() {
            return Some(id);
        }
    }
    None
}

// ===========================================================================
// Future table — backing storage for `Future<T>`
// ===========================================================================

/// Entry in the global future table.
///
/// Each future has a completion flag, an optional result buffer, and a list of
/// green threads waiting for it. When the future is completed, all waiters are
/// moved from Blocked to Ready and pushed to the global run queue.
///
/// The result is stored as a `Vec<u8>` byte buffer, allowing futures to carry
/// composite return types (e.g., String = `(ptr, len)` = 16 bytes) in addition
/// to simple `i64` values (8 bytes).
struct FutureEntry {
    /// Whether the future has been completed.
    completed: AtomicBool,
    /// The result bytes, set once when the future is completed.
    ///
    /// For simple `i64` results, this holds 8 bytes (little-endian).
    /// For composite types like String `(ptr, len)`, this holds 16 bytes.
    result: Mutex<Option<Vec<u8>>>,
    /// Green thread IDs blocked waiting for this future.
    waiters: Mutex<Vec<GreenThreadId>>,
}

/// Global future table: maps future handles to their entries.
static FUTURE_TABLE: OnceLock<Mutex<HashMap<u64, FutureEntry>>> = OnceLock::new();

/// Monotonic counter for allocating unique future handles.
static FUTURE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Returns a reference to the global future table.
fn get_future_table() -> &'static Mutex<HashMap<u64, FutureEntry>> {
    FUTURE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Creates a new pending future and returns its handle.
///
/// The returned handle is an opaque `i64` that can be passed to
/// [`kodo_future_complete`] and [`kodo_future_await`].
///
/// # Safety
///
/// Safe to call from any context (green thread or main thread).
#[no_mangle]
pub unsafe extern "C" fn kodo_future_new() -> i64 {
    let id = FUTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let entry = FutureEntry {
        completed: AtomicBool::new(false),
        result: Mutex::new(None),
        waiters: Mutex::new(Vec::new()),
    };
    let table = get_future_table();
    if let Ok(mut t) = table.lock() {
        t.insert(id, entry);
    }
    #[allow(clippy::cast_possible_wrap)]
    {
        id as i64
    }
}

/// Completes a future with the given `i64` result value.
///
/// Stores the result as 8 bytes (little-endian) in the future's byte buffer.
/// All green threads waiting on this future are moved from
/// [`ThreadStatus::Blocked`] to [`ThreadStatus::Ready`] and pushed
/// into the global run queue so workers can pick them up.
///
/// # Safety
///
/// `handle` must be a valid future handle returned by [`kodo_future_new`].
/// Must only be called once per future; subsequent calls are no-ops.
#[no_mangle]
pub unsafe extern "C" fn kodo_future_complete(handle: i64, result: i64) {
    let bytes = result.to_le_bytes().to_vec();
    future_complete_inner(handle, bytes);
}

/// Completes a future with an arbitrary byte buffer.
///
/// Copies `data_size` bytes from the memory pointed to by `data_ptr` into
/// the future's result buffer. This supports composite return types such as
/// String `(ptr: i64, len: i64)` which require 16 bytes.
///
/// # Safety
///
/// - `handle` must be a valid future handle returned by [`kodo_future_new`].
/// - `data_ptr` must point to a readable buffer of at least `data_size` bytes.
/// - Must only be called once per future; subsequent calls are no-ops.
#[no_mangle]
pub unsafe extern "C" fn kodo_future_complete_bytes(handle: i64, data_ptr: i64, data_size: i64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = data_size as usize;
    let mut bytes = vec![0u8; size];
    if size > 0 {
        // SAFETY: caller guarantees data_ptr points to data_size readable bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(data_ptr as *const u8, bytes.as_mut_ptr(), size);
        }
    }
    future_complete_inner(handle, bytes);
}

/// Shared implementation for completing a future with a byte buffer.
///
/// Stores the bytes, marks the future as completed, and wakes all waiters.
fn future_complete_inner(handle: i64, bytes: Vec<u8>) {
    #[allow(clippy::cast_sign_loss)]
    let id = handle as u64;
    let table = get_future_table();
    let Ok(t) = table.lock() else { return };
    let Some(entry) = t.get(&id) else { return };

    // Store the result and mark completed.
    if let Ok(mut r) = entry.result.lock() {
        *r = Some(bytes);
    }
    entry.completed.store(true, Ordering::Release);

    // Wake all waiting green threads.
    let waiters = if let Ok(mut w) = entry.waiters.lock() {
        std::mem::take(&mut *w)
    } else {
        Vec::new()
    };
    drop(t); // release future table lock before touching scheduler

    if !waiters.is_empty() {
        let sched = get_scheduler();
        if let Ok(mut threads) = sched.threads.lock() {
            for wid in &waiters {
                if let Some(thread) = threads.get_mut(wid) {
                    thread.status = ThreadStatus::Ready;
                }
            }
        }
        if let Ok(mut queue) = sched.global_queue.lock() {
            for wid in waiters {
                queue.push_back(wid);
            }
        }
        sched.park_condvar.notify_all();
    }
}

/// Awaits a future, blocking the current green thread if not yet complete.
///
/// If the future is already completed, returns the `i64` result immediately
/// (reading 8 bytes from the stored byte buffer). Otherwise, adds the current
/// green thread to the future's waiter list, marks it as
/// [`ThreadStatus::Blocked`], and switches back to the scheduler. When the
/// future is completed, the thread is woken and returns the result.
///
/// # Safety
///
/// Must be called from within a green thread context (i.e., the scheduler
/// has switched to a green thread). `handle` must be a valid future handle.
#[no_mangle]
pub unsafe extern "C" fn kodo_future_await(handle: i64) -> i64 {
    #[allow(clippy::cast_sign_loss)]
    let id = handle as u64;

    loop {
        // Check if the future is already completed.
        let table = get_future_table();
        if let Ok(t) = table.lock() {
            if let Some(entry) = t.get(&id) {
                if entry.completed.load(Ordering::Acquire) {
                    // Future is done — extract i64 from byte buffer.
                    if let Ok(r) = entry.result.lock() {
                        return extract_i64_from_result(r.as_deref());
                    }
                    return 0;
                }
                // Not completed — register ourselves as a waiter.
                let current = CURRENT_THREAD.get();
                if let Some(tid) = current {
                    if let Ok(mut w) = entry.waiters.lock() {
                        w.push(tid);
                    }
                } else {
                    // Not running inside a green thread — drain the
                    // green thread scheduler so spawned tasks can make
                    // progress and complete the future we're waiting on.
                    drop(t);
                    // SAFETY: kodo_green_run is safe after kodo_green_init.
                    unsafe {
                        kodo_green_run();
                    }
                    continue;
                }
            } else {
                // Unknown future handle — return 0.
                return 0;
            }
        } else {
            return 0;
        }

        // Block the current green thread — shared with kodo_future_await_bytes.
        if !future_await_block_current() {
            return 0;
        }
        // Loop back to check if the future is now completed.
    }
}

/// Awaits a future and copies the result bytes to caller-provided buffer.
///
/// This variant is used for composite return types (e.g., String) where
/// the result is larger than a single `i64`. Copies `data_size` bytes from
/// the future's result buffer into memory at `out_ptr`.
///
/// # Safety
///
/// - Must be called from within a green thread context.
/// - `handle` must be a valid future handle.
/// - `out_ptr` must point to a writable buffer of at least `data_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_future_await_bytes(handle: i64, out_ptr: i64, data_size: i64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = data_size as usize;
    #[allow(clippy::cast_sign_loss)]
    let id = handle as u64;

    loop {
        let table = get_future_table();
        if let Ok(t) = table.lock() {
            if let Some(entry) = t.get(&id) {
                if entry.completed.load(Ordering::Acquire) {
                    // Future is done — copy bytes to output buffer.
                    if let Ok(r) = entry.result.lock() {
                        if let Some(bytes) = r.as_ref() {
                            let copy_len = bytes.len().min(size);
                            if copy_len > 0 {
                                // SAFETY: caller guarantees out_ptr is writable
                                // for data_size bytes. copy_len <= size.
                                unsafe {
                                    std::ptr::copy_nonoverlapping(
                                        bytes.as_ptr(),
                                        out_ptr as *mut u8,
                                        copy_len,
                                    );
                                }
                            }
                        }
                    }
                    return;
                }
                let current = CURRENT_THREAD.get();
                if let Some(tid) = current {
                    if let Ok(mut w) = entry.waiters.lock() {
                        w.push(tid);
                    }
                } else {
                    drop(t);
                    // SAFETY: kodo_green_run is safe after kodo_green_init.
                    unsafe {
                        kodo_green_run();
                    }
                    continue;
                }
            } else {
                return;
            }
        } else {
            return;
        }

        if !future_await_block_current() {
            return;
        }
    }
}

/// Extracts an `i64` from the first 8 bytes of a result buffer.
///
/// Returns 0 if the buffer is `None` or shorter than 8 bytes.
fn extract_i64_from_result(bytes: Option<&[u8]>) -> i64 {
    match bytes {
        Some(b) if b.len() >= 8 => {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&b[..8]);
            i64::from_le_bytes(arr)
        }
        _ => 0,
    }
}

/// Blocks the current green thread and switches back to the scheduler.
///
/// Returns `true` if the thread was successfully blocked and resumed,
/// `false` if there was an error (no current thread, lock failure).
fn future_await_block_current() -> bool {
    let current_id = CURRENT_THREAD.get();
    let Some(tid) = current_id else { return false };
    let sched = get_scheduler();

    // Mark ourselves as Blocked.
    if let Ok(mut threads) = sched.threads.lock() {
        if let Some(thread) = threads.get_mut(&tid) {
            thread.status = ThreadStatus::Blocked;
        }
    }

    // Get our context pointer and switch back to the scheduler.
    let ctx_ptr: *mut crate::context::Context = {
        let Ok(mut threads) = sched.threads.lock() else {
            return false;
        };
        let Some(t) = threads.get_mut(&tid) else {
            return false;
        };
        &raw mut t.context
    };

    SCHEDULER_CONTEXT.with(|sched_ctx| {
        // SAFETY: Both contexts are valid. The green thread's context lives
        // in the HashMap (stable address). The scheduler context is on
        // thread-local storage. Cooperative scheduling ensures single access.
        unsafe {
            switch_context(ctx_ptr, sched_ctx.get());
        }
    });

    // We've been woken up — reset yield flag and check result.
    SHOULD_YIELD.set(true);
    true
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::AtomicI64;

    // -----------------------------------------------------------------------
    // GreenThreadId tests
    // -----------------------------------------------------------------------

    #[test]
    fn green_thread_id_unique() {
        const COUNT: usize = 1_000;
        let ids: HashSet<u64> = (0..COUNT).map(|_| GreenThreadId::next().0).collect();
        assert_eq!(
            ids.len(),
            COUNT,
            "every GreenThreadId should be unique within a process"
        );
    }

    // -----------------------------------------------------------------------
    // GreenThread construction tests
    // -----------------------------------------------------------------------

    unsafe fn noop_entry(_arg: usize) {}

    #[test]
    fn green_thread_new_has_ready_status() {
        // SAFETY: noop_entry is a valid function pointer; we never switch to
        // this thread, so the stack is freed without being used.
        let thread = unsafe { GreenThread::new(noop_entry, 0) };
        assert_eq!(
            thread.status,
            ThreadStatus::Ready,
            "a freshly created thread must start as Ready"
        );
    }

    #[test]
    fn green_thread_new_has_no_future_or_result() {
        // SAFETY: same as above.
        let thread = unsafe { GreenThread::new(noop_entry, 0) };
        assert!(thread.future_id.is_none());
        assert!(thread.result.is_none());
    }

    #[test]
    fn green_thread_ids_are_distinct() {
        // SAFETY: same as above.
        let t1 = unsafe { GreenThread::new(noop_entry, 0) };
        let t2 = unsafe { GreenThread::new(noop_entry, 0) };
        assert_ne!(
            t1.id, t2.id,
            "two distinct green threads must have different IDs"
        );
    }

    // -----------------------------------------------------------------------
    // Stack allocation tests
    // -----------------------------------------------------------------------

    #[test]
    fn stack_alloc_free_roundtrip() {
        // SAFETY: alloc_stack returns a valid mapping; free_stack unmaps it
        // with the same size.  No memory access occurs between the two calls.
        let size = get_stack_size();
        unsafe {
            let ptr = alloc_stack(size);
            assert!(!ptr.is_null(), "alloc_stack must return a non-null pointer");
            free_stack(ptr, size);
        }
    }

    #[test]
    fn stack_is_writable_after_alloc() {
        let size = get_stack_size();
        let guard = page_size();
        // SAFETY: ptr is a valid mapping.  The guard page at the bottom is
        // PROT_NONE so we write to the first byte *after* the guard and
        // the last byte of the mapping.
        unsafe {
            let ptr = alloc_stack(size);
            // Skip guard page — writing to ptr[0] would SIGSEGV.
            ptr.add(guard).write(0xAB);
            ptr.add(size - 1).write(0xCD);
            assert_eq!(ptr.add(guard).read(), 0xAB);
            assert_eq!(ptr.add(size - 1).read(), 0xCD);
            free_stack(ptr, size);
        }
    }

    #[test]
    fn get_stack_size_returns_at_least_two_pages() {
        let size = get_stack_size();
        let ps = page_size();
        assert!(
            size >= ps * 2,
            "stack size must be at least two pages (guard + usable)"
        );
    }

    #[test]
    fn page_size_is_positive_and_power_of_two() {
        let ps = page_size();
        assert!(ps > 0, "page size must be positive");
        assert!(ps.is_power_of_two(), "page size should be a power of two");
    }

    // -----------------------------------------------------------------------
    // Drop test
    // -----------------------------------------------------------------------

    #[test]
    fn green_thread_drop_frees_stack() {
        // Creating and immediately dropping a GreenThread must not crash or
        // leak memory (verified by the OS on process exit in sanitiser mode).
        // SAFETY: noop_entry is valid; we never switch to the thread.
        let thread = unsafe { GreenThread::new(noop_entry, 0) };
        drop(thread);
        // If we reach here the Drop impl ran without a segfault or panic.
    }

    // -----------------------------------------------------------------------
    // Scheduler tests
    // -----------------------------------------------------------------------

    // Note: Because SCHEDULER is a global OnceLock and tests run in the same
    // process, we can only initialise it once.  These tests work around that
    // by using a dedicated init function and testing the internal mechanics
    // that don't depend on the global singleton.

    #[test]
    fn scheduler_init_creates_workers() {
        let sched = Scheduler::new(2);
        assert_eq!(sched.num_workers, 2, "should create 2 workers");
        assert!(
            !sched.shutdown.load(Ordering::Relaxed),
            "should not be shut down"
        );
        assert!(
            sched.threads.lock().is_ok(),
            "thread map should be accessible"
        );
        assert!(
            sched.global_queue.lock().is_ok(),
            "global queue should be accessible"
        );
    }

    #[test]
    fn spawn_single_thread_runs() {
        static FLAG: AtomicBool = AtomicBool::new(false);

        extern "C" fn set_flag() {
            FLAG.store(true, Ordering::SeqCst);
        }

        FLAG.store(false, Ordering::SeqCst);

        // Use the full scheduler API.
        // SAFETY: calling the extern "C" scheduler API.
        unsafe {
            kodo_green_init(1);
            kodo_green_spawn(set_flag as *const () as i64);
            kodo_green_run();
        }

        assert!(
            FLAG.load(Ordering::SeqCst),
            "green thread should have set the flag"
        );
    }

    #[test]
    fn spawn_multiple_threads_all_run() {
        static COUNTER: AtomicI64 = AtomicI64::new(0);

        extern "C" fn increment() {
            COUNTER.fetch_add(1, Ordering::SeqCst);
        }

        COUNTER.store(0, Ordering::SeqCst);

        // Use the full scheduler API.  We must ensure init is called.
        // SAFETY: calling the extern "C" API is safe in test context.
        unsafe {
            kodo_green_init(2);
        }

        for _ in 0..10 {
            // SAFETY: increment is a valid extern "C" fn() pointer.
            unsafe {
                kodo_green_spawn(increment as *const () as i64);
            }
        }

        // Run the scheduler — blocks until all threads complete.
        // SAFETY: init was called and threads were spawned.
        unsafe {
            kodo_green_run();
        }

        assert_eq!(
            COUNTER.load(Ordering::SeqCst),
            10,
            "all 10 green threads should have run"
        );
    }

    #[test]
    fn yield_does_not_crash() {
        // Calling kodo_green_maybe_yield outside a green thread should be
        // a no-op (CURRENT_THREAD is None, SHOULD_YIELD is false).
        // SAFETY: safe to call outside a green thread — it returns immediately.
        unsafe {
            kodo_green_maybe_yield();
        }
    }

    #[test]
    fn scheduler_completes_when_all_done() {
        static DONE: AtomicBool = AtomicBool::new(false);

        extern "C" fn mark_done() {
            DONE.store(true, Ordering::SeqCst);
        }

        DONE.store(false, Ordering::SeqCst);

        // SAFETY: calling the extern "C" API.
        unsafe {
            kodo_green_init(1);
            kodo_green_spawn(mark_done as *const () as i64);
            kodo_green_run();
        }

        assert!(
            DONE.load(Ordering::SeqCst),
            "scheduler should complete after all threads finish"
        );
    }

    // -----------------------------------------------------------------------
    // Future table tests
    // -----------------------------------------------------------------------

    #[test]
    fn future_new_returns_positive_handle() {
        // SAFETY: kodo_future_new is safe to call from any context.
        let handle = unsafe { kodo_future_new() };
        assert!(handle > 0, "future handle should be a positive integer");
    }

    #[test]
    fn future_complete_then_await_returns_result() {
        // Create a future, complete it immediately, then await should return
        // the result without blocking (since we're not in a green thread).
        // SAFETY: calling the extern "C" API from a test context.
        unsafe {
            let handle = kodo_future_new();
            kodo_future_complete(handle, 42);
            let result = kodo_future_await(handle);
            assert_eq!(result, 42, "awaiting a completed future should return 42");
        }
    }

    #[test]
    fn future_handles_are_unique() {
        // Each call to kodo_future_new should return a distinct handle.
        // SAFETY: calling the extern "C" API from a test context.
        let handles: Vec<i64> = (0..100).map(|_| unsafe { kodo_future_new() }).collect();
        let unique: HashSet<i64> = handles.iter().copied().collect();
        assert_eq!(unique.len(), 100, "every future handle should be unique");
    }

    #[test]
    fn future_await_unknown_handle_returns_zero() {
        // Awaiting a handle that was never created should return 0.
        // SAFETY: calling the extern "C" API from a test context.
        unsafe {
            let result = kodo_future_await(999_999);
            assert_eq!(result, 0, "unknown future should return 0");
        }
    }

    #[test]
    fn future_complete_bytes_then_await_bytes_roundtrip() {
        // Complete a future with 16 bytes (simulating a String ptr+len pair)
        // and read them back via kodo_future_await_bytes.
        // SAFETY: calling the extern "C" API from a test context.
        unsafe {
            let handle = kodo_future_new();

            // Simulate a String value: ptr=0x1234, len=5
            let data: [i64; 2] = [0x1234, 5];
            kodo_future_complete_bytes(
                handle,
                data.as_ptr() as i64,
                std::mem::size_of_val(&data) as i64,
            );

            let mut out: [i64; 2] = [0, 0];
            kodo_future_await_bytes(
                handle,
                out.as_mut_ptr() as i64,
                std::mem::size_of_val(&out) as i64,
            );

            assert_eq!(out[0], 0x1234, "ptr component should match");
            assert_eq!(out[1], 5, "len component should match");
        }
    }

    #[test]
    fn future_complete_bytes_then_await_i64_reads_first_8_bytes() {
        // Complete a future with bytes but await with kodo_future_await —
        // should read the first 8 bytes as an i64.
        // SAFETY: calling the extern "C" API from a test context.
        unsafe {
            let handle = kodo_future_new();
            let value: i64 = 99;
            kodo_future_complete_bytes(
                handle,
                &value as *const i64 as i64,
                std::mem::size_of::<i64>() as i64,
            );
            let result = kodo_future_await(handle);
            assert_eq!(result, 99, "i64 await should extract value from bytes");
        }
    }
}
