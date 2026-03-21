//! Runtime support for the Kōdo test framework.
//!
//! Provides assertion builtins and test lifecycle functions that compiled
//! test binaries call at runtime. Assertion failures set a global flag
//! instead of aborting, allowing the test harness to report all failures
//! in a test before moving to the next one.
//!
//! ## Timeout support
//!
//! [`kodo_test_set_timeout`] spawns a timer thread that terminates the process
//! if the current test does not call [`kodo_test_clear_timeout`] within the
//! allotted time. This prevents runaway tests from blocking the test suite.
//!
//! ## Isolation stubs
//!
//! [`kodo_test_isolate_start`] and [`kodo_test_isolate_end`] are currently
//! no-ops reserved for future state-snapshotting support.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// Atomic flag indicating whether a timeout is currently active for the
/// running test. Shared between the test thread and the timer thread.
static TIMEOUT_ACTIVE: AtomicBool = AtomicBool::new(false);

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

/// Marks a test as skipped.
///
/// Prints "skipped" on the same line that [`kodo_test_start`] opened,
/// then resets the failure flag (so the skip is not counted as a failure).
///
/// # Safety
///
/// `name_slot` is unused but kept for ABI symmetry with [`kodo_test_start`].
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub extern "C" fn kodo_test_skip() {
    let _ = writeln!(std::io::stdout(), "skipped");
}

/// Prints a test summary line.
///
/// Outputs a summary like "test result: ok. 5 passed; 0 failed; 0 skipped; 0 todo" or
/// "test result: FAILED. 3 passed; 2 failed; 1 skipped; 0 todo".
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub extern "C" fn kodo_test_summary(total: i64, passed: i64, failed: i64, skipped: i64, todo: i64) {
    let _ = writeln!(std::io::stdout());
    let status = if failed > 0 { "FAILED" } else { "ok" };
    let _ = writeln!(
        std::io::stdout(),
        "test result: {status}. {passed} passed; {failed} failed; {skipped} skipped; {todo} todo; {total} total"
    );
}

/// Sets a timeout for the current test in milliseconds.
///
/// Spawns a background timer thread. If [`kodo_test_clear_timeout`] is not
/// called before the duration elapses, the process exits with code 1 and
/// prints a diagnostic to stderr.
///
/// Calling this function while a previous timeout is still active replaces
/// it: the old timer thread will find `TIMEOUT_ACTIVE` still `true`, but only
/// the first one to observe the flag set will ever fire (both see `true`).
/// For deterministic behavior, always call [`kodo_test_clear_timeout`] before
/// setting a new timeout.
///
/// # Safety
///
/// May be called from any thread. Uses `AtomicBool` for synchronization.
#[no_mangle]
pub unsafe extern "C" fn kodo_test_set_timeout(ms: i64) {
    TIMEOUT_ACTIVE.store(true, Ordering::SeqCst);
    #[allow(clippy::cast_sign_loss)]
    let duration = Duration::from_millis(ms as u64);
    thread::spawn(move || {
        thread::sleep(duration);
        if TIMEOUT_ACTIVE.load(Ordering::SeqCst) {
            let _ = writeln!(std::io::stderr(), "test timeout: exceeded {ms}ms");
            std::process::exit(1);
        }
    });
}

/// Clears the current test timeout, preventing the timer thread from firing.
///
/// Call this at the end of any test that used [`kodo_test_set_timeout`].
///
/// # Safety
///
/// May be called from any thread. Uses `AtomicBool` for synchronization.
#[no_mangle]
pub unsafe extern "C" fn kodo_test_clear_timeout() {
    TIMEOUT_ACTIVE.store(false, Ordering::SeqCst);
}

/// Marks the start of test isolation.
///
/// Currently a no-op. Future versions will snapshot global state so that
/// mutations made during a test can be rolled back by [`kodo_test_isolate_end`].
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_test_isolate_start() {
    // Placeholder: state snapshotting will be implemented here.
}

/// Marks the end of test isolation.
///
/// Currently a no-op. Future versions will restore the global state that was
/// snapshotted by [`kodo_test_isolate_start`].
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_test_isolate_end() {
    // Placeholder: state restoration will be implemented here.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that read/write the global `TEST_FAILED` flag,
    /// preventing race conditions when cargo runs tests in parallel.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Helper: reset the global TEST_FAILED flag before each test that
    /// inspects it, to avoid cross-test contamination.
    ///
    /// # Safety
    ///
    /// Must only be called while holding `TEST_LOCK`.
    unsafe fn reset_test_failed() {
        unsafe {
            TEST_FAILED = false;
        }
    }

    // -- kodo_assert --

    #[test]
    fn assert_passes_on_nonzero() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert(1);
            assert!(!TEST_FAILED, "assert(1) should not set failure flag");
        }
    }

    #[test]
    fn assert_fails_on_zero() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert(0);
            assert!(TEST_FAILED, "assert(0) should set failure flag");
        }
    }

    // -- kodo_assert_true --

    #[test]
    fn assert_true_passes_on_nonzero() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_true(1);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_true_fails_on_zero() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_true(0);
            assert!(TEST_FAILED);
        }
    }

    // -- kodo_assert_false --

    #[test]
    fn assert_false_passes_on_zero() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_false(0);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_false_fails_on_nonzero() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_false(1);
            assert!(TEST_FAILED);
        }
    }

    // -- kodo_assert_eq_int --

    #[test]
    fn assert_eq_int_passes_on_equal() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_int(42, 42);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_eq_int_fails_on_different() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_int(42, 99);
            assert!(TEST_FAILED);
        }
    }

    #[test]
    fn assert_eq_int_negative_values() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_int(-10, -10);
            assert!(!TEST_FAILED);
        }
    }

    // -- kodo_assert_eq_bool --

    #[test]
    fn assert_eq_bool_passes_both_true() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_bool(1, 1);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_eq_bool_passes_both_false() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_bool(0, 0);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_eq_bool_fails_true_vs_false() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_bool(1, 0);
            assert!(TEST_FAILED);
        }
    }

    #[test]
    fn assert_eq_bool_nonzero_is_true() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            // Any non-zero value is truthy, so 5 == 1 as booleans.
            kodo_assert_eq_bool(5, 1);
            assert!(!TEST_FAILED);
        }
    }

    // -- kodo_assert_eq_float --

    #[test]
    fn assert_eq_float_passes_on_equal() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_float(3.14, 3.14);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_eq_float_fails_on_different() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_float(3.14, 2.71);
            assert!(TEST_FAILED);
        }
    }

    // -- kodo_assert_ne_int --

    #[test]
    fn assert_ne_int_passes_on_different() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_ne_int(1, 2);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_ne_int_fails_on_equal() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_ne_int(5, 5);
            assert!(TEST_FAILED);
        }
    }

    // -- kodo_assert_ne_bool --

    #[test]
    fn assert_ne_bool_passes_on_different() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_ne_bool(1, 0);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_ne_bool_fails_on_same() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_ne_bool(1, 1);
            assert!(TEST_FAILED);
        }
    }

    // -- kodo_assert_ne_float --

    #[test]
    fn assert_ne_float_passes_on_different() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_ne_float(1.0, 2.0);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_ne_float_fails_on_equal() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_ne_float(1.0, 1.0);
            assert!(TEST_FAILED);
        }
    }

    // -- kodo_assert_eq_string / kodo_assert_ne_string --

    #[test]
    fn assert_eq_string_passes_on_equal() {
        let a = "hello";
        let b = "hello";
        // Build 16-byte string slots: [ptr: i64, len: i64].
        let slot_a: [i64; 2] = [a.as_ptr() as i64, a.len() as i64];
        let slot_b: [i64; 2] = [b.as_ptr() as i64, b.len() as i64];
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_string(slot_a.as_ptr() as i64, slot_b.as_ptr() as i64);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_eq_string_fails_on_different() {
        let a = "hello";
        let b = "world";
        let slot_a: [i64; 2] = [a.as_ptr() as i64, a.len() as i64];
        let slot_b: [i64; 2] = [b.as_ptr() as i64, b.len() as i64];
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_eq_string(slot_a.as_ptr() as i64, slot_b.as_ptr() as i64);
            assert!(TEST_FAILED);
        }
    }

    #[test]
    fn assert_ne_string_passes_on_different() {
        let a = "foo";
        let b = "bar";
        let slot_a: [i64; 2] = [a.as_ptr() as i64, a.len() as i64];
        let slot_b: [i64; 2] = [b.as_ptr() as i64, b.len() as i64];
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_ne_string(slot_a.as_ptr() as i64, slot_b.as_ptr() as i64);
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn assert_ne_string_fails_on_equal() {
        let a = "same";
        let b = "same";
        let slot_a: [i64; 2] = [a.as_ptr() as i64, a.len() as i64];
        let slot_b: [i64; 2] = [b.as_ptr() as i64, b.len() as i64];
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert_ne_string(slot_a.as_ptr() as i64, slot_b.as_ptr() as i64);
            assert!(TEST_FAILED);
        }
    }

    // -- kodo_test_end --
    // Note: kodo_test_end and kodo_test_summary print to stdout using
    // the "test result:" format, which confuses the Rust test runner's
    // output parser. We test the underlying flag logic instead.

    #[test]
    fn test_failed_flag_starts_false() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            assert!(!TEST_FAILED);
        }
    }

    #[test]
    fn test_failed_flag_set_by_assertion() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            reset_test_failed();
            kodo_assert(0); // triggers failure
            assert!(TEST_FAILED);
            reset_test_failed(); // clean up
        }
    }

    // -- kodo_test_set_timeout / kodo_test_clear_timeout --
    //
    // These tests mutate the global TIMEOUT_ACTIVE flag, so they must not run
    // concurrently with each other.  We use a process-wide Mutex as a
    // serialisation token — holding the lock for the entire test body ensures
    // that two timeout tests can never interleave.

    /// Mutex used to serialise timeout tests that share the global
    /// `TIMEOUT_ACTIVE` flag.  The `bool` payload is unused; only the lock
    /// matters.
    static TIMEOUT_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn timeout_can_be_set_and_cleared() {
        // 5 000 ms — far enough in the future that it will never fire during
        // a normal test run, but short enough to avoid hanging CI forever.
        let _guard = TIMEOUT_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            kodo_test_set_timeout(5_000);
            kodo_test_clear_timeout();
            // Reaching this point means the flag was cleared successfully.
            assert!(
                !TIMEOUT_ACTIVE.load(Ordering::SeqCst),
                "TIMEOUT_ACTIVE should be false after clear"
            );
        }
    }

    #[test]
    fn timeout_active_flag_is_true_after_set() {
        let _guard = TIMEOUT_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            kodo_test_set_timeout(5_000);
            assert!(
                TIMEOUT_ACTIVE.load(Ordering::SeqCst),
                "TIMEOUT_ACTIVE should be true immediately after set"
            );
            // Always clear to avoid interfering with other tests.
            kodo_test_clear_timeout();
        }
    }

    // -- kodo_test_isolate_start / kodo_test_isolate_end --

    #[test]
    fn isolation_stubs_do_not_panic() {
        // These are currently no-ops; we just verify they can be called safely.
        unsafe {
            kodo_test_isolate_start();
            kodo_test_isolate_end();
        }
    }
}
