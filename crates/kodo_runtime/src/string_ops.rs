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
