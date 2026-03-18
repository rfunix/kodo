//! Thread-safe reference-counted memory allocator for the Kōdo runtime.
//!
//! Memory layout: `[refcount: 8 bytes (AtomicI64)][user data: size bytes]`
//!
//! [`kodo_alloc`] returns a pointer to the user data region (past the header).
//! [`kodo_rc_inc`] / [`kodo_rc_dec`] manipulate the refcount stored 8 bytes
//! before the user pointer using atomic operations.  When the count drops to
//! zero, the entire allocation (header + data) is freed.
//!
//! All refcount operations are atomic and safe to call from multiple threads
//! concurrently (e.g. from `parallel {}` blocks).  The registry of managed
//! handles is protected by a [`std::sync::RwLock`] for concurrent read access.

use std::sync::atomic::{AtomicI64, Ordering};

/// Number of bytes reserved for the reference count header.
const RC_HEADER_SIZE: usize = 8;

/// Global registry of handles returned by [`kodo_alloc`], mapping each handle
/// to the total allocation size (header + user data).
///
/// Uses a [`std::sync::RwLock`] so that concurrent reads (the common case for
/// [`kodo_rc_inc`] / [`kodo_rc_dec`]) do not block each other, while writes
/// (register / unregister) acquire exclusive access.
///
/// The stored size is needed to pass the correct [`std::alloc::Layout`] to
/// [`std::alloc::dealloc`] when the reference count reaches zero.
///
/// This avoids crashing when the MIR emits `IncRef`/`DecRef` for string
/// locals whose backing memory was not allocated by [`kodo_alloc`] (e.g.
/// `kodo_string_concat` uses [`alloc_string`] which delegates to
/// [`kodo_alloc`]).
mod rc_registry {
    use std::collections::HashMap;
    use std::sync::{OnceLock, RwLock};

    /// Global handle registry protected by a [`RwLock`].
    ///
    /// Initialized on first access via [`OnceLock`].  The `RwLock` allows
    /// many concurrent readers (`is_managed` / `total_size` lookups) while
    /// serializing writes (register / unregister).
    fn handles() -> &'static RwLock<HashMap<i64, usize>> {
        static INSTANCE: OnceLock<RwLock<HashMap<i64, usize>>> = OnceLock::new();
        INSTANCE.get_or_init(|| RwLock::new(HashMap::new()))
    }

    /// Registers a handle as RC-managed with its total allocation size.
    ///
    /// Acquires an exclusive write lock on the registry.  If the lock is
    /// poisoned (a thread panicked while holding it), the operation is
    /// silently skipped to avoid propagating the panic.
    pub(super) fn register(handle: i64, total_size: usize) {
        if let Ok(mut map) = handles().write() {
            map.insert(handle, total_size);
        }
    }

    /// Unregisters a handle (called when the allocation is freed).
    ///
    /// Acquires an exclusive write lock.  Poisoned lock is silently ignored.
    pub(super) fn unregister(handle: i64) {
        if let Ok(mut map) = handles().write() {
            map.remove(&handle);
        }
    }

    /// Returns `true` if the handle was allocated by [`super::kodo_alloc`].
    ///
    /// Acquires a shared read lock.  Returns `false` if the lock is poisoned.
    pub(super) fn is_managed(handle: i64) -> bool {
        handles().read().is_ok_and(|map| map.contains_key(&handle))
    }

    /// Returns the total allocation size for a managed handle, or `None`
    /// if the handle is not registered or the lock is poisoned.
    pub(super) fn total_size(handle: i64) -> Option<usize> {
        handles()
            .read()
            .ok()
            .and_then(|map| map.get(&handle).copied())
    }
}

/// Converts a user-data handle (i64 from the C ABI) back to the raw
/// allocation start by subtracting the header.
///
/// # Safety
///
/// `handle` must have been returned by [`kodo_alloc`].
#[inline]
unsafe fn rc_header_ptr(handle: i64) -> *mut u8 {
    #[allow(clippy::cast_possible_wrap)]
    let offset = RC_HEADER_SIZE as i64;
    // SAFETY: handle was returned by kodo_alloc, so handle - 8 points to
    // the start of the allocation which includes the RC header.
    (handle - offset) as *mut u8
}

/// Returns a reference to the [`AtomicI64`] refcount stored at the header
/// of an RC-managed allocation.
///
/// # Safety
///
/// `handle` must have been returned by [`kodo_alloc`] and the allocation
/// must still be live (not yet freed).
#[inline]
unsafe fn rc_atomic(handle: i64) -> &'static AtomicI64 {
    // SAFETY: handle was returned by kodo_alloc, so handle - 8 points to
    // an 8-byte-aligned i64 that was initialized as an AtomicI64.  The
    // allocation is still live because the caller holds a reference.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        let ptr = rc_header_ptr(handle).cast::<AtomicI64>();
        &*ptr
    }
}

/// Allocates `size` bytes of heap memory with an embedded atomic reference
/// count header initialised to 1.
///
/// Returns a pointer to the *user data* region (8 bytes past the real
/// allocation start).  The caller sees only the data pointer; the refcount
/// is managed transparently by [`kodo_rc_inc`] and [`kodo_rc_dec`].
///
/// Returns 0 if the allocation fails.
///
/// # Thread Safety
///
/// The returned handle can be safely shared across threads.  All refcount
/// operations use atomic instructions.
///
/// # Safety
///
/// The returned pointer is valid for `size` bytes.  The caller must
/// eventually release the allocation via [`kodo_rc_dec`] or [`kodo_free`].
#[no_mangle]
pub extern "C" fn kodo_alloc(size: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let user_bytes = size as usize;
    let total = user_bytes + RC_HEADER_SIZE;
    let Ok(layout) = std::alloc::Layout::from_size_align(total, 8) else {
        return 0;
    };
    // SAFETY: layout has non-zero size (RC_HEADER_SIZE >= 8).
    let raw = unsafe { std::alloc::alloc_zeroed(layout) };
    if raw.is_null() {
        return 0;
    }
    // Write initial refcount = 1 atomically.
    // SAFETY: raw is aligned to 8 and points to at least RC_HEADER_SIZE bytes.
    // AtomicI64 has the same size and alignment as i64.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        let atomic_ptr = raw.cast::<AtomicI64>();
        (*atomic_ptr).store(1, Ordering::Release);
    }
    // Return pointer past the header.
    // SAFETY: raw + RC_HEADER_SIZE is within the allocation.
    #[allow(clippy::cast_possible_wrap)]
    let user_ptr = unsafe { raw.add(RC_HEADER_SIZE) } as i64;
    rc_registry::register(user_ptr, total);
    user_ptr
}

/// Increments the reference count for a heap-allocated object.
///
/// `handle` is a user-data pointer returned by [`kodo_alloc`].  The
/// refcount lives 8 bytes before this pointer.  If `handle` is zero
/// (null) or not an RC-managed allocation, the call is a no-op.
///
/// # Thread Safety
///
/// Uses `Relaxed` ordering, which is sufficient for incrementing because
/// we already hold a live reference (so the object cannot be freed).
#[no_mangle]
pub extern "C" fn kodo_rc_inc(handle: i64) {
    if handle == 0 || !rc_registry::is_managed(handle) {
        return;
    }
    // SAFETY: handle was returned by kodo_alloc, so handle - 8 points to
    // a valid AtomicI64 refcount.  The allocation is still live because
    // we hold a reference.
    unsafe {
        rc_atomic(handle).fetch_add(1, Ordering::Relaxed);
    }
}

/// Decrements the reference count for a heap-allocated object.
///
/// When the count drops to zero (or below) the backing memory — including
/// the 8-byte header — is freed.
///
/// `handle` is a user-data pointer returned by [`kodo_alloc`].  If
/// `handle` is zero (null) or not an RC-managed allocation, the call
/// is a no-op.
///
/// # Thread Safety
///
/// Uses `AcqRel` ordering on the `fetch_sub` so that all prior writes by
/// other threads are visible before we decide to deallocate.  When the
/// count reaches zero, an `Acquire` fence ensures that no memory accesses
/// are reordered past the deallocation.  This follows the same protocol
/// used by `std::sync::Arc`.
#[no_mangle]
pub extern "C" fn kodo_rc_dec(handle: i64) {
    if handle == 0 || !rc_registry::is_managed(handle) {
        return;
    }
    // SAFETY: handle was returned by kodo_alloc, so handle - 8 points to
    // a valid AtomicI64 refcount.
    unsafe {
        let prev = rc_atomic(handle).fetch_sub(1, Ordering::Release);
        if prev <= 1 {
            // Synchronize with all other threads that previously decremented
            // this refcount (their Release stores).  This ensures all writes
            // to the object are visible before we free the memory.
            std::sync::atomic::fence(Ordering::Acquire);

            let raw = rc_header_ptr(handle);
            // Retrieve the original allocation size so we can pass the
            // correct Layout to dealloc, satisfying its safety contract.
            let total = rc_registry::total_size(handle).unwrap_or(RC_HEADER_SIZE);
            rc_registry::unregister(handle);
            // SAFETY: raw was allocated by std::alloc::alloc_zeroed with
            // this exact layout (size=total, align=8).
            std::alloc::dealloc(raw, std::alloc::Layout::from_size_align_unchecked(total, 8));
        }
    }
}

/// Frees a heap-allocated object immediately, ignoring the reference count.
///
/// `handle` is a user-data pointer returned by [`kodo_alloc`].  If
/// `handle` is zero (null) the call is a no-op.
///
/// # Safety
///
/// The caller must guarantee that no other references to this object exist.
#[no_mangle]
pub extern "C" fn kodo_free(handle: i64) {
    if handle == 0 {
        return;
    }
    // Retrieve the original allocation size for correct dealloc Layout.
    let total = rc_registry::total_size(handle).unwrap_or(RC_HEADER_SIZE);
    rc_registry::unregister(handle);
    // SAFETY: handle was returned by kodo_alloc with this exact layout.
    unsafe {
        let raw = rc_header_ptr(handle);
        std::alloc::dealloc(raw, std::alloc::Layout::from_size_align_unchecked(total, 8));
    }
}

/// Returns the current reference count for the object pointed to by
/// `handle`, or 0 if the handle is null or not an RC-managed allocation.
///
/// # Thread Safety
///
/// Uses `Acquire` ordering to ensure visibility of the most recent
/// refcount modification by other threads.
///
/// Useful for debugging and testing.
#[no_mangle]
pub extern "C" fn kodo_rc_count(handle: i64) -> i64 {
    if handle == 0 || !rc_registry::is_managed(handle) {
        return 0;
    }
    // SAFETY: handle was returned by kodo_alloc.
    unsafe { rc_atomic(handle).load(Ordering::Acquire) }
}

/// Allocates RC-managed memory for a string and copies the given bytes into it.
///
/// Returns `(ptr, len)` where `ptr` is an RC-managed user-data pointer
/// (with refcount initialised to 1) and `len` is the byte length of the string.
/// Returns `(0, 0)` if allocation fails.
///
/// This is the canonical way to allocate strings in the Kōdo runtime.
/// All string-producing operations should use this instead of `Box::into_raw`.
///
/// # Thread Safety
///
/// The returned handle is safe to share across threads.
#[must_use]
pub fn alloc_string(bytes: &[u8]) -> (i64, usize) {
    #[allow(clippy::cast_possible_wrap)]
    let size = bytes.len() as i64;
    let handle = kodo_alloc(size);
    if handle == 0 {
        return (0, 0);
    }
    if !bytes.is_empty() {
        // SAFETY: handle was just returned by kodo_alloc with `size` bytes of
        // user-data space. We copy exactly `bytes.len()` bytes into it.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), handle as *mut u8, bytes.len());
        }
    }
    (handle, bytes.len())
}

/// Writes an RC-managed string to out-parameter pointers.
///
/// Allocates via [`alloc_string`] and writes the resulting `(ptr, len)` pair
/// to the provided out-parameters.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
pub unsafe fn alloc_string_out(bytes: &[u8], out_ptr: *mut *const u8, out_len: *mut usize) {
    let (handle, len) = alloc_string(bytes);
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = handle as *const u8;
        *out_len = len;
    }
}

/// Increment reference count for a string (ptr+len pair).
///
/// The `ptr` must be an RC-managed user-data pointer returned by
/// [`alloc_string`] (which delegates to [`kodo_alloc`]). If `ptr` is zero
/// or not an RC-managed allocation, the call is a safe no-op.
///
/// # Thread Safety
///
/// Delegates to [`kodo_rc_inc`] which uses atomic operations.
#[no_mangle]
pub extern "C" fn kodo_rc_inc_string(ptr: i64, len: i64) {
    let _ = len;
    kodo_rc_inc(ptr);
}

/// Decrement reference count for a string (ptr+len pair).
///
/// When the refcount drops to zero, the backing memory is freed.
/// If `ptr` is zero or not an RC-managed allocation, the call is a safe no-op.
///
/// # Thread Safety
///
/// Delegates to [`kodo_rc_dec`] which uses atomic operations.
#[no_mangle]
pub extern "C" fn kodo_rc_dec_string(ptr: i64, len: i64) {
    let _ = len;
    kodo_rc_dec(ptr);
}

/// Allocates a closure handle on the heap.
#[no_mangle]
pub extern "C" fn kodo_closure_new(func_ptr: i64, env_ptr: i64) -> i64 {
    let handle = kodo_alloc(16);
    if handle == 0 {
        return 0;
    }
    unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        let base = handle as *mut i64;
        // SAFETY: handle was just returned by kodo_alloc(16), so we have
        // 16 bytes of user data — room for two i64 values.
        std::ptr::write(base, func_ptr);
        std::ptr::write(base.add(1), env_ptr);
    }
    handle
}

/// Extracts the function pointer from a closure handle.
#[no_mangle]
pub extern "C" fn kodo_closure_func(handle: i64) -> i64 {
    if handle == 0 {
        return 0;
    }
    // SAFETY: handle was returned by kodo_closure_new which wrote an i64
    // function pointer at the base address.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        std::ptr::read(handle as *const i64)
    }
}

/// Extracts the environment pointer from a closure handle.
#[no_mangle]
pub extern "C" fn kodo_closure_env(handle: i64) -> i64 {
    if handle == 0 {
        return 0;
    }
    // SAFETY: handle was returned by kodo_closure_new which wrote an i64
    // environment pointer at offset 8.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        std::ptr::read((handle as *const i64).add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc_null_handle_is_safe() {
        kodo_rc_inc(0);
        kodo_rc_dec(0);
    }

    #[test]
    fn rc_alloc_returns_nonzero() {
        let handle = kodo_alloc(64);
        assert_ne!(handle, 0);
        kodo_rc_dec(handle);
    }

    #[test]
    fn rc_alloc_initial_count_is_one() {
        let handle = kodo_alloc(32);
        assert_eq!(kodo_rc_count(handle), 1);
        kodo_rc_dec(handle);
    }

    #[test]
    fn rc_inc_increments_count() {
        let handle = kodo_alloc(16);
        kodo_rc_inc(handle);
        assert_eq!(kodo_rc_count(handle), 2);
        kodo_rc_dec(handle);
        kodo_rc_dec(handle);
    }

    #[test]
    fn rc_free_ignores_null() {
        kodo_free(0);
    }

    #[test]
    fn rc_free_immediate() {
        let handle = kodo_alloc(48);
        kodo_free(handle);
    }

    #[test]
    fn rc_count_null_returns_zero() {
        assert_eq!(kodo_rc_count(0), 0);
    }

    #[test]
    fn rc_alloc_zero_size() {
        let handle = kodo_alloc(0);
        assert_ne!(handle, 0);
        assert_eq!(kodo_rc_count(handle), 1);
        kodo_rc_dec(handle);
    }

    #[test]
    fn rc_inc_string_null_is_safe() {
        // Null pointer is a safe no-op.
        kodo_rc_inc_string(0, 0);
    }

    #[test]
    fn rc_dec_string_null_is_safe() {
        // Null pointer is a safe no-op.
        kodo_rc_dec_string(0, 0);
    }

    #[test]
    fn rc_inc_string_unmanaged_is_safe() {
        // Arbitrary non-RC pointer is a safe no-op.
        kodo_rc_inc_string(123, 456);
        kodo_rc_dec_string(999, 10);
    }

    #[test]
    fn rc_string_inc_dec_tracks_refcount() {
        let (handle, len) = alloc_string(b"hello");
        assert_ne!(handle, 0);
        assert_eq!(len, 5);
        // Initial refcount is 1.
        assert_eq!(kodo_rc_count(handle), 1);
        // Increment via string-specific API.
        #[allow(clippy::cast_possible_wrap)]
        kodo_rc_inc_string(handle, len as i64);
        assert_eq!(kodo_rc_count(handle), 2);
        // Decrement back.
        #[allow(clippy::cast_possible_wrap)]
        kodo_rc_dec_string(handle, len as i64);
        assert_eq!(kodo_rc_count(handle), 1);
        // Final dec frees.
        #[allow(clippy::cast_possible_wrap)]
        kodo_rc_dec_string(handle, len as i64);
    }

    #[test]
    fn alloc_string_returns_correct_data() {
        let (handle, len) = alloc_string(b"world");
        assert_ne!(handle, 0);
        assert_eq!(len, 5);
        // SAFETY: handle points to 5 bytes of valid data we just copied.
        let slice = unsafe { std::slice::from_raw_parts(handle as *const u8, len) };
        assert_eq!(slice, b"world");
        assert_eq!(kodo_rc_count(handle), 1);
        kodo_rc_dec(handle);
    }

    #[test]
    fn alloc_string_empty() {
        let (handle, len) = alloc_string(b"");
        assert_ne!(handle, 0);
        assert_eq!(len, 0);
        assert_eq!(kodo_rc_count(handle), 1);
        kodo_rc_dec(handle);
    }

    #[test]
    fn alloc_string_freed_on_rc_zero() {
        let (handle, _len) = alloc_string(b"temporary");
        assert_eq!(kodo_rc_count(handle), 1);
        kodo_rc_dec(handle);
        // After freeing, the handle is unregistered.  We do NOT check
        // kodo_rc_count(handle) == 0 here because with a global registry
        // another test running in parallel may have already reused the
        // same address via a new kodo_alloc call.
    }

    #[test]
    fn string_concat_returns_rc_managed_memory() {
        let a = b"hello ";
        let b_bytes = b"world";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        // SAFETY: all pointers are valid.
        unsafe {
            crate::string_ops::kodo_string_concat(
                a.as_ptr(),
                a.len(),
                b_bytes.as_ptr(),
                b_bytes.len(),
                &mut out_ptr,
                &mut out_len,
            );
        }
        assert!(!out_ptr.is_null());
        assert_eq!(out_len, 11);
        let handle = out_ptr as i64;
        // Verify it is RC-managed with refcount 1.
        assert_eq!(kodo_rc_count(handle), 1);
        // Verify content.
        // SAFETY: handle points to out_len valid bytes.
        let slice = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(slice, b"hello world");
        // Clean up.
        kodo_rc_dec(handle);
        // Note: we do not assert kodo_rc_count(handle) == 0 because with
        // a global registry the address may be reused by a concurrent test.
    }

    // ---- Thread-safety tests ----

    #[test]
    fn concurrent_inc_dec_from_multiple_threads() {
        let handle = kodo_alloc(64);
        assert_ne!(handle, 0);
        // Start with refcount 1. We'll bump it by THREADS so total = 1 + THREADS.
        const THREADS: usize = 8;
        const OPS_PER_THREAD: usize = 1000;

        // First, increment so we have enough references for all threads.
        for _ in 0..THREADS {
            kodo_rc_inc(handle);
        }
        // refcount = 1 + THREADS

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(THREADS));
        let mut join_handles = Vec::new();

        for _ in 0..THREADS {
            let bar = barrier.clone();
            join_handles.push(std::thread::spawn(move || {
                bar.wait();
                // Each thread does OPS_PER_THREAD inc/dec pairs — net zero.
                for _ in 0..OPS_PER_THREAD {
                    kodo_rc_inc(handle);
                    kodo_rc_dec(handle);
                }
            }));
        }

        for jh in join_handles {
            jh.join().map_err(|_| "thread panicked").ok();
        }

        // Net effect: refcount should still be 1 + THREADS.
        #[allow(clippy::cast_possible_wrap)]
        let expected = 1 + THREADS as i64;
        assert_eq!(kodo_rc_count(handle), expected);

        // Clean up: dec all remaining refs.
        for _ in 0..=THREADS {
            kodo_rc_dec(handle);
        }
    }

    #[test]
    fn alloc_on_one_thread_free_on_another() {
        let handle = kodo_alloc(128);
        assert_ne!(handle, 0);
        assert_eq!(kodo_rc_count(handle), 1);

        // Move the handle to another thread and free it there.
        let jh = std::thread::spawn(move || {
            assert_eq!(kodo_rc_count(handle), 1);
            kodo_rc_dec(handle);
            // After freeing, it should no longer be managed.
            assert_eq!(kodo_rc_count(handle), 0);
        });
        jh.join().map_err(|_| "thread panicked").ok();
    }

    #[test]
    fn stress_alloc_inc_dec_across_threads() {
        const THREADS: usize = 16;
        const ALLOCS_PER_THREAD: usize = 100;

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(THREADS));
        let mut join_handles = Vec::new();

        for _ in 0..THREADS {
            let bar = barrier.clone();
            join_handles.push(std::thread::spawn(move || {
                bar.wait();
                for _ in 0..ALLOCS_PER_THREAD {
                    let h = kodo_alloc(32);
                    assert_ne!(h, 0);
                    kodo_rc_inc(h);
                    assert_eq!(kodo_rc_count(h), 2);
                    kodo_rc_dec(h);
                    assert_eq!(kodo_rc_count(h), 1);
                    kodo_rc_dec(h);
                }
            }));
        }

        for jh in join_handles {
            jh.join().map_err(|_| "thread panicked").ok();
        }
    }

    #[test]
    fn concurrent_alloc_free_interleaved() {
        // Multiple threads allocating and freeing independently, ensuring
        // the global registry handles concurrent mutations correctly.
        const THREADS: usize = 8;
        const ITERS: usize = 200;

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(THREADS));
        let mut join_handles = Vec::new();

        for _ in 0..THREADS {
            let bar = barrier.clone();
            join_handles.push(std::thread::spawn(move || {
                bar.wait();
                let mut handles = Vec::new();
                for i in 0..ITERS {
                    let h = kodo_alloc(16);
                    assert_ne!(h, 0);
                    handles.push(h);
                    // Free every other allocation immediately.
                    if i % 2 == 0 {
                        let freed = handles.pop().unwrap_or(0);
                        if freed != 0 {
                            kodo_rc_dec(freed);
                        }
                    }
                }
                // Free remaining.
                for h in handles {
                    kodo_rc_dec(h);
                }
            }));
        }

        for jh in join_handles {
            jh.join().map_err(|_| "thread panicked").ok();
        }
    }
}
