//! Edge-case tests for the Kōdo runtime string FFI functions.
//!
//! Exercises `kodo_string_*` functions through their `extern "C"` interface,
//! covering empty strings, ASCII, multi-byte UTF-8, and boundary conditions.

use kodo_runtime::string_ops::{
    kodo_string_contains, kodo_string_ends_with, kodo_string_eq, kodo_string_length,
    kodo_string_starts_with,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Converts a `&str` to the `(*const u8, usize)` pair expected by the FFI.
fn str_to_parts(s: &str) -> (*const u8, usize) {
    (s.as_ptr(), s.len())
}

// ===========================================================================
// string_length
// ===========================================================================

#[test]
fn string_length_empty() {
    let (ptr, len) = str_to_parts("");
    assert_eq!(kodo_string_length(ptr, len), 0);
}

#[test]
fn string_length_ascii() {
    let (ptr, len) = str_to_parts("hello");
    assert_eq!(kodo_string_length(ptr, len), 5);
}

#[test]
fn string_length_multibyte() {
    // Each Japanese character is 3 bytes in UTF-8.
    let s = "\u{30b3}\u{30fc}\u{30c9}"; // "コード"
    let (ptr, len) = str_to_parts(s);
    // kodo_string_length returns byte length, not character count.
    assert_eq!(kodo_string_length(ptr, len), 9);
}

#[test]
fn string_length_emoji() {
    let s = "\u{1f980}"; // crab emoji, 4 bytes in UTF-8
    let (ptr, len) = str_to_parts(s);
    assert_eq!(kodo_string_length(ptr, len), 4);
}

// ===========================================================================
// string_contains
// ===========================================================================

#[test]
fn string_contains_present() {
    let (hay_ptr, hay_len) = str_to_parts("hello world");
    let (needle_ptr, needle_len) = str_to_parts("world");
    // SAFETY: both pointers point to valid UTF-8 byte slices on the stack.
    let result = unsafe { kodo_string_contains(hay_ptr, hay_len, needle_ptr, needle_len) };
    assert_eq!(result, 1);
}

#[test]
fn string_contains_absent() {
    let (hay_ptr, hay_len) = str_to_parts("hello world");
    let (needle_ptr, needle_len) = str_to_parts("xyz");
    // SAFETY: both pointers point to valid UTF-8 byte slices on the stack.
    let result = unsafe { kodo_string_contains(hay_ptr, hay_len, needle_ptr, needle_len) };
    assert_eq!(result, 0);
}

#[test]
fn string_contains_empty_needle() {
    let (hay_ptr, hay_len) = str_to_parts("anything");
    let (needle_ptr, needle_len) = str_to_parts("");
    // SAFETY: both pointers point to valid UTF-8 byte slices on the stack.
    let result = unsafe { kodo_string_contains(hay_ptr, hay_len, needle_ptr, needle_len) };
    // An empty needle is always contained in any string.
    assert_eq!(result, 1);
}

#[test]
fn string_contains_both_empty() {
    let (hay_ptr, hay_len) = str_to_parts("");
    let (needle_ptr, needle_len) = str_to_parts("");
    // SAFETY: both pointers point to valid (empty) byte slices.
    let result = unsafe { kodo_string_contains(hay_ptr, hay_len, needle_ptr, needle_len) };
    assert_eq!(result, 1);
}

#[test]
fn string_contains_needle_longer_than_haystack() {
    let (hay_ptr, hay_len) = str_to_parts("hi");
    let (needle_ptr, needle_len) = str_to_parts("hello world");
    // SAFETY: both pointers point to valid UTF-8 byte slices on the stack.
    let result = unsafe { kodo_string_contains(hay_ptr, hay_len, needle_ptr, needle_len) };
    assert_eq!(result, 0);
}

// ===========================================================================
// string_eq
// ===========================================================================

#[test]
fn string_eq_equal() {
    let (p1, l1) = str_to_parts("kodo");
    let (p2, l2) = str_to_parts("kodo");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_eq(p1, l1, p2, l2) };
    assert_eq!(result, 1);
}

#[test]
fn string_eq_different_content() {
    let (p1, l1) = str_to_parts("kodo");
    let (p2, l2) = str_to_parts("rust");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_eq(p1, l1, p2, l2) };
    assert_eq!(result, 0);
}

#[test]
fn string_eq_different_lengths() {
    let (p1, l1) = str_to_parts("kodo");
    let (p2, l2) = str_to_parts("kod");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_eq(p1, l1, p2, l2) };
    assert_eq!(result, 0);
}

#[test]
fn string_eq_both_empty() {
    let (p1, l1) = str_to_parts("");
    let (p2, l2) = str_to_parts("");
    // SAFETY: both pointers point to valid (empty) byte slices.
    let result = unsafe { kodo_string_eq(p1, l1, p2, l2) };
    assert_eq!(result, 1);
}

// ===========================================================================
// string_starts_with
// ===========================================================================

#[test]
fn string_starts_with_matching_prefix() {
    let (hay_ptr, hay_len) = str_to_parts("hello world");
    let (pfx_ptr, pfx_len) = str_to_parts("hello");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_starts_with(hay_ptr, hay_len, pfx_ptr, pfx_len) };
    assert_eq!(result, 1);
}

#[test]
fn string_starts_with_non_matching_prefix() {
    let (hay_ptr, hay_len) = str_to_parts("hello world");
    let (pfx_ptr, pfx_len) = str_to_parts("world");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_starts_with(hay_ptr, hay_len, pfx_ptr, pfx_len) };
    assert_eq!(result, 0);
}

#[test]
fn string_starts_with_empty_prefix() {
    let (hay_ptr, hay_len) = str_to_parts("anything");
    let (pfx_ptr, pfx_len) = str_to_parts("");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_starts_with(hay_ptr, hay_len, pfx_ptr, pfx_len) };
    // Every string starts with the empty string.
    assert_eq!(result, 1);
}

#[test]
fn string_starts_with_full_string() {
    let s = "exact";
    let (hay_ptr, hay_len) = str_to_parts(s);
    let (pfx_ptr, pfx_len) = str_to_parts(s);
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_starts_with(hay_ptr, hay_len, pfx_ptr, pfx_len) };
    assert_eq!(result, 1);
}

// ===========================================================================
// string_ends_with
// ===========================================================================

#[test]
fn string_ends_with_matching_suffix() {
    let (hay_ptr, hay_len) = str_to_parts("hello world");
    let (sfx_ptr, sfx_len) = str_to_parts("world");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_ends_with(hay_ptr, hay_len, sfx_ptr, sfx_len) };
    assert_eq!(result, 1);
}

#[test]
fn string_ends_with_non_matching_suffix() {
    let (hay_ptr, hay_len) = str_to_parts("hello world");
    let (sfx_ptr, sfx_len) = str_to_parts("hello");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_ends_with(hay_ptr, hay_len, sfx_ptr, sfx_len) };
    assert_eq!(result, 0);
}

#[test]
fn string_ends_with_empty_suffix() {
    let (hay_ptr, hay_len) = str_to_parts("anything");
    let (sfx_ptr, sfx_len) = str_to_parts("");
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_ends_with(hay_ptr, hay_len, sfx_ptr, sfx_len) };
    // Every string ends with the empty string.
    assert_eq!(result, 1);
}

#[test]
fn string_ends_with_full_string() {
    let s = "exact";
    let (hay_ptr, hay_len) = str_to_parts(s);
    let (sfx_ptr, sfx_len) = str_to_parts(s);
    // SAFETY: both pointers point to valid UTF-8 byte slices.
    let result = unsafe { kodo_string_ends_with(hay_ptr, hay_len, sfx_ptr, sfx_len) };
    assert_eq!(result, 1);
}
