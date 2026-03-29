//! Regex builtins for the Kōdo runtime.
//!
//! Provides FFI-callable functions for regular expression operations:
//! - `kodo_regex_match` — tests whether a pattern matches anywhere in a string
//! - `kodo_regex_find`  — returns the first match as `Option<String>`
//! - `kodo_regex_replace` — replaces all non-overlapping matches with a replacement

use regex::Regex;

// SAFETY helper: converts a raw pointer/length pair into a &str.
// Returns None if the bytes are not valid UTF-8.
//
// # Safety
//
// `ptr` must point to `len` valid bytes.
unsafe fn bytes_to_str<'a>(ptr: *const u8, len: usize) -> Option<&'a str> {
    // SAFETY: caller guarantees ptr/len form a valid byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes).ok()
}

/// Returns 1 if `pattern` matches anywhere in `text`, 0 otherwise.
///
/// An invalid regex pattern always returns 0.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_regex_match(
    pattern_ptr: *const u8,
    pattern_len: usize,
    text_ptr: *const u8,
    text_len: usize,
) -> i64 {
    // SAFETY: caller guarantees both ptr/len pairs are valid UTF-8 byte slices.
    let (Some(pattern), Some(text)) = (unsafe { bytes_to_str(pattern_ptr, pattern_len) }, unsafe {
        bytes_to_str(text_ptr, text_len)
    }) else {
        return 0;
    };
    let Ok(re) = Regex::new(pattern) else {
        return 0;
    };
    i64::from(re.is_match(text))
}

/// Finds the first match of `pattern` in `text`.
///
/// Returns 0 (Some) if a match was found and sets `*out_ptr`/`*out_len` to the
/// matched substring (heap-allocated, must be freed with `kodo_string_free`).
/// Returns 1 (None) when there is no match or the pattern is invalid.
///
/// # Safety
///
/// All pointer/length pairs must point to valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_regex_find(
    pattern_ptr: *const u8,
    pattern_len: usize,
    text_ptr: *const u8,
    text_len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) -> i64 {
    // SAFETY: caller guarantees all ptr/len pairs are valid UTF-8 byte slices,
    //         and that out_ptr/out_len are valid writable pointers.
    let (Some(pattern), Some(text)) = (unsafe { bytes_to_str(pattern_ptr, pattern_len) }, unsafe {
        bytes_to_str(text_ptr, text_len)
    }) else {
        return 1; // None
    };
    let Ok(re) = Regex::new(pattern) else {
        return 1; // None
    };
    let Some(mat) = re.find(text) else {
        return 1; // None
    };
    let matched = mat.as_str().to_owned().into_bytes().into_boxed_slice();
    let len = matched.len();
    let ptr = Box::into_raw(matched) as *const u8;
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
    0 // Some
}

/// Replaces all non-overlapping matches of `pattern` in `text` with `replacement`.
///
/// The result is written via `out_ptr`/`out_len` (heap-allocated, must be freed
/// with `kodo_string_free`). If the pattern is invalid the original `text` is
/// returned unchanged.
///
/// # Safety
///
/// All pointer/length pairs must point to valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_regex_replace(
    pattern_ptr: *const u8,
    pattern_len: usize,
    text_ptr: *const u8,
    text_len: usize,
    repl_ptr: *const u8,
    repl_len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    // SAFETY: caller guarantees all ptr/len pairs are valid UTF-8 byte slices,
    //         and that out_ptr/out_len are valid writable pointers.
    let (Some(pattern), Some(text), Some(repl)) = (
        unsafe { bytes_to_str(pattern_ptr, pattern_len) },
        unsafe { bytes_to_str(text_ptr, text_len) },
        unsafe { bytes_to_str(repl_ptr, repl_len) },
    ) else {
        // On invalid UTF-8, return empty string.
        let result: Box<[u8]> = Box::new([]);
        let len = result.len();
        let ptr = Box::into_raw(result) as *const u8;
        // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe {
            *out_ptr = ptr;
            *out_len = len;
        }
        return;
    };
    let result = if let Ok(re) = Regex::new(pattern) {
        re.replace_all(text, repl).into_owned()
    } else {
        // Invalid regex — return text unchanged.
        text.to_owned()
    };
    let bytes = result.into_bytes().into_boxed_slice();
    let len = bytes.len();
    let ptr = Box::into_raw(bytes) as *const u8;
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_match_digit_found() {
        let pattern = b"\\d+";
        let text = b"abc123";
        let result =
            unsafe { kodo_regex_match(pattern.as_ptr(), pattern.len(), text.as_ptr(), text.len()) };
        assert_eq!(result, 1);
    }

    #[test]
    fn regex_match_digit_not_found() {
        let pattern = b"\\d+";
        let text = b"abcdef";
        let result =
            unsafe { kodo_regex_match(pattern.as_ptr(), pattern.len(), text.as_ptr(), text.len()) };
        assert_eq!(result, 0);
    }

    #[test]
    fn regex_match_invalid_pattern_returns_zero() {
        let pattern = b"[invalid";
        let text = b"hello";
        let result =
            unsafe { kodo_regex_match(pattern.as_ptr(), pattern.len(), text.as_ptr(), text.len()) };
        assert_eq!(result, 0);
    }

    #[test]
    fn regex_find_returns_some_on_match() {
        let pattern = b"\\w+";
        let text = b"hello world";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        let disc = unsafe {
            kodo_regex_find(
                pattern.as_ptr(),
                pattern.len(),
                text.as_ptr(),
                text.len(),
                &raw mut out_ptr,
                &raw mut out_len,
            )
        };
        assert_eq!(disc, 0); // Some
        let matched =
            unsafe { std::str::from_utf8(std::slice::from_raw_parts(out_ptr, out_len)).unwrap() };
        assert_eq!(matched, "hello");
        // Free the allocation.
        unsafe {
            let _ = Box::from_raw(std::slice::from_raw_parts_mut(out_ptr as *mut u8, out_len));
        }
    }

    #[test]
    fn regex_find_returns_none_on_no_match() {
        let pattern = b"\\d+";
        let text = b"no digits here";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        let disc = unsafe {
            kodo_regex_find(
                pattern.as_ptr(),
                pattern.len(),
                text.as_ptr(),
                text.len(),
                &raw mut out_ptr,
                &raw mut out_len,
            )
        };
        assert_eq!(disc, 1); // None
    }

    #[test]
    fn regex_replace_substitutes_all_matches() {
        let pattern = b"o";
        let text = b"hello world";
        let repl = b"0";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe {
            kodo_regex_replace(
                pattern.as_ptr(),
                pattern.len(),
                text.as_ptr(),
                text.len(),
                repl.as_ptr(),
                repl.len(),
                &raw mut out_ptr,
                &raw mut out_len,
            );
        }
        let result =
            unsafe { std::str::from_utf8(std::slice::from_raw_parts(out_ptr, out_len)).unwrap() };
        assert_eq!(result, "hell0 w0rld");
        unsafe {
            let _ = Box::from_raw(std::slice::from_raw_parts_mut(out_ptr as *mut u8, out_len));
        }
    }

    #[test]
    fn regex_replace_invalid_pattern_returns_text_unchanged() {
        let pattern = b"[invalid";
        let text = b"hello";
        let repl = b"x";
        let mut out_ptr: *const u8 = std::ptr::null();
        let mut out_len: usize = 0;
        unsafe {
            kodo_regex_replace(
                pattern.as_ptr(),
                pattern.len(),
                text.as_ptr(),
                text.len(),
                repl.as_ptr(),
                repl.len(),
                &raw mut out_ptr,
                &raw mut out_len,
            );
        }
        let result =
            unsafe { std::str::from_utf8(std::slice::from_raw_parts(out_ptr, out_len)).unwrap() };
        assert_eq!(result, "hello");
        unsafe {
            let _ = Box::from_raw(std::slice::from_raw_parts_mut(out_ptr as *mut u8, out_len));
        }
    }
}
