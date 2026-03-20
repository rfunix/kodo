//! Generic channels — type-erased binary serialization for any Kodo type.
//!
//! Channels store payloads as raw byte buffers. The compiler passes the
//! type's size at each send/recv call site based on the layout computed
//! during codegen. This allows channels to carry structs, enums, lists,
//! and any other type without runtime type information.
//!
//! ## Integration with green threads
//!
//! The [`kodo_channel_generic_recv`] function uses `try_recv()` in a loop
//! with [`crate::green::kodo_green_maybe_yield`] between attempts, so it
//! cooperates with the green thread scheduler instead of blocking an OS
//! thread.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

/// A generic channel that transports raw byte buffers.
///
/// Both sender and receiver are wrapped in `Arc<Mutex<…>>` so multiple
/// green threads can safely share access via the global registry.
struct GenericChannel {
    /// The sending half of the channel.
    sender: Arc<Mutex<mpsc::Sender<Vec<u8>>>>,
    /// The receiving half of the channel.
    receiver: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

/// Global registry of live generic channels, keyed by handle.
static GENERIC_CHANNEL_REGISTRY: Mutex<Option<HashMap<i64, GenericChannel>>> = Mutex::new(None);

/// Monotonically increasing counter for generic channel handles.
///
/// Starts at 1 so that handle 0 can be reserved as an invalid/null sentinel.
static GENERIC_CHANNEL_COUNTER: AtomicI64 = AtomicI64::new(1);

/// Returns a mutable reference to the registry, initialising it on first access.
fn with_registry<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut HashMap<i64, GenericChannel>) -> R,
{
    let mut guard = GENERIC_CHANNEL_REGISTRY.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    Some(f(map))
}

/// Creates a new generic channel and returns an opaque integer handle.
///
/// The handle can be passed to [`kodo_channel_generic_send`],
/// [`kodo_channel_generic_recv`], and [`kodo_channel_generic_free`].
#[no_mangle]
pub extern "C" fn kodo_channel_generic_new() -> i64 {
    let handle = GENERIC_CHANNEL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    let entry = GenericChannel {
        sender: Arc::new(Mutex::new(tx)),
        receiver: Arc::new(Mutex::new(rx)),
    };
    with_registry(|map| {
        map.insert(handle, entry);
    });
    handle
}

/// Sends `data_size` bytes from `data_ptr` through a generic channel.
///
/// The data is copied into a heap-allocated `Vec<u8>` before being sent,
/// so the caller may free or overwrite the source buffer immediately after
/// this function returns.
///
/// If the handle is invalid or the receiver has been dropped, the call is
/// a silent no-op (fire-and-forget semantics).
///
/// # Safety
///
/// - `data_ptr` must point to a readable buffer of at least `data_size` bytes.
/// - `data_size` must be non-negative.
#[no_mangle]
pub unsafe extern "C" fn kodo_channel_generic_send(handle: i64, data_ptr: i64, data_size: i64) {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = data_size as usize;

    let mut buf = vec![0u8; size];
    if size > 0 {
        // SAFETY: caller guarantees data_ptr points to data_size readable bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(data_ptr as *const u8, buf.as_mut_ptr(), size);
        }
    }

    let sender = with_registry(|map| map.get(&handle).map(|ch| Arc::clone(&ch.sender))).flatten();

    if let Some(tx_arc) = sender {
        if let Ok(tx) = tx_arc.lock() {
            let _ = tx.send(buf);
        }
    }
}

/// Receives data from a generic channel, copying the bytes to `out_ptr`.
///
/// Uses `try_recv()` in a loop with green thread yields between attempts,
/// so it cooperates with the scheduler instead of blocking an OS thread.
///
/// Returns `1` on success (data was copied to `out_ptr`), or `0` if the
/// channel has been closed (all senders dropped).
///
/// # Safety
///
/// - `out_ptr` must point to a writable buffer of at least `data_size` bytes.
/// - `data_size` must match the size used by the sender.
#[no_mangle]
pub unsafe extern "C" fn kodo_channel_generic_recv(
    handle: i64,
    out_ptr: i64,
    data_size: i64,
) -> i64 {
    let receiver =
        with_registry(|map| map.get(&handle).map(|ch| Arc::clone(&ch.receiver))).flatten();

    let Some(rx_arc) = receiver else {
        return 0;
    };

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let size = data_size as usize;

    loop {
        let result = {
            let Ok(rx) = rx_arc.lock() else {
                return 0;
            };
            rx.try_recv()
        };

        match result {
            Ok(data) => {
                let copy_len = size.min(data.len());
                if copy_len > 0 {
                    // SAFETY: caller guarantees out_ptr is writable for data_size bytes.
                    unsafe {
                        std::ptr::copy_nonoverlapping(data.as_ptr(), out_ptr as *mut u8, copy_len);
                    }
                }
                return 1;
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Yield to the green thread scheduler and retry.
                // SAFETY: kodo_green_maybe_yield is safe to call — it checks
                // whether we are inside a green thread and is a no-op if not.
                unsafe {
                    crate::green::kodo_green_maybe_yield();
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return 0;
            }
        }
    }
}

/// Frees the generic channel identified by `handle`.
///
/// Drops both sender and receiver, closing the channel. Subsequent send or
/// recv calls on this handle will be no-ops / return 0.
#[no_mangle]
pub extern "C" fn kodo_channel_generic_free(handle: i64) {
    with_registry(|map| {
        map.remove(&handle);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_channel_send_recv_int() {
        let ch = kodo_channel_generic_new();
        let value: i64 = 42;
        let size = std::mem::size_of::<i64>() as i64;

        // SAFETY: we pass a pointer to a valid i64 and its exact size.
        unsafe {
            kodo_channel_generic_send(ch, std::ptr::addr_of!(value) as i64, size);
        }

        let mut result: i64 = 0;
        // SAFETY: we pass a pointer to a valid i64 buffer.
        let ok =
            unsafe { kodo_channel_generic_recv(ch, std::ptr::addr_of_mut!(result) as i64, size) };

        assert_eq!(ok, 1, "recv should succeed");
        assert_eq!(result, 42, "received value should match sent value");

        kodo_channel_generic_free(ch);
    }

    #[test]
    fn generic_channel_send_recv_struct() {
        /// A multi-field struct to simulate sending complex data.
        #[repr(C)]
        #[derive(Debug, PartialEq, Clone, Copy)]
        struct Point {
            x: i64,
            y: i64,
            z: i64,
        }

        let ch = kodo_channel_generic_new();
        let point = Point {
            x: 10,
            y: 20,
            z: 30,
        };
        let size = std::mem::size_of::<Point>() as i64;

        // SAFETY: passing a pointer to a valid Point struct.
        unsafe {
            kodo_channel_generic_send(ch, std::ptr::addr_of!(point) as i64, size);
        }

        let mut received = Point { x: 0, y: 0, z: 0 };
        // SAFETY: passing a pointer to a valid Point buffer.
        let ok =
            unsafe { kodo_channel_generic_recv(ch, std::ptr::addr_of_mut!(received) as i64, size) };

        assert_eq!(ok, 1, "recv should succeed");
        assert_eq!(received, point, "received struct should match sent struct");

        kodo_channel_generic_free(ch);
    }

    #[test]
    fn generic_channel_disconnected_returns_zero() {
        let ch = kodo_channel_generic_new();

        // Free the channel (drops sender and receiver).
        kodo_channel_generic_free(ch);

        let mut buf: i64 = 0;
        let size = std::mem::size_of::<i64>() as i64;

        // Recv on a freed channel should return 0 (handle not found).
        // SAFETY: passing a valid buffer pointer.
        let ok = unsafe { kodo_channel_generic_recv(ch, std::ptr::addr_of_mut!(buf) as i64, size) };

        assert_eq!(ok, 0, "recv on freed channel should return 0");
    }

    #[test]
    fn generic_channel_free_is_idempotent() {
        let ch = kodo_channel_generic_new();
        kodo_channel_generic_free(ch);
        // Freeing again should not panic.
        kodo_channel_generic_free(ch);
    }

    #[test]
    fn generic_channel_multiple_sends() {
        let ch = kodo_channel_generic_new();
        let size = std::mem::size_of::<i64>() as i64;

        for i in 0..5_i64 {
            // SAFETY: passing a pointer to a valid i64.
            unsafe {
                kodo_channel_generic_send(ch, std::ptr::addr_of!(i) as i64, size);
            }
        }

        for expected in 0..5_i64 {
            let mut result: i64 = -1;
            // SAFETY: passing a pointer to a valid i64 buffer.
            let ok = unsafe {
                kodo_channel_generic_recv(ch, std::ptr::addr_of_mut!(result) as i64, size)
            };
            assert_eq!(ok, 1);
            assert_eq!(result, expected);
        }

        kodo_channel_generic_free(ch);
    }
}
