//! Property-based testing engine — random value generation and basic shrinking.
//!
//! Provides `extern "C"` functions called by compiled Kōdo test code to
//! generate random values for `forall` bindings and perform basic shrinking
//! when a property test fails.
//!
//! The RNG is stored in a `thread_local!` so that multiple test threads do not
//! interfere with each other. Call [`kodo_prop_start`] to initialise the engine
//! before calling any generator function.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Thread-local RNG state
// ---------------------------------------------------------------------------

thread_local! {
    /// Per-thread RNG for property tests.
    ///
    /// Seeded by `kodo_prop_start`; defaults to an entropy-seeded RNG if
    /// `kodo_prop_start` was never called.
    static PROP_RNG: RefCell<StdRng> = RefCell::new(StdRng::from_entropy());
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Initialises the property testing RNG with an iteration count and seed.
///
/// If `seed` is `0`, a fresh entropy-based seed is used (non-deterministic).
/// Any other value produces a fully deterministic sequence.
///
/// `iterations` is accepted for API symmetry with the Kōdo runtime protocol
/// but is not stored — the caller manages the loop.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code.
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_start(_iterations: i64, seed: i64) {
    PROP_RNG.with(|rng| {
        let new_rng = if seed == 0 {
            StdRng::from_entropy()
        } else {
            #[allow(clippy::cast_sign_loss)]
            StdRng::seed_from_u64(seed as u64)
        };
        *rng.borrow_mut() = new_rng;
    });
}

// ---------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------

/// Generates a random `Int` in the closed range `[min, max]`.
///
/// If `min > max` the values are swapped silently.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code after
/// [`kodo_prop_start`].
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_gen_int(min: i64, max: i64) -> i64 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    PROP_RNG.with(|rng| rng.borrow_mut().gen_range(lo..=hi))
}

/// Generates a random `Bool` as an `i64` — either `0` (false) or `1` (true).
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code after
/// [`kodo_prop_start`].
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_gen_bool() -> i64 {
    PROP_RNG.with(|rng| i64::from(rng.borrow_mut().gen::<bool>()))
}

/// Generates a random `Float64` in the closed range `[min, max]`.
///
/// If `min > max` the values are swapped silently.
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code after
/// [`kodo_prop_start`].
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_gen_float(min: f64, max: f64) -> f64 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    PROP_RNG.with(|rng| {
        // gen_range requires lo < hi; handle the degenerate case.
        if (hi - lo).abs() < f64::EPSILON {
            return lo;
        }
        rng.borrow_mut().gen_range(lo..=hi)
    })
}

/// Generates a random `String` of up to `max_len` printable ASCII characters.
///
/// Returns a slot pointer — an `i64` that points to a heap-allocated
/// `[i64; 2]` pair `[ptr, len]` where `ptr` points to the string bytes and
/// `len` is the byte count.  This matches the slot format used by the Kōdo
/// codegen for `String` values.
///
/// The string bytes and the slot are intentionally leaked (managed by the
/// Kōdo runtime's arena lifetime).
///
/// # Safety
///
/// May only be called from single-threaded compiled Kōdo test code after
/// [`kodo_prop_start`].
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_gen_string(max_len: i64) -> i64 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let cap = if max_len <= 0 {
        0usize
    } else {
        max_len as usize
    };

    let len: usize = if cap == 0 {
        0
    } else {
        PROP_RNG.with(|rng| rng.borrow_mut().gen_range(0..=cap))
    };

    // Generate printable ASCII characters (0x20–0x7E).
    let bytes: Vec<u8> = PROP_RNG.with(|rng| {
        let mut r = rng.borrow_mut();
        (0..len).map(|_| r.gen_range(0x20u8..=0x7Eu8)).collect()
    });

    // Leak the byte buffer so its pointer stays valid beyond this call.
    let bytes_box = bytes.into_boxed_slice();
    let ptr = Box::into_raw(bytes_box) as *const u8;

    // Allocate and leak a [ptr: i64, len: i64] slot.
    #[allow(clippy::cast_possible_wrap)]
    let slot = Box::new([ptr as i64, len as i64]);
    Box::into_raw(slot) as i64
}

// ---------------------------------------------------------------------------
// Shrinking
// ---------------------------------------------------------------------------

/// Shrinks an `Int` one step toward zero.
///
/// Strategy:
/// - `0` → `0` (already minimal)
/// - any other value `v` → `v / 2` (halving converges to 0 quickly)
///
/// # Safety
///
/// May be called from any compiled Kōdo test code (no global state access).
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_shrink_int(value: i64) -> i64 {
    if value == 0 {
        0
    } else {
        value / 2
    }
}

/// Shrinks a `Bool` to `false` (`0`).
///
/// `false` is the minimal Boolean value by convention.
///
/// # Safety
///
/// May be called from any compiled Kōdo test code (no global state access).
#[no_mangle]
pub unsafe extern "C" fn kodo_prop_shrink_bool(_value: i64) -> i64 {
    0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gen_int_in_range() {
        unsafe {
            kodo_prop_start(100, 42);
            for _ in 0..100 {
                let v = kodo_prop_gen_int(0, 10);
                assert!(v >= 0 && v <= 10, "got {v}");
            }
        }
    }

    #[test]
    fn gen_bool_is_zero_or_one() {
        unsafe {
            kodo_prop_start(100, 42);
            for _ in 0..100 {
                let v = kodo_prop_gen_bool();
                assert!(v == 0 || v == 1, "got {v}");
            }
        }
    }

    #[test]
    fn gen_float_in_range() {
        unsafe {
            kodo_prop_start(100, 42);
            for _ in 0..100 {
                let v = kodo_prop_gen_float(-1.0, 1.0);
                assert!(v >= -1.0 && v <= 1.0, "got {v}");
            }
        }
    }

    #[test]
    fn shrink_int_toward_zero() {
        unsafe {
            assert_eq!(kodo_prop_shrink_int(100), 50);
            assert_eq!(kodo_prop_shrink_int(1), 0);
            assert_eq!(kodo_prop_shrink_int(0), 0);
            assert_eq!(kodo_prop_shrink_int(-100), -50);
        }
    }

    #[test]
    fn deterministic_seed() {
        unsafe {
            kodo_prop_start(10, 42);
            let a = kodo_prop_gen_int(0, 1000);
            kodo_prop_start(10, 42);
            let b = kodo_prop_gen_int(0, 1000);
            assert_eq!(a, b, "same seed should produce same value");
        }
    }

    #[test]
    fn gen_int_swapped_range() {
        // min > max should be handled gracefully.
        unsafe {
            kodo_prop_start(50, 7);
            for _ in 0..50 {
                let v = kodo_prop_gen_int(10, 0);
                assert!(v >= 0 && v <= 10, "swapped range: got {v}");
            }
        }
    }

    #[test]
    fn gen_float_degenerate_equal_bounds() {
        unsafe {
            kodo_prop_start(10, 1);
            let v = kodo_prop_gen_float(3.14, 3.14);
            // Should return the bound without panicking.
            assert!((v - 3.14).abs() < f64::EPSILON, "got {v}");
        }
    }

    #[test]
    fn gen_string_length_within_bound() {
        unsafe {
            kodo_prop_start(50, 99);
            for _ in 0..50 {
                let slot = kodo_prop_gen_string(20);
                assert_ne!(slot, 0, "slot pointer must be non-null");
                let pair = &*(slot as *const [i64; 2]);
                let str_len = pair[1];
                assert!(
                    str_len >= 0 && str_len <= 20,
                    "string len {str_len} out of bounds"
                );
            }
        }
    }

    #[test]
    fn gen_string_zero_max_len() {
        unsafe {
            kodo_prop_start(5, 1);
            let slot = kodo_prop_gen_string(0);
            assert_ne!(slot, 0);
            let pair = &*(slot as *const [i64; 2]);
            assert_eq!(pair[1], 0, "max_len=0 must produce empty string");
        }
    }

    #[test]
    fn gen_string_printable_ascii() {
        unsafe {
            kodo_prop_start(10, 13);
            let slot = kodo_prop_gen_string(64);
            let pair = &*(slot as *const [i64; 2]);
            let ptr = pair[0] as *const u8;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let len = pair[1] as usize;
            let bytes = std::slice::from_raw_parts(ptr, len);
            for &b in bytes {
                assert!(
                    (0x20..=0x7E).contains(&b),
                    "non-printable byte 0x{b:02X} in generated string"
                );
            }
        }
    }

    #[test]
    fn shrink_bool_always_false() {
        unsafe {
            assert_eq!(kodo_prop_shrink_bool(0), 0);
            assert_eq!(kodo_prop_shrink_bool(1), 0);
            assert_eq!(kodo_prop_shrink_bool(42), 0);
        }
    }
}
