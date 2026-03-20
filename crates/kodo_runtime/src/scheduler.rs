//! Cooperative scheduler and concurrency primitives for the Kōdo runtime.
//!
//! Provides task spawning, a cooperative scheduler, parallel groups,
//! channels for inter-thread communication, and an async thread pool
//! with future-based awaiting.

/// A queued task — either a plain function pointer or one with an environment.
enum Task {
    /// A zero-argument task (no captures).
    Plain(extern "C" fn()),
    /// A task with a captured environment.
    ///
    /// The function takes a pointer to the environment data. The `env` Vec
    /// owns the copied environment bytes so they stay alive until the task
    /// runs.
    WithEnv {
        /// Function pointer that accepts an env pointer as its sole argument.
        func: extern "C" fn(i64),
        /// Owned copy of the environment data.
        env: Vec<u8>,
    },
}

/// Task queue for the cooperative scheduler.
///
/// Stores tasks that have been spawned — either plain function pointers or
/// functions paired with captured environment data.
/// All tasks are executed when `kodo_run_scheduler` is called.
static TASK_QUEUE: std::sync::Mutex<Vec<Task>> = std::sync::Mutex::new(Vec::new());

/// A task inside a parallel group.
enum ParallelTask {
    /// A plain function with no captures.
    Plain(extern "C" fn()),
    /// A function that takes a captured-environment pointer.
    WithEnv {
        /// The function pointer.
        func: extern "C" fn(i64),
        /// Captured environment bytes (copied from caller stack).
        env: Vec<u8>,
    },
}

/// A group of tasks to be executed in parallel.
struct ParallelGroup {
    /// The tasks queued for parallel execution.
    tasks: Vec<ParallelTask>,
}

static PARALLEL_GROUPS: std::sync::Mutex<Vec<Option<ParallelGroup>>> =
    std::sync::Mutex::new(Vec::new());
static PARALLEL_GROUP_COUNTER: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Creates a new parallel group and returns its identifier.
#[no_mangle]
pub extern "C" fn kodo_parallel_begin() -> i64 {
    let id = PARALLEL_GROUP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut groups) = PARALLEL_GROUPS.lock() {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let idx = id as usize;
        if groups.len() <= idx {
            groups.resize_with(idx + 1, || None);
        }
        groups[idx] = Some(ParallelGroup { tasks: Vec::new() });
    }
    id
}

/// Adds a task to a parallel group.
///
/// # Safety
///
/// When `env_size > 0`, the caller must ensure `env_ptr` points to
/// a readable buffer of at least `env_size` bytes.
#[no_mangle]
pub extern "C" fn kodo_parallel_spawn(group: i64, fn_ptr: i64, env_ptr: i64, env_size: i64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = env_size as usize;
    let task = if size == 0 {
        // SAFETY: `fn_ptr` is a valid function pointer from Cranelift codegen.
        let func: extern "C" fn() = unsafe { std::mem::transmute(fn_ptr) };
        ParallelTask::Plain(func)
    } else {
        let mut env = vec![0u8; size];
        // SAFETY: caller guarantees `env_ptr` points to `env_size` readable bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(env_ptr as *const u8, env.as_mut_ptr(), size);
        }
        // SAFETY: `fn_ptr` is a valid function pointer from Cranelift codegen.
        let func: extern "C" fn(i64) = unsafe { std::mem::transmute(fn_ptr) };
        ParallelTask::WithEnv { func, env }
    };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = group as usize;
    if let Ok(mut groups) = PARALLEL_GROUPS.lock() {
        if let Some(Some(g)) = groups.get_mut(idx) {
            g.tasks.push(task);
        }
    }
}

/// Joins all tasks in a parallel group using [`std::thread::scope`].
#[no_mangle]
pub extern "C" fn kodo_parallel_join(group: i64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = group as usize;
    let tasks = {
        if let Ok(mut groups) = PARALLEL_GROUPS.lock() {
            groups
                .get_mut(idx)
                .and_then(Option::take)
                .map(|g| g.tasks)
                .unwrap_or_default()
        } else {
            return;
        }
    };
    std::thread::scope(|s| {
        for task in &tasks {
            match task {
                ParallelTask::Plain(func) => {
                    let f = *func;
                    s.spawn(move || f());
                }
                ParallelTask::WithEnv { func, env } => {
                    let f = *func;
                    let ptr = env.as_ptr() as i64;
                    s.spawn(move || f(ptr));
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Channel support — inter-thread communication via `std::sync::mpsc`
// ---------------------------------------------------------------------------

/// A value that can be sent through a generic channel.
///
/// Channels in Kōdo support multiple primitive types: integers, booleans,
/// and heap-allocated strings. This enum wraps all supported payloads so a
/// single `mpsc::channel` can carry any of them.
#[derive(Debug, Clone)]
enum ChannelValue {
    /// A 64-bit integer value.
    Int(i64),
    /// A boolean value.
    Bool(bool),
    /// A heap-allocated string, stored as a `Vec<u8>`.
    ///
    /// The runtime copies the bytes on send and reconstructs a `(ptr, len)`
    /// pair on recv, so callers do not share memory across threads.
    StringVal(Vec<u8>),
}

/// A channel pair stored in the global registry.
struct ChannelEntry {
    /// The sending half (wrapped in `Arc<Mutex<…>>` so multiple threads can send).
    sender: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Sender<ChannelValue>>>,
    /// The receiving half (wrapped in `Arc<Mutex<…>>` so we can access it
    /// without holding the registry lock, preventing deadlocks on recv).
    receiver_arc: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<ChannelValue>>>,
    /// Buffer for a value peeked by `channel_select_*`.
    ///
    /// When a select operation discovers data on a channel via `try_recv`, it
    /// stores the consumed value here. The next `channel_recv` call checks
    /// this buffer first, returning the peeked value without touching the
    /// underlying `mpsc` receiver.
    peeked: std::sync::Arc<std::sync::Mutex<Option<ChannelValue>>>,
}

/// Global registry of live channels, keyed by handle.
static CHANNEL_REGISTRY: std::sync::Mutex<Vec<Option<ChannelEntry>>> =
    std::sync::Mutex::new(Vec::new());

/// Monotonically increasing counter for channel handles.
static CHANNEL_COUNTER: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Creates a new channel and returns an opaque integer handle.
///
/// The handle can be passed to [`kodo_channel_send`], [`kodo_channel_recv`],
/// and [`kodo_channel_free`].
#[no_mangle]
pub extern "C" fn kodo_channel_new() -> i64 {
    let id = CHANNEL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (tx, rx) = std::sync::mpsc::channel();
    let entry = ChannelEntry {
        sender: std::sync::Arc::new(std::sync::Mutex::new(tx)),
        receiver_arc: std::sync::Arc::new(std::sync::Mutex::new(rx)),
        peeked: std::sync::Arc::new(std::sync::Mutex::new(None)),
    };
    if let Ok(mut registry) = CHANNEL_REGISTRY.lock() {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let idx = id as usize;
        if registry.len() <= idx {
            registry.resize_with(idx + 1, || None);
        }
        registry[idx] = Some(entry);
    }
    id
}

/// Clones the sender `Arc` for a given channel handle.
///
/// Returns `None` if the handle is invalid or the registry lock is poisoned.
fn channel_get_sender(
    handle: i64,
) -> Option<std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Sender<ChannelValue>>>> {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = handle as usize;
    let registry = CHANNEL_REGISTRY.lock().ok()?;
    registry
        .get(idx)
        .and_then(Option::as_ref)
        .map(|entry| std::sync::Arc::clone(&entry.sender))
}

/// Clones the receiver `Arc` for a given channel handle.
///
/// Returns `None` if the handle is invalid or the registry lock is poisoned.
fn channel_get_receiver(
    handle: i64,
) -> Option<std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<ChannelValue>>>> {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = handle as usize;
    let registry = CHANNEL_REGISTRY.lock().ok()?;
    registry
        .get(idx)
        .and_then(Option::as_ref)
        .map(|entry| std::sync::Arc::clone(&entry.receiver_arc))
}

/// Clones the peeked-value `Arc` for a given channel handle.
///
/// Returns `None` if the handle is invalid or the registry lock is poisoned.
fn channel_get_peeked(
    handle: i64,
) -> Option<std::sync::Arc<std::sync::Mutex<Option<ChannelValue>>>> {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = handle as usize;
    let registry = CHANNEL_REGISTRY.lock().ok()?;
    registry
        .get(idx)
        .and_then(Option::as_ref)
        .map(|entry| std::sync::Arc::clone(&entry.peeked))
}

/// Attempts a non-blocking receive on the channel. If data is available,
/// stores it in the channel's `peeked` buffer and returns `true`.
fn try_peek_channel(handle: i64) -> bool {
    let Some(rx_arc) = channel_get_receiver(handle) else {
        return false;
    };
    let Some(peeked_arc) = channel_get_peeked(handle) else {
        return false;
    };
    // If there is already a peeked value, this channel has data ready.
    if let Ok(peeked) = peeked_arc.lock() {
        if peeked.is_some() {
            return true;
        }
    }
    // Try a non-blocking recv. We bind the guard explicitly so the
    // temporary lives long enough (avoiding E0597).
    let received = {
        let guard = rx_arc.lock();
        if let Ok(rx) = guard {
            rx.try_recv().ok()
        } else {
            None
        }
    };
    if let Some(val) = received {
        if let Ok(mut peeked) = peeked_arc.lock() {
            *peeked = Some(val);
        }
        true
    } else {
        false
    }
}

/// Sends an integer `value` through the channel identified by `handle`.
///
/// If the handle is invalid or the receiver has been dropped the call is a
/// silent no-op (matching the fire-and-forget semantics of Kōdo channels).
#[no_mangle]
pub extern "C" fn kodo_channel_send(handle: i64, value: i64) {
    let Some(tx_arc) = channel_get_sender(handle) else {
        return;
    };
    let guard = tx_arc.lock();
    if let Ok(tx) = guard {
        let _ = tx.send(ChannelValue::Int(value));
    }
}

/// Receives an integer value from the channel identified by `handle` (blocking).
///
/// If a value was buffered by a prior `channel_select_*` call, it is returned
/// immediately without touching the underlying `mpsc` receiver. Otherwise
/// blocks until a value arrives.
///
/// Returns the received `i64` value. If the channel is closed, the handle
/// is invalid, or the received value is not an integer, returns `0`.
#[no_mangle]
pub extern "C" fn kodo_channel_recv(handle: i64) -> i64 {
    // Check for a peeked value first (deposited by channel_select_*).
    if let Some(peeked_arc) = channel_get_peeked(handle) {
        if let Ok(mut peeked) = peeked_arc.lock() {
            if let Some(val) = peeked.take() {
                return match val {
                    ChannelValue::Int(v) => v,
                    ChannelValue::Bool(b) => i64::from(b),
                    ChannelValue::StringVal(_) => 0,
                };
            }
        }
    }

    let Some(rx_arc) = channel_get_receiver(handle) else {
        return 0;
    };
    let guard = rx_arc.lock();
    if let Ok(rx) = guard {
        match rx.recv() {
            Ok(ChannelValue::Int(v)) => v,
            Ok(ChannelValue::Bool(b)) => i64::from(b),
            _ => 0,
        }
    } else {
        0
    }
}

/// Sends a boolean value through the channel identified by `handle`.
#[no_mangle]
pub extern "C" fn kodo_channel_send_bool(handle: i64, value: i64) {
    let Some(tx_arc) = channel_get_sender(handle) else {
        return;
    };
    let guard = tx_arc.lock();
    if let Ok(tx) = guard {
        let _ = tx.send(ChannelValue::Bool(value != 0));
    }
}

/// Receives a boolean value from the channel identified by `handle` (blocking).
///
/// If a value was buffered by a prior `channel_select_*` call, it is returned
/// immediately. Otherwise blocks until a value arrives.
///
/// Returns `1` for `true`, `0` for `false`.
#[no_mangle]
pub extern "C" fn kodo_channel_recv_bool(handle: i64) -> i64 {
    // Check for a peeked value first (deposited by channel_select_*).
    if let Some(peeked_arc) = channel_get_peeked(handle) {
        if let Ok(mut peeked) = peeked_arc.lock() {
            if let Some(val) = peeked.take() {
                return match val {
                    ChannelValue::Bool(b) => i64::from(b),
                    ChannelValue::Int(v) => i64::from(v != 0),
                    ChannelValue::StringVal(_) => 0,
                };
            }
        }
    }

    let Some(rx_arc) = channel_get_receiver(handle) else {
        return 0;
    };
    let guard = rx_arc.lock();
    if let Ok(rx) = guard {
        match rx.recv() {
            Ok(ChannelValue::Bool(b)) => i64::from(b),
            Ok(ChannelValue::Int(v)) => i64::from(v != 0),
            _ => 0,
        }
    } else {
        0
    }
}

/// Sends a string through the channel identified by `handle`.
///
/// # Safety
///
/// `ptr` must point to a valid byte buffer of at least `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_channel_send_string(handle: i64, ptr: *const u8, len: usize) {
    let Some(tx_arc) = channel_get_sender(handle) else {
        return;
    };
    // SAFETY: caller guarantees ptr is valid for `len` bytes.
    let bytes = if ptr.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
    };
    let guard = tx_arc.lock();
    if let Ok(tx) = guard {
        let _ = tx.send(ChannelValue::StringVal(bytes));
    }
}

/// Receives a string from the channel identified by `handle` (blocking).
///
/// If a value was buffered by a prior `channel_select_*` call, it is returned
/// immediately. Otherwise blocks until a value arrives.
///
/// # Safety
///
/// `out_ptr` and `out_len` must point to valid writable locations.
#[no_mangle]
pub unsafe extern "C" fn kodo_channel_recv_string(
    handle: i64,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) {
    let write_empty = || {
        // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe {
            *out_ptr = std::ptr::null_mut();
            *out_len = 0;
        }
    };

    // Check for a peeked value first (deposited by channel_select_*).
    if let Some(peeked_arc) = channel_get_peeked(handle) {
        if let Ok(mut peeked) = peeked_arc.lock() {
            if let Some(val) = peeked.take() {
                if let ChannelValue::StringVal(bytes) = val {
                    let len = bytes.len();
                    let boxed = bytes.into_boxed_slice();
                    let raw = Box::into_raw(boxed).cast::<u8>();
                    // SAFETY: caller guarantees out_ptr/out_len are writable.
                    unsafe {
                        *out_ptr = raw;
                        *out_len = len;
                    }
                    return;
                }
                write_empty();
                return;
            }
        }
    }

    let Some(rx_arc) = channel_get_receiver(handle) else {
        write_empty();
        return;
    };
    let received = if let Ok(rx) = rx_arc.lock() {
        rx.recv().ok()
    } else {
        None
    };
    match received {
        Some(ChannelValue::StringVal(bytes)) => {
            let len = bytes.len();
            let boxed = bytes.into_boxed_slice();
            let raw = Box::into_raw(boxed).cast::<u8>();
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe {
                *out_ptr = raw;
                *out_len = len;
            }
        }
        _ => {
            write_empty();
        }
    }
}

/// Waits on two channels and returns the index (0 or 1) of the first channel
/// that has data available.
///
/// Polls both channels in a loop using non-blocking `try_recv`. When data is
/// found, it is stored in the channel's `peeked` buffer so the subsequent
/// `channel_recv` call retrieves it without blocking.
///
/// Calls `kodo_green_maybe_yield` between attempts to cooperate with the
/// green-thread scheduler.
///
/// # Safety
///
/// Both `ch1` and `ch2` must be valid channel handles obtained from
/// `kodo_channel_new`.
#[no_mangle]
pub extern "C" fn kodo_channel_select_2(ch1: i64, ch2: i64) -> i64 {
    loop {
        if try_peek_channel(ch1) {
            return 0;
        }
        if try_peek_channel(ch2) {
            return 1;
        }
        // Yield to the green-thread scheduler before retrying.
        // SAFETY: kodo_green_maybe_yield is always safe to call.
        unsafe {
            crate::green::kodo_green_maybe_yield();
        }
        std::thread::yield_now();
    }
}

/// Waits on three channels and returns the index (0, 1, or 2) of the first
/// channel that has data available.
///
/// Polls all three channels in a loop using non-blocking `try_recv`. When data
/// is found, it is stored in the channel's `peeked` buffer so the subsequent
/// `channel_recv` call retrieves it without blocking.
///
/// Calls `kodo_green_maybe_yield` between attempts to cooperate with the
/// green-thread scheduler.
///
/// # Safety
///
/// All three handles must be valid channel handles obtained from
/// `kodo_channel_new`.
#[no_mangle]
pub extern "C" fn kodo_channel_select_3(ch1: i64, ch2: i64, ch3: i64) -> i64 {
    loop {
        if try_peek_channel(ch1) {
            return 0;
        }
        if try_peek_channel(ch2) {
            return 1;
        }
        if try_peek_channel(ch3) {
            return 2;
        }
        // Yield to the green-thread scheduler before retrying.
        // SAFETY: kodo_green_maybe_yield is always safe to call.
        unsafe {
            crate::green::kodo_green_maybe_yield();
        }
        std::thread::yield_now();
    }
}

/// Frees the channel identified by `handle`, dropping both sender and receiver.
#[no_mangle]
pub extern "C" fn kodo_channel_free(handle: i64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = handle as usize;
    if let Ok(mut registry) = CHANNEL_REGISTRY.lock() {
        if let Some(slot) = registry.get_mut(idx) {
            *slot = None;
        }
    }
}

/// Spawns a task by adding its function pointer to the task queue.
///
/// The task will be executed when the scheduler runs (at the end of `main`).
#[no_mangle]
pub extern "C" fn kodo_spawn_task(task: extern "C" fn()) {
    if let Ok(mut queue) = TASK_QUEUE.lock() {
        queue.push(Task::Plain(task));
    }
}

/// Spawns a task that carries a captured environment.
///
/// `fn_ptr` is a function that takes a single `i64` argument (a pointer to
/// the environment data). `env_ptr` points to the environment buffer in the
/// caller's stack frame and `env_size` is its size in bytes. The runtime
/// copies the environment so it remains valid when the task eventually runs.
///
/// # Safety
///
/// The caller must ensure that `env_ptr` points to a readable buffer of at
/// least `env_size` bytes.
#[no_mangle]
pub extern "C" fn kodo_spawn_task_with_env(fn_ptr: i64, env_ptr: i64, env_size: i64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = env_size as usize;
    let mut env = vec![0u8; size];
    if size > 0 {
        // SAFETY: The caller guarantees `env_ptr` points to `env_size`
        // readable bytes (the captures packed on the caller's stack).
        unsafe {
            std::ptr::copy_nonoverlapping(env_ptr as *const u8, env.as_mut_ptr(), size);
        }
    }
    // SAFETY: `fn_ptr` is a valid function pointer produced by Cranelift
    // codegen with the `extern "C" fn(i64)` signature.
    let func: extern "C" fn(i64) = unsafe { std::mem::transmute(fn_ptr) };
    if let Ok(mut queue) = TASK_QUEUE.lock() {
        queue.push(Task::WithEnv { func, env });
    }
}

/// Runs all spawned tasks in FIFO order.
///
/// Called automatically by the runtime after `kodo_main` returns.
/// Tasks may spawn additional tasks, which are executed in subsequent
/// passes until the queue is empty.
#[no_mangle]
pub extern "C" fn kodo_run_scheduler() {
    loop {
        let tasks: Vec<Task> = {
            if let Ok(mut queue) = TASK_QUEUE.lock() {
                std::mem::take(&mut *queue)
            } else {
                break;
            }
        };
        if tasks.is_empty() {
            break;
        }
        for task in tasks {
            match task {
                Task::Plain(func) => func(),
                Task::WithEnv { func, env } => func(env.as_ptr() as i64),
            }
        }
    }
}

/// Legacy no-op spawn stub (kept for backwards compatibility).
#[no_mangle]
pub extern "C" fn kodo_spawn() {
    // Legacy stub — new code uses kodo_spawn_task.
}

// ---------------------------------------------------------------------------
// Async runtime — thread-pool-based spawn/await with future handles
// ---------------------------------------------------------------------------

/// Number of worker threads in the async thread pool.
const ASYNC_THREAD_POOL_SIZE: usize = 4;

/// A boxed task closure that can be sent to a worker thread.
type BoxedTask = Box<dyn FnOnce() + Send>;

/// The sender half of the thread pool's task channel, wrapped in a Mutex
/// so it can be stored in a static.
type PoolSender = std::sync::Mutex<std::sync::mpsc::Sender<BoxedTask>>;

/// An entry in the global future table.
///
/// Each future tracks whether the spawned task has completed and stores
/// the result value. A `Condvar` is used so that `kodo_await` can block
/// efficiently until the result is ready.
struct FutureEntry {
    /// The result value, set by the worker thread upon completion.
    result: std::sync::Mutex<Option<i64>>,
    /// Flag indicating whether the task has finished.
    completed: std::sync::atomic::AtomicBool,
    /// Condition variable to wake up threads waiting on this future.
    condvar: std::sync::Condvar,
}

/// Global table of outstanding futures, indexed by handle.
static FUTURE_TABLE: std::sync::Mutex<Vec<Option<std::sync::Arc<FutureEntry>>>> =
    std::sync::Mutex::new(Vec::new());

/// Monotonically increasing counter for future handles.
static FUTURE_COUNTER: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Lazily initialised global thread pool sender.
///
/// Worker threads are spawned on the first call to [`kodo_spawn_async`].
/// Tasks are sent via an `mpsc` channel so workers can pick them up.
static THREAD_POOL_TX: std::sync::OnceLock<PoolSender> = std::sync::OnceLock::new();

/// Initialises the thread pool (idempotent — only the first call spawns threads).
fn ensure_thread_pool() -> &'static PoolSender {
    THREAD_POOL_TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        for _ in 0..ASYNC_THREAD_POOL_SIZE {
            let rx = std::sync::Arc::clone(&rx);
            std::thread::spawn(move || {
                loop {
                    let task = {
                        let Ok(guard) = rx.lock() else { break };
                        match guard.recv() {
                            Ok(task) => task,
                            Err(_) => break, // channel closed
                        }
                    };
                    task();
                }
            });
        }
        std::sync::Mutex::new(tx)
    })
}

/// Spawns an async task on the thread pool and returns a future handle.
///
/// `fn_ptr` is a function pointer. When `env_size` is 0 the function has
/// signature `extern "C" fn() -> i64` (no captures). When `env_size > 0`
/// the function has signature `extern "C" fn(i64) -> i64` and receives a
/// pointer to a heap-copied environment buffer.
///
/// The returned handle is an opaque integer that can be passed to
/// [`kodo_await`] to retrieve the result.
#[no_mangle]
pub extern "C" fn kodo_spawn_async(fn_ptr: i64, env_ptr: i64, env_size: i64) -> i64 {
    let handle = FUTURE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let entry = std::sync::Arc::new(FutureEntry {
        result: std::sync::Mutex::new(None),
        completed: std::sync::atomic::AtomicBool::new(false),
        condvar: std::sync::Condvar::new(),
    });

    // Register the future in the global table.
    if let Ok(mut table) = FUTURE_TABLE.lock() {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let idx = handle as usize;
        if table.len() <= idx {
            table.resize_with(idx + 1, || None);
        }
        table[idx] = Some(std::sync::Arc::clone(&entry));
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = env_size as usize;

    if size == 0 {
        // No captures: call as fn() -> i64.
        // SAFETY: `fn_ptr` is a valid function pointer produced by Cranelift
        // codegen with the `extern "C" fn() -> i64` signature.
        let func: extern "C" fn() -> i64 = unsafe { std::mem::transmute(fn_ptr) };

        let pool_tx = ensure_thread_pool();
        if let Ok(tx) = pool_tx.lock() {
            let _ = tx.send(Box::new(move || {
                let result = func();
                if let Ok(mut guard) = entry.result.lock() {
                    *guard = Some(result);
                }
                entry
                    .completed
                    .store(true, std::sync::atomic::Ordering::Release);
                entry.condvar.notify_all();
            }));
        }
    } else {
        // With captures: copy env and call as fn(i64) -> i64.
        let mut env = vec![0u8; size];
        // SAFETY: The caller guarantees `env_ptr` points to `env_size`
        // readable bytes (the captures packed on the caller's stack).
        unsafe {
            std::ptr::copy_nonoverlapping(env_ptr as *const u8, env.as_mut_ptr(), size);
        }
        let func: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr) };

        let pool_tx = ensure_thread_pool();
        if let Ok(tx) = pool_tx.lock() {
            let _ = tx.send(Box::new(move || {
                let env_heap_ptr = env.as_ptr() as i64;
                let result = func(env_heap_ptr);
                if let Ok(mut guard) = entry.result.lock() {
                    *guard = Some(result);
                }
                entry
                    .completed
                    .store(true, std::sync::atomic::Ordering::Release);
                entry.condvar.notify_all();
            }));
        }
    }

    handle
}

/// Blocks until the async task identified by `handle` completes, then returns
/// its result.
///
/// If the handle is invalid or the future table is inaccessible, returns `0`.
#[no_mangle]
pub extern "C" fn kodo_await(handle: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = handle as usize;

    // Retrieve the Arc<FutureEntry> from the table so we can wait on it
    // without holding the table lock.
    let entry = {
        let Ok(table) = FUTURE_TABLE.lock() else {
            return 0;
        };
        match table.get(idx).and_then(|slot| slot.as_ref()) {
            Some(arc) => std::sync::Arc::clone(arc),
            None => return 0,
        }
    };

    // Wait for completion using condvar (avoids busy-spin).
    if !entry.completed.load(std::sync::atomic::Ordering::Acquire) {
        let guard = entry.result.lock();
        if let Ok(guard) = guard {
            let _unused = entry.condvar.wait_while(guard, |_| {
                !entry.completed.load(std::sync::atomic::Ordering::Acquire)
            });
        }
    }

    // Read the result.
    let result = entry.result.lock().ok().and_then(|g| *g).unwrap_or(0);

    // Remove the entry from the table to free memory.
    if let Ok(mut table) = FUTURE_TABLE.lock() {
        if let Some(slot) = table.get_mut(idx) {
            *slot = None;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_task_and_scheduler() {
        use std::sync::atomic::{AtomicI64, Ordering};
        static COUNTER: AtomicI64 = AtomicI64::new(0);

        extern "C" fn increment_counter() {
            COUNTER.fetch_add(1, Ordering::SeqCst);
        }

        COUNTER.store(0, Ordering::SeqCst);
        kodo_spawn_task(increment_counter);
        kodo_spawn_task(increment_counter);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 0, "tasks not yet run");
        kodo_run_scheduler();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 2, "both tasks ran");
    }

    #[test]
    fn spawn_does_not_panic() {
        kodo_spawn();
    }

    #[test]
    fn parallel_group_unique_ids() {
        let id1 = kodo_parallel_begin();
        let id2 = kodo_parallel_begin();
        assert_ne!(id1, id2);
    }

    #[test]
    fn parallel_empty_group_join() {
        let id = kodo_parallel_begin();
        kodo_parallel_join(id);
    }

    #[test]
    fn channel_new_unique() {
        let id1 = kodo_channel_new();
        let id2 = kodo_channel_new();
        assert_ne!(id1, id2);
        kodo_channel_free(id1);
        kodo_channel_free(id2);
    }

    #[test]
    fn channel_send_recv() {
        let ch = kodo_channel_new();
        kodo_channel_send(ch, 42);
        let val = kodo_channel_recv(ch);
        assert_eq!(val, 42);
        kodo_channel_free(ch);
    }

    #[test]
    fn channel_bool_roundtrip() {
        let ch = kodo_channel_new();
        kodo_channel_send_bool(ch, 1);
        let val = kodo_channel_recv_bool(ch);
        assert_eq!(val, 1);
        kodo_channel_free(ch);
    }

    #[test]
    fn spawn_async_and_await() {
        extern "C" fn compute() -> i64 {
            42
        }
        let handle = kodo_spawn_async(compute as *const () as i64, 0, 0);
        let result = kodo_await(handle);
        assert_eq!(result, 42);
    }

    #[test]
    fn await_invalid_handle_returns_zero() {
        let result = kodo_await(999_999);
        assert_eq!(result, 0);
    }

    #[test]
    fn channel_select_2_returns_ready_channel() {
        let ch1 = kodo_channel_new();
        let ch2 = kodo_channel_new();
        // Send data to ch2 only — select should return 1.
        kodo_channel_send(ch2, 99);
        let idx = kodo_channel_select_2(ch1, ch2);
        assert_eq!(idx, 1);
        // The peeked value should be retrievable via recv.
        let val = kodo_channel_recv(ch2);
        assert_eq!(val, 99);
        kodo_channel_free(ch1);
        kodo_channel_free(ch2);
    }

    #[test]
    fn channel_select_2_returns_first_ready() {
        let ch1 = kodo_channel_new();
        let ch2 = kodo_channel_new();
        // Send data to ch1 — select should return 0.
        kodo_channel_send(ch1, 42);
        let idx = kodo_channel_select_2(ch1, ch2);
        assert_eq!(idx, 0);
        let val = kodo_channel_recv(ch1);
        assert_eq!(val, 42);
        kodo_channel_free(ch1);
        kodo_channel_free(ch2);
    }

    #[test]
    fn channel_select_3_returns_ready_channel() {
        let ch1 = kodo_channel_new();
        let ch2 = kodo_channel_new();
        let ch3 = kodo_channel_new();
        // Send data to ch3 only — select should return 2.
        kodo_channel_send(ch3, 77);
        let idx = kodo_channel_select_3(ch1, ch2, ch3);
        assert_eq!(idx, 2);
        let val = kodo_channel_recv(ch3);
        assert_eq!(val, 77);
        kodo_channel_free(ch1);
        kodo_channel_free(ch2);
        kodo_channel_free(ch3);
    }
}
