//! HTTP server builtins for the Kōdo runtime.
//!
//! Provides FFI-callable functions for creating synchronous HTTP servers
//! using the `tiny_http` library. Servers and requests are managed via
//! opaque i64 handles.

use crate::helpers::write_string_out_mut;

/// Creates a new HTTP server listening on the given port.
///
/// Returns an opaque handle to the server, or 0 on failure.
#[no_mangle]
pub extern "C" fn kodo_http_server_new(port: i64) -> i64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let port_u16 = port as u16;
    let addr = format!("0.0.0.0:{port_u16}");
    match tiny_http::Server::http(&addr) {
        Ok(server) => {
            let boxed = Box::new(server);
            // SAFETY: intentionally leaks so caller manages via opaque handle.
            // Freed by `kodo_http_server_free`.
            Box::into_raw(boxed) as i64
        }
        Err(_) => 0,
    }
}

/// Blocks until a request is received on the server, returns request handle.
///
/// Returns 0 if the server is closed or an error occurs.
///
/// # Safety
///
/// `handle` must be a valid handle from `kodo_http_server_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_http_server_recv(handle: i64) -> i64 {
    if handle == 0 {
        return 0;
    }
    // SAFETY: caller guarantees handle was returned by kodo_http_server_new.
    let server = unsafe { &*(handle as *const tiny_http::Server) };
    match server.recv() {
        Ok(request) => {
            let boxed = Box::new(request);
            // SAFETY: intentionally leaks so caller manages via opaque handle.
            Box::into_raw(boxed) as i64
        }
        Err(_) => 0,
    }
}

/// Gets the HTTP method of a request as a string.
///
/// # Safety
///
/// `req` must be a valid handle from `kodo_http_server_recv`.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_http_request_method(
    req: i64,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) {
    if req == 0 || out_ptr.is_null() || out_len.is_null() {
        return;
    }
    // SAFETY: caller guarantees req was returned by kodo_http_server_recv.
    let request = unsafe { &*(req as *const tiny_http::Request) };
    let method = request.method().to_string();
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe { write_string_out_mut(&method, out_ptr, out_len) };
}

/// Gets the URL path of a request as a string.
///
/// # Safety
///
/// `req` must be a valid handle from `kodo_http_server_recv`.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_http_request_path(
    req: i64,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) {
    if req == 0 || out_ptr.is_null() || out_len.is_null() {
        return;
    }
    // SAFETY: caller guarantees req was returned by kodo_http_server_recv.
    let request = unsafe { &*(req as *const tiny_http::Request) };
    let path = request.url().to_string();
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe { write_string_out_mut(&path, out_ptr, out_len) };
}

/// Gets the request body as a string.
///
/// # Safety
///
/// `req` must be a valid handle from `kodo_http_server_recv`.
/// `out_ptr` and `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_http_request_body(
    req: i64,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) {
    if req == 0 || out_ptr.is_null() || out_len.is_null() {
        return;
    }
    // SAFETY: caller guarantees req was returned by kodo_http_server_recv.
    let request = unsafe { &mut *(req as *mut tiny_http::Request) };
    let mut body = String::new();
    let _ = std::io::Read::read_to_string(request.as_reader(), &mut body);
    // SAFETY: caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe { write_string_out_mut(&body, out_ptr, out_len) };
}

/// Sends an HTTP response to a request.
///
/// After calling this, the request handle is consumed and must not be used again.
///
/// # Safety
///
/// `req` must be a valid handle from `kodo_http_server_recv`.
/// `body_ptr` must point to `body_len` valid UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn kodo_http_respond(
    req: i64,
    status: i64,
    body_ptr: *const u8,
    body_len: usize,
) {
    if req == 0 {
        return;
    }
    // SAFETY: caller guarantees req was returned by kodo_http_server_recv.
    // We take ownership — this consumes the request.
    let request = unsafe { *Box::from_raw(req as *mut tiny_http::Request) };
    let body_bytes = if body_ptr.is_null() {
        &[] as &[u8]
    } else {
        // SAFETY: caller guarantees body_ptr/body_len form a valid slice.
        unsafe { std::slice::from_raw_parts(body_ptr, body_len) }
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let status_code = tiny_http::StatusCode(status as u16);
    let response =
        tiny_http::Response::from_data(body_bytes.to_vec()).with_status_code(status_code);
    let _ = request.respond(response);
}

/// Frees an HTTP server handle.
///
/// # Safety
///
/// `handle` must be a valid handle from `kodo_http_server_new`, or 0.
#[no_mangle]
pub unsafe extern "C" fn kodo_http_server_free(handle: i64) {
    if handle == 0 {
        return;
    }
    // SAFETY: caller guarantees handle was returned by kodo_http_server_new.
    let _ = unsafe { Box::from_raw(handle as *mut tiny_http::Server) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_new_and_free() {
        // Use port 0 to let the OS pick a free port.
        let handle = kodo_http_server_new(0);
        assert_ne!(handle, 0, "server should be created");
        unsafe { kodo_http_server_free(handle) };
    }

    #[test]
    fn server_new_returns_nonzero() {
        let handle = kodo_http_server_new(0);
        assert_ne!(handle, 0);
        unsafe { kodo_http_server_free(handle) };
    }
}
