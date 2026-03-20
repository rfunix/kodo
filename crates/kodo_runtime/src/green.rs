//! # Green Thread Data Structures
//!
//! Provides the [`GreenThread`] struct — a cooperatively-scheduled lightweight
//! thread with its own `mmap`'d stack and CPU context.  Each green thread is
//! independent of OS threads; the scheduler (`kodo_runtime::scheduler`) is
//! responsible for switching between them.
//!
//! ## Memory layout
//!
//! Each green thread owns a contiguous `STACK_SIZE`-byte region obtained via
//! `mmap(MAP_PRIVATE | MAP_ANONYMOUS)`.  The stack pointer starts at the **top**
//! (highest address) of this region and grows downward, following the
//! System V AMD64 / AAPCS64 ABI convention.
//!
//! The region is freed via `munmap` when the [`GreenThread`] is dropped.

use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default stack size per green thread (64 KB).
pub const STACK_SIZE: usize = 64 * 1024;

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

/// Allocates a fixed-size stack using `mmap`.
///
/// Returns a pointer to the **base** (lowest address) of the mapping.
/// The caller is responsible for computing the stack top as `base + size`.
///
/// # Safety
///
/// The returned pointer must eventually be passed to [`free_stack`] with the
/// same `size` to avoid a memory leak.  The mapping is readable and writable
/// but not executable.
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
    pub context: crate::context::Context,
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
    /// Allocates a [`STACK_SIZE`]-byte stack via `mmap` and initialises the
    /// CPU context so that the first [`crate::context::switch_context`] into
    /// this thread begins executing `entry(arg)`.
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
        // SAFETY: alloc_stack returns a valid, writable region of `STACK_SIZE`
        // bytes.  We compute the top as base + size (stacks grow downward).
        let stack = unsafe { alloc_stack(STACK_SIZE) };
        // SAFETY: stack + STACK_SIZE is within the allocated region.
        let stack_top = unsafe { stack.add(STACK_SIZE) };

        let mut ctx = crate::context::Context::default();
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
            stack_size: STACK_SIZE,
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
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
        unsafe {
            let ptr = alloc_stack(STACK_SIZE);
            assert!(!ptr.is_null(), "alloc_stack must return a non-null pointer");
            free_stack(ptr, STACK_SIZE);
        }
    }

    #[test]
    fn stack_is_writable_after_alloc() {
        // SAFETY: ptr is a valid PROT_READ | PROT_WRITE mapping.
        unsafe {
            let ptr = alloc_stack(STACK_SIZE);
            // Write to the first and last bytes to confirm accessibility.
            ptr.write(0xAB);
            ptr.add(STACK_SIZE - 1).write(0xCD);
            assert_eq!(ptr.read(), 0xAB);
            assert_eq!(ptr.add(STACK_SIZE - 1).read(), 0xCD);
            free_stack(ptr, STACK_SIZE);
        }
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
}
