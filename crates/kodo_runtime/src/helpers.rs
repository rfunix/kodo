//! Internal helper functions shared across runtime modules.
//!
//! Provides utilities for writing strings to out-parameter pointers,
//! used by I/O, HTTP, and JSON builtins.

/// Writes a string to out-parameter pointers using RC-managed memory.
///
/// Allocates via [`crate::memory::alloc_string`] so the resulting string is
/// reference-counted and properly freed when its refcount drops to zero.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
pub(crate) unsafe fn write_string_out(s: &str, out_ptr: *mut *const u8, out_len: *mut usize) {
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(s.as_bytes(), out_ptr, out_len);
    }
}

/// Writes a string to mutable out-parameter pointers using RC-managed memory.
///
/// Similar to [`write_string_out`] but uses `*mut *mut u8` for compatibility
/// with builtins that return freeable strings.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
pub(crate) unsafe fn write_string_out_mut(s: &str, out_ptr: *mut *mut u8, out_len: *mut usize) {
    let (handle, len) = crate::memory::alloc_string(s.as_bytes());
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = handle as *mut u8;
        *out_len = len;
    }
}
