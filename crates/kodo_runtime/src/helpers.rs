//! Internal helper functions shared across runtime modules.
//!
//! Provides utilities for writing strings to out-parameter pointers,
//! used by I/O, HTTP, and JSON builtins.

/// Writes a Rust `String` to out-parameter pointers.
///
/// Leaks the string's bytes as a heap allocation, setting `out_ptr` and
/// `out_len` to point to the data. The caller is responsible for freeing.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
pub(crate) unsafe fn write_string_out(s: String, out_ptr: *mut *const u8, out_len: *mut usize) {
    let bytes = s.into_bytes().into_boxed_slice();
    let len = bytes.len();
    // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
    let ptr = Box::into_raw(bytes) as *const u8;
    // SAFETY: caller guarantees these are valid writable pointers.
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
}

/// Writes a Rust `String` to mutable out-parameter pointers.
///
/// Similar to [`write_string_out`] but uses `*mut *mut u8` for compatibility
/// with builtins that return freeable strings.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
pub(crate) unsafe fn write_string_out_mut(s: String, out_ptr: *mut *mut u8, out_len: *mut usize) {
    let bytes = s.into_bytes().into_boxed_slice();
    let len = bytes.len();
    // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
    // Freed by `kodo_string_free`.
    let raw_slice = Box::into_raw(bytes);
    unsafe {
        // SAFETY: raw_slice is a valid fat pointer from Box::into_raw.
        let ptr = (*raw_slice).as_mut_ptr();
        // SAFETY: caller guarantees these are valid writable pointers.
        *out_ptr = ptr;
        *out_len = len;
    }
}
