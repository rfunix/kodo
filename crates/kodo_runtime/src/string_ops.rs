//! String manipulation builtins for the Kōdo runtime.
//!
//! Provides FFI-callable functions for string operations such as
//! `contains`, `starts_with`, `ends_with`, `trim`, `to_upper`, `to_lower`,
//! `substring`, `concat`, `index_of`, `replace`, `split`, equality, and free.

use std::io::Write;

/// Returns the length of a string in Unicode code points (characters).
///
/// For ASCII-only strings this equals the byte length. For strings with
/// multi-byte UTF-8 characters (accented letters, CJK, emoji) the result
/// is the number of characters, which may be less than the byte length.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_length(ptr: *const u8, len: usize) -> i64 {
    // SAFETY: Caller guarantees ptr/len form a valid UTF-8 byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let Ok(s) = std::str::from_utf8(bytes) else {
        // Fallback: if somehow invalid UTF-8, return byte count.
        #[allow(clippy::cast_possible_wrap)]
        return len as i64;
    };
    #[allow(clippy::cast_possible_wrap)]
    let result = s.chars().count() as i64;
    result
}

/// Returns the byte length of a string (number of UTF-8 bytes).
///
/// Unlike [`kodo_string_length`] which counts Unicode code points,
/// this function returns the raw byte count. Useful for low-level
/// operations that need to work with the underlying byte representation.
///
/// # Safety
///
/// `ptr` must point to `len` valid bytes.
#[no_mangle]
pub extern "C" fn kodo_string_byte_length(_ptr: *const u8, len: usize) -> i64 {
    #[allow(clippy::cast_possible_wrap)]
    let result = len as i64;
    result
}

/// Returns the number of Unicode code points in a string.
///
/// This is an alias for [`kodo_string_length`] — both return
/// character count, not byte count.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_char_count(ptr: *const u8, len: usize) -> i64 {
    // SAFETY: Caller guarantees ptr/len form a valid UTF-8 byte slice.
    unsafe { kodo_string_length(ptr, len) }
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
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(&result, out_ptr, out_len);
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
        // Empty pattern: return a copy of the original string.
        // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe {
            crate::memory::alloc_string_out(haystack, out_ptr, out_len);
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
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(&result, out_ptr, out_len);
    }
}

/// Returns the character (Unicode codepoint as i64) at the given character index.
///
/// Index is character-based (iterates UTF-8 codepoints, not bytes).
/// Returns -1 if `index` is out of bounds or negative.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_char_at(ptr: *const u8, len: usize, index: i64) -> i64 {
    if index < 0 {
        return -1;
    }
    // SAFETY: Caller guarantees ptr/len form a valid UTF-8 byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let Ok(s) = std::str::from_utf8(bytes) else {
        return -1;
    };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let target = index as usize;
    match s.chars().nth(target) {
        Some(c) => i64::from(c as u32),
        None => -1,
    }
}

/// Repeats a string `count` times, writing the result via out-parameters.
///
/// If `count` is zero or negative, returns an empty string.
/// The caller is responsible for eventually freeing the allocated memory.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_repeat(
    ptr: *const u8,
    len: usize,
    count: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    if count <= 0 {
        // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe {
            crate::memory::alloc_string_out(&[], out_ptr, out_len);
        }
        return;
    }
    // SAFETY: Caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let n = count as usize;
    let mut result = Vec::with_capacity(len * n);
    for _ in 0..n {
        result.extend_from_slice(bytes);
    }
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(&result, out_ptr, out_len);
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
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(&result, out_ptr, out_len);
    }
}

/// Returns a lowercase copy of the string.
///
/// Allocates a new RC-managed string on the heap.
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
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(&result, out_ptr, out_len);
    }
}

/// Returns a substring from character index `start` to `end` (exclusive).
///
/// Indices are Unicode code point positions, not byte offsets. For example,
/// `substring(0, 3)` on `"héllo"` returns `"hél"` (3 characters), not
/// the first 3 bytes which would split the multi-byte `é`.
///
/// Out-of-range indices are clamped to the string's character length.
/// If `start >= end` (after clamping), an empty string is returned.
/// Negative indices are treated as 0.
///
/// The result is a sub-slice of the original string (no allocation needed)
/// because UTF-8 character boundaries always align with byte boundaries.
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
    // SAFETY: Caller guarantees ptr/len form a valid UTF-8 byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };

    // Try to interpret as valid UTF-8 for character-based indexing.
    let Ok(s) = std::str::from_utf8(bytes) else {
        // Fallback for invalid UTF-8: return empty string.
        // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe {
            *out_ptr = bytes.as_ptr();
            *out_len = 0;
        }
        return;
    };

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let start_char = start.max(0) as usize;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let end_char = end.max(0) as usize;

    if end_char <= start_char {
        // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe {
            *out_ptr = bytes.as_ptr();
            *out_len = 0;
        }
        return;
    }

    // Map character positions to byte offsets using char_indices().
    // We need byte offsets for positions `start_char` and `end_char`.
    let mut start_byte = len; // default: beyond string (start past end)
    let mut end_byte = len; // default: clamp to end of string
    for (char_pos, (byte_idx, _ch)) in s.char_indices().enumerate() {
        if char_pos == start_char {
            start_byte = byte_idx;
        }
        if char_pos == end_char {
            end_byte = byte_idx;
            break;
        }
    }

    // SAFETY: start_byte and end_byte are valid UTF-8 character boundaries
    // within [0, len]. Caller guarantees out_ptr and out_len are valid.
    unsafe {
        *out_ptr = bytes.as_ptr().add(start_byte);
        *out_len = end_byte - start_byte;
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
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(s.as_bytes(), out_ptr, out_len);
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
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(s.as_bytes(), out_ptr, out_len);
    }
}

/// Converts a boolean to its string representation ("true" or "false").
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_bool_to_string(
    value: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    let s = if value != 0 { "true" } else { "false" };
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(s.as_bytes(), out_ptr, out_len);
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
/// For RC-managed strings (allocated via [`crate::memory::alloc_string`]),
/// this decrements the refcount, freeing the memory when it reaches zero.
///
/// Does nothing if `ptr` is null or `len` is zero.
///
/// # Safety
///
/// `ptr` must have been allocated by the runtime (either via `alloc_string`
/// or legacy `Box::into_raw`), or be null.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    // Delegate to the RC system. If the pointer is RC-managed, its refcount
    // will be decremented (and the memory freed when it reaches zero).
    // If it is not RC-managed, kodo_rc_dec is a safe no-op.
    #[allow(clippy::cast_possible_wrap)]
    crate::memory::kodo_rc_dec_string(ptr as i64, len as i64);
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

/// Splits a string by newline characters and returns a `List<String>`.
///
/// Each resulting line is stored as a `(ptr, len)` pair on the heap,
/// following the same format as `kodo_string_split`. Empty trailing
/// lines are preserved.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_lines(ptr: *const u8, len: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let byte_len = len as usize;
    // SAFETY: Caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, byte_len) };

    let list = crate::collections::kodo_list_new();

    let mut start = 0;
    while start <= byte_len {
        // Find next newline.
        let remaining = &bytes[start..];
        let found = remaining.iter().position(|&b| b == b'\n');

        if let Some(pos) = found {
            // SAFETY: start + pos <= byte_len, so the pointer is within bounds.
            let sub_ptr = unsafe { ptr.add(start) };
            let sub_len = pos;
            #[allow(clippy::cast_possible_wrap)]
            let pair = Box::new([sub_ptr as i64, sub_len as i64]);
            // SAFETY: intentionally leaks so the pair can be stored in the list.
            let pair_ptr = Box::into_raw(pair) as i64;
            // SAFETY: list is valid.
            unsafe { crate::collections::kodo_list_push(list, pair_ptr) };
            start += pos + 1;
        } else {
            // Last segment.
            // SAFETY: start <= byte_len, so the pointer is within bounds.
            let sub_ptr = unsafe { ptr.add(start) };
            let sub_len = byte_len - start;
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

/// Parses a string as a decimal integer.
///
/// Returns the parsed `i64` value, or 0 if the string is not a valid integer.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_parse_int(ptr: *const u8, len: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let byte_len = len as usize;
    // SAFETY: Caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, byte_len) };
    let s = std::str::from_utf8(bytes).unwrap_or("");
    let trimmed = s.trim();
    trimmed.parse::<i64>().unwrap_or(0)
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

/// Contract failure handler for recoverable mode — logs warning but continues execution.
///
/// Unlike [`kodo_contract_fail`], this function does **not** abort the process.
/// It prints a warning to stderr and returns normally, allowing execution to
/// continue with a default return value. This is useful for production services
/// that should not crash on contract violations.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_contract_fail_recoverable(ptr: *const u8, len: usize) {
    // SAFETY: Caller guarantees ptr/len form a valid UTF-8 slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let msg = std::str::from_utf8(bytes).unwrap_or("<invalid utf-8>");
    eprintln!("WARNING: contract violation (recoverable): {msg}");
}

// ---------------------------------------------------------------------------
// String character iterator
// ---------------------------------------------------------------------------

/// Internal state for a character iterator over a string.
///
/// Iterates over UTF-8 codepoints. Each call to `advance` moves to the
/// next codepoint; `value` returns the current codepoint as an `i64`.
struct StringCharsIterator {
    /// Pointer to the start of the string bytes.
    ptr: *const u8,
    /// Total length of the string in bytes.
    len: usize,
    /// Current byte offset in the string.
    offset: usize,
    /// Current character value (Unicode codepoint as i64).
    current: i64,
}

/// Creates a new character iterator for a string.
///
/// Returns an opaque handle (as i64) to a heap-allocated `StringCharsIterator`.
/// The iterator starts before the first character; call `advance` to move to
/// the first element.
///
/// # Safety
///
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_chars(ptr: i64, len: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let byte_len = len as usize;
    let iter = Box::new(StringCharsIterator {
        ptr: ptr as *const u8,
        len: byte_len,
        offset: 0,
        current: 0,
    });
    // SAFETY: intentionally leaks so caller manages via opaque handle.
    // Freed by `kodo_string_chars_free`.
    Box::into_raw(iter) as i64
}

/// Advances the string character iterator to the next codepoint.
///
/// Returns 1 if a character was available, 0 if the iterator is exhausted.
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_string_chars`.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_chars_advance(iter_ptr: i64) -> i64 {
    if iter_ptr == 0 {
        return 0;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_string_chars.
    let iter = unsafe { &mut *(iter_ptr as *mut StringCharsIterator) };
    if iter.offset >= iter.len {
        return 0;
    }
    // SAFETY: caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(iter.ptr, iter.len) };
    // Decode the next UTF-8 codepoint.
    let b0 = bytes[iter.offset];
    let (codepoint, char_len) = if b0 < 0x80 {
        (i64::from(b0), 1)
    } else if b0 < 0xE0 && iter.offset + 1 < iter.len {
        let cp = (i64::from(b0 & 0x1F) << 6) | i64::from(bytes[iter.offset + 1] & 0x3F);
        (cp, 2)
    } else if b0 < 0xF0 && iter.offset + 2 < iter.len {
        let cp = (i64::from(b0 & 0x0F) << 12)
            | (i64::from(bytes[iter.offset + 1] & 0x3F) << 6)
            | i64::from(bytes[iter.offset + 2] & 0x3F);
        (cp, 3)
    } else if iter.offset + 3 < iter.len {
        let cp = (i64::from(b0 & 0x07) << 18)
            | (i64::from(bytes[iter.offset + 1] & 0x3F) << 12)
            | (i64::from(bytes[iter.offset + 2] & 0x3F) << 6)
            | i64::from(bytes[iter.offset + 3] & 0x3F);
        (cp, 4)
    } else {
        // Invalid or truncated UTF-8: skip one byte and use replacement char.
        (0xFFFD, 1)
    };
    iter.current = codepoint;
    iter.offset += char_len;
    1
}

/// Returns the current character value from the iterator as an Int (codepoint).
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_string_chars`.
/// Must be called after a successful `kodo_string_chars_advance` call.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_chars_value(iter_ptr: i64) -> i64 {
    if iter_ptr == 0 {
        return 0;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_string_chars.
    let iter = unsafe { &*(iter_ptr as *const StringCharsIterator) };
    iter.current
}

/// Frees a string character iterator previously allocated by `kodo_string_chars`.
///
/// Does nothing if `iter_ptr` is zero (null handle).
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_string_chars`, or zero.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_chars_free(iter_ptr: i64) {
    if iter_ptr == 0 {
        return;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_string_chars
    // (i.e. Box::into_raw on a Box<StringCharsIterator>).
    let _ = unsafe { Box::from_raw(iter_ptr as *mut StringCharsIterator) };
}

// ---------------------------------------------------------------------------
// Character classification free functions
// ---------------------------------------------------------------------------

/// Returns 1 if the given Unicode codepoint is an alphabetic character, 0 otherwise.
///
/// Covers ASCII `A-Z`, `a-z` and all Unicode alphabetic characters.
#[no_mangle]
pub extern "C" fn kodo_is_alpha(code: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let result = char::from_u32(code as u32).is_some_and(char::is_alphabetic);
    i64::from(result)
}

/// Returns 1 if the given Unicode codepoint is a digit (`0-9`), 0 otherwise.
#[no_mangle]
pub extern "C" fn kodo_is_digit(code: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let result = char::from_u32(code as u32).is_some_and(|c| c.is_ascii_digit());
    i64::from(result)
}

/// Returns 1 if the given Unicode codepoint is alphanumeric, 0 otherwise.
#[no_mangle]
pub extern "C" fn kodo_is_alphanumeric(code: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let result = char::from_u32(code as u32).is_some_and(char::is_alphanumeric);
    i64::from(result)
}

/// Returns 1 if the given Unicode codepoint is whitespace, 0 otherwise.
#[no_mangle]
pub extern "C" fn kodo_is_whitespace(code: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let result = char::from_u32(code as u32).is_some_and(char::is_whitespace);
    i64::from(result)
}

/// Converts a Unicode codepoint to a single-character string.
///
/// Returns the character as a heap-allocated string via out-parameters.
/// If the codepoint is invalid, returns an empty string.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_char_from_code(
    code: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let ch = char::from_u32(code as u32);
    match ch {
        Some(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe {
                crate::memory::alloc_string_out(s.as_bytes(), out_ptr, out_len);
            }
        }
        None => {
            // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe {
                crate::memory::alloc_string_out(&[], out_ptr, out_len);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// StringBuilder
// ---------------------------------------------------------------------------

/// Internal state for a mutable string builder.
struct StringBuilder {
    /// Accumulated bytes.
    buf: Vec<u8>,
}

/// Creates a new empty `StringBuilder`.
///
/// Returns an opaque handle (as i64) to a heap-allocated `StringBuilder`.
#[no_mangle]
pub extern "C" fn kodo_string_builder_new() -> i64 {
    let sb = Box::new(StringBuilder {
        buf: Vec::with_capacity(64),
    });
    // SAFETY: intentionally leaks so caller manages via opaque handle.
    Box::into_raw(sb) as i64
}

/// Appends a string to the `StringBuilder`.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `kodo_string_builder_new`.
/// `ptr` must point to `len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_builder_push(handle: i64, ptr: *const u8, len: usize) {
    if handle == 0 {
        return;
    }
    // SAFETY: caller guarantees handle was returned by kodo_string_builder_new.
    let sb = unsafe { &mut *(handle as *mut StringBuilder) };
    // SAFETY: caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    sb.buf.extend_from_slice(bytes);
}

/// Appends a single Unicode codepoint to the `StringBuilder`.
///
/// If the codepoint is invalid, nothing is appended.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `kodo_string_builder_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_builder_push_char(handle: i64, code: i64) {
    if handle == 0 {
        return;
    }
    // SAFETY: caller guarantees handle was returned by kodo_string_builder_new.
    let sb = unsafe { &mut *(handle as *mut StringBuilder) };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    if let Some(c) = char::from_u32(code as u32) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        sb.buf.extend_from_slice(s.as_bytes());
    }
}

/// Converts the `StringBuilder` contents to a String and consumes it.
///
/// The builder is freed after this call. The returned string is
/// heap-allocated via out-parameters.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `kodo_string_builder_new`.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_builder_to_string(
    handle: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    if handle == 0 {
        // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe {
            crate::memory::alloc_string_out(&[], out_ptr, out_len);
        }
        return;
    }
    // SAFETY: caller guarantees handle was returned by kodo_string_builder_new
    // (i.e. Box::into_raw on a Box<StringBuilder>).
    let sb = unsafe { Box::from_raw(handle as *mut StringBuilder) };
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(&sb.buf, out_ptr, out_len);
    }
}

/// Returns the current length (in bytes) of the `StringBuilder`.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `kodo_string_builder_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_string_builder_len(handle: i64) -> i64 {
    if handle == 0 {
        return 0;
    }
    // SAFETY: caller guarantees handle was returned by kodo_string_builder_new.
    let sb = unsafe { &*(handle as *const StringBuilder) };
    #[allow(clippy::cast_possible_wrap)]
    let result = sb.buf.len() as i64;
    result
}

// ---------------------------------------------------------------------------
// Number formatting
// ---------------------------------------------------------------------------

/// Formats an integer in the given base (2-36).
///
/// Returns the formatted string via out-parameters. If `base` is out of
/// range, returns `"<invalid base>"`.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_format_int(
    value: i64,
    base: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    let s = match base {
        2 => format!("{value:b}"),
        8 => format!("{value:o}"),
        10 => format!("{value}"),
        16 => format!("{value:x}"),
        _ if (2..=36).contains(&base) => {
            // Manual base conversion for bases other than 2, 8, 10, 16.
            if value == 0 {
                "0".to_string()
            } else {
                const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
                let mut result = Vec::new();
                let negative = value < 0;
                // Use unsigned absolute value to handle i64::MIN correctly.
                #[allow(clippy::cast_sign_loss)]
                let mut v = if negative {
                    (value as u64).wrapping_neg()
                } else {
                    value as u64
                };
                #[allow(clippy::cast_sign_loss)]
                let b = base as u64;
                while v > 0 {
                    #[allow(clippy::cast_possible_truncation)]
                    result.push(DIGITS[(v % b) as usize]);
                    v /= b;
                }
                if negative {
                    result.push(b'-');
                }
                result.reverse();
                String::from_utf8(result).unwrap_or_default()
            }
        }
        _ => "<invalid base>".to_string(),
    };
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        crate::memory::alloc_string_out(s.as_bytes(), out_ptr, out_len);
    }
}

/// Returns the current Unix epoch timestamp in milliseconds.
#[no_mangle]
pub extern "C" fn kodo_timestamp() -> i64 {
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let result = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    result
}

/// Sleeps for the given number of milliseconds.
#[no_mangle]
pub extern "C" fn kodo_sleep(ms: i64) {
    if ms > 0 {
        #[allow(clippy::cast_sign_loss)]
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_length_ascii() {
        let s = "hello";
        assert_eq!(unsafe { kodo_string_length(s.as_ptr(), s.len()) }, 5);
    }

    #[test]
    fn string_length_unicode_accented() {
        let s = "héllo"; // 'é' is 2 bytes, but 1 character
        assert_eq!(unsafe { kodo_string_length(s.as_ptr(), s.len()) }, 5);
        // byte_length should be 6 (h=1, é=2, l=1, l=1, o=1)
        assert_eq!(kodo_string_byte_length(s.as_ptr(), s.len()), 6);
    }

    #[test]
    fn string_length_unicode_emoji() {
        let s = "hi\u{1F600}!"; // "hi😀!" — emoji is 4 bytes
        assert_eq!(unsafe { kodo_string_length(s.as_ptr(), s.len()) }, 4);
        assert_eq!(kodo_string_byte_length(s.as_ptr(), s.len()), 7);
    }

    #[test]
    fn string_length_unicode_cjk() {
        let s = "\u{4E16}\u{754C}"; // "世界" — 2 chars, 6 bytes
        assert_eq!(unsafe { kodo_string_length(s.as_ptr(), s.len()) }, 2);
        assert_eq!(kodo_string_byte_length(s.as_ptr(), s.len()), 6);
    }

    #[test]
    fn string_length_empty() {
        let s = "";
        assert_eq!(unsafe { kodo_string_length(s.as_ptr(), s.len()) }, 0);
    }

    #[test]
    fn string_char_count_matches_length() {
        let s = "héllo\u{1F600}";
        let len = unsafe { kodo_string_length(s.as_ptr(), s.len()) };
        let count = unsafe { kodo_string_char_count(s.as_ptr(), s.len()) };
        assert_eq!(len, count);
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
    fn bool_to_string_true() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_bool_to_string(1, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "true");
    }

    #[test]
    fn bool_to_string_false() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_bool_to_string(0, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "false");
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

    #[test]
    fn string_chars_ascii() {
        let s = "abc";
        #[allow(clippy::cast_possible_wrap)]
        let iter = unsafe { kodo_string_chars(s.as_ptr() as i64, s.len() as i64) };
        assert_ne!(iter, 0);

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 97); // 'a'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 98); // 'b'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 99); // 'c'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 0); // exhausted

        unsafe { kodo_string_chars_free(iter) };
    }

    #[test]
    fn string_chars_unicode() {
        let s = "a\u{00E9}"; // "aé" — 'a' is 1 byte, 'é' is 2 bytes (U+00E9)
        #[allow(clippy::cast_possible_wrap)]
        let iter = unsafe { kodo_string_chars(s.as_ptr() as i64, s.len() as i64) };

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 97); // 'a'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 0xE9); // 'é'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 0);

        unsafe { kodo_string_chars_free(iter) };
    }

    #[test]
    fn string_chars_emoji() {
        let s = "\u{1F600}"; // 😀 — 4-byte UTF-8
        #[allow(clippy::cast_possible_wrap)]
        let iter = unsafe { kodo_string_chars(s.as_ptr() as i64, s.len() as i64) };

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 0x1F600);

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 0);

        unsafe { kodo_string_chars_free(iter) };
    }

    #[test]
    fn string_chars_empty() {
        let s = "";
        #[allow(clippy::cast_possible_wrap)]
        let iter = unsafe { kodo_string_chars(s.as_ptr() as i64, s.len() as i64) };

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 0);

        unsafe { kodo_string_chars_free(iter) };
    }

    #[test]
    fn string_chars_free_null_does_not_crash() {
        unsafe { kodo_string_chars_free(0) };
    }

    #[test]
    fn string_chars_three_byte_utf8() {
        let s = "\u{4E16}"; // '世' — 3-byte UTF-8 (U+4E16)
        #[allow(clippy::cast_possible_wrap)]
        let iter = unsafe { kodo_string_chars(s.as_ptr() as i64, s.len() as i64) };

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 0x4E16);

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 0);

        unsafe { kodo_string_chars_free(iter) };
    }

    #[test]
    fn string_chars_mixed_lengths() {
        let s = "A\u{00F1}\u{4E16}\u{1F600}"; // A (1), ñ (2), 世 (3), 😀 (4)
        #[allow(clippy::cast_possible_wrap)]
        let iter = unsafe { kodo_string_chars(s.as_ptr() as i64, s.len() as i64) };

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 65); // 'A'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 0xF1); // 'ñ'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 0x4E16); // '世'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 1);
        assert_eq!(unsafe { kodo_string_chars_value(iter) }, 0x1F600); // '😀'

        assert_eq!(unsafe { kodo_string_chars_advance(iter) }, 0);

        unsafe { kodo_string_chars_free(iter) };
    }

    #[test]
    fn string_parse_int_valid() {
        let s = "42";
        #[allow(clippy::cast_possible_wrap)]
        let result = unsafe { kodo_string_parse_int(s.as_ptr(), s.len() as i64) };
        assert_eq!(result, 42);
    }

    #[test]
    fn string_parse_int_negative() {
        let s = "-17";
        #[allow(clippy::cast_possible_wrap)]
        let result = unsafe { kodo_string_parse_int(s.as_ptr(), s.len() as i64) };
        assert_eq!(result, -17);
    }

    #[test]
    fn string_parse_int_invalid() {
        let s = "not_a_number";
        #[allow(clippy::cast_possible_wrap)]
        let result = unsafe { kodo_string_parse_int(s.as_ptr(), s.len() as i64) };
        assert_eq!(result, 0);
    }

    #[test]
    fn string_parse_int_with_whitespace() {
        let s = "  123  ";
        #[allow(clippy::cast_possible_wrap)]
        let result = unsafe { kodo_string_parse_int(s.as_ptr(), s.len() as i64) };
        assert_eq!(result, 123);
    }

    #[test]
    fn string_parse_int_empty() {
        let s = "";
        #[allow(clippy::cast_possible_wrap)]
        let result = unsafe { kodo_string_parse_int(s.as_ptr(), s.len() as i64) };
        assert_eq!(result, 0);
    }

    #[test]
    fn string_lines_basic() {
        let s = "hello\nworld\nfoo";
        #[allow(clippy::cast_possible_wrap)]
        let list = unsafe { kodo_string_lines(s.as_ptr(), s.len() as i64) };
        assert_ne!(list, 0);
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 3);
    }

    #[test]
    fn string_lines_trailing_newline() {
        let s = "a\nb\n";
        #[allow(clippy::cast_possible_wrap)]
        let list = unsafe { kodo_string_lines(s.as_ptr(), s.len() as i64) };
        // "a\nb\n" splits into ["a", "b", ""]
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 3);
    }

    #[test]
    fn string_lines_empty() {
        let s = "";
        #[allow(clippy::cast_possible_wrap)]
        let list = unsafe { kodo_string_lines(s.as_ptr(), s.len() as i64) };
        // Empty string yields one empty line.
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 1);
    }

    #[test]
    fn string_char_at_ascii() {
        let s = "hello";
        let result = unsafe { kodo_string_char_at(s.as_ptr(), s.len(), 1) };
        assert_eq!(result, 101); // 'e'
    }

    #[test]
    fn string_char_at_unicode() {
        let s = "a\u{00E9}\u{4E16}"; // "aé世"
        let result = unsafe { kodo_string_char_at(s.as_ptr(), s.len(), 1) };
        assert_eq!(result, 0xE9); // 'é'
    }

    #[test]
    fn string_char_at_out_of_bounds() {
        let s = "hello";
        let result = unsafe { kodo_string_char_at(s.as_ptr(), s.len(), 100) };
        assert_eq!(result, -1);
    }

    #[test]
    fn string_char_at_negative() {
        let s = "hello";
        let result = unsafe { kodo_string_char_at(s.as_ptr(), s.len(), -1) };
        assert_eq!(result, -1);
    }

    #[test]
    fn string_repeat_basic() {
        let s = "ab";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_string_repeat(s.as_ptr(), s.len(), 3, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "ababab");
    }

    #[test]
    fn string_repeat_zero() {
        let s = "ab";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_string_repeat(s.as_ptr(), s.len(), 0, &mut out_ptr, &mut out_len) };
        assert_eq!(out_len, 0);
    }

    // -----------------------------------------------------------------------
    // kodo_string_replace — additional edge case tests
    // -----------------------------------------------------------------------

    #[test]
    fn string_replace_no_match() {
        let hay = "hello world";
        let pattern = "xyz";
        let replacement = "replaced";
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
        assert_eq!(std::str::from_utf8(result).unwrap(), "hello world");
    }

    #[test]
    fn string_replace_empty_pattern() {
        let hay = "hello";
        let pattern = "";
        let replacement = "X";
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
        // Empty pattern returns a copy of the original string.
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "hello");
    }

    #[test]
    fn string_replace_multiple_occurrences() {
        let hay = "aaa";
        let pattern = "a";
        let replacement = "bb";
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
        assert_eq!(std::str::from_utf8(result).unwrap(), "bbbbbb");
    }

    #[test]
    fn string_replace_with_empty_replacement() {
        let hay = "hello world";
        let pattern = " ";
        let replacement = "";
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
        assert_eq!(std::str::from_utf8(result).unwrap(), "helloworld");
    }

    // -----------------------------------------------------------------------
    // kodo_string_split tests
    // -----------------------------------------------------------------------

    /// Helper to extract the string content from a split/lines list element.
    ///
    /// Each element in the list is a pointer to a `[i64; 2]` pair (ptr, len).
    unsafe fn extract_list_string(list: i64, index: i64) -> String {
        let mut value: i64 = 0;
        let mut is_some: i64 = 0;
        unsafe {
            crate::collections::kodo_list_get(list, index, &mut value, &mut is_some);
        }
        assert_eq!(is_some, 1, "list element at index {index} not found");
        // value is a pointer to [i64; 2] = [ptr, len]
        let pair = unsafe { &*(value as *const [i64; 2]) };
        let str_ptr = pair[0] as *const u8;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let str_len = pair[1] as usize;
        let bytes = unsafe { std::slice::from_raw_parts(str_ptr, str_len) };
        std::str::from_utf8(bytes).unwrap().to_string()
    }

    #[test]
    fn string_split_basic() {
        let hay = "a,b,c";
        let sep = ",";
        let list = unsafe { kodo_string_split(hay.as_ptr(), hay.len(), sep.as_ptr(), sep.len()) };
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 3);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "a");
        assert_eq!(unsafe { extract_list_string(list, 1) }, "b");
        assert_eq!(unsafe { extract_list_string(list, 2) }, "c");
    }

    #[test]
    fn string_split_no_delimiter_found() {
        let hay = "hello";
        let sep = ",";
        let list = unsafe { kodo_string_split(hay.as_ptr(), hay.len(), sep.as_ptr(), sep.len()) };
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 1);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "hello");
    }

    #[test]
    fn string_split_empty_string() {
        let hay = "";
        let sep = ",";
        let list = unsafe { kodo_string_split(hay.as_ptr(), hay.len(), sep.as_ptr(), sep.len()) };
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 1);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "");
    }

    #[test]
    fn string_split_empty_separator() {
        let hay = "hello";
        let sep = "";
        let list = unsafe { kodo_string_split(hay.as_ptr(), hay.len(), sep.as_ptr(), sep.len()) };
        // Empty separator returns the whole string as one element.
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 1);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "hello");
    }

    #[test]
    fn string_split_multi_char_separator() {
        let hay = "a::b::c";
        let sep = "::";
        let list = unsafe { kodo_string_split(hay.as_ptr(), hay.len(), sep.as_ptr(), sep.len()) };
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 3);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "a");
        assert_eq!(unsafe { extract_list_string(list, 1) }, "b");
        assert_eq!(unsafe { extract_list_string(list, 2) }, "c");
    }

    #[test]
    fn string_split_trailing_separator() {
        let hay = "a,b,";
        let sep = ",";
        let list = unsafe { kodo_string_split(hay.as_ptr(), hay.len(), sep.as_ptr(), sep.len()) };
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 3);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "a");
        assert_eq!(unsafe { extract_list_string(list, 1) }, "b");
        assert_eq!(unsafe { extract_list_string(list, 2) }, "");
    }

    // -----------------------------------------------------------------------
    // kodo_string_lines — additional edge case tests
    // -----------------------------------------------------------------------

    #[test]
    fn string_lines_content_verification() {
        let s = "hello\nworld\nfoo";
        #[allow(clippy::cast_possible_wrap)]
        let list = unsafe { kodo_string_lines(s.as_ptr(), s.len() as i64) };
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 3);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "hello");
        assert_eq!(unsafe { extract_list_string(list, 1) }, "world");
        assert_eq!(unsafe { extract_list_string(list, 2) }, "foo");
    }

    #[test]
    fn string_lines_crlf() {
        // CRLF: the \r is preserved as part of the line content since
        // kodo_string_lines only splits on \n.
        let s = "a\r\nb\r\n";
        #[allow(clippy::cast_possible_wrap)]
        let list = unsafe { kodo_string_lines(s.as_ptr(), s.len() as i64) };
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 3);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "a\r");
        assert_eq!(unsafe { extract_list_string(list, 1) }, "b\r");
        assert_eq!(unsafe { extract_list_string(list, 2) }, "");
    }

    #[test]
    fn string_lines_single_newline() {
        let s = "\n";
        #[allow(clippy::cast_possible_wrap)]
        let list = unsafe { kodo_string_lines(s.as_ptr(), s.len() as i64) };
        // "\n" splits into ["", ""]
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 2);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "");
        assert_eq!(unsafe { extract_list_string(list, 1) }, "");
    }

    #[test]
    fn string_lines_no_newline() {
        let s = "single line";
        #[allow(clippy::cast_possible_wrap)]
        let list = unsafe { kodo_string_lines(s.as_ptr(), s.len() as i64) };
        assert_eq!(unsafe { crate::collections::kodo_list_length(list) }, 1);
        assert_eq!(unsafe { extract_list_string(list, 0) }, "single line");
    }

    #[test]
    fn contract_fail_recoverable_does_not_abort() {
        let msg = "test contract violation";
        // This should NOT abort — it just prints a warning and returns.
        unsafe { kodo_contract_fail_recoverable(msg.as_ptr(), msg.len()) };
        // If we reach here, the test passes (function returned normally).
    }

    #[test]
    fn contract_fail_recoverable_invalid_utf8() {
        let bytes: [u8; 2] = [0xFF, 0xFE];
        // Should handle invalid UTF-8 gracefully without aborting.
        unsafe { kodo_contract_fail_recoverable(bytes.as_ptr(), bytes.len()) };
    }

    // -----------------------------------------------------------------------
    // Character classification tests
    // -----------------------------------------------------------------------

    #[test]
    fn is_alpha_letters() {
        assert_eq!(kodo_is_alpha(65), 1); // 'A'
        assert_eq!(kodo_is_alpha(122), 1); // 'z'
        assert_eq!(kodo_is_alpha(48), 0); // '0'
        assert_eq!(kodo_is_alpha(32), 0); // ' '
        assert_eq!(kodo_is_alpha(95), 0); // '_'
    }

    #[test]
    fn is_digit_numbers() {
        assert_eq!(kodo_is_digit(48), 1); // '0'
        assert_eq!(kodo_is_digit(57), 1); // '9'
        assert_eq!(kodo_is_digit(65), 0); // 'A'
        assert_eq!(kodo_is_digit(32), 0); // ' '
    }

    #[test]
    fn is_alphanumeric_mixed() {
        assert_eq!(kodo_is_alphanumeric(65), 1); // 'A'
        assert_eq!(kodo_is_alphanumeric(48), 1); // '0'
        assert_eq!(kodo_is_alphanumeric(32), 0); // ' '
        assert_eq!(kodo_is_alphanumeric(95), 0); // '_'
    }

    #[test]
    fn is_whitespace_chars() {
        assert_eq!(kodo_is_whitespace(32), 1); // ' '
        assert_eq!(kodo_is_whitespace(9), 1); // '\t'
        assert_eq!(kodo_is_whitespace(10), 1); // '\n'
        assert_eq!(kodo_is_whitespace(65), 0); // 'A'
    }

    #[test]
    fn char_from_code_ascii() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_char_from_code(65, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "A");
    }

    #[test]
    fn char_from_code_unicode() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_char_from_code(0xE9, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "\u{00E9}");
    }

    #[test]
    fn char_from_code_invalid() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        // Invalid Unicode codepoint — should return empty string.
        unsafe { kodo_char_from_code(0x110000, &mut out_ptr, &mut out_len) };
        assert_eq!(out_len, 0);
    }

    // -----------------------------------------------------------------------
    // StringBuilder tests
    // -----------------------------------------------------------------------

    #[test]
    fn string_builder_basic() {
        let sb = kodo_string_builder_new();
        assert_ne!(sb, 0);
        let s1 = "hello";
        let s2 = " world";
        unsafe { kodo_string_builder_push(sb, s1.as_ptr(), s1.len()) };
        unsafe { kodo_string_builder_push(sb, s2.as_ptr(), s2.len()) };
        assert_eq!(unsafe { kodo_string_builder_len(sb) }, 11);
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_string_builder_to_string(sb, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "hello world");
    }

    #[test]
    fn string_builder_push_char() {
        let sb = kodo_string_builder_new();
        unsafe { kodo_string_builder_push_char(sb, 72) }; // 'H'
        unsafe { kodo_string_builder_push_char(sb, 105) }; // 'i'
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_string_builder_to_string(sb, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "Hi");
    }

    #[test]
    fn string_builder_empty() {
        let sb = kodo_string_builder_new();
        assert_eq!(unsafe { kodo_string_builder_len(sb) }, 0);
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_string_builder_to_string(sb, &mut out_ptr, &mut out_len) };
        assert_eq!(out_len, 0);
    }

    // -----------------------------------------------------------------------
    // format_int tests
    // -----------------------------------------------------------------------

    #[test]
    fn format_int_decimal() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_format_int(42, 10, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "42");
    }

    #[test]
    fn format_int_hex() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_format_int(255, 16, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "ff");
    }

    #[test]
    fn format_int_binary() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_format_int(10, 2, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "1010");
    }

    #[test]
    fn format_int_invalid_base() {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe { kodo_format_int(42, 1, &mut out_ptr, &mut out_len) };
        let result = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(std::str::from_utf8(result).unwrap(), "<invalid base>");
    }

    // -----------------------------------------------------------------------
    // timestamp and sleep tests
    // -----------------------------------------------------------------------

    #[test]
    fn timestamp_returns_nonzero() {
        let ts = kodo_timestamp();
        assert!(ts > 0);
    }

    #[test]
    fn sleep_zero_does_not_block() {
        kodo_sleep(0);
        kodo_sleep(-1);
        // If we reach here, no crash.
    }

    // -----------------------------------------------------------------------
    // Unicode-aware substring tests
    // -----------------------------------------------------------------------

    /// Helper to call kodo_string_substring and return the result as a &str.
    unsafe fn call_substring(s: &str, start: i64, end: i64) -> String {
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe {
            kodo_string_substring(s.as_ptr(), s.len(), start, end, &mut out_ptr, &mut out_len);
        }
        let bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        std::str::from_utf8(bytes).unwrap().to_string()
    }

    #[test]
    fn substring_ascii_unchanged() {
        // ASCII strings should behave identically to byte-based substring.
        let result = unsafe { call_substring("hello", 1, 4) };
        assert_eq!(result, "ell");
    }

    #[test]
    fn substring_ascii_full_string() {
        let result = unsafe { call_substring("hello", 0, 5) };
        assert_eq!(result, "hello");
    }

    #[test]
    fn substring_multibyte_accented() {
        // "héllo" — 'é' is 2 bytes but 1 character.
        // substring(0, 3) should return "hél" (3 chars), not split 'é'.
        let result = unsafe { call_substring("héllo", 0, 3) };
        assert_eq!(result, "hél");
    }

    #[test]
    fn substring_multibyte_skip_accented() {
        // "héllo" — substring(1, 4) should return "éll".
        let result = unsafe { call_substring("héllo", 1, 4) };
        assert_eq!(result, "éll");
    }

    #[test]
    fn substring_emoji() {
        // "hi😀bye" — emoji is 4 bytes but 1 character.
        let s = "hi\u{1F600}bye";
        let result = unsafe { call_substring(s, 0, 3) };
        assert_eq!(result, "hi\u{1F600}");
    }

    #[test]
    fn substring_emoji_after() {
        // "hi😀bye" — substring(3, 6) should return "bye".
        let s = "hi\u{1F600}bye";
        let result = unsafe { call_substring(s, 3, 6) };
        assert_eq!(result, "bye");
    }

    #[test]
    fn substring_cjk() {
        // "世界你好" — each char is 3 bytes.
        let s = "\u{4E16}\u{754C}\u{4F60}\u{597D}";
        let result = unsafe { call_substring(s, 1, 3) };
        assert_eq!(result, "\u{754C}\u{4F60}");
    }

    #[test]
    fn substring_mixed_multibyte() {
        // "Añ世😀" — 1-byte, 2-byte, 3-byte, 4-byte characters.
        let s = "A\u{00F1}\u{4E16}\u{1F600}";
        let result = unsafe { call_substring(s, 1, 3) };
        assert_eq!(result, "\u{00F1}\u{4E16}");
    }

    #[test]
    fn substring_empty_string() {
        let result = unsafe { call_substring("", 0, 0) };
        assert_eq!(result, "");
    }

    #[test]
    fn substring_start_equals_end() {
        let result = unsafe { call_substring("hello", 2, 2) };
        assert_eq!(result, "");
    }

    #[test]
    fn substring_start_greater_than_end() {
        let result = unsafe { call_substring("hello", 3, 1) };
        assert_eq!(result, "");
    }

    #[test]
    fn substring_start_beyond_length() {
        let result = unsafe { call_substring("hi", 10, 20) };
        assert_eq!(result, "");
    }

    #[test]
    fn substring_end_beyond_length_clamps() {
        // Clamp end to string length.
        let result = unsafe { call_substring("hello", 2, 100) };
        assert_eq!(result, "llo");
    }

    #[test]
    fn substring_negative_start() {
        // Negative start treated as 0.
        let result = unsafe { call_substring("hello", -5, 3) };
        assert_eq!(result, "hel");
    }

    #[test]
    fn substring_negative_end() {
        // Negative end treated as 0, so start >= end → empty.
        let result = unsafe { call_substring("hello", 0, -1) };
        assert_eq!(result, "");
    }
}
