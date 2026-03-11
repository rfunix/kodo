//! Numeric builtin functions for the Kōdo runtime.
//!
//! Provides FFI-callable functions for math operations such as
//! abs, min, max, and clamp.

/// Returns the absolute value of an integer.
#[no_mangle]
pub extern "C" fn kodo_abs(n: i64) -> i64 {
    n.wrapping_abs()
}

/// Returns the minimum of two integers.
#[no_mangle]
pub extern "C" fn kodo_min(a: i64, b: i64) -> i64 {
    if a < b {
        a
    } else {
        b
    }
}

/// Returns the maximum of two integers.
#[no_mangle]
pub extern "C" fn kodo_max(a: i64, b: i64) -> i64 {
    if a > b {
        a
    } else {
        b
    }
}

/// Clamps a value between a minimum and maximum.
#[no_mangle]
pub extern "C" fn kodo_clamp(val: i64, lo: i64, hi: i64) -> i64 {
    if val < lo {
        lo
    } else if val > hi {
        hi
    } else {
        val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_works() {
        assert_eq!(kodo_abs(42), 42);
        assert_eq!(kodo_abs(-42), 42);
        assert_eq!(kodo_abs(0), 0);
    }

    #[test]
    fn min_works() {
        assert_eq!(kodo_min(3, 7), 3);
        assert_eq!(kodo_min(7, 3), 3);
        assert_eq!(kodo_min(5, 5), 5);
    }

    #[test]
    fn max_works() {
        assert_eq!(kodo_max(3, 7), 7);
        assert_eq!(kodo_max(7, 3), 7);
        assert_eq!(kodo_max(5, 5), 5);
    }

    #[test]
    fn clamp_works() {
        assert_eq!(kodo_clamp(5, 1, 10), 5);
        assert_eq!(kodo_clamp(-5, 1, 10), 1);
        assert_eq!(kodo_clamp(15, 1, 10), 10);
    }
}
