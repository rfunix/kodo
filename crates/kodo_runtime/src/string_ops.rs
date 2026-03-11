//! String manipulation builtins for the Kōdo runtime.
//!
//! Provides FFI-callable functions for string operations such as
//! `contains`, `starts_with`, `ends_with`, `trim`, `to_upper`, `to_lower`,
//! `substring`, `concat`, `index_of`, `replace`, `split`, equality, and free.

use std::io::Write;

/// Returns the length of a string (number of bytes).
///
/// # Safety
///
/// `ptr` must point to `len` valid bytes.
#[no_mangle]
pub extern "C" fn kodo_string_length(_ptr: *const u8, len: usize) -> i64 {
    #[allow(clippy::cast_possible_wrap)]
    let result = len as i64;
    result
}

/// Returns 1 if the haystack string contains the needle string, 0 otherwise.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_string_contains(
    hay_ptr: *const u8,
    hay_len: usize,
    needle_ptr: *const u8,
    needle_len: usize,
) -> i64 {
    // SAFETY: Caller guarantees both pointer/length pairs are valid byte slices.
    let haystack = unsafe { std::slice::from_raw_parts(hay_ptr, hay_len) };
    let needle = unsafe { std::slice::from_raw_parts(needle_ptr, needle_len) };
    // Byte-level substring search — no UTF-8 decoding needed.
    i64::from(contains_bytes(haystack, needle))
}

/// Returns 1 if the string starts with the given prefix, 0 otherwise.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_string_starts_with(
    hay_ptr: *const u8,
    hay_len: usize,
    prefix_ptr: *const u8,
    prefix_len: usize,
) -> i64 {
    // SAFETY: Caller guarantees both pointer/length pairs are valid byte slices.
    let haystack = unsafe { std::slice::from_raw_parts(hay_ptr, hay_len) };
    let prefix = unsafe { std::slice::from_raw_parts(prefix_ptr, prefix_len) };
    i64::from(haystack.starts_with(prefix))
}

/// Returns 1 if the string ends with the given suffix, 0 otherwise.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_string_ends_with(
    hay_ptr: *const u8,
    hay_len: usize,
    suffix_ptr: *const u8,
    suffix_len: usize,
) -> i64 {
    // SAFETY: Caller guarantees both pointer/length pairs are valid byte slices.
    let haystack = unsafe { std::slice::from_raw_parts(hay_ptr, hay_len) };
    let suffix = unsafe { std::slice::from_raw_parts(suffix_ptr, suffix_len) };
    i64::from(haystack.ends_with(suffix))
}

/// Returns 1 if two strings are equal (same length and bytes), 0 otherwise.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_string_eq(
    ptr1: *const u8,
    len1: usize,
    ptr2: *const u8,
    len2: usize,
) -> i64 {
    if len1 != len2 {
        return 0;
    }
    // SAFETY: Caller guarantees both pointer/length pairs are valid byte slices.
    let s1 = unsafe { std::slice::from_raw_parts(ptr1, len1) };
    let s2 = unsafe { std::slice::from_raw_parts(ptr2, len2) };
    i64::from(s1 == s2)
}

/// Concatenates two strings, writing the result via out-parameters.
///
/// The caller is responsible for eventually freeing the allocated memory.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_concat(
    ptr1: *const u8,
    len1: usize,
    ptr2: *const u8,
    len2: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    // SAFETY: Caller guarantees both pointer/length pairs are valid byte slices.
    let s1 = unsafe { std::slice::from_raw_parts(ptr1, len1) };
    let s2 = unsafe { std::slice::from_raw_parts(ptr2, len2) };
    let mut result = Vec::with_capacity(len1 + len2);
    result.extend_from_slice(s1);
    result.extend_from_slice(s2);
    let boxed = result.into_boxed_slice();
    let result_len = boxed.len();
    // SAFETY: Box::into_raw intentionally leaks the allocation so the caller
    // can manage the memory via (ptr, len). Freed by `kodo_string_free`.
    let result_ptr = Box::into_raw(boxed) as *const u8;
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = result_ptr;
        *out_len = result_len;
    }
}

/// Returns the byte index of the first occurrence of needle in haystack, or -1 if not found.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_string_index_of(
    hay_ptr: *const u8,
    hay_len: usize,
    needle_ptr: *const u8,
    needle_len: usize,
) -> i64 {
    // SAFETY: Caller guarantees both pointer/length pairs are valid byte slices.
    let haystack = unsafe { std::slice::from_raw_parts(hay_ptr, hay_len) };
    let needle = unsafe { std::slice::from_raw_parts(needle_ptr, needle_len) };
    if needle.is_empty() {
        return 0;
    }
    if needle_len > hay_len {
        return -1;
    }
    for i in 0..=(hay_len - needle_len) {
        if haystack[i..i + needle_len] == *needle {
            #[allow(clippy::cast_possible_wrap)]
            return i as i64;
        }
    }
    -1
}

/// Replaces all occurrences of a pattern in a string, writing the result via out-parameters.
///
/// The caller is responsible for eventually freeing the allocated memory.
///
/// # Safety
///
/// All pointer/length pairs must point to valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_string_replace(
    hay_ptr: *const u8,
    hay_len: usize,
    pattern_ptr: *const u8,
    pattern_len: usize,
    replacement_ptr: *const u8,
    replacement_len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    // SAFETY: Caller guarantees all pointer/length pairs are valid byte slices.
    let haystack = unsafe { std::slice::from_raw_parts(hay_ptr, hay_len) };
    let pattern = unsafe { std::slice::from_raw_parts(pattern_ptr, pattern_len) };
    let replacement = unsafe { std::slice::from_raw_parts(replacement_ptr, replacement_len) };

    if pattern.is_empty() {
        // Empty pattern: return the original string (no replacement)
        let copy = haystack.to_vec().into_boxed_slice();
        let result_len = copy.len();
        // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
        let result_ptr = Box::into_raw(copy) as *const u8;
        // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe {
            *out_ptr = result_ptr;
            *out_len = result_len;
        }
        return;
    }

    let mut result = Vec::with_capacity(hay_len);
    let mut i = 0;
    while i < hay_len {
        if i + pattern_len <= hay_len && haystack[i..i + pattern_len] == *pattern {
            result.extend_from_slice(replacement);
            i += pattern_len;
        } else {
            result.push(haystack[i]);
            i += 1;
        }
    }
    let boxed = result.into_boxed_slice();
    let result_len = boxed.len();
    // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
    let result_ptr = Box::into_raw(boxed) as *const u8;
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = result_ptr;
        *out_len = result_len;
    }
}

/// Byte-level substring search.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Returns a trimmed copy of the string (whitespace removed from both ends).
///
/// The result pointer and length are written to `out_ptr` and `out_len`.
/// The trimmed string is a sub-slice of the original, so no allocation is needed.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_trim(
    ptr: *const u8,
    len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    // SAFETY: Caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    // Find the trimmed sub-slice at the byte level.
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(len);
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |p| p + 1);
    // SAFETY: start <= end <= len, so the sub-slice is valid.
    // Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = bytes.as_ptr().add(start);
        *out_len = end - start;
    }
}

/// Returns an uppercase copy of the string.
///
/// Allocates a new string on the heap. Caller does not need to free
/// (managed by the Kōdo runtime's arena).
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_to_upper(
    ptr: *const u8,
    len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    // SAFETY: Caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    // ASCII-level uppercase conversion (safe for any byte sequence).
    let mut result = Vec::with_capacity(len);
    for &b in bytes {
        result.push(b.to_ascii_uppercase());
    }
    let boxed = result.into_boxed_slice();
    let result_len = boxed.len();
    // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
    let result_ptr = Box::into_raw(boxed) as *const u8;
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = result_ptr;
        *out_len = result_len;
    }
}

/// Returns a lowercase copy of the string.
///
/// Allocates a new string on the heap.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_to_lower(
    ptr: *const u8,
    len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    // SAFETY: Caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    // ASCII-level lowercase conversion (safe for any byte sequence).
    let mut result = Vec::with_capacity(len);
    for &b in bytes {
        result.push(b.to_ascii_lowercase());
    }
    let boxed = result.into_boxed_slice();
    let result_len = boxed.len();
    // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
    let result_ptr = Box::into_raw(boxed) as *const u8;
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = result_ptr;
        *out_len = result_len;
    }
}

/// Returns a substring from `start` to `end` byte indices.
///
/// Clamps indices to valid range. The result is a sub-slice (no allocation).
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_substring(
    ptr: *const u8,
    len: usize,
    start: i64,
    end: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    // SAFETY: Caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let start_idx = (start.max(0) as usize).min(len);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let end_idx = (end.max(0) as usize).min(len);
    let actual_end = end_idx.max(start_idx);
    // SAFETY: start_idx <= actual_end <= len, so the sub-slice is valid.
    // Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = bytes.as_ptr().add(start_idx);
        *out_len = actual_end - start_idx;
    }
}

/// Converts an integer to its decimal string representation.
///
/// Allocates a new string on the heap.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_int_to_string(
    value: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    let s = value.to_string();
    let boxed = s.into_boxed_str();
    let result_len = boxed.len();
    // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
    let result_ptr = Box::into_raw(boxed) as *const u8;
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = result_ptr;
        *out_len = result_len;
    }
}

/// Converts an integer to a 64-bit float.
#[no_mangle]
pub extern "C" fn kodo_int_to_float64(value: i64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let result = value as f64;
    result
}

/// Converts a 64-bit float to its string representation.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_float64_to_string(
    value: f64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    let s = value.to_string();
    let boxed = s.into_boxed_str();
    let result_len = boxed.len();
    // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
    let result_ptr = Box::into_raw(boxed) as *const u8;
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = result_ptr;
        *out_len = result_len;
    }
}

/// Converts a 64-bit float to an integer (truncates toward zero).
#[no_mangle]
pub extern "C" fn kodo_float64_to_int(value: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let result = value as i64;
    result
}

/// Frees a heap-allocated string previously returned by runtime functions
/// (e.g. `kodo_string_concat`, `kodo_string_replace`, `kodo_int_to_string`).
///
/// Does nothing if `ptr` is null or `len` is zero.
///
/// # Safety
///
/// `ptr` must have been allocated by `Box::into_raw` on a `Box<[u8]>` of
/// exactly `len` bytes, or be null.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    // SAFETY: caller guarantees ptr was allocated via Box::into_raw on a
    // Box<[u8]> of exactly `len` bytes.
    let _ = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) };
}

/// Splits a string by a separator and returns a `List<String>`.
///
/// Each resulting substring is allocated as a new (ptr, len) pair on the heap.
/// The list contains pointers to these string values.
///
/// The returned list stores each string as two consecutive i64 values (ptr, len),
/// but since our list holds single i64 elements, we actually return a list of
/// string pointers. Each string "pointer" is a pointer to a heap-allocated
/// (ptr: *const u8, len: usize) pair.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_string_split(
    hay_ptr: *const u8,
    hay_len: usize,
    sep_ptr: *const u8,
    sep_len: usize,
) -> i64 {
    // SAFETY: Caller guarantees both pointer/length pairs are valid byte slices.
    let haystack = unsafe { std::slice::from_raw_parts(hay_ptr, hay_len) };
    let separator = unsafe { std::slice::from_raw_parts(sep_ptr, sep_len) };

    let list = crate::collections::kodo_list_new();

    if separator.is_empty() {
        // Empty separator: return the whole string as a single element.
        #[allow(clippy::cast_possible_wrap)]
        let pair = Box::new([hay_ptr as i64, hay_len as i64]);
        // SAFETY: intentionally leaks so the pair can be stored in the list.
        let pair_ptr = Box::into_raw(pair) as i64;
        // SAFETY: list is valid, just created above.
        unsafe { crate::collections::kodo_list_push(list, pair_ptr) };
        return list;
    }

    let mut start = 0;
    while start <= haystack.len() {
        // Find next occurrence of separator.
        let remaining = &haystack[start..];
        let found = remaining
            .windows(separator.len())
            .position(|w| w == separator);

        if let Some(pos) = found {
            // Allocate a (ptr, len) pair for this substring.
            // SAFETY: start + pos <= hay_len, so the pointer is within bounds.
            let sub_ptr = unsafe { hay_ptr.add(start) };
            let sub_len = pos;
            #[allow(clippy::cast_possible_wrap)]
            let pair = Box::new([sub_ptr as i64, sub_len as i64]);
            // SAFETY: intentionally leaks so the pair can be stored in the list.
            let pair_ptr = Box::into_raw(pair) as i64;
            // SAFETY: list is valid.
            unsafe { crate::collections::kodo_list_push(list, pair_ptr) };
            start += pos + separator.len();
        } else {
            // Last segment.
            // SAFETY: start <= hay_len, so the pointer is within bounds.
            let sub_ptr = unsafe { hay_ptr.add(start) };
            let sub_len = haystack.len() - start;
            #[allow(clippy::cast_possible_wrap)]
            let pair = Box::new([sub_ptr as i64, sub_len as i64]);
            // SAFETY: intentionally leaks so the pair can be stored in the list.
            let pair_ptr = Box::into_raw(pair) as i64;
            // SAFETY: list is valid.
            unsafe { crate::collections::kodo_list_push(list, pair_ptr) };
            break;
        }
    }

    list
}

/// Prints a string followed by a newline to stdout.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_println(ptr: *const u8, len: usize) {
    // SAFETY: Caller guarantees ptr/len form a valid UTF-8 slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(bytes);
    let _ = handle.write_all(b"\n");
    let _ = handle.flush();
}

/// Prints a string to stdout without a trailing newline.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_print(ptr: *const u8, len: usize) {
    // SAFETY: Caller guarantees ptr/len form a valid UTF-8 slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(bytes);
    let _ = handle.flush();
}

/// Prints an integer to stdout followed by a newline.
#[no_mangle]
pub extern "C" fn kodo_print_int(n: i64) {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = write!(handle, "{n}");
    let _ = handle.write_all(b"\n");
    let _ = handle.flush();
}

/// Prints a float to stdout without a newline.
#[no_mangle]
pub extern "C" fn kodo_print_float(value: f64) {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = write!(handle, "{value}");
    let _ = handle.flush();
}

/// Prints a float to stdout followed by a newline.
#[no_mangle]
pub extern "C" fn kodo_println_float(value: f64) {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = write!(handle, "{value}");
    let _ = handle.write_all(b"\n");
    let _ = handle.flush();
}

/// Called when a contract (`requires`/`ensures`) check fails at runtime.
///
/// Prints an error message to stderr and aborts the process.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_contract_fail(ptr: *const u8, len: usize) {
    // SAFETY: Caller guarantees ptr/len form a valid UTF-8 slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let msg = std::str::from_utf8(bytes).unwrap_or("<invalid utf-8>");
    eprintln!("contract violation: {msg}");
    std::process::abort();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_length_works() {
        let s = "hello";
        assert_eq!(kodo_string_length(s.as_ptr(), s.len()), 5);
    }

    #[test]
    fn string_contains_works() {
        let hay = "hello world";
        let needle = "world";
        let result =
            unsafe { kodo_string_contains(hay.as_ptr(), hay.len(), needle.as_ptr(), needle.len()) };
        assert_eq!(result, 1);
        let missing = "xyz";
        let result = unsafe {
            kodo_string_contains(hay.as_ptr(), hay.len(), missing.as_ptr(), missing.len())
        };
        assert_eq!(result, 0);
    }

    #[test]
    fn string_starts_with_works() {
        let s = "hello world";
        let prefix = "hello";
        let result =
            unsafe { kodo_string_starts_with(s.as_ptr(), s.len(), prefix.as_ptr(), prefix.len()) };
        assert_eq!(result, 1);
        let bad = "world";
        let result =
            unsafe { kodo_string_starts_with(s.as_ptr(), s.len(), bad.as_ptr(), bad.len()) };
        assert_eq!(result, 0);
    }

    #[test]
    fn string_ends_with_works() {
        let s = "hello world";
        let suffix = "world";
        let result =
            unsafe { kodo_string_ends_with(s.as_ptr(), s.len(), suffix.as_ptr(), suffix.len()) };
        assert_eq!(result, 1);
    }

    #[test]
    fn string_eq_equal() {
        let a = "hello";
        let b = "hello";
        let result = unsafe { kodo_string_eq(a.as_ptr(), a.len(), b.as_ptr(), b.len()) };
        assert_eq!(result, 1);
    }

    #[test]
    fn string_eq_different() {
        let a = "hello";
        let b = "world";
        let result = unsafe { kodo_string_eq(a.as_ptr(), a.len(), b.as_ptr(), b.len()) };
        assert_eq!(result, 0);
    }

    #[test]
    fn string_trim_works() {
        let s = "  hello  ";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_string_trim(s.as_ptr(), s.len(), &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "hello");
    }

    #[test]
    fn string_to_upper_works() {
        let s = "hello";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_string_to_upper(s.as_ptr(), s.len(), &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "HELLO");
    }

    #[test]
    fn string_concat_works() {
        let a = "hello ";
        let b = "world";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe {
            kodo_string_concat(
                a.as_ptr(),
                a.len(),
                b.as_ptr(),
                b.len(),
                &mut out_ptr,
                &mut out_len,
            );
        }
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "hello world");
    }

    #[test]
    fn string_index_of_works() {
        let hay = "hello world";
        let needle = "world";
        let result =
            unsafe { kodo_string_index_of(hay.as_ptr(), hay.len(), needle.as_ptr(), needle.len()) };
        assert_eq!(result, 6);
    }

    #[test]
    fn string_replace_works() {
        let hay = "hello world";
        let pattern = "world";
        let replacement = "kodo";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe {
            kodo_string_replace(
                hay.as_ptr(),
                hay.len(),
                pattern.as_ptr(),
                pattern.len(),
                replacement.as_ptr(),
                replacement.len(),
                &mut out_ptr,
                &mut out_len,
            );
        }
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "hello kodo");
    }

    #[test]
    fn int_to_string_works() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_int_to_string(42, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "42");
    }

    #[test]
    fn int_to_float64_works() {
        assert!((kodo_int_to_float64(42) - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn float64_to_int_works() {
        assert_eq!(kodo_float64_to_int(3.7), 3);
        assert_eq!(kodo_float64_to_int(-2.9), -2);
    }

    #[test]
    fn string_free_null_does_not_crash() {
        unsafe { kodo_string_free(std::ptr::null_mut(), 0) };
        unsafe { kodo_string_free(std::ptr::null_mut(), 42) };
    }
}
