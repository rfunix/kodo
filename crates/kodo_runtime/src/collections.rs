//! List and Map collection builtins for the Kōdo runtime.
//!
//! Provides heap-allocated dynamic lists and hash maps accessible
//! through FFI. Lists store `i64` elements; maps use open addressing
//! with linear probing.

/// Represents a heap-allocated dynamic list.
///
/// Each element is stored as an `i64` (values for Int, pointers for String).
/// This struct is never directly exposed to Kōdo code — the runtime manages
/// it through opaque pointer handles.
#[repr(C)]
struct KodoList {
    /// Pointer to the element array.
    data: *mut i64,
    /// Number of elements currently in the list.
    len: usize,
    /// Allocated capacity (number of i64 slots).
    capacity: usize,
}

/// Creates a new empty list.
///
/// Returns a pointer (as i64) to a heap-allocated `KodoList`.
#[no_mangle]
pub extern "C" fn kodo_list_new() -> i64 {
    let list = Box::new(KodoList {
        data: std::ptr::null_mut(),
        len: 0,
        capacity: 0,
    });
    // SAFETY: intentionally leaks so caller manages via opaque pointer. Freed by `kodo_list_free`.
    Box::into_raw(list) as i64
}

/// Pushes an element onto the end of a list.
///
/// Grows the backing array if needed (doubling strategy).
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_push(list_ptr: i64, value: i64) {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    #[allow(clippy::cast_possible_truncation)]
    let list = unsafe { &mut *(list_ptr as *mut KodoList) };
    if list.len == list.capacity {
        let new_cap = if list.capacity == 0 {
            4
        } else {
            list.capacity * 2
        };
        let new_layout = std::alloc::Layout::array::<i64>(new_cap);
        let Ok(layout) = new_layout else {
            eprintln!("fatal: out of memory in kodo runtime (list_push layout)");
            std::process::abort();
        };
        let new_data = if list.data.is_null() {
            // SAFETY: layout is valid and non-zero size.
            #[allow(clippy::cast_ptr_alignment)]
            unsafe {
                std::alloc::alloc(layout).cast::<i64>()
            }
        } else {
            let old_layout_result = std::alloc::Layout::array::<i64>(list.capacity);
            let Ok(old_layout) = old_layout_result else {
                eprintln!("fatal: out of memory in kodo runtime (list_push realloc layout)");
                std::process::abort();
            };
            // SAFETY: list.data was allocated with old_layout, new size >= old size.
            #[allow(clippy::cast_ptr_alignment)]
            unsafe {
                std::alloc::realloc(list.data.cast::<u8>(), old_layout, layout.size()).cast::<i64>()
            }
        };
        if new_data.is_null() {
            eprintln!("fatal: out of memory in kodo runtime (list_push)");
            std::process::abort();
        }
        list.data = new_data;
        list.capacity = new_cap;
    }
    // SAFETY: list.len < list.capacity, data is valid.
    unsafe { *list.data.add(list.len) = value };
    list.len += 1;
}

/// Returns the element at the given index, or 0 if out of bounds.
///
/// Returns a two-value result: (value, `is_some`) where `is_some` is 1 if the
/// index was valid, 0 otherwise. The values are written to out parameters.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
/// `out_value` and `out_is_some` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_get(
    list_ptr: i64,
    index: i64,
    out_value: *mut i64,
    out_is_some: *mut i64,
) {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    #[allow(clippy::cast_possible_truncation)]
    let list = unsafe { &*(list_ptr as *const KodoList) };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = index as usize;
    if idx < list.len {
        // SAFETY: idx < len, data is valid.
        // Caller guarantees out_value and out_is_some are valid writable pointers.
        unsafe {
            *out_value = *list.data.add(idx);
            *out_is_some = 1;
        }
    } else {
        // SAFETY: Caller guarantees out_value and out_is_some are valid writable pointers.
        unsafe {
            *out_value = 0;
            *out_is_some = 0;
        }
    }
}

/// Returns the number of elements in the list.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_length(list_ptr: i64) -> i64 {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    #[allow(clippy::cast_possible_truncation)]
    let list = unsafe { &*(list_ptr as *const KodoList) };
    #[allow(clippy::cast_possible_wrap)]
    let result = list.len as i64;
    result
}

/// Returns 1 if the list contains the given value, 0 otherwise.
///
/// Comparison is done by i64 equality (works for Int values).
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_contains(list_ptr: i64, value: i64) -> i64 {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    #[allow(clippy::cast_possible_truncation)]
    let list = unsafe { &*(list_ptr as *const KodoList) };
    for i in 0..list.len {
        // SAFETY: i < len, data is valid.
        if unsafe { *list.data.add(i) } == value {
            return 1;
        }
    }
    0
}

/// Removes and returns the last element from a list.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
/// `out_value` and `out_is_some` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_pop(list_ptr: i64, out_value: *mut i64, out_is_some: *mut i64) {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &mut *(list_ptr as *mut KodoList) };
    if list.len > 0 {
        list.len -= 1;
        // SAFETY: list.len was > 0, data is valid up to old len.
        unsafe {
            *out_value = *list.data.add(list.len);
            *out_is_some = 1;
        }
    } else {
        unsafe {
            *out_value = 0;
            *out_is_some = 0;
        }
    }
}

/// Simplified pop that returns the last element directly.
///
/// Returns the last element, or 0 if the list is empty. This wrapper matches
/// the type checker's signature: `list_pop(List<Int>) -> Int`.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_pop_simple(list_ptr: i64) -> i64 {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &mut *(list_ptr as *mut KodoList) };
    if list.len > 0 {
        list.len -= 1;
        // SAFETY: list.len was > 0, data is valid up to old len.
        unsafe { *list.data.add(list.len) }
    } else {
        0
    }
}

/// Removes the element at the given index, shifting subsequent elements left.
///
/// Returns 1 if the index was valid, 0 otherwise.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_remove(list_ptr: i64, index: i64) -> i64 {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &mut *(list_ptr as *mut KodoList) };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = index as usize;
    if idx >= list.len {
        return 0;
    }
    // SAFETY: idx < list.len, data is valid.
    unsafe {
        let src = list.data.add(idx + 1);
        let dst = list.data.add(idx);
        std::ptr::copy(src, dst, list.len - idx - 1);
    }
    list.len -= 1;
    1
}

/// Sets the element at the given index.
///
/// Returns 1 if the index was valid, 0 otherwise.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_set(list_ptr: i64, index: i64, value: i64) -> i64 {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &mut *(list_ptr as *mut KodoList) };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let idx = index as usize;
    if idx >= list.len {
        return 0;
    }
    // SAFETY: idx < list.len, data is valid.
    unsafe { *list.data.add(idx) = value };
    1
}

/// Returns 1 if the list is empty, 0 otherwise.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_is_empty(list_ptr: i64) -> i64 {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &*(list_ptr as *const KodoList) };
    i64::from(list.len == 0)
}

/// Reverses the elements of a list in place.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_reverse(list_ptr: i64) {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &mut *(list_ptr as *mut KodoList) };
    if list.len <= 1 {
        return;
    }
    let mut i = 0;
    let mut j = list.len - 1;
    while i < j {
        // SAFETY: i < j < list.len, data is valid.
        unsafe {
            let a = *list.data.add(i);
            let b = *list.data.add(j);
            *list.data.add(i) = b;
            *list.data.add(j) = a;
        }
        i += 1;
        j -= 1;
    }
}

/// Returns a new list containing elements from `start` (inclusive) to `end` (exclusive).
///
/// Indices are clamped to the valid range `[0, len]`. If `start >= end` after
/// clamping, an empty list is returned.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_slice(list_ptr: i64, start: i64, end: i64) -> i64 {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &*(list_ptr as *const KodoList) };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let start_idx = (start.max(0) as usize).min(list.len);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let end_idx = (end.max(0) as usize).min(list.len);
    let actual_end = end_idx.max(start_idx);

    let new_list = kodo_list_new();
    for i in start_idx..actual_end {
        // SAFETY: i < list.len, data is valid.
        let val = unsafe { *list.data.add(i) };
        // SAFETY: new_list is valid, just created above.
        unsafe { kodo_list_push(new_list, val) };
    }
    new_list
}

/// Sorts the elements of a list of `Int` values in ascending order (in place).
///
/// Uses the standard library's sort algorithm on the underlying i64 array.
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_sort(list_ptr: i64) {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &mut *(list_ptr as *mut KodoList) };
    if list.len <= 1 || list.data.is_null() {
        return;
    }
    // SAFETY: list.data points to list.len valid i64 elements.
    let slice = unsafe { std::slice::from_raw_parts_mut(list.data, list.len) };
    slice.sort_unstable();
}

/// Joins a `List<String>` into a single string with the given separator.
///
/// Each element in the list is an opaque pointer to a heap-allocated `[i64; 2]`
/// pair `(ptr, len)` representing a string. This follows the same storage format
/// as `kodo_string_split`.
///
/// Returns a pointer to a heap-allocated `[i64; 2]` pair `(ptr, len)` for the
/// resulting joined string (same format as string split elements).
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`.
/// `sep_ptr` must point to `sep_len` valid bytes.
/// Each element in the list must be a valid pointer to a `[i64; 2]` pair.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_join(
    list_ptr: i64,
    sep_ptr: *const u8,
    sep_len: i64,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) {
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new.
    let list = unsafe { &*(list_ptr as *const KodoList) };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let separator_len = sep_len as usize;
    // SAFETY: caller guarantees sep_ptr points to sep_len valid bytes.
    let separator = unsafe { std::slice::from_raw_parts(sep_ptr, separator_len) };

    let mut result = Vec::new();

    for i in 0..list.len {
        if i > 0 {
            result.extend_from_slice(separator);
        }
        // SAFETY: i < list.len, data[i] is a valid pointer to [i64; 2].
        let pair_ptr = unsafe { *list.data.add(i) } as *const i64;
        if !pair_ptr.is_null() {
            // SAFETY: pair_ptr points to a valid [i64; 2] pair.
            let str_ptr = unsafe { *pair_ptr } as *const u8;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let str_len = unsafe { *pair_ptr.add(1) } as usize;
            if !str_ptr.is_null() && str_len > 0 {
                // SAFETY: str_ptr points to str_len valid bytes.
                let bytes = unsafe { std::slice::from_raw_parts(str_ptr, str_len) };
                result.extend_from_slice(bytes);
            }
        }
    }

    let boxed = result.into_boxed_slice();
    let result_len = boxed.len();
    // SAFETY: intentionally leaks so caller manages memory via (ptr, len).
    let result_ptr = Box::into_raw(boxed) as *const u8;
    // SAFETY: Caller guarantees out_ptr and out_len are valid writable pointers.
    unsafe {
        *out_ptr = result_ptr;
        *out_len = result_len;
    }
}

/// Frees a heap-allocated `KodoList` and its backing data array.
///
/// Does nothing if `list_ptr` is zero (null handle).
///
/// # Safety
///
/// `list_ptr` must be a valid pointer returned by `kodo_list_new`, or zero.
/// After calling this function, the list pointer must not be used again.
#[no_mangle]
pub unsafe extern "C" fn kodo_list_free(list_ptr: i64) {
    if list_ptr == 0 {
        return;
    }
    // SAFETY: caller guarantees list_ptr was returned by kodo_list_new
    // (i.e. Box::into_raw on a Box<KodoList>).
    let list = unsafe { Box::from_raw(list_ptr as *mut KodoList) };
    if !list.data.is_null() && list.capacity > 0 {
        let layout = std::alloc::Layout::array::<i64>(list.capacity);
        if let Ok(layout) = layout {
            // SAFETY: list.data was allocated via std::alloc::alloc with this layout.
            unsafe { std::alloc::dealloc(list.data.cast::<u8>(), layout) };
        }
    }
    // list is dropped here, freeing the KodoList struct itself.
}

// ---------------------------------------------------------------------------
// List Iterator
// ---------------------------------------------------------------------------

/// Iterator over a `KodoList`, walking elements sequentially.
#[repr(C)]
struct KodoListIterator {
    /// Pointer to the list being iterated (not owned).
    list: *mut KodoList,
    /// Current index into the list data array.
    index: usize,
}

/// Creates a new iterator over a list.
///
/// Returns a pointer (as i64) to a heap-allocated `KodoListIterator`.
/// The iterator does **not** own the list — the caller must keep the list
/// alive for the lifetime of the iterator.
#[no_mangle]
pub extern "C" fn kodo_list_iter(list_handle: i64) -> i64 {
    let iter = Box::new(KodoListIterator {
        list: list_handle as *mut KodoList,
        index: 0,
    });
    Box::into_raw(iter) as i64
}

/// Advances the iterator to the next element.
///
/// Returns 1 if an element is available (call `kodo_list_iterator_value`
/// to retrieve it), or 0 if the iterator is exhausted.
#[no_mangle]
pub extern "C" fn kodo_list_iterator_advance(iter_handle: i64) -> i64 {
    // SAFETY: caller guarantees iter_handle was returned by kodo_list_iter.
    let iter = unsafe { &mut *(iter_handle as *mut KodoListIterator) };
    // SAFETY: iter.list was a valid KodoList pointer when the iterator was created.
    let list = unsafe { &*iter.list };
    if iter.index < list.len {
        iter.index += 1;
        1
    } else {
        0
    }
}

/// Returns the current element value after a successful `advance`.
///
/// Must only be called after `kodo_list_iterator_advance` returned 1.
/// The element is at `index - 1` because `advance` increments before returning.
#[no_mangle]
pub extern "C" fn kodo_list_iterator_value(iter_handle: i64) -> i64 {
    // SAFETY: caller guarantees iter_handle was returned by kodo_list_iter.
    let iter = unsafe { &*(iter_handle as *mut KodoListIterator) };
    // SAFETY: iter.list was a valid KodoList pointer when the iterator was created.
    let list = unsafe { &*iter.list };
    let idx = iter.index.saturating_sub(1);
    if idx < list.len {
        // SAFETY: data[idx] is within the allocated region of the list.
        unsafe { *list.data.add(idx) }
    } else {
        0
    }
}

/// Frees a list iterator.
///
/// Does nothing if the handle is 0 (null).
#[no_mangle]
pub extern "C" fn kodo_list_iterator_free(iter_handle: i64) {
    if iter_handle == 0 {
        return;
    }
    // SAFETY: caller guarantees iter_handle was returned by kodo_list_iter.
    let _ = unsafe { Box::from_raw(iter_handle as *mut KodoListIterator) };
}

// ---------------------------------------------------------------------------
// Map (hash map with open addressing)
// ---------------------------------------------------------------------------

/// Represents a key-value pair in a hash map entry.
#[derive(Clone)]
#[repr(C)]
struct KodoMapEntry {
    /// The key (i64 value or pointer).
    key: i64,
    /// The value (i64 value or pointer).
    value: i64,
    /// Whether this entry is occupied.
    occupied: bool,
}

/// Represents a heap-allocated hash map.
///
/// Uses open addressing with linear probing for simplicity.
#[repr(C)]
struct KodoMap {
    /// Pointer to the entry array.
    entries: *mut KodoMapEntry,
    /// Number of occupied entries.
    len: usize,
    /// Total allocated capacity.
    capacity: usize,
}

/// Default initial capacity for a new map.
const MAP_INITIAL_CAPACITY: usize = 16;

/// Creates a new empty map.
///
/// Returns a pointer (as i64) to a heap-allocated `KodoMap`.
#[no_mangle]
pub extern "C" fn kodo_map_new() -> i64 {
    let entries = vec![
        KodoMapEntry {
            key: 0,
            value: 0,
            occupied: false,
        };
        MAP_INITIAL_CAPACITY
    ];
    let boxed = entries.into_boxed_slice();
    // SAFETY: intentionally leaks the entries array; ownership moves to KodoMap.
    let entries_ptr = Box::into_raw(boxed).cast::<KodoMapEntry>();
    let map = Box::new(KodoMap {
        entries: entries_ptr,
        len: 0,
        capacity: MAP_INITIAL_CAPACITY,
    });
    // SAFETY: intentionally leaks so caller manages via opaque pointer. Freed by `kodo_map_free`.
    Box::into_raw(map) as i64
}

/// Computes a simple hash for an i64 key.
fn map_hash(key: i64, capacity: usize) -> usize {
    // FNV-inspired mixing.
    #[allow(clippy::cast_sign_loss)]
    let k = key as u64;
    let mixed = k.wrapping_mul(0x517c_c1b7_2722_0a95);
    #[allow(clippy::cast_possible_truncation)]
    let index = mixed as usize;
    index % capacity
}

/// Inserts a key-value pair into the map, overwriting any existing entry with the same key.
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_insert(map_ptr: i64, key: i64, value: i64) {
    // SAFETY: caller guarantees map_ptr was returned by kodo_map_new.
    #[allow(clippy::cast_possible_truncation)]
    let map = unsafe { &mut *(map_ptr as *mut KodoMap) };

    // Grow if load factor > 0.75.
    if map.len * 4 >= map.capacity * 3 {
        map_grow(map);
    }

    let mut idx = map_hash(key, map.capacity);
    loop {
        // SAFETY: idx < capacity, entries is valid.
        let entry = unsafe { &mut *map.entries.add(idx) };
        if !entry.occupied {
            entry.key = key;
            entry.value = value;
            entry.occupied = true;
            map.len += 1;
            return;
        }
        if entry.key == key {
            entry.value = value;
            return;
        }
        idx = (idx + 1) % map.capacity;
    }
}

/// Grows the map's backing array by doubling capacity and rehashing all entries.
fn map_grow(map: &mut KodoMap) {
    let new_cap = map.capacity * 2;
    let new_entries = vec![
        KodoMapEntry {
            key: 0,
            value: 0,
            occupied: false,
        };
        new_cap
    ];
    let new_boxed = new_entries.into_boxed_slice();
    // SAFETY: intentionally leaks the new entries array; ownership moves to KodoMap.
    let new_ptr = Box::into_raw(new_boxed).cast::<KodoMapEntry>();

    // Rehash existing entries.
    for i in 0..map.capacity {
        // SAFETY: i < old capacity, entries is valid.
        let old_entry = unsafe { &*map.entries.add(i) };
        if old_entry.occupied {
            let mut idx = map_hash(old_entry.key, new_cap);
            loop {
                // SAFETY: idx < new_cap, new_ptr is valid.
                let new_entry = unsafe { &mut *new_ptr.add(idx) };
                if !new_entry.occupied {
                    new_entry.key = old_entry.key;
                    new_entry.value = old_entry.value;
                    new_entry.occupied = true;
                    break;
                }
                idx = (idx + 1) % new_cap;
            }
        }
    }

    // Free old entries.
    // SAFETY: entries was allocated as a Box<[KodoMapEntry]> with capacity elements.
    let _ = unsafe {
        Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            map.entries,
            map.capacity,
        ))
    };
    map.entries = new_ptr;
    map.capacity = new_cap;
}

/// Gets the value for a given key.
///
/// Returns the value via out parameters. `out_is_some` is set to 1 if found, 0 otherwise.
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`.
/// `out_value` and `out_is_some` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_get(
    map_ptr: i64,
    key: i64,
    out_value: *mut i64,
    out_is_some: *mut i64,
) {
    // SAFETY: caller guarantees map_ptr was returned by kodo_map_new.
    #[allow(clippy::cast_possible_truncation)]
    let map = unsafe { &*(map_ptr as *const KodoMap) };
    let mut idx = map_hash(key, map.capacity);
    for _ in 0..map.capacity {
        // SAFETY: idx < capacity, entries is valid.
        let entry = unsafe { &*map.entries.add(idx) };
        if !entry.occupied {
            // SAFETY: caller guarantees out_value and out_is_some are valid writable pointers.
            unsafe {
                *out_value = 0;
                *out_is_some = 0;
            }
            return;
        }
        if entry.key == key {
            // SAFETY: caller guarantees out_value and out_is_some are valid writable pointers.
            unsafe {
                *out_value = entry.value;
                *out_is_some = 1;
            }
            return;
        }
        idx = (idx + 1) % map.capacity;
    }
    // SAFETY: caller guarantees out_value and out_is_some are valid writable pointers.
    unsafe {
        *out_value = 0;
        *out_is_some = 0;
    }
}

/// Returns 1 if the map contains the given key, 0 otherwise.
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_contains_key(map_ptr: i64, key: i64) -> i64 {
    // SAFETY: caller guarantees map_ptr was returned by kodo_map_new.
    #[allow(clippy::cast_possible_truncation)]
    let map = unsafe { &*(map_ptr as *const KodoMap) };
    let mut idx = map_hash(key, map.capacity);
    for _ in 0..map.capacity {
        // SAFETY: idx < capacity, entries is valid.
        let entry = unsafe { &*map.entries.add(idx) };
        if !entry.occupied {
            return 0;
        }
        if entry.key == key {
            return 1;
        }
        idx = (idx + 1) % map.capacity;
    }
    0
}

/// Returns the number of entries in the map.
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_length(map_ptr: i64) -> i64 {
    // SAFETY: caller guarantees map_ptr was returned by kodo_map_new.
    #[allow(clippy::cast_possible_truncation)]
    let map = unsafe { &*(map_ptr as *const KodoMap) };
    #[allow(clippy::cast_possible_wrap)]
    let result = map.len as i64;
    result
}

/// Removes a key-value pair from the map.
///
/// Returns 1 if the key was found and removed, 0 otherwise.
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_remove(map_ptr: i64, key: i64) -> i64 {
    if map_ptr == 0 {
        return 0;
    }
    // SAFETY: caller guarantees map_ptr was returned by kodo_map_new.
    let map = unsafe { &mut *(map_ptr as *mut KodoMap) };
    if map.entries.is_null() || map.capacity == 0 {
        return 0;
    }
    let mut idx = map_hash(key, map.capacity);
    for _ in 0..map.capacity {
        // SAFETY: idx < capacity, entries is valid.
        let entry = unsafe { &mut *map.entries.add(idx) };
        if !entry.occupied {
            return 0; // Key not found
        }
        if entry.key == key {
            entry.occupied = false;
            entry.key = 0;
            entry.value = 0;
            map.len -= 1;
            return 1;
        }
        idx = (idx + 1) % map.capacity;
    }
    0
}

/// Returns 1 if the map is empty, 0 otherwise.
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_is_empty(map_ptr: i64) -> i64 {
    if map_ptr == 0 {
        return 1;
    }
    // SAFETY: caller guarantees map_ptr was returned by kodo_map_new.
    let map = unsafe { &*(map_ptr as *const KodoMap) };
    i64::from(map.len == 0)
}

/// Frees a heap-allocated `KodoMap` and its backing entries array.
///
/// Does nothing if `map_ptr` is zero (null handle).
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`, or zero.
/// After calling this function, the map pointer must not be used again.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_free(map_ptr: i64) {
    if map_ptr == 0 {
        return;
    }
    // SAFETY: caller guarantees map_ptr was returned by kodo_map_new
    // (i.e. Box::into_raw on a Box<KodoMap>).
    let map = unsafe { Box::from_raw(map_ptr as *mut KodoMap) };
    if !map.entries.is_null() && map.capacity > 0 {
        // SAFETY: entries was allocated as a Box<[KodoMapEntry]> with capacity elements.
        let _ = unsafe {
            Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                map.entries,
                map.capacity,
            ))
        };
    }
    // map is dropped here, freeing the KodoMap struct itself.
}

// ---------------------------------------------------------------------------
// Map iterator builtins
// ---------------------------------------------------------------------------

/// Internal state for an iterator over map keys.
///
/// Scans the map's entry array, skipping unoccupied slots. Each call to
/// `advance` moves to the next occupied entry.
struct KodoMapKeysIterator {
    /// Pointer to the owning map (for access to entries).
    map_ptr: *const KodoMap,
    /// Current scan index in the entry array.
    index: usize,
    /// Current key value.
    current_key: i64,
}

/// Internal state for an iterator over map values.
struct KodoMapValuesIterator {
    /// Pointer to the owning map (for access to entries).
    map_ptr: *const KodoMap,
    /// Current scan index in the entry array.
    index: usize,
    /// Current value.
    current_value: i64,
}

/// Creates a new key iterator for a map.
///
/// Returns an opaque handle (as i64) to a heap-allocated `KodoMapKeysIterator`.
/// The iterator starts before the first key; call `advance` to move to
/// the first element.
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_keys(map_ptr: i64) -> i64 {
    if map_ptr == 0 {
        return 0;
    }
    let iter = Box::new(KodoMapKeysIterator {
        map_ptr: map_ptr as *const KodoMap,
        index: 0,
        current_key: 0,
    });
    // SAFETY: intentionally leaks so caller manages via opaque handle.
    // Freed by `kodo_map_keys_free`.
    Box::into_raw(iter) as i64
}

/// Advances the map key iterator to the next occupied entry.
///
/// Returns 1 if a key was found, 0 if the iterator is exhausted.
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_map_keys`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_keys_advance(iter_ptr: i64) -> i64 {
    if iter_ptr == 0 {
        return 0;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_map_keys.
    let iter = unsafe { &mut *(iter_ptr as *mut KodoMapKeysIterator) };
    // SAFETY: caller guarantees map_ptr is a valid KodoMap pointer.
    let map = unsafe { &*iter.map_ptr };
    while iter.index < map.capacity {
        // SAFETY: index < capacity, entries is valid.
        let entry = unsafe { &*map.entries.add(iter.index) };
        iter.index += 1;
        if entry.occupied {
            iter.current_key = entry.key;
            return 1;
        }
    }
    0
}

/// Returns the current key from the map key iterator.
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_map_keys`.
/// Must be called after a successful `kodo_map_keys_advance` call.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_keys_value(iter_ptr: i64) -> i64 {
    if iter_ptr == 0 {
        return 0;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_map_keys.
    let iter = unsafe { &*(iter_ptr as *const KodoMapKeysIterator) };
    iter.current_key
}

/// Frees a map key iterator previously allocated by `kodo_map_keys`.
///
/// Does nothing if `iter_ptr` is zero (null handle).
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_map_keys`, or zero.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_keys_free(iter_ptr: i64) {
    if iter_ptr == 0 {
        return;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_map_keys
    // (i.e. Box::into_raw on a Box<KodoMapKeysIterator>).
    let _ = unsafe { Box::from_raw(iter_ptr as *mut KodoMapKeysIterator) };
}

/// Creates a new value iterator for a map.
///
/// Returns an opaque handle (as i64) to a heap-allocated `KodoMapValuesIterator`.
///
/// # Safety
///
/// `map_ptr` must be a valid pointer returned by `kodo_map_new`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_values(map_ptr: i64) -> i64 {
    if map_ptr == 0 {
        return 0;
    }
    let iter = Box::new(KodoMapValuesIterator {
        map_ptr: map_ptr as *const KodoMap,
        index: 0,
        current_value: 0,
    });
    // SAFETY: intentionally leaks so caller manages via opaque handle.
    // Freed by `kodo_map_values_free`.
    Box::into_raw(iter) as i64
}

/// Advances the map value iterator to the next occupied entry.
///
/// Returns 1 if a value was found, 0 if the iterator is exhausted.
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_map_values`.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_values_advance(iter_ptr: i64) -> i64 {
    if iter_ptr == 0 {
        return 0;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_map_values.
    let iter = unsafe { &mut *(iter_ptr as *mut KodoMapValuesIterator) };
    // SAFETY: caller guarantees map_ptr is a valid KodoMap pointer.
    let map = unsafe { &*iter.map_ptr };
    while iter.index < map.capacity {
        // SAFETY: index < capacity, entries is valid.
        let entry = unsafe { &*map.entries.add(iter.index) };
        iter.index += 1;
        if entry.occupied {
            iter.current_value = entry.value;
            return 1;
        }
    }
    0
}

/// Returns the current value from the map value iterator.
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_map_values`.
/// Must be called after a successful `kodo_map_values_advance` call.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_values_value(iter_ptr: i64) -> i64 {
    if iter_ptr == 0 {
        return 0;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_map_values.
    let iter = unsafe { &*(iter_ptr as *const KodoMapValuesIterator) };
    iter.current_value
}

/// Frees a map value iterator previously allocated by `kodo_map_values`.
///
/// Does nothing if `iter_ptr` is zero (null handle).
///
/// # Safety
///
/// `iter_ptr` must be a valid pointer returned by `kodo_map_values`, or zero.
#[no_mangle]
pub unsafe extern "C" fn kodo_map_values_free(iter_ptr: i64) {
    if iter_ptr == 0 {
        return;
    }
    // SAFETY: caller guarantees iter_ptr was returned by kodo_map_values
    // (i.e. Box::into_raw on a Box<KodoMapValuesIterator>).
    let _ = unsafe { Box::from_raw(iter_ptr as *mut KodoMapValuesIterator) };
}

// ---------------------------------------------------------------------------
// Actor runtime builtins
// ---------------------------------------------------------------------------

/// Size of each field slot in actor state (8 bytes for i64 alignment).
const ACTOR_FIELD_SIZE: usize = 8;

/// Allocates a new actor state buffer of `state_size` bytes on the heap.
///
/// Returns an opaque handle (as i64) to the allocated buffer, or 0 if
/// `state_size` is non-positive.
///
/// The buffer is zero-initialized so all fields start at their default value.
#[no_mangle]
pub extern "C" fn kodo_actor_new(state_size: i64) -> i64 {
    if state_size <= 0 {
        return 0;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = state_size as usize;
    let buffer = vec![0u8; size].into_boxed_slice();
    // SAFETY: intentionally leaks so caller manages via opaque handle.
    // Freed by `kodo_actor_free`.
    let ptr = Box::into_raw(buffer);
    ptr.cast::<u8>() as i64
}

/// Reads an i64 value from the actor state buffer at the given byte offset.
///
/// Returns 0 if `actor_ptr` is zero (null handle) or the offset is negative.
///
/// # Safety
///
/// `actor_ptr` must be a valid pointer returned by `kodo_actor_new`, or zero.
/// `offset` must be aligned to 8 bytes and within the allocated buffer.
#[no_mangle]
pub extern "C" fn kodo_actor_get_field(actor_ptr: i64, offset: i64) -> i64 {
    if actor_ptr == 0 || offset < 0 {
        return 0;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let off = offset as usize;
    // SAFETY: caller guarantees actor_ptr points to a valid buffer of
    // sufficient size and offset is within bounds and 8-byte aligned.
    unsafe {
        let base = actor_ptr as *const u8;
        #[allow(clippy::cast_ptr_alignment)]
        let field_ptr = base.add(off).cast::<i64>();
        *field_ptr
    }
}

/// Writes an i64 value to the actor state buffer at the given byte offset.
///
/// Does nothing if `actor_ptr` is zero (null handle) or the offset is negative.
///
/// # Safety
///
/// `actor_ptr` must be a valid pointer returned by `kodo_actor_new`, or zero.
/// `offset` must be aligned to 8 bytes and within the allocated buffer.
#[no_mangle]
pub extern "C" fn kodo_actor_set_field(actor_ptr: i64, offset: i64, value: i64) {
    if actor_ptr == 0 || offset < 0 {
        return;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let off = offset as usize;
    // SAFETY: caller guarantees actor_ptr points to a valid buffer of
    // sufficient size and offset is within bounds and 8-byte aligned.
    unsafe {
        let base = actor_ptr as *mut u8;
        #[allow(clippy::cast_ptr_alignment)]
        let field_ptr = base.add(off).cast::<i64>();
        *field_ptr = value;
    }
}

/// Queues a message to an actor by spawning a task that calls the handler.
///
/// `handler_fn` is a function pointer to the compiled handler (which takes
/// `(actor_ptr: i64, arg: i64)` as parameters). The task is enqueued in the
/// scheduler and runs when `kodo_run_scheduler` is called.
///
/// Does nothing if `actor_ptr` is zero (null handle) or `handler_fn` is zero.
#[no_mangle]
pub extern "C" fn kodo_actor_send(actor_ptr: i64, handler_fn: i64, arg: i64) {
    if actor_ptr == 0 || handler_fn == 0 {
        return;
    }
    // Pack actor_ptr and arg into a two-element environment buffer.
    let env: [i64; 2] = [actor_ptr, arg];
    let env_ptr = env.as_ptr() as i64;
    #[allow(clippy::cast_possible_wrap)]
    let env_size = (ACTOR_FIELD_SIZE * 2) as i64;
    crate::scheduler::kodo_spawn_task_with_env(handler_fn, env_ptr, env_size);
}

/// Frees an actor state buffer previously allocated by `kodo_actor_new`.
///
/// Does nothing if `actor_ptr` is zero (null handle).
///
/// # Safety
///
/// `actor_ptr` must be a valid pointer returned by `kodo_actor_new`, or zero.
/// The `state_size` must match the value originally passed to `kodo_actor_new`.
/// After calling this function, the actor pointer must not be used again.
#[no_mangle]
pub extern "C" fn kodo_actor_free(actor_ptr: i64, state_size: i64) {
    if actor_ptr == 0 || state_size <= 0 {
        return;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = state_size as usize;
    // SAFETY: caller guarantees actor_ptr was returned by kodo_actor_new
    // with exactly `state_size` bytes (i.e. Box::into_raw on a Box<[u8]>
    // of `size` bytes).
    let _ = unsafe {
        Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            actor_ptr as *mut u8,
            size,
        ))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_new_and_push() {
        let list = kodo_list_new();
        assert_ne!(list, 0);
        unsafe { kodo_list_push(list, 42) };
        assert_eq!(unsafe { kodo_list_length(list) }, 1);
        unsafe { kodo_list_push(list, 99) };
        assert_eq!(unsafe { kodo_list_length(list) }, 2);
    }

    #[test]
    fn list_get_works() {
        let list = kodo_list_new();
        unsafe {
            kodo_list_push(list, 10);
            kodo_list_push(list, 20);
        }
        let mut value: i64 = 0;
        let mut is_some: i64 = 0;
        unsafe { kodo_list_get(list, 0, &mut value, &mut is_some) };
        assert_eq!(is_some, 1);
        assert_eq!(value, 10);
        unsafe { kodo_list_get(list, 5, &mut value, &mut is_some) };
        assert_eq!(is_some, 0);
    }

    #[test]
    fn list_contains_works() {
        let list = kodo_list_new();
        unsafe {
            kodo_list_push(list, 10);
            kodo_list_push(list, 20);
        }
        assert_eq!(unsafe { kodo_list_contains(list, 10) }, 1);
        assert_eq!(unsafe { kodo_list_contains(list, 30) }, 0);
    }

    #[test]
    fn list_grows_dynamically() {
        let list = kodo_list_new();
        for i in 0..100 {
            unsafe { kodo_list_push(list, i) };
        }
        assert_eq!(unsafe { kodo_list_length(list) }, 100);
    }

    #[test]
    fn map_new_and_insert() {
        let map = kodo_map_new();
        assert_ne!(map, 0);
        unsafe { kodo_map_insert(map, 1, 100) };
        assert_eq!(unsafe { kodo_map_length(map) }, 1);
    }

    #[test]
    fn map_get_works() {
        let map = kodo_map_new();
        unsafe {
            kodo_map_insert(map, 1, 100);
            kodo_map_insert(map, 2, 200);
        }
        let mut value: i64 = 0;
        let mut is_some: i64 = 0;
        unsafe { kodo_map_get(map, 1, &mut value, &mut is_some) };
        assert_eq!(is_some, 1);
        assert_eq!(value, 100);
        unsafe { kodo_map_get(map, 3, &mut value, &mut is_some) };
        assert_eq!(is_some, 0);
    }

    #[test]
    fn map_contains_key_works() {
        let map = kodo_map_new();
        unsafe { kodo_map_insert(map, 42, 1) };
        assert_eq!(unsafe { kodo_map_contains_key(map, 42) }, 1);
        assert_eq!(unsafe { kodo_map_contains_key(map, 99) }, 0);
    }

    #[test]
    fn actor_new_returns_nonzero() {
        let handle = kodo_actor_new(16);
        assert_ne!(handle, 0);
        kodo_actor_free(handle, 16);
    }

    #[test]
    fn actor_set_get_roundtrip() {
        let actor = kodo_actor_new(24);
        kodo_actor_set_field(actor, 0, 42);
        kodo_actor_set_field(actor, 8, 100);
        assert_eq!(kodo_actor_get_field(actor, 0), 42);
        assert_eq!(kodo_actor_get_field(actor, 8), 100);
        kodo_actor_free(actor, 24);
    }

    #[test]
    fn actor_null_safe() {
        assert_eq!(kodo_actor_get_field(0, 0), 0);
        kodo_actor_set_field(0, 0, 42);
        kodo_actor_free(0, 8);
    }

    #[test]
    fn map_keys_iterator_basic() {
        let map = kodo_map_new();
        unsafe {
            kodo_map_insert(map, 10, 100);
            kodo_map_insert(map, 20, 200);
            kodo_map_insert(map, 30, 300);
        }
        let iter = unsafe { kodo_map_keys(map) };
        assert_ne!(iter, 0);

        let mut keys = Vec::new();
        while unsafe { kodo_map_keys_advance(iter) } == 1 {
            keys.push(unsafe { kodo_map_keys_value(iter) });
        }
        keys.sort();
        assert_eq!(keys, vec![10, 20, 30]);

        unsafe { kodo_map_keys_free(iter) };
    }

    #[test]
    fn map_values_iterator_basic() {
        let map = kodo_map_new();
        unsafe {
            kodo_map_insert(map, 1, 100);
            kodo_map_insert(map, 2, 200);
            kodo_map_insert(map, 3, 300);
        }
        let iter = unsafe { kodo_map_values(map) };
        assert_ne!(iter, 0);

        let mut values = Vec::new();
        while unsafe { kodo_map_values_advance(iter) } == 1 {
            values.push(unsafe { kodo_map_values_value(iter) });
        }
        values.sort();
        assert_eq!(values, vec![100, 200, 300]);

        unsafe { kodo_map_values_free(iter) };
    }

    #[test]
    fn map_keys_empty_map() {
        let map = kodo_map_new();
        let iter = unsafe { kodo_map_keys(map) };
        assert_eq!(unsafe { kodo_map_keys_advance(iter) }, 0);
        unsafe { kodo_map_keys_free(iter) };
    }

    #[test]
    fn map_values_empty_map() {
        let map = kodo_map_new();
        let iter = unsafe { kodo_map_values(map) };
        assert_eq!(unsafe { kodo_map_values_advance(iter) }, 0);
        unsafe { kodo_map_values_free(iter) };
    }

    #[test]
    fn map_keys_free_null_does_not_crash() {
        unsafe { kodo_map_keys_free(0) };
    }

    #[test]
    fn map_values_free_null_does_not_crash() {
        unsafe { kodo_map_values_free(0) };
    }

    #[test]
    fn map_keys_null_map_returns_zero() {
        let iter = unsafe { kodo_map_keys(0) };
        assert_eq!(iter, 0);
    }

    #[test]
    fn map_values_null_map_returns_zero() {
        let iter = unsafe { kodo_map_values(0) };
        assert_eq!(iter, 0);
    }

    #[test]
    fn map_keys_overwritten_value() {
        let map = kodo_map_new();
        unsafe {
            kodo_map_insert(map, 1, 100);
            kodo_map_insert(map, 1, 200); // overwrite
        }
        let iter = unsafe { kodo_map_keys(map) };
        let mut keys = Vec::new();
        while unsafe { kodo_map_keys_advance(iter) } == 1 {
            keys.push(unsafe { kodo_map_keys_value(iter) });
        }
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], 1);
        unsafe { kodo_map_keys_free(iter) };
    }

    #[test]
    fn map_keys_many_entries() {
        let map = kodo_map_new();
        for i in 0..50 {
            unsafe { kodo_map_insert(map, i, i * 10) };
        }
        let iter = unsafe { kodo_map_keys(map) };
        let mut count = 0;
        while unsafe { kodo_map_keys_advance(iter) } == 1 {
            count += 1;
        }
        assert_eq!(count, 50);
        unsafe { kodo_map_keys_free(iter) };
    }

    #[test]
    fn map_remove_existing_key() {
        let map = kodo_map_new();
        unsafe {
            kodo_map_insert(map, 1, 100);
            kodo_map_insert(map, 2, 200);
        }
        assert_eq!(unsafe { kodo_map_length(map) }, 2);
        assert_eq!(unsafe { kodo_map_remove(map, 1) }, 1);
        assert_eq!(unsafe { kodo_map_length(map) }, 1);
        assert_eq!(unsafe { kodo_map_contains_key(map, 1) }, 0);
        assert_eq!(unsafe { kodo_map_contains_key(map, 2) }, 1);
    }

    #[test]
    fn map_remove_nonexistent_key() {
        let map = kodo_map_new();
        unsafe { kodo_map_insert(map, 1, 100) };
        assert_eq!(unsafe { kodo_map_remove(map, 99) }, 0);
        assert_eq!(unsafe { kodo_map_length(map) }, 1);
    }

    #[test]
    fn map_remove_null_map() {
        assert_eq!(unsafe { kodo_map_remove(0, 1) }, 0);
    }

    #[test]
    fn map_is_empty_works() {
        let map = kodo_map_new();
        assert_eq!(unsafe { kodo_map_is_empty(map) }, 1);
        unsafe { kodo_map_insert(map, 1, 100) };
        assert_eq!(unsafe { kodo_map_is_empty(map) }, 0);
        unsafe { kodo_map_remove(map, 1) };
        assert_eq!(unsafe { kodo_map_is_empty(map) }, 1);
    }

    #[test]
    fn map_is_empty_null_map() {
        assert_eq!(unsafe { kodo_map_is_empty(0) }, 1);
    }

    #[test]
    fn list_slice_basic() {
        let list = kodo_list_new();
        unsafe {
            kodo_list_push(list, 10);
            kodo_list_push(list, 20);
            kodo_list_push(list, 30);
            kodo_list_push(list, 40);
            kodo_list_push(list, 50);
        }
        let sliced = unsafe { kodo_list_slice(list, 1, 4) };
        assert_eq!(unsafe { kodo_list_length(sliced) }, 3);
        let mut val: i64 = 0;
        let mut is_some: i64 = 0;
        unsafe { kodo_list_get(sliced, 0, &mut val, &mut is_some) };
        assert_eq!(val, 20);
        unsafe { kodo_list_get(sliced, 1, &mut val, &mut is_some) };
        assert_eq!(val, 30);
        unsafe { kodo_list_get(sliced, 2, &mut val, &mut is_some) };
        assert_eq!(val, 40);
    }

    #[test]
    fn list_slice_clamped() {
        let list = kodo_list_new();
        unsafe {
            kodo_list_push(list, 1);
            kodo_list_push(list, 2);
        }
        // Out-of-range indices should be clamped.
        let sliced = unsafe { kodo_list_slice(list, -5, 100) };
        assert_eq!(unsafe { kodo_list_length(sliced) }, 2);
    }

    #[test]
    fn list_slice_empty_range() {
        let list = kodo_list_new();
        unsafe { kodo_list_push(list, 1) };
        let sliced = unsafe { kodo_list_slice(list, 3, 1) };
        assert_eq!(unsafe { kodo_list_length(sliced) }, 0);
    }

    #[test]
    fn list_sort_ascending() {
        let list = kodo_list_new();
        unsafe {
            kodo_list_push(list, 5);
            kodo_list_push(list, 1);
            kodo_list_push(list, 3);
            kodo_list_push(list, 2);
            kodo_list_push(list, 4);
        }
        unsafe { kodo_list_sort(list) };
        let mut val: i64 = 0;
        let mut is_some: i64 = 0;
        for i in 0..5 {
            unsafe { kodo_list_get(list, i, &mut val, &mut is_some) };
            assert_eq!(val, i + 1);
        }
    }

    #[test]
    fn list_sort_empty() {
        let list = kodo_list_new();
        // Should not crash on empty list.
        unsafe { kodo_list_sort(list) };
        assert_eq!(unsafe { kodo_list_length(list) }, 0);
    }

    #[test]
    fn list_sort_single_element() {
        let list = kodo_list_new();
        unsafe { kodo_list_push(list, 42) };
        unsafe { kodo_list_sort(list) };
        assert_eq!(unsafe { kodo_list_length(list) }, 1);
    }
}
