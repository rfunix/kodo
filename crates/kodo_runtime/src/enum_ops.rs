//! Runtime support for `Option` and `Result` enum operations.
//!
//! These functions are called by the LLVM backend for synthetic enum methods
//! like `unwrap()`, `is_ok()`, etc. The Cranelift backend handles these inline,
//! but the LLVM backend emits them as external function calls.
//!
//! ## Enum layout
//!
//! All Kōdo enums use a two-word layout: `[discriminant: i64, payload: i64]`.
//!
//! - `Ok` / `Some` have discriminant **0**, with the inner value at offset 8.
//! - `Err` / `None` have discriminant **!= 0**, with the error/unit at offset 8.
//!
//! The LLVM backend passes a **pointer** (as `i64`) to the enum's stack slot.
//! Each function casts this `i64` back to `*const i64` to read the fields.

use std::io::Write;

/// Returns 1 if the `Result` discriminant is 0 (`Ok`), else 0.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_result_is_ok(enum_ptr: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    i64::from(disc == 0)
}

/// Returns 1 if the `Result` discriminant is non-zero (`Err`), else 0.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_result_is_err(enum_ptr: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    i64::from(disc != 0)
}

/// Extracts the `Ok` payload (offset 8). Aborts if discriminant is `Err`.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_result_unwrap(enum_ptr: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    if disc != 0 {
        let _ = writeln!(std::io::stderr(), "called unwrap() on Err value");
        std::process::abort();
    }
    // SAFETY: offset 1 is within the 16-byte allocation.
    unsafe { *ptr.add(1) }
}

/// Extracts the `Err` payload (offset 8). Aborts if discriminant is `Ok`.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_result_unwrap_err(enum_ptr: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    if disc == 0 {
        let _ = writeln!(std::io::stderr(), "called unwrap_err() on Ok value");
        std::process::abort();
    }
    // SAFETY: offset 1 is within the 16-byte allocation.
    unsafe { *ptr.add(1) }
}

/// Extracts the `Ok` payload, or returns `default` if `Err`.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_result_unwrap_or(enum_ptr: i64, default: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    if disc == 0 {
        // SAFETY: offset 1 is within the 16-byte allocation.
        unsafe { *ptr.add(1) }
    } else {
        default
    }
}

/// Returns 1 if the `Option` discriminant is 0 (`Some`), else 0.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_option_is_some(enum_ptr: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    i64::from(disc == 0)
}

/// Returns 1 if the `Option` discriminant is non-zero (`None`), else 0.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_option_is_none(enum_ptr: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    i64::from(disc != 0)
}

/// Extracts the `Some` payload (offset 8). Aborts if `None`.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_option_unwrap(enum_ptr: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    if disc != 0 {
        let _ = writeln!(std::io::stderr(), "called unwrap() on None value");
        std::process::abort();
    }
    // SAFETY: offset 1 is within the 16-byte allocation.
    unsafe { *ptr.add(1) }
}

/// Extracts the `Some` payload, or returns `default` if `None`.
///
/// # Safety
///
/// `enum_ptr` must be a valid pointer (cast to `i64`) to a 16-byte enum slot.
#[no_mangle]
pub unsafe extern "C" fn kodo_option_unwrap_or(enum_ptr: i64, default: i64) -> i64 {
    // SAFETY: caller guarantees enum_ptr points to a valid [i64; 2] slot.
    let ptr = enum_ptr as *const i64;
    let disc = unsafe { *ptr };
    if disc == 0 {
        // SAFETY: offset 1 is within the 16-byte allocation.
        unsafe { *ptr.add(1) }
    } else {
        default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create an Ok/Some enum slot: discriminant 0, payload = value.
    fn ok_slot(value: i64) -> [i64; 2] {
        [0, value]
    }

    /// Helper to create an Err/None enum slot: discriminant 1, payload = value.
    fn err_slot(value: i64) -> [i64; 2] {
        [1, value]
    }

    // -- Result is_ok / is_err --

    #[test]
    fn result_is_ok_returns_1_for_ok() {
        let slot = ok_slot(42);
        let result = unsafe { kodo_result_is_ok(slot.as_ptr() as i64) };
        assert_eq!(result, 1);
    }

    #[test]
    fn result_is_ok_returns_0_for_err() {
        let slot = err_slot(99);
        let result = unsafe { kodo_result_is_ok(slot.as_ptr() as i64) };
        assert_eq!(result, 0);
    }

    #[test]
    fn result_is_err_returns_1_for_err() {
        let slot = err_slot(99);
        let result = unsafe { kodo_result_is_err(slot.as_ptr() as i64) };
        assert_eq!(result, 1);
    }

    #[test]
    fn result_is_err_returns_0_for_ok() {
        let slot = ok_slot(42);
        let result = unsafe { kodo_result_is_err(slot.as_ptr() as i64) };
        assert_eq!(result, 0);
    }

    // -- Result unwrap --

    #[test]
    fn result_unwrap_extracts_ok_value() {
        let slot = ok_slot(42);
        let value = unsafe { kodo_result_unwrap(slot.as_ptr() as i64) };
        assert_eq!(value, 42);
    }

    // -- Result unwrap_err --

    #[test]
    fn result_unwrap_err_extracts_err_value() {
        let slot = err_slot(99);
        let value = unsafe { kodo_result_unwrap_err(slot.as_ptr() as i64) };
        assert_eq!(value, 99);
    }

    // -- Result unwrap_or --

    #[test]
    fn result_unwrap_or_returns_ok_value() {
        let slot = ok_slot(42);
        let value = unsafe { kodo_result_unwrap_or(slot.as_ptr() as i64, 0) };
        assert_eq!(value, 42);
    }

    #[test]
    fn result_unwrap_or_returns_default_on_err() {
        let slot = err_slot(99);
        let value = unsafe { kodo_result_unwrap_or(slot.as_ptr() as i64, 7) };
        assert_eq!(value, 7);
    }

    // -- Option is_some / is_none --

    #[test]
    fn option_is_some_returns_1_for_some() {
        let slot = ok_slot(42);
        let result = unsafe { kodo_option_is_some(slot.as_ptr() as i64) };
        assert_eq!(result, 1);
    }

    #[test]
    fn option_is_some_returns_0_for_none() {
        let slot = err_slot(0);
        let result = unsafe { kodo_option_is_some(slot.as_ptr() as i64) };
        assert_eq!(result, 0);
    }

    #[test]
    fn option_is_none_returns_1_for_none() {
        let slot = err_slot(0);
        let result = unsafe { kodo_option_is_none(slot.as_ptr() as i64) };
        assert_eq!(result, 1);
    }

    #[test]
    fn option_is_none_returns_0_for_some() {
        let slot = ok_slot(42);
        let result = unsafe { kodo_option_is_none(slot.as_ptr() as i64) };
        assert_eq!(result, 0);
    }

    // -- Option unwrap --

    #[test]
    fn option_unwrap_extracts_some_value() {
        let slot = ok_slot(42);
        let value = unsafe { kodo_option_unwrap(slot.as_ptr() as i64) };
        assert_eq!(value, 42);
    }

    // -- Option unwrap_or --

    #[test]
    fn option_unwrap_or_returns_some_value() {
        let slot = ok_slot(42);
        let value = unsafe { kodo_option_unwrap_or(slot.as_ptr() as i64, 0) };
        assert_eq!(value, 42);
    }

    #[test]
    fn option_unwrap_or_returns_default_on_none() {
        let slot = err_slot(0);
        let value = unsafe { kodo_option_unwrap_or(slot.as_ptr() as i64, 7) };
        assert_eq!(value, 7);
    }

    // -- Edge cases --

    #[test]
    fn result_unwrap_zero_payload() {
        let slot = ok_slot(0);
        let value = unsafe { kodo_result_unwrap(slot.as_ptr() as i64) };
        assert_eq!(value, 0);
    }

    #[test]
    fn result_unwrap_negative_payload() {
        let slot = ok_slot(-100);
        let value = unsafe { kodo_result_unwrap(slot.as_ptr() as i64) };
        assert_eq!(value, -100);
    }

    #[test]
    fn option_unwrap_max_value() {
        let slot = ok_slot(i64::MAX);
        let value = unsafe { kodo_option_unwrap(slot.as_ptr() as i64) };
        assert_eq!(value, i64::MAX);
    }
}
