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

/// Returns the square root of a float.
#[no_mangle]
pub extern "C" fn kodo_sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// Returns base raised to the power of exp.
#[no_mangle]
pub extern "C" fn kodo_pow(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

/// Returns the sine of x (radians).
#[no_mangle]
pub extern "C" fn kodo_sin(x: f64) -> f64 {
    x.sin()
}

/// Returns the cosine of x (radians).
#[no_mangle]
pub extern "C" fn kodo_cos(x: f64) -> f64 {
    x.cos()
}

/// Returns the natural logarithm of x.
#[no_mangle]
pub extern "C" fn kodo_log(x: f64) -> f64 {
    x.ln()
}

/// Returns the largest integer less than or equal to x.
#[no_mangle]
pub extern "C" fn kodo_floor(x: f64) -> f64 {
    x.floor()
}

/// Returns the smallest integer greater than or equal to x.
#[no_mangle]
pub extern "C" fn kodo_ceil(x: f64) -> f64 {
    x.ceil()
}

/// Rounds x to the nearest integer.
#[no_mangle]
pub extern "C" fn kodo_round(x: f64) -> f64 {
    x.round()
}

/// Returns a random integer in the range `[min, max]` (inclusive).
#[no_mangle]
pub extern "C" fn kodo_rand_int(min_val: i64, max_val: i64) -> i64 {
    if min_val >= max_val {
        return min_val;
    }
    #[allow(clippy::cast_sign_loss)]
    let range = (max_val - min_val) as u64 + 1;
    #[allow(clippy::cast_possible_wrap)]
    let offset = (fastrand::u64(0..range)) as i64;
    min_val + offset
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

    #[test]
    fn sqrt_works() {
        assert!((kodo_sqrt(4.0) - 2.0).abs() < f64::EPSILON);
        assert!((kodo_sqrt(9.0) - 3.0).abs() < f64::EPSILON);
        assert!(kodo_sqrt(0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pow_works() {
        assert!((kodo_pow(2.0, 3.0) - 8.0).abs() < f64::EPSILON);
        assert!((kodo_pow(10.0, 0.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn sin_cos_works() {
        assert!(kodo_sin(0.0).abs() < f64::EPSILON);
        assert!((kodo_cos(0.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn log_works() {
        assert!((kodo_log(1.0)).abs() < f64::EPSILON);
        assert!((kodo_log(std::f64::consts::E) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn floor_ceil_round_works() {
        assert!((kodo_floor(2.7) - 2.0).abs() < f64::EPSILON);
        assert!((kodo_ceil(2.3) - 3.0).abs() < f64::EPSILON);
        assert!((kodo_round(2.5) - 3.0).abs() < f64::EPSILON);
        assert!((kodo_round(2.4) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rand_int_in_range() {
        for _ in 0..100 {
            let val = kodo_rand_int(5, 10);
            assert!(val >= 5 && val <= 10, "rand_int out of range: {val}");
        }
    }

    #[test]
    fn rand_int_min_equals_max() {
        assert_eq!(kodo_rand_int(7, 7), 7);
    }

    #[test]
    fn rand_int_min_greater_than_max() {
        // When min >= max, function returns min.
        assert_eq!(kodo_rand_int(10, 5), 10);
    }

    #[test]
    fn abs_i64_min() {
        // i64::MIN.wrapping_abs() == i64::MIN (overflow wraps).
        // This documents the edge-case behavior.
        assert_eq!(kodo_abs(i64::MIN), i64::MIN);
    }
}
