//! Edge-case tests for the Kōdo runtime collection FFI functions.
//!
//! Exercises `KodoList` and `KodoMap` through their `extern "C"` interface,
//! covering empty collections, boundary indices, and removal semantics.

use kodo_runtime::collections::{
    kodo_list_contains, kodo_list_free, kodo_list_get, kodo_list_is_empty, kodo_list_length,
    kodo_list_new, kodo_list_pop_simple, kodo_list_push, kodo_list_remove, kodo_list_reverse,
    kodo_list_set, kodo_map_free, kodo_map_insert, kodo_map_is_empty, kodo_map_length,
    kodo_map_new, kodo_map_remove,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Reads the element at `index` from a list, returning `(value, is_some)`.
///
/// # Safety
///
/// `list` must be a valid handle from `kodo_list_new`.
unsafe fn list_get(list: i64, index: i64) -> (i64, i64) {
    let mut value: i64 = 0;
    let mut is_some: i64 = 0;
    // SAFETY: list is a valid handle; out-pointers are stack-allocated and writable.
    unsafe { kodo_list_get(list, index, &mut value, &mut is_some) };
    (value, is_some)
}

// ===========================================================================
// List — basic lifecycle
// ===========================================================================

#[test]
fn list_new_push_length() {
    let list = kodo_list_new();
    assert_ne!(list, 0, "kodo_list_new should return a non-zero handle");

    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        assert_eq!(kodo_list_length(list), 0);

        kodo_list_push(list, 10);
        kodo_list_push(list, 20);
        kodo_list_push(list, 30);

        assert_eq!(kodo_list_length(list), 3);
        kodo_list_free(list);
    }
}

// ===========================================================================
// List — pop_simple
// ===========================================================================

#[test]
fn list_pop_simple_on_empty() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        let val = kodo_list_pop_simple(list);
        assert_eq!(val, 0, "popping from an empty list should return 0");
        assert_eq!(kodo_list_length(list), 0);
        kodo_list_free(list);
    }
}

#[test]
fn list_pop_simple_returns_last() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 100);
        kodo_list_push(list, 200);

        let val = kodo_list_pop_simple(list);
        assert_eq!(val, 200);
        assert_eq!(kodo_list_length(list), 1);

        let val = kodo_list_pop_simple(list);
        assert_eq!(val, 100);
        assert_eq!(kodo_list_length(list), 0);

        kodo_list_free(list);
    }
}

// ===========================================================================
// List — remove
// ===========================================================================

#[test]
fn list_remove_valid_index() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 1);
        kodo_list_push(list, 2);
        kodo_list_push(list, 3);

        let ok = kodo_list_remove(list, 1); // remove element at index 1 (value 2)
        assert_eq!(ok, 1, "removing a valid index should return 1");
        assert_eq!(kodo_list_length(list), 2);

        // Elements should now be [1, 3].
        let (v0, _) = list_get(list, 0);
        let (v1, _) = list_get(list, 1);
        assert_eq!(v0, 1);
        assert_eq!(v1, 3);

        kodo_list_free(list);
    }
}

#[test]
fn list_remove_invalid_index() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 42);

        let ok = kodo_list_remove(list, 5);
        assert_eq!(ok, 0, "removing an out-of-bounds index should return 0");
        assert_eq!(kodo_list_length(list), 1);

        let ok = kodo_list_remove(list, -1);
        assert_eq!(ok, 0, "negative index should be treated as out-of-bounds");

        kodo_list_free(list);
    }
}

// ===========================================================================
// List — set
// ===========================================================================

#[test]
fn list_set_valid_index() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 10);
        kodo_list_push(list, 20);

        let ok = kodo_list_set(list, 0, 99);
        assert_eq!(ok, 1);

        let (val, is_some) = list_get(list, 0);
        assert_eq!(is_some, 1);
        assert_eq!(val, 99);

        kodo_list_free(list);
    }
}

#[test]
fn list_set_invalid_index() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 10);

        let ok = kodo_list_set(list, 5, 99);
        assert_eq!(ok, 0, "setting an out-of-bounds index should return 0");

        let ok = kodo_list_set(list, -1, 99);
        assert_eq!(ok, 0, "setting a negative index should return 0");

        kodo_list_free(list);
    }
}

// ===========================================================================
// List — reverse
// ===========================================================================

#[test]
fn list_reverse_empty() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new. Reversing an empty list is a no-op.
    unsafe {
        kodo_list_reverse(list);
        assert_eq!(kodo_list_length(list), 0);
        kodo_list_free(list);
    }
}

#[test]
fn list_reverse_single_element() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 42);
        kodo_list_reverse(list);

        let (val, is_some) = list_get(list, 0);
        assert_eq!(is_some, 1);
        assert_eq!(val, 42);

        kodo_list_free(list);
    }
}

#[test]
fn list_reverse_multiple_elements() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 1);
        kodo_list_push(list, 2);
        kodo_list_push(list, 3);
        kodo_list_push(list, 4);

        kodo_list_reverse(list);

        let (v0, _) = list_get(list, 0);
        let (v1, _) = list_get(list, 1);
        let (v2, _) = list_get(list, 2);
        let (v3, _) = list_get(list, 3);
        assert_eq!(v0, 4);
        assert_eq!(v1, 3);
        assert_eq!(v2, 2);
        assert_eq!(v3, 1);

        kodo_list_free(list);
    }
}

// ===========================================================================
// List — is_empty
// ===========================================================================

#[test]
fn list_is_empty_on_empty() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        assert_eq!(kodo_list_is_empty(list), 1);
        kodo_list_free(list);
    }
}

#[test]
fn list_is_empty_on_non_empty() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 1);
        assert_eq!(kodo_list_is_empty(list), 0);
        kodo_list_free(list);
    }
}

// ===========================================================================
// List — contains
// ===========================================================================

#[test]
fn list_contains_existing_value() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 10);
        kodo_list_push(list, 20);
        kodo_list_push(list, 30);

        assert_eq!(kodo_list_contains(list, 20), 1);
        assert_eq!(kodo_list_contains(list, 10), 1);
        assert_eq!(kodo_list_contains(list, 30), 1);

        kodo_list_free(list);
    }
}

#[test]
fn list_contains_non_existing_value() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        kodo_list_push(list, 10);

        assert_eq!(kodo_list_contains(list, 99), 0);
        assert_eq!(kodo_list_contains(list, 0), 0);
        assert_eq!(kodo_list_contains(list, -1), 0);

        kodo_list_free(list);
    }
}

#[test]
fn list_contains_on_empty() {
    let list = kodo_list_new();
    // SAFETY: list was just created by kodo_list_new.
    unsafe {
        assert_eq!(kodo_list_contains(list, 1), 0);
        kodo_list_free(list);
    }
}

// ===========================================================================
// Map — basic lifecycle
// ===========================================================================

#[test]
fn map_new_insert_length() {
    let map = kodo_map_new();
    assert_ne!(map, 0, "kodo_map_new should return a non-zero handle");

    // SAFETY: map was just created by kodo_map_new.
    unsafe {
        assert_eq!(kodo_map_length(map), 0);

        kodo_map_insert(map, 1, 100);
        kodo_map_insert(map, 2, 200);

        assert_eq!(kodo_map_length(map), 2);

        kodo_map_free(map);
    }
}

#[test]
fn map_insert_overwrites_existing_key() {
    let map = kodo_map_new();
    // SAFETY: map was just created by kodo_map_new.
    unsafe {
        kodo_map_insert(map, 1, 100);
        kodo_map_insert(map, 1, 999);

        // Length should still be 1 (overwrite, not duplicate).
        assert_eq!(kodo_map_length(map), 1);

        kodo_map_free(map);
    }
}

// ===========================================================================
// Map — remove
// ===========================================================================

#[test]
fn map_remove_existing_key() {
    let map = kodo_map_new();
    // SAFETY: map was just created by kodo_map_new.
    unsafe {
        kodo_map_insert(map, 1, 100);
        kodo_map_insert(map, 2, 200);

        let ok = kodo_map_remove(map, 1);
        assert_eq!(ok, 1, "removing an existing key should return 1");
        assert_eq!(kodo_map_length(map), 1);

        kodo_map_free(map);
    }
}

#[test]
fn map_remove_non_existing_key() {
    let map = kodo_map_new();
    // SAFETY: map was just created by kodo_map_new.
    unsafe {
        kodo_map_insert(map, 1, 100);

        let ok = kodo_map_remove(map, 999);
        assert_eq!(ok, 0, "removing a non-existing key should return 0");
        assert_eq!(kodo_map_length(map), 1);

        kodo_map_free(map);
    }
}

#[test]
fn map_remove_from_empty() {
    let map = kodo_map_new();
    // SAFETY: map was just created by kodo_map_new.
    unsafe {
        let ok = kodo_map_remove(map, 42);
        assert_eq!(ok, 0);
        kodo_map_free(map);
    }
}

// ===========================================================================
// Map — is_empty
// ===========================================================================

#[test]
fn map_is_empty_on_empty() {
    let map = kodo_map_new();
    // SAFETY: map was just created by kodo_map_new.
    unsafe {
        assert_eq!(kodo_map_is_empty(map), 1);
        kodo_map_free(map);
    }
}

#[test]
fn map_is_empty_on_non_empty() {
    let map = kodo_map_new();
    // SAFETY: map was just created by kodo_map_new.
    unsafe {
        kodo_map_insert(map, 1, 100);
        assert_eq!(kodo_map_is_empty(map), 0);
        kodo_map_free(map);
    }
}

#[test]
fn map_is_empty_after_remove_all() {
    let map = kodo_map_new();
    // SAFETY: map was just created by kodo_map_new.
    unsafe {
        kodo_map_insert(map, 1, 100);
        kodo_map_remove(map, 1);
        assert_eq!(kodo_map_is_empty(map), 1);
        kodo_map_free(map);
    }
}
