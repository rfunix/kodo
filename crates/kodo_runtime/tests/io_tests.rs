//! I/O tests for the Kōdo runtime FFI functions.
//!
//! Exercises file read/write/delete/exists and directory-exists through
//! their `extern "C"` interface, using temporary files for isolation.

use kodo_runtime::io_ops::{
    kodo_dir_exists, kodo_file_delete, kodo_file_exists, kodo_file_read, kodo_file_write,
};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Converts a `&str` to the `(*const u8, usize)` pair expected by the FFI.
fn str_to_parts(s: &str) -> (*const u8, usize) {
    (s.as_ptr(), s.len())
}

/// Returns a unique temporary file path (does not create the file).
fn temp_file_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("kodo_test_{}_{}", std::process::id(), name));
    p
}

/// Reads the string returned via FFI out-parameters.
///
/// # Safety
///
/// `ptr` must point to `len` valid bytes that were heap-allocated by the runtime.
unsafe fn read_out_string(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    // SAFETY: caller guarantees ptr/len describe a valid, heap-allocated byte slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

// ===========================================================================
// file_write + file_read cycle
// ===========================================================================

#[test]
fn file_write_and_read_roundtrip() {
    let path = temp_file_path("roundtrip.txt");
    let path_str = path.to_str().unwrap();
    let (path_ptr, path_len) = str_to_parts(path_str);

    let content = "Hello from Kodo runtime tests!";
    let (content_ptr, content_len) = str_to_parts(content);

    let mut out_ptr: *const u8 = std::ptr::null();
    let mut out_len: usize = 0;

    // SAFETY: all pointers are valid stack-allocated variables or string slices.
    unsafe {
        let write_rc = kodo_file_write(
            path_ptr,
            path_len,
            content_ptr,
            content_len,
            &mut out_ptr,
            &mut out_len,
        );
        assert_eq!(write_rc, 0, "file_write should succeed");

        let mut read_ptr: *const u8 = std::ptr::null();
        let mut read_len: usize = 0;
        let read_rc = kodo_file_read(path_ptr, path_len, &mut read_ptr, &mut read_len);
        assert_eq!(read_rc, 0, "file_read should succeed");

        let read_content = read_out_string(read_ptr, read_len);
        assert_eq!(read_content, content);
    }

    // Cleanup.
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_read_nonexistent() {
    let path = "/tmp/kodo_definitely_does_not_exist_xyz.txt";
    let (path_ptr, path_len) = str_to_parts(path);

    let mut out_ptr: *const u8 = std::ptr::null();
    let mut out_len: usize = 0;

    // SAFETY: all pointers are valid stack-allocated variables or string slices.
    unsafe {
        let rc = kodo_file_read(path_ptr, path_len, &mut out_ptr, &mut out_len);
        assert_eq!(rc, 1, "file_read on nonexistent file should return error");
    }
}

// ===========================================================================
// file_exists
// ===========================================================================

#[test]
fn file_exists_for_existing_file() {
    let path = temp_file_path("exists_test.txt");
    std::fs::write(&path, "test").unwrap();

    let path_str = path.to_str().unwrap();
    let (ptr, len) = str_to_parts(path_str);

    // SAFETY: ptr/len describe a valid UTF-8 string slice.
    let result = unsafe { kodo_file_exists(ptr, len) };
    assert_eq!(result, 1);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_exists_for_nonexistent_file() {
    let path = "/tmp/kodo_nonexistent_file_abc123.txt";
    let (ptr, len) = str_to_parts(path);

    // SAFETY: ptr/len describe a valid UTF-8 string slice.
    let result = unsafe { kodo_file_exists(ptr, len) };
    assert_eq!(result, 0);
}

// ===========================================================================
// file_delete
// ===========================================================================

#[test]
fn file_delete_existing_file() {
    let path = temp_file_path("delete_test.txt");
    std::fs::write(&path, "delete me").unwrap();

    let path_str = path.to_str().unwrap();
    let (ptr, len) = str_to_parts(path_str);

    // SAFETY: ptr/len describe a valid UTF-8 string slice.
    unsafe {
        let rc = kodo_file_delete(ptr, len);
        assert_eq!(rc, 0, "file_delete should succeed on an existing file");
    }

    assert!(!path.exists(), "file should no longer exist after deletion");
}

#[test]
fn file_delete_nonexistent_file() {
    let path = "/tmp/kodo_nonexistent_delete_xyz.txt";
    let (ptr, len) = str_to_parts(path);

    // SAFETY: ptr/len describe a valid UTF-8 string slice.
    unsafe {
        let rc = kodo_file_delete(ptr, len);
        assert_eq!(rc, 1, "file_delete on nonexistent file should return error");
    }
}

// ===========================================================================
// dir_exists
// ===========================================================================

#[test]
fn dir_exists_for_valid_directory() {
    let path = std::env::temp_dir();
    let path_str = path.to_str().unwrap();
    let (ptr, len) = str_to_parts(path_str);

    // SAFETY: ptr/len describe a valid UTF-8 string slice.
    let result = unsafe { kodo_dir_exists(ptr, len) };
    assert_eq!(result, 1);
}

#[test]
fn dir_exists_for_nonexistent_directory() {
    let path = "/tmp/kodo_nonexistent_dir_xyz_123";
    let (ptr, len) = str_to_parts(path);

    // SAFETY: ptr/len describe a valid UTF-8 string slice.
    let result = unsafe { kodo_dir_exists(ptr, len) };
    assert_eq!(result, 0);
}

#[test]
fn dir_exists_for_file_not_dir() {
    let path = temp_file_path("not_a_dir.txt");
    std::fs::write(&path, "i am a file").unwrap();

    let path_str = path.to_str().unwrap();
    let (ptr, len) = str_to_parts(path_str);

    // SAFETY: ptr/len describe a valid UTF-8 string slice.
    let result = unsafe { kodo_dir_exists(ptr, len) };
    assert_eq!(result, 0, "a file should not be reported as a directory");

    let _ = std::fs::remove_file(&path);
}
