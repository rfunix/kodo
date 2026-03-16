//! Runtime support for the Kōdo test framework.
//!
//! Provides assertion builtins and test lifecycle functions that compiled
//! test binaries call at runtime. Assertion failures set a global flag
//! instead of aborting, allowing the test harness to report all failures
//! in a test before moving to the next one.

use std::io::Write;

/// Global flag indicating whether the current test has any assertion failure.
///
/// This is intentionally a `static mut` rather than an `AtomicBool` because
/// compiled Kōdo test binaries are single-threaded and we need to minimize
/// overhead in the hot assertion path.
static mut TEST_FAILED: bool = false;

/// Asserts that a boolean condition is true.
///
/// If `cond` is 0 (false), prints "assertion failed" to stderr and sets
/// the test failure flag.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
/// Accesses the global `TEST_FAILED` flag without synchronization.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert(cond: i64) {
    if cond == 0 {
        let _ = writeln!(std::io::stderr(), "assertion failed");
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that a boolean condition is true (alias for [`kodo_assert`]).
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
/// Accesses the global `TEST_FAILED` flag without synchronization.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert_true(cond: i64) {
    if cond == 0 {
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: expected true, got false"
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that a boolean condition is false.
///
/// If `cond` is non-zero (true), prints an error message and sets the
/// test failure flag.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
/// Accesses the global `TEST_FAILED` flag without synchronization.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert_false(cond: i64) {
    if cond != 0 {
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: expected false, got true"
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that two integers are equal.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert_eq_int(left: i64, right: i64) {
    if left != right {
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: assert_eq\n  left:  {left}\n  right: {right}"
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that two strings are equal.
///
/// Strings are passed as `(pointer, length)` pairs.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 byte slices.
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_assert_eq_string(left_slot: i64, right_slot: i64) {
    // SAFETY: Caller passes pointers to 16-byte string slots: [ptr: i64, len: i64].
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let left = unsafe {
        let slot = left_slot as *const i64;
        let ptr = *slot as *const u8;
        let len = *slot.add(1) as usize;
        std::slice::from_raw_parts(ptr, len)
    };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let right = unsafe {
        let slot = right_slot as *const i64;
        let ptr = *slot as *const u8;
        let len = *slot.add(1) as usize;
        std::slice::from_raw_parts(ptr, len)
    };
    if left != right {
        let left_s = std::str::from_utf8(left).unwrap_or("<invalid utf-8>");
        let right_s = std::str::from_utf8(right).unwrap_or("<invalid utf-8>");
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: assert_eq\n  left:  \"{left_s}\"\n  right: \"{right_s}\""
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that two booleans are equal.
///
/// Booleans are represented as `i64` where 0 is false and non-zero is true.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert_eq_bool(left: i64, right: i64) {
    let left_bool = left != 0;
    let right_bool = right != 0;
    if left_bool != right_bool {
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: assert_eq\n  left:  {left_bool}\n  right: {right_bool}"
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that two floats are equal.
///
/// Uses exact bit-level equality comparison (no epsilon). For approximate
/// comparison, use a tolerance-based assertion when available.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert_eq_float(left: f64, right: f64) {
    #[allow(clippy::float_cmp)]
    if left != right {
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: assert_eq\n  left:  {left}\n  right: {right}"
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that two integers are not equal.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert_ne_int(left: i64, right: i64) {
    if left == right {
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: assert_ne\n  both values: {left}"
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that two strings are not equal.
///
/// Strings are passed as `(pointer, length)` pairs.
///
/// # Safety
///
/// Both pointer/length pairs must point to valid UTF-8 byte slices.
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
#[allow(clippy::similar_names)]
pub unsafe extern "C" fn kodo_assert_ne_string(left_slot: i64, right_slot: i64) {
    // SAFETY: Caller passes pointers to 16-byte string slots: [ptr: i64, len: i64].
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let left = unsafe {
        let slot = left_slot as *const i64;
        let ptr = *slot as *const u8;
        let len = *slot.add(1) as usize;
        std::slice::from_raw_parts(ptr, len)
    };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let right = unsafe {
        let slot = right_slot as *const i64;
        let ptr = *slot as *const u8;
        let len = *slot.add(1) as usize;
        std::slice::from_raw_parts(ptr, len)
    };
    if left == right {
        let left_s = std::str::from_utf8(left).unwrap_or("<invalid utf-8>");
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: assert_ne\n  both values: \"{left_s}\""
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that two booleans are not equal.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert_ne_bool(left: i64, right: i64) {
    let left_bool = left != 0;
    let right_bool = right != 0;
    if left_bool == right_bool {
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: assert_ne\n  both values: {left_bool}"
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Asserts that two floats are not equal.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_assert_ne_float(left: f64, right: f64) {
    #[allow(clippy::float_cmp)]
    if left == right {
        let _ = writeln!(
            std::io::stderr(),
            "assertion failed: assert_ne\n  both values: {left}"
        );
        // SAFETY: Single-threaded access from compiled test binary.
        unsafe {
            TEST_FAILED = true;
        }
    }
}

/// Marks the start of a test case.
///
/// Prints "test {name} ... " (without newline) and resets the failure flag.
///
/// # Safety
///
/// `name_ptr` must point to `name_len` valid UTF-8 bytes.
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_test_start(name_slot: i64) {
    // SAFETY: Single-threaded access from compiled test binary.
    unsafe {
        TEST_FAILED = false;
    }
    // SAFETY: Caller passes a pointer to a 16-byte string slot: [ptr: i64, len: i64].
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let name_bytes = unsafe {
        let slot = name_slot as *const i64;
        let ptr = *slot as *const u8;
        let len = *slot.add(1) as usize;
        std::slice::from_raw_parts(ptr, len)
    };
    let name = std::str::from_utf8(name_bytes).unwrap_or("<invalid utf-8>");
    let _ = write!(std::io::stdout(), "test {name} ... ");
    let _ = std::io::stdout().flush();
}

/// Marks the end of a test case.
///
/// Prints "ok" or "FAILED" depending on whether any assertion failed,
/// then returns 0 (pass) or 1 (fail).
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_test_end() -> i64 {
    // SAFETY: Single-threaded access from compiled test binary.
    let failed = unsafe { TEST_FAILED };
    if failed {
        let _ = writeln!(std::io::stdout(), "FAILED");
    } else {
        let _ = writeln!(std::io::stdout(), "ok");
    }
    i64::from(failed)
}

/// Prints a test summary line.
///
/// Outputs a summary like "test result: ok. 5 passed; 0 failed" or
/// "test result: FAILED. 3 passed; 2 failed".
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub extern "C" fn kodo_test_summary(total: i64, passed: i64, failed: i64) {
    let _ = writeln!(std::io::stdout());
    let status = if failed > 0 { "FAILED" } else { "ok" };
    let _ = writeln!(
        std::io::stdout(),
        "test result: {status}. {passed} passed; {failed} failed; {total} total"
    );
}
