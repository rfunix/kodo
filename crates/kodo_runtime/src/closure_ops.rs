//! Runtime support for closure environment capture operations.
//!
//! These functions are called by the LLVM backend for the synthetic
//! `__env_pack` and `__env_load` operations. The Cranelift backend handles
//! these inline (allocating stack slots and emitting loads/stores directly),
//! but the LLVM backend emits them as external function calls.
//!
//! ## Environment layout
//!
//! A closure environment is a heap-allocated buffer of `i64` values.
//! Each captured variable occupies 8 bytes at a known offset.
//!
//! - `__env_pack(v0, v1, ..., vN)` allocates `(N+1) * 8` bytes, stores each
//!   value at its offset, and returns a pointer (as `i64`).
//! - `__env_load(env_ptr, byte_offset)` reads the `i64` at `env_ptr + offset`.
//! - `__env_load_string(env_ptr, byte_offset)` reads a pointer at the given
//!   offset, then copies the 16-byte String slot `(ptr, len)` from that
//!   address. For simplicity in the C ABI, this returns the pointer to the
//!   string slot (the caller dereferences it in LLVM IR).

use std::io::Write;

/// Allocates an environment buffer and packs captured values into it.
///
/// This is a **variadic** C function. The first argument is the number of
/// captures (`count`), followed by `count` `i64` values to pack.
///
/// Returns a pointer (as `i64`) to the allocated buffer.
///
/// # Safety
///
/// The caller must pass exactly `count` additional `i64` arguments after
/// the count parameter. The variadic arguments are read via `va_list`.
///
/// # Note
///
/// Because Rust does not support C variadic functions in a stable way that
/// also works with `#[no_mangle] extern "C"`, we use a fixed-arity helper
/// approach: the LLVM backend actually passes the values as a pointer to a
/// stack-allocated array. However, looking at the LLVM IR generated, the
/// call uses LLVM's variadic calling convention.
///
/// As an alternative, we provide fixed-arity versions for common capture
/// counts (0 through 8), and a fallback pointer-based version.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_0() -> i64 {
    // SAFETY: allocating zero bytes; return a sentinel non-null pointer.
    let layout = std::alloc::Layout::from_size_align(8, 8);
    let Ok(layout) = layout else { return 0 };
    // SAFETY: layout is valid and non-zero size.
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    ptr as i64
}

/// Packs 1 captured value into an environment buffer.
///
/// # Safety
///
/// Returned pointer is valid for 8 bytes.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_1(v0: i64) -> i64 {
    env_pack_n(&[v0])
}

/// Packs 2 captured values into an environment buffer.
///
/// # Safety
///
/// Returned pointer is valid for 16 bytes.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_2(v0: i64, v1: i64) -> i64 {
    env_pack_n(&[v0, v1])
}

/// Packs 3 captured values into an environment buffer.
///
/// # Safety
///
/// Returned pointer is valid for 24 bytes.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_3(v0: i64, v1: i64, v2: i64) -> i64 {
    env_pack_n(&[v0, v1, v2])
}

/// Packs 4 captured values into an environment buffer.
///
/// # Safety
///
/// Returned pointer is valid for 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_4(v0: i64, v1: i64, v2: i64, v3: i64) -> i64 {
    env_pack_n(&[v0, v1, v2, v3])
}

/// Packs 5 captured values into an environment buffer.
///
/// # Safety
///
/// Returned pointer is valid for 40 bytes.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_5(v0: i64, v1: i64, v2: i64, v3: i64, v4: i64) -> i64 {
    env_pack_n(&[v0, v1, v2, v3, v4])
}

/// Packs 6 captured values into an environment buffer.
///
/// # Safety
///
/// Returned pointer is valid for 48 bytes.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_6(v0: i64, v1: i64, v2: i64, v3: i64, v4: i64, v5: i64) -> i64 {
    env_pack_n(&[v0, v1, v2, v3, v4, v5])
}

/// Packs 7 captured values into an environment buffer.
///
/// # Safety
///
/// Returned pointer is valid for 56 bytes.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_7(
    v0: i64,
    v1: i64,
    v2: i64,
    v3: i64,
    v4: i64,
    v5: i64,
    v6: i64,
) -> i64 {
    env_pack_n(&[v0, v1, v2, v3, v4, v5, v6])
}

/// Packs 8 captured values into an environment buffer.
///
/// # Safety
///
/// Returned pointer is valid for 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn __env_pack_8(
    v0: i64,
    v1: i64,
    v2: i64,
    v3: i64,
    v4: i64,
    v5: i64,
    v6: i64,
    v7: i64,
) -> i64 {
    env_pack_n(&[v0, v1, v2, v3, v4, v5, v6, v7])
}

/// Internal helper: allocate and pack N values.
fn env_pack_n(values: &[i64]) -> i64 {
    let size = values.len() * 8;
    // Ensure at least 8 bytes to avoid zero-size allocation.
    let alloc_size = if size == 0 { 8 } else { size };
    let Ok(layout) = std::alloc::Layout::from_size_align(alloc_size, 8) else {
        return 0;
    };
    // SAFETY: layout is valid and non-zero size.
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        let _ = writeln!(std::io::stderr(), "env_pack: allocation failed");
        std::process::abort();
    }
    #[allow(clippy::cast_ptr_alignment)]
    let slot_ptr = ptr.cast::<i64>();
    for (i, val) in values.iter().enumerate() {
        // SAFETY: we allocated enough space for all values.
        unsafe {
            *slot_ptr.add(i) = *val;
        }
    }
    ptr as i64
}

/// Loads an `i64` value from an environment buffer at the given byte offset.
///
/// # Safety
///
/// `env_ptr` must be a valid pointer (as `i64`) to a heap-allocated
/// environment buffer, and `byte_offset` must be within bounds.
#[no_mangle]
pub unsafe extern "C" fn __env_load(env_ptr: i64, byte_offset: i64) -> i64 {
    // SAFETY: caller guarantees the pointer and offset are valid.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let ptr = unsafe { (env_ptr as *const u8).add(byte_offset as usize) };
    #[allow(clippy::cast_ptr_alignment)]
    unsafe {
        *(ptr.cast::<i64>())
    }
}

/// Loads a string from an environment buffer into a destination slot.
///
/// The environment stores a pointer to a 16-byte string slot `(ptr, len)`.
/// This function reads that pointer from `env_ptr + byte_offset`, then
/// copies the 16-byte string slot into `dest_slot`.
///
/// Signature: `void __env_load_string(dest_slot, env_ptr, byte_offset)`
///
/// # Safety
///
/// - `dest_slot` must be a valid pointer (as `i64`) to a 16-byte
///   writable string slot `[ptr: i64, len: i64]`.
/// - `env_ptr` must be a valid pointer to an environment buffer.
/// - `byte_offset` must point to a valid string slot pointer within it.
#[no_mangle]
pub unsafe extern "C" fn __env_load_string(dest_slot: i64, env_ptr: i64, byte_offset: i64) {
    // Load the pointer to the source string slot from the env buffer.
    // SAFETY: caller guarantees the pointer and offset are valid.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let src_addr = unsafe {
        let ptr = (env_ptr as *const u8).add(byte_offset as usize);
        #[allow(clippy::cast_ptr_alignment)]
        *(ptr.cast::<i64>())
    };
    // Copy 16 bytes (ptr + len) from source to destination.
    // SAFETY: both src and dest point to valid 16-byte slots.
    let src = src_addr as *const i64;
    let dst = dest_slot as *mut i64;
    unsafe {
        *dst = *src;
        *dst.add(1) = *src.add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_pack_0_returns_non_null() {
        let ptr = unsafe { __env_pack_0() };
        assert_ne!(ptr, 0);
    }

    #[test]
    fn env_pack_1_and_load() {
        let env = unsafe { __env_pack_1(42) };
        assert_ne!(env, 0);
        let val = unsafe { __env_load(env, 0) };
        assert_eq!(val, 42);
    }

    #[test]
    fn env_pack_3_and_load_all() {
        let env = unsafe { __env_pack_3(10, 20, 30) };
        assert_ne!(env, 0);
        assert_eq!(unsafe { __env_load(env, 0) }, 10);
        assert_eq!(unsafe { __env_load(env, 8) }, 20);
        assert_eq!(unsafe { __env_load(env, 16) }, 30);
    }

    #[test]
    fn env_pack_8_and_load_last() {
        let env = unsafe { __env_pack_8(1, 2, 3, 4, 5, 6, 7, 8) };
        assert_ne!(env, 0);
        assert_eq!(unsafe { __env_load(env, 56) }, 8);
        assert_eq!(unsafe { __env_load(env, 0) }, 1);
    }

    #[test]
    fn env_load_string_copies_slot() {
        // Store a pointer to a string slot in the env buffer.
        let string_slot: [i64; 2] = [0xDEAD, 5]; // fake ptr + len
        let slot_ptr = string_slot.as_ptr() as i64;
        let env = unsafe { __env_pack_1(slot_ptr) };
        // Destination slot to receive the copy.
        let mut dest: [i64; 2] = [0, 0];
        unsafe { __env_load_string(dest.as_mut_ptr() as i64, env, 0) };
        assert_eq!(dest[0], 0xDEAD);
        assert_eq!(dest[1], 5);
    }
}
