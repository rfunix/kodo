//! Reference-counted memory allocator for the Kōdo runtime.
//!
//! Memory layout: `[refcount: 8 bytes (i64)][user data: size bytes]`
//!
//! [`kodo_alloc`] returns a pointer to the user data region (past the header).
//! [`kodo_rc_inc`] / [`kodo_rc_dec`] manipulate the refcount stored 8 bytes
//! before the user pointer.  When the count drops to zero, the entire
//! allocation (header + data) is freed.
//!
//! For now all operations are non-atomic (single-threaded runtime).

/// Number of bytes reserved for the reference count header.
const RC_HEADER_SIZE: usize = 8;

/// Thread-local set of handles returned by [`kodo_alloc`], used to
/// distinguish RC-managed allocations from legacy `Box::into_raw`
/// strings.  Only handles present in this set are manipulated by
/// [`kodo_rc_inc`] and [`kodo_rc_dec`]; all other values are treated
/// as a safe no-op.
///
/// This avoids crashing when the MIR emits `IncRef`/`DecRef` for
/// string locals whose backing memory was not allocated by
/// [`kodo_alloc`] (e.g. `kodo_string_concat` uses `Box::into_raw`).
mod rc_registry {
    use std::cell::RefCell;
    use std::collections::HashSet;

    thread_local! {
        static HANDLES: RefCell<HashSet<i64>> = RefCell::new(HashSet::new());
    }

    /// Registers a handle as RC-managed.
    pub(super) fn register(handle: i64) {
        HANDLES.with(|set| {
            set.borrow_mut().insert(handle);
        });
    }

    /// Unregisters a handle (called when the allocation is freed).
    pub(super) fn unregister(handle: i64) {
        HANDLES.with(|set| {
            set.borrow_mut().remove(&handle);
        });
    }

    /// Returns `true` if the handle was allocated by [`super::kodo_alloc`].
    pub(super) fn is_managed(handle: i64) -> bool {
        HANDLES.with(|set| set.borrow().contains(&handle))
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

/// Allocates `size` bytes of heap memory with an embedded reference count
/// header initialised to 1.
///
/// Returns a pointer to the *user data* region (8 bytes past the real
/// allocation start).  The caller sees only the data pointer; the refcount
/// is managed transparently by [`kodo_rc_inc`] and [`kodo_rc_dec`].
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
    // Write initial refcount = 1.
    // SAFETY: raw is aligned to 8 and points to at least RC_HEADER_SIZE bytes.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        std::ptr::write(raw.cast::<i64>(), 1);
    }
    // Return pointer past the header.
    // SAFETY: raw + RC_HEADER_SIZE is within the allocation.
    #[allow(clippy::cast_possible_wrap)]
    let user_ptr = unsafe { raw.add(RC_HEADER_SIZE) } as i64;
    rc_registry::register(user_ptr);
    user_ptr
}

/// Increments the reference count for a heap-allocated object.
///
/// `handle` is a user-data pointer returned by [`kodo_alloc`].  The
/// refcount lives 8 bytes before this pointer.  If `handle` is zero
/// (null) or not an RC-managed allocation, the call is a no-op.
#[no_mangle]
pub extern "C" fn kodo_rc_inc(handle: i64) {
    if handle == 0 || !rc_registry::is_managed(handle) {
        return;
    }
    // SAFETY: handle was returned by kodo_alloc, so handle - 8 points to
    // a valid i64 refcount.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        let rc_ptr = rc_header_ptr(handle).cast::<i64>();
        *rc_ptr += 1;
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
#[no_mangle]
pub extern "C" fn kodo_rc_dec(handle: i64) {
    if handle == 0 || !rc_registry::is_managed(handle) {
        return;
    }
    // SAFETY: handle was returned by kodo_alloc, so handle - 8 points to
    // a valid i64 refcount.
    unsafe {
        let raw = rc_header_ptr(handle);
        #[allow(clippy::cast_ptr_alignment)]
        let rc_ptr = raw.cast::<i64>();
        *rc_ptr -= 1;
        if *rc_ptr <= 0 {
            rc_registry::unregister(handle);
            // SAFETY: raw was allocated by std::alloc::alloc_zeroed with
            // alignment 8.  The Rust global allocator (system allocator)
            // tracks allocation sizes internally, so passing a minimal
            // layout with the correct alignment is safe for dealloc.
            std::alloc::dealloc(raw, std::alloc::Layout::from_size_align_unchecked(1, 8));
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
    rc_registry::unregister(handle);
    // SAFETY: handle was returned by kodo_alloc.
    unsafe {
        let raw = rc_header_ptr(handle);
        std::alloc::dealloc(raw, std::alloc::Layout::from_size_align_unchecked(1, 8));
    }
}

/// Returns the current reference count for the object pointed to by
/// `handle`, or 0 if the handle is null or not an RC-managed allocation.
///
/// Useful for debugging and testing.
#[no_mangle]
pub extern "C" fn kodo_rc_count(handle: i64) -> i64 {
    if handle == 0 || !rc_registry::is_managed(handle) {
        return 0;
    }
    // SAFETY: handle was returned by kodo_alloc.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        let rc_ptr = rc_header_ptr(handle).cast::<i64>();
        *rc_ptr
    }
}

/// Increment reference count for a string (ptr+len pair).
///
/// Currently a no-op — strings allocated by the runtime (e.g. via
/// `kodo_string_concat`) use `Box::into_raw` and do not have an RC header.
/// String RC will be integrated when string allocation migrates to
/// [`kodo_alloc`].
///
/// **Alpha limitation**: Because these are no-ops, intermediate strings from
/// operations like concat/split/substring are not freed until the process exits.
/// This is acceptable for short-lived programs (CLIs, scripts) but can cause
/// memory growth in long-running services. The fix requires migrating all string
/// allocation paths (`kodo_string_concat`, `kodo_string_split`, etc.) to use
/// `kodo_alloc` with RC headers — a non-trivial refactor deferred to beta.
/// See `docs/KNOWN_LIMITATIONS.md` for user-facing documentation.
#[no_mangle]
pub extern "C" fn kodo_rc_inc_string(ptr: i64, len: i64) {
    let _ = (ptr, len);
}

/// Decrement reference count for a string (ptr+len pair).
///
/// Currently a no-op — see [`kodo_rc_inc_string`] for rationale.
#[no_mangle]
pub extern "C" fn kodo_rc_dec_string(ptr: i64, len: i64) {
    let _ = (ptr, len);
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
    fn rc_inc_string_noop() {
        kodo_rc_inc_string(0, 0);
        kodo_rc_inc_string(123, 456);
    }

    #[test]
    fn rc_dec_string_noop() {
        kodo_rc_dec_string(0, 0);
        kodo_rc_dec_string(123, 456);
    }
}
