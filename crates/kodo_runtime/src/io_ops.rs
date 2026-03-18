//! I/O builtins for the Kōdo runtime.
//!
//! Provides FFI-callable functions for file I/O, environment variables,
//! time operations, and JSON parsing.

use crate::helpers::{write_string_out, write_string_out_mut};

/// Checks if a file exists at the given path.
///
/// Returns 1 if the file exists, 0 otherwise.
///
/// # Safety
///
/// `path_ptr` must point to `path_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_file_exists(path_ptr: *const u8, path_len: usize) -> i64 {
    // SAFETY: caller guarantees valid UTF-8 bytes at path_ptr..path_ptr+path_len.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path_str) = std::str::from_utf8(path_bytes) else {
        return 0;
    };
    i64::from(std::path::Path::new(path_str).exists())
}

/// Reads a file into a heap-allocated string.
///
/// Returns 0 on success (Ok), 1 on error (Err). In both cases,
/// `out_ptr`/`out_len` are set to the content string or error message.
///
/// # Safety
///
/// `path_ptr` must point to `path_len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_file_read(
    path_ptr: *const u8,
    path_len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) -> i64 {
    // SAFETY: caller guarantees valid UTF-8 bytes at path_ptr..path_ptr+path_len.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let path_str = match std::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("invalid path: {e}");
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out(&msg, out_ptr, out_len) };
            return 1;
        }
    };
    match std::fs::read_to_string(path_str) {
        Ok(contents) => {
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out(&contents, out_ptr, out_len) };
            0
        }
        Err(e) => {
            let msg = format!("{e}");
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out(&msg, out_ptr, out_len) };
            1
        }
    }
}

/// Writes content to a file.
///
/// Returns 0 on success (Ok), 1 on error (Err). On error,
/// `out_ptr`/`out_len` are set to the error message.
///
/// # Safety
///
/// `path_ptr` must point to `path_len` valid UTF-8 bytes.
/// `content_ptr` must point to `content_len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_file_write(
    path_ptr: *const u8,
    path_len: usize,
    content_ptr: *const u8,
    content_len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) -> i64 {
    // SAFETY: caller guarantees valid UTF-8 bytes.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let content_bytes = unsafe { std::slice::from_raw_parts(content_ptr, content_len) };
    let path_str = match std::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("invalid path: {e}");
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out(&msg, out_ptr, out_len) };
            return 1;
        }
    };
    match std::fs::write(path_str, content_bytes) {
        Ok(()) => 0,
        Err(e) => {
            let msg = format!("{e}");
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out(&msg, out_ptr, out_len) };
            1
        }
    }
}

// ---------------------------------------------------------------------------
// Time builtins
// ---------------------------------------------------------------------------

/// Returns the current Unix timestamp in seconds.
#[no_mangle]
pub extern "C" fn kodo_time_now() -> i64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(d.as_secs()).unwrap_or(i64::MAX)
}

/// Returns the current Unix timestamp in milliseconds.
#[no_mangle]
pub extern "C" fn kodo_time_now_ms() -> i64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
}

/// Formats a Unix timestamp as ISO 8601 UTC string.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_time_format(
    timestamp: i64,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) {
    /// Seconds per minute.
    const SECS_PER_MIN: u64 = 60;
    /// Seconds per hour.
    const SECS_PER_HOUR: u64 = 3600;
    /// Seconds per day.
    const SECS_PER_DAY: u64 = 86400;
    if out_ptr.is_null() || out_len.is_null() {
        return;
    }
    #[allow(clippy::cast_sign_loss)]
    let secs = if timestamp < 0 { 0 } else { timestamp as u64 };
    let days_since_epoch = secs / SECS_PER_DAY;
    let time_of_day = secs % SECS_PER_DAY;
    let hour = time_of_day / SECS_PER_HOUR;
    let minute = (time_of_day % SECS_PER_HOUR) / SECS_PER_MIN;
    let second = time_of_day % SECS_PER_MIN;
    #[allow(clippy::cast_possible_wrap)]
    let z = days_since_epoch as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    #[allow(clippy::cast_sign_loss)]
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    #[allow(clippy::cast_possible_wrap)]
    let y_raw = yoe as i64 + era * 400;
    #[allow(clippy::cast_sign_loss)]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y_raw + 1 } else { y_raw };
    let formatted = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z");
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe { write_string_out_mut(&formatted, out_ptr, out_len) };
}

/// Returns elapsed milliseconds since start timestamp.
#[no_mangle]
pub extern "C" fn kodo_time_elapsed_ms(start_ms: i64) -> i64 {
    let now = kodo_time_now_ms();
    let diff = now - start_ms;
    if diff < 0 {
        0
    } else {
        diff
    }
}

// ---------------------------------------------------------------------------
// Environment builtins
// ---------------------------------------------------------------------------

/// Gets an environment variable value.
///
/// # Safety
///
/// `key_ptr` must point to valid UTF-8. `out_ptr` and `out_len` must be valid.
#[no_mangle]
pub unsafe extern "C" fn kodo_env_get(
    key_ptr: *const u8,
    key_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) {
    if out_ptr.is_null() || out_len.is_null() {
        return;
    }
    if key_ptr.is_null() {
        // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe { write_string_out_mut("", out_ptr, out_len) };
        return;
    }
    // SAFETY: caller guarantees key_ptr/key_len form a valid UTF-8 slice.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    let val = std::env::var(key).unwrap_or_default();
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe { write_string_out_mut(&val, out_ptr, out_len) };
}

/// Sets an environment variable.
///
/// # Safety
///
/// `key_ptr` and `val_ptr` must point to valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn kodo_env_set(
    key_ptr: *const u8,
    key_len: usize,
    val_ptr: *const u8,
    val_len: usize,
) {
    if key_ptr.is_null() || val_ptr.is_null() {
        return;
    }
    // SAFETY: caller guarantees key_ptr/key_len and val_ptr/val_len form valid UTF-8 slices.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    let val =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(val_ptr, val_len)) };
    // SAFETY: setting environment variables is inherently unsafe in multi-threaded programs.
    // The Kōdo runtime serialises env access through the scheduler.
    unsafe { std::env::set_var(key, val) };
}

// ---------------------------------------------------------------------------
// JSON parsing builtins
// ---------------------------------------------------------------------------

/// Parses a JSON string and returns an opaque handle to the parsed value.
///
/// Returns a non-zero handle on success, or 0 on parse error.
/// The handle must be freed with `kodo_json_free` when no longer needed.
///
/// # Safety
///
/// `str_ptr` must point to `str_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_parse(str_ptr: *const u8, str_len: usize) -> i64 {
    if str_ptr.is_null() {
        return 0;
    }
    // SAFETY: caller guarantees str_ptr/str_len form a valid UTF-8 slice.
    let bytes = unsafe { std::slice::from_raw_parts(str_ptr, str_len) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return 0;
    };
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => {
            let boxed = Box::new(value);
            // SAFETY: intentionally leaks so caller manages via opaque handle.
            // Freed by `kodo_json_free`.
            Box::into_raw(boxed) as i64
        }
        Err(_) => 0,
    }
}

/// Retrieves a string value from a parsed JSON object by key.
///
/// Returns 0 on success (writing the string to `out_ptr`/`out_len`),
/// or -1 if the key does not exist or the value is not a string.
/// The caller must free the output string with `kodo_string_free`.
///
/// # Safety
///
/// `handle` must be a valid handle returned by `kodo_json_parse`.
/// `key_ptr` must point to `key_len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_json_get_string(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i64 {
    if handle == 0 || key_ptr.is_null() || out_ptr.is_null() || out_len.is_null() {
        return -1;
    }
    // SAFETY: caller guarantees handle is a valid pointer from kodo_json_parse.
    let value = unsafe { &*(handle as *const serde_json::Value) };
    // SAFETY: caller guarantees key_ptr/key_len form a valid UTF-8 slice.
    let key_bytes = unsafe { std::slice::from_raw_parts(key_ptr, key_len) };
    let Ok(key) = std::str::from_utf8(key_bytes) else {
        return -1;
    };
    match value.get(key) {
        Some(serde_json::Value::String(s)) => {
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out_mut(&s.clone(), out_ptr, out_len) };
            0
        }
        _ => -1,
    }
}

/// Retrieves an integer value from a parsed JSON object by key.
///
/// Returns the integer value if the key exists and the value is a number
/// that fits in i64. Returns 0 if the key does not exist or the value
/// is not an integer (callers should use `kodo_json_get_string` for
/// type-safe access).
///
/// # Safety
///
/// `handle` must be a valid handle returned by `kodo_json_parse`.
/// `key_ptr` must point to `key_len` valid UTF-8 bytes.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_json_get_int(handle: i64, key_ptr: *const u8, key_len: usize) -> i64 {
    if handle == 0 || key_ptr.is_null() {
        return 0;
    }
    // SAFETY: caller guarantees handle is a valid pointer from kodo_json_parse.
    let value = unsafe { &*(handle as *const serde_json::Value) };
    // SAFETY: caller guarantees key_ptr/key_len form a valid UTF-8 slice.
    let key_bytes = unsafe { std::slice::from_raw_parts(key_ptr, key_len) };
    let Ok(key) = std::str::from_utf8(key_bytes) else {
        return 0;
    };
    match value.get(key) {
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0),
        _ => 0,
    }
}

/// Serializes a JSON handle back to a JSON string.
///
/// Returns the string via out-parameters, following the standard
/// `write_string_out_mut` pattern used by all string-returning builtins.
///
/// # Safety
///
/// `handle` must be a valid handle returned by `kodo_json_parse` or
/// `kodo_json_new_object`.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_stringify(
    handle: i64,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) {
    if out_ptr.is_null() || out_len.is_null() {
        return;
    }
    if handle == 0 {
        // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe { write_string_out_mut("", out_ptr, out_len) };
        return;
    }
    // SAFETY: handle was returned by kodo_json_parse or kodo_json_new_object.
    let value = unsafe { &*(handle as *const serde_json::Value) };
    let s = serde_json::to_string(value).unwrap_or_default();
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe { write_string_out_mut(&s, out_ptr, out_len) };
}

/// Gets a boolean value from a JSON object by key.
///
/// Returns 1 for true, 0 for false, -1 if key not found or wrong type.
///
/// # Safety
///
/// `handle` must be a valid handle returned by `kodo_json_parse`.
/// `key_ptr` must point to `key_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_get_bool(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
) -> i64 {
    if handle == 0 || key_ptr.is_null() {
        return -1;
    }
    // SAFETY: handle was returned by kodo_json_parse.
    let value = unsafe { &*(handle as *const serde_json::Value) };
    // SAFETY: key_ptr/key_len describe a valid UTF-8 string.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    match value.get(key).and_then(serde_json::Value::as_bool) {
        Some(true) => 1,
        Some(false) => 0,
        None => -1,
    }
}

/// Gets a float value from a JSON object by key.
///
/// Returns the float value, or 0.0 if key not found or wrong type.
///
/// # Safety
///
/// `handle` must be a valid handle returned by `kodo_json_parse`.
/// `key_ptr` must point to `key_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_get_float(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
) -> f64 {
    if handle == 0 || key_ptr.is_null() {
        return 0.0;
    }
    // SAFETY: handle was returned by kodo_json_parse.
    let value = unsafe { &*(handle as *const serde_json::Value) };
    // SAFETY: key_ptr/key_len describe a valid UTF-8 string.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
}

/// Gets an array from a JSON object by key, returning it as a List handle.
///
/// Each array element is stored as a new JSON handle in the list.
/// Returns 0 if key not found or not an array.
///
/// # Safety
///
/// `handle` must be a valid handle returned by `kodo_json_parse`.
/// `key_ptr` must point to `key_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_get_array(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
) -> i64 {
    if handle == 0 || key_ptr.is_null() {
        return 0;
    }
    // SAFETY: handle was returned by kodo_json_parse.
    let value = unsafe { &*(handle as *const serde_json::Value) };
    // SAFETY: key_ptr/key_len describe a valid UTF-8 string.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    match value.get(key).and_then(|v| v.as_array()) {
        Some(arr) => {
            // Create a new list and push each element as a JSON handle.
            let list_ptr = crate::collections::kodo_list_new();
            for elem in arr {
                let elem_handle = Box::into_raw(Box::new(elem.clone())) as i64;
                // SAFETY: list_ptr was just created, elem_handle is valid.
                unsafe { crate::collections::kodo_list_push(list_ptr, elem_handle) };
            }
            list_ptr
        }
        None => 0,
    }
}

/// Frees a parsed JSON value previously returned by `kodo_json_parse`.
///
/// Does nothing if `handle` is 0 (null handle).
///
/// # Safety
///
/// `handle` must be a valid handle returned by `kodo_json_parse`, or 0.
/// After calling this function, the handle must not be used again.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_free(handle: i64) {
    if handle == 0 {
        return;
    }
    // SAFETY: caller guarantees handle was returned by kodo_json_parse
    // (i.e. Box::into_raw on a Box<serde_json::Value>).
    let _ = unsafe { Box::from_raw(handle as *mut serde_json::Value) };
}

// ---------------------------------------------------------------------------
// CLI builtins
// ---------------------------------------------------------------------------

/// Returns command-line arguments as a `List<String>` handle.
///
/// Each argument is pushed as a (ptr, len) pair packed into an i64 handle.
/// The first element is the binary name.
#[no_mangle]
pub extern "C" fn kodo_args() -> i64 {
    let list = crate::collections::kodo_list_new();
    for arg in std::env::args() {
        let (handle, len) = crate::memory::alloc_string(arg.as_bytes());
        #[allow(clippy::cast_possible_wrap)]
        let len_i64 = len as i64;
        // Store as heap-allocated [i64; 2] pair (ptr, len) — same format as
        // `kodo_string_split`, so `list_join` and other List<String> ops work.
        let pair = Box::new([handle, len_i64]);
        // SAFETY: intentionally leaks so caller manages via the list handle.
        let pair_ptr = Box::into_raw(pair) as i64;
        // SAFETY: list was just created and is valid.
        unsafe {
            crate::collections::kodo_list_push(list, pair_ptr);
        }
    }
    list
}

/// Reads a line from standard input.
///
/// Writes the line (with trailing newline stripped) to `out_ptr`/`out_len`.
///
/// # Safety
///
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_readln(out_ptr: *mut *mut u8, out_len: *mut usize) {
    if out_ptr.is_null() || out_len.is_null() {
        return;
    }
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    // Strip trailing newline.
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe { write_string_out_mut(&line, out_ptr, out_len) };
}

/// Exits the process with the given exit code.
#[no_mangle]
pub extern "C" fn kodo_exit(code: i64) {
    #[allow(clippy::cast_possible_truncation)]
    std::process::exit(code as i32);
}

// ---------------------------------------------------------------------------
// Extended file I/O builtins
// ---------------------------------------------------------------------------

/// Appends content to a file.
///
/// Returns 0 on success, 1 on error. On error, `out_ptr`/`out_len` contain the message.
///
/// # Safety
///
/// All pointers must be valid and point to valid UTF-8.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_file_append(
    path_ptr: *const u8,
    path_len: usize,
    content_ptr: *const u8,
    content_len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) -> i64 {
    use std::io::Write;
    // SAFETY: caller guarantees valid UTF-8 bytes.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let content_bytes = unsafe { std::slice::from_raw_parts(content_ptr, content_len) };
    let path_str = match std::str::from_utf8(path_bytes) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("invalid path: {e}");
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out(&msg, out_ptr, out_len) };
            return 1;
        }
    };
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path_str)
    {
        Ok(mut f) => match f.write_all(content_bytes) {
            Ok(()) => 0,
            Err(e) => {
                let msg = format!("{e}");
                // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
                unsafe { write_string_out(&msg, out_ptr, out_len) };
                1
            }
        },
        Err(e) => {
            let msg = format!("{e}");
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out(&msg, out_ptr, out_len) };
            1
        }
    }
}

/// Deletes a file at the given path.
///
/// Returns 0 on success, 1 on error.
///
/// # Safety
///
/// `path_ptr` must point to `path_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_file_delete(path_ptr: *const u8, path_len: usize) -> i64 {
    // SAFETY: caller guarantees valid UTF-8 bytes at path_ptr..path_ptr+path_len.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path_str) = std::str::from_utf8(path_bytes) else {
        return 1;
    };
    match std::fs::remove_file(path_str) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// Lists files in a directory, returning a `List<String>` handle.
///
/// Returns 0 if the path is invalid or not a directory.
///
/// # Safety
///
/// `path_ptr` must point to `path_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_dir_list(path_ptr: *const u8, path_len: usize) -> i64 {
    // SAFETY: caller guarantees valid UTF-8 bytes at path_ptr..path_ptr+path_len.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path_str) = std::str::from_utf8(path_bytes) else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(path_str) else {
        return 0;
    };
    let list = crate::collections::kodo_list_new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let (handle, len) = crate::memory::alloc_string(name.as_bytes());
        #[allow(clippy::cast_possible_wrap)]
        let len_i64 = len as i64;
        // Store as heap-allocated [i64; 2] pair — same format as string_split.
        let pair = Box::new([handle, len_i64]);
        // SAFETY: intentionally leaks so caller manages via the list handle.
        let pair_ptr = Box::into_raw(pair) as i64;
        // SAFETY: list was just created and is valid.
        unsafe {
            crate::collections::kodo_list_push(list, pair_ptr);
        }
    }
    list
}

/// Checks if a directory exists at the given path.
///
/// Returns 1 if it exists and is a directory, 0 otherwise.
///
/// # Safety
///
/// `path_ptr` must point to `path_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_dir_exists(path_ptr: *const u8, path_len: usize) -> i64 {
    // SAFETY: caller guarantees valid UTF-8 bytes at path_ptr..path_ptr+path_len.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path_str) = std::str::from_utf8(path_bytes) else {
        return 0;
    };
    let path = std::path::Path::new(path_str);
    i64::from(path.is_dir())
}

// ---------------------------------------------------------------------------
// JSON builder builtins
// ---------------------------------------------------------------------------

/// Creates a new empty JSON object and returns an opaque handle.
#[no_mangle]
pub extern "C" fn kodo_json_new_object() -> i64 {
    let obj = serde_json::Value::Object(serde_json::Map::new());
    let boxed = Box::new(obj);
    // SAFETY: intentionally leaks so caller manages via opaque handle.
    // Freed by `kodo_json_free`.
    Box::into_raw(boxed) as i64
}

/// Sets a string field on a JSON object.
///
/// # Safety
///
/// `handle` must be a valid handle from `kodo_json_parse` or `kodo_json_new_object`.
/// `key_ptr`/`key_len` and `val_ptr`/`val_len` must point to valid UTF-8.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_json_set_string(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
    val_ptr: *const u8,
    val_len: usize,
) {
    if handle == 0 || key_ptr.is_null() || val_ptr.is_null() {
        return;
    }
    // SAFETY: handle was returned by kodo_json_parse or kodo_json_new_object.
    let value = unsafe { &mut *(handle as *mut serde_json::Value) };
    // SAFETY: caller guarantees valid UTF-8 slices.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    let val =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(val_ptr, val_len)) };
    if let serde_json::Value::Object(map) = value {
        map.insert(key.to_string(), serde_json::Value::String(val.to_string()));
    }
}

/// Sets an integer field on a JSON object.
///
/// # Safety
///
/// `handle` must be a valid handle. `key_ptr`/`key_len` must point to valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_set_int(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
    int_value: i64,
) {
    if handle == 0 || key_ptr.is_null() {
        return;
    }
    // SAFETY: handle was returned by kodo_json_parse or kodo_json_new_object.
    let value = unsafe { &mut *(handle as *mut serde_json::Value) };
    // SAFETY: caller guarantees valid UTF-8 slice.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    if let serde_json::Value::Object(map) = value {
        map.insert(
            key.to_string(),
            serde_json::Value::Number(serde_json::Number::from(int_value)),
        );
    }
}

/// Sets a boolean field on a JSON object.
///
/// # Safety
///
/// `handle` must be a valid handle. `key_ptr`/`key_len` must point to valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_set_bool(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
    bool_value: i64,
) {
    if handle == 0 || key_ptr.is_null() {
        return;
    }
    // SAFETY: handle was returned by kodo_json_parse or kodo_json_new_object.
    let value = unsafe { &mut *(handle as *mut serde_json::Value) };
    // SAFETY: caller guarantees valid UTF-8 slice.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    if let serde_json::Value::Object(map) = value {
        map.insert(key.to_string(), serde_json::Value::Bool(bool_value != 0));
    }
}

/// Sets a float field on a JSON object.
///
/// NaN and Infinity values cannot be represented in JSON, so they are
/// silently ignored (the field is not inserted).
///
/// # Safety
///
/// `handle` must be a valid handle. `key_ptr`/`key_len` must point to valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_set_float(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
    float_value: f64,
) {
    if handle == 0 || key_ptr.is_null() {
        return;
    }
    // SAFETY: handle was returned by kodo_json_parse or kodo_json_new_object.
    let value = unsafe { &mut *(handle as *mut serde_json::Value) };
    // SAFETY: caller guarantees valid UTF-8 slice.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    if let serde_json::Value::Object(map) = value {
        // serde_json::Number::from_f64 returns None for NaN/Infinity.
        if let Some(num) = serde_json::Number::from_f64(float_value) {
            map.insert(key.to_string(), serde_json::Value::Number(num));
        }
    }
}

/// Gets a nested JSON object by key, returning a new handle to a clone of it.
///
/// Returns 0 if the key is not found or the value is not an object.
///
/// # Safety
///
/// `handle` must be a valid handle returned by `kodo_json_parse`.
/// `key_ptr` must point to `key_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_json_get_object(
    handle: i64,
    key_ptr: *const u8,
    key_len: usize,
) -> i64 {
    if handle == 0 || key_ptr.is_null() {
        return 0;
    }
    // SAFETY: handle was returned by kodo_json_parse.
    let value = unsafe { &*(handle as *const serde_json::Value) };
    // SAFETY: key_ptr/key_len describe a valid UTF-8 string.
    let key =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };
    match value.get(key) {
        Some(v @ serde_json::Value::Object(_)) => {
            let cloned = Box::new(v.clone());
            // SAFETY: intentionally leaks so caller manages via opaque handle.
            Box::into_raw(cloned) as i64
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_exists_works() {
        let existing = "Cargo.toml";
        let result = unsafe { kodo_file_exists(existing.as_ptr(), existing.len()) };
        assert_eq!(result, 1);
        let missing = "definitely_nonexistent_file_xyz.ko";
        let result = unsafe { kodo_file_exists(missing.as_ptr(), missing.len()) };
        assert_eq!(result, 0);
    }

    #[test]
    fn time_now_positive() {
        assert!(kodo_time_now() > 0);
    }

    #[test]
    fn time_now_ms_positive() {
        assert!(kodo_time_now_ms() > 0);
    }

    #[test]
    fn time_format_epoch() {
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut l: usize = 0;
        unsafe { kodo_time_format(0, &mut p, &mut l) };
        // SAFETY: p points to l valid bytes (RC-managed memory).
        let s = unsafe { std::slice::from_raw_parts(p, l) };
        assert_eq!(s, b"1970-01-01T00:00:00Z");
        crate::memory::kodo_rc_dec(p as i64);
    }

    #[test]
    fn time_elapsed_nonneg() {
        assert!(kodo_time_elapsed_ms(kodo_time_now_ms()) >= 0);
    }

    #[test]
    fn json_parse_valid() {
        let json = r#"{"name": "kodo", "version": 1}"#;
        let handle = unsafe { kodo_json_parse(json.as_ptr(), json.len()) };
        assert_ne!(handle, 0);
        unsafe { kodo_json_free(handle) };
    }

    #[test]
    fn json_parse_invalid() {
        let bad = "not json {{{";
        let handle = unsafe { kodo_json_parse(bad.as_ptr(), bad.len()) };
        assert_eq!(handle, 0);
    }

    #[test]
    fn json_get_int_works() {
        let json = r#"{"count": 42}"#;
        let handle = unsafe { kodo_json_parse(json.as_ptr(), json.len()) };
        let key = "count";
        let value = unsafe { kodo_json_get_int(handle, key.as_ptr(), key.len()) };
        assert_eq!(value, 42);
        unsafe { kodo_json_free(handle) };
    }

    #[test]
    fn json_set_float_works() {
        let handle = kodo_json_new_object();
        let key = "price";
        unsafe { kodo_json_set_float(handle, key.as_ptr(), key.len(), 3.14) };
        // Stringify and verify
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut l: usize = 0;
        unsafe { kodo_json_stringify(handle, &mut p, &mut l) };
        // SAFETY: p points to l valid bytes (RC-managed memory).
        let s = unsafe { std::slice::from_raw_parts(p, l) };
        assert!(s.windows(4).any(|w| w == b"3.14"));
        crate::memory::kodo_rc_dec(p as i64);
        unsafe { kodo_json_free(handle) };
    }

    #[test]
    fn json_set_float_nan() {
        let handle = kodo_json_new_object();
        let key = "bad";
        unsafe { kodo_json_set_float(handle, key.as_ptr(), key.len(), f64::NAN) };
        // NaN should not be inserted, so object should be empty.
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut l: usize = 0;
        unsafe { kodo_json_stringify(handle, &mut p, &mut l) };
        // SAFETY: p points to l valid bytes (RC-managed memory).
        let s = unsafe { std::slice::from_raw_parts(p, l) };
        assert_eq!(s, b"{}");
        crate::memory::kodo_rc_dec(p as i64);
        unsafe { kodo_json_free(handle) };
    }

    #[test]
    fn json_get_object_works() {
        let json = r#"{"a": {"b": 1}}"#;
        let handle = unsafe { kodo_json_parse(json.as_ptr(), json.len()) };
        assert_ne!(handle, 0);
        let key_a = "a";
        let nested = unsafe { kodo_json_get_object(handle, key_a.as_ptr(), key_a.len()) };
        assert_ne!(nested, 0);
        let key_b = "b";
        let val = unsafe { kodo_json_get_int(nested, key_b.as_ptr(), key_b.len()) };
        assert_eq!(val, 1);
        unsafe { kodo_json_free(nested) };
        unsafe { kodo_json_free(handle) };
    }

    #[test]
    fn json_get_object_missing_key() {
        let json = r#"{"a": 1}"#;
        let handle = unsafe { kodo_json_parse(json.as_ptr(), json.len()) };
        let key = "missing";
        let result = unsafe { kodo_json_get_object(handle, key.as_ptr(), key.len()) };
        assert_eq!(result, 0);
        unsafe { kodo_json_free(handle) };
    }

    #[test]
    fn env_roundtrip() {
        let (k, v) = ("KODO_IO_TEST", "hi");
        unsafe {
            kodo_env_set(k.as_ptr(), k.len(), v.as_ptr(), v.len());
        }
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut l: usize = 0;
        unsafe {
            kodo_env_get(k.as_ptr(), k.len(), &mut p, &mut l);
        }
        // SAFETY: p points to l valid bytes (RC-managed memory).
        let s = unsafe { std::slice::from_raw_parts(p, l) };
        assert_eq!(s, b"hi");
        crate::memory::kodo_rc_dec(p as i64);
    }
}
