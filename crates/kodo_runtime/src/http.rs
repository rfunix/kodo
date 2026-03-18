//! HTTP client builtins for the Kōdo runtime.
//!
//! Provides FFI-callable functions for HTTP GET and POST requests
//! using the `ureq` library.

use crate::helpers::write_string_out_mut;

/// Performs an HTTP GET request to the given URL.
///
/// On success, writes the response body to `out_ptr`/`out_len` and returns 0.
/// On error, writes the error message to `out_ptr`/`out_len` and returns 1.
/// The caller must free the output string with `kodo_string_free`.
///
/// # Safety
///
/// `url_ptr` must point to `url_len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_http_get(
    url_ptr: *const u8,
    url_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i64 {
    if url_ptr.is_null() || out_ptr.is_null() || out_len.is_null() {
        return 1;
    }
    // SAFETY: caller guarantees url_ptr/url_len form a valid UTF-8 slice.
    let url_bytes = unsafe { std::slice::from_raw_parts(url_ptr, url_len) };
    let Ok(url) = std::str::from_utf8(url_bytes) else {
        // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe { write_string_out_mut("invalid URL: not UTF-8", out_ptr, out_len) };
        return 1;
    };
    match ureq::get(url).call() {
        Ok(response) => match response.into_body().read_to_string() {
            Ok(body) => {
                // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
                unsafe { write_string_out_mut(&body, out_ptr, out_len) };
                0
            }
            Err(e) => {
                let msg = format!("failed to read response body: {e}");
                // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
                unsafe { write_string_out_mut(&msg, out_ptr, out_len) };
                1
            }
        },
        Err(e) => {
            let msg = format!("HTTP GET failed: {e}");
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out_mut(&msg, out_ptr, out_len) };
            1
        }
    }
}

/// Performs an HTTP POST request to the given URL with the provided body.
///
/// On success, writes the response body to `out_ptr`/`out_len` and returns 0.
/// On error, writes the error message to `out_ptr`/`out_len` and returns 1.
/// The caller must free the output string with `kodo_string_free`.
///
/// # Safety
///
/// `url_ptr` must point to `url_len` valid UTF-8 bytes.
/// `body_ptr` must point to `body_len` valid UTF-8 bytes.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_http_post(
    url_ptr: *const u8,
    url_len: usize,
    body_ptr: *const u8,
    body_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i64 {
    if url_ptr.is_null() || body_ptr.is_null() || out_ptr.is_null() || out_len.is_null() {
        return 1;
    }
    // SAFETY: caller guarantees valid UTF-8 slices.
    let url_bytes = unsafe { std::slice::from_raw_parts(url_ptr, url_len) };
    let post_body = unsafe { std::slice::from_raw_parts(body_ptr, body_len) };
    let Ok(url) = std::str::from_utf8(url_bytes) else {
        // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe { write_string_out_mut("invalid URL: not UTF-8", out_ptr, out_len) };
        return 1;
    };
    let Ok(content) = std::str::from_utf8(post_body) else {
        // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
        unsafe { write_string_out_mut("invalid body: not UTF-8", out_ptr, out_len) };
        return 1;
    };
    match ureq::post(url)
        .header("Content-Type", "application/json")
        .send(content)
    {
        Ok(response) => match response.into_body().read_to_string() {
            Ok(resp_body) => {
                // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
                unsafe { write_string_out_mut(&resp_body, out_ptr, out_len) };
                0
            }
            Err(e) => {
                let msg = format!("failed to read response body: {e}");
                // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
                unsafe { write_string_out_mut(&msg, out_ptr, out_len) };
                1
            }
        },
        Err(e) => {
            let msg = format!("HTTP POST failed: {e}");
            // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
            unsafe { write_string_out_mut(&msg, out_ptr, out_len) };
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string_ops::kodo_string_free;

    #[test]
    fn http_get_invalid_url_returns_error() {
        let url = "not-a-valid-url";
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let status = unsafe { kodo_http_get(url.as_ptr(), url.len(), &mut out_ptr, &mut out_len) };
        assert_eq!(status, 1);
        assert!(!out_ptr.is_null());
        unsafe { kodo_string_free(out_ptr, out_len) };
    }

    #[test]
    fn http_get_null_ptr_returns_error() {
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let status = unsafe { kodo_http_get(std::ptr::null(), 0, &mut out_ptr, &mut out_len) };
        assert_eq!(status, 1);
    }

    #[test]
    fn http_post_null_ptr_returns_error() {
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let status = unsafe {
            kodo_http_post(
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &mut out_ptr,
                &mut out_len,
            )
        };
        assert_eq!(status, 1);
    }
}
