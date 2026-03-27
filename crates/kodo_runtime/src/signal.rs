//! # Signal Handler for Growable Stacks
//!
//! Installs a SIGSEGV handler that detects stack overflow on green thread
//! guard pages and grows the stack transparently.
//!
//! When a green thread's stack pointer reaches the guard page, the handler:
//! 1. Checks if the fault address matches a registered guard page
//! 2. If yes: extends the stack by making the guard page writable and
//!    allocating a new guard page below
//! 3. If no: re-raises SIGSEGV for default handling (real crash)

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// Maximum stack size per green thread (8 MB).
const MAX_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Registry mapping guard page addresses to stack metadata.
///
/// Each entry stores `(stack_base, current_total_size)` so the handler
/// can grow the stack and update the guard page.
static GUARD_REGISTRY: LazyLock<Mutex<HashMap<usize, GuardEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether the signal handler has been installed.
static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Metadata for a registered guard page.
#[derive(Debug, Clone, Copy)]
struct GuardEntry {
    /// Base (lowest address) of the entire stack mapping.
    stack_base: usize,
    /// Current total size of the stack mapping.
    stack_size: usize,
}

/// Installs the SIGSEGV signal handler for growable stacks.
///
/// Safe to call multiple times — only installs once.
///
/// # Panics
///
/// Panics if `sigaction` fails to install the handler.
pub fn install_signal_handler() {
    if HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        return; // Already installed
    }

    // SAFETY: We set up a valid sigaction with SA_SIGINFO to receive
    // the fault address via siginfo_t.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = sigsegv_handler as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
        libc::sigemptyset(std::ptr::addr_of_mut!(sa.sa_mask));

        let ret = libc::sigaction(libc::SIGSEGV, std::ptr::addr_of!(sa), std::ptr::null_mut());
        assert!(
            ret == 0,
            "failed to install SIGSEGV handler for growable stacks"
        );

        // Also handle SIGBUS on macOS (guard page violations sometimes
        // arrive as SIGBUS instead of SIGSEGV on Apple Silicon).
        #[cfg(target_os = "macos")]
        {
            let ret = libc::sigaction(libc::SIGBUS, std::ptr::addr_of!(sa), std::ptr::null_mut());
            assert!(
                ret == 0,
                "failed to install SIGBUS handler for growable stacks"
            );
        }
    }
}

/// Registers a guard page for a green thread stack.
///
/// Called by [`super::green::alloc_stack`] after setting up the guard page.
pub fn register_guard(guard_addr: usize, stack_base: usize, stack_size: usize) {
    if let Ok(mut registry) = GUARD_REGISTRY.lock() {
        registry.insert(
            guard_addr,
            GuardEntry {
                stack_base,
                stack_size,
            },
        );
    }
}

/// Unregisters a guard page when a green thread's stack is freed.
///
/// Called by [`super::green::free_stack`].
pub fn unregister_guard(stack_base: usize) {
    if let Ok(mut registry) = GUARD_REGISTRY.lock() {
        // Remove any entry whose stack_base matches (guard addr may have changed).
        registry.retain(|_, entry| entry.stack_base != stack_base);
    }
}

/// The SIGSEGV/SIGBUS signal handler.
///
/// Checks if the fault address is a known guard page. If so, grows the
/// stack. Otherwise, re-raises the signal for default handling.
///
/// # Safety
///
/// This is a signal handler — only async-signal-safe operations are allowed.
/// We use a `try_lock` on the mutex; if the lock is held (unlikely during
/// signal), we fall through to default handling.
extern "C" fn sigsegv_handler(
    _sig: libc::c_int,
    info: *mut libc::siginfo_t,
    _ctx: *mut libc::c_void,
) {
    // SAFETY: info is valid when SA_SIGINFO is set.
    // On Linux, si_addr() is a method; on macOS, si_addr is a field.
    #[cfg(target_os = "linux")]
    let fault_addr = unsafe { (*info).si_addr() as usize };
    #[cfg(not(target_os = "linux"))]
    let fault_addr = unsafe { (*info).si_addr as usize };
    let ps = super::green::page_size();
    let fault_page = fault_addr & !(ps - 1);

    // Try to lock the registry — if contended, fall through to crash.
    let Ok(mut registry) = GUARD_REGISTRY.try_lock() else {
        reraise();
        return;
    };

    // Look up the fault page in our registry.
    if let Some(entry) = registry.remove(&fault_page) {
        let new_size = entry.stack_size + entry.stack_size; // double

        if new_size > MAX_STACK_SIZE {
            // Stack exceeded maximum — abort with message.
            drop(registry);
            let msg = b"kodo: stack overflow - green thread exceeded 8MB limit\n";
            unsafe {
                libc::write(2, msg.as_ptr().cast(), msg.len());
            }
            reraise();
            return;
        }

        // Make the old guard page writable (it becomes part of usable stack).
        // SAFETY: fault_page is page-aligned and within our mapping.
        let ret = unsafe {
            libc::mprotect(
                fault_page as *mut libc::c_void,
                ps,
                libc::PROT_READ | libc::PROT_WRITE,
            )
        };
        if ret != 0 {
            drop(registry);
            reraise();
            return;
        }

        // Allocate new region below the current stack.
        let growth = entry.stack_size;
        let new_base = entry.stack_base - growth;

        // SAFETY: mmap with MAP_FIXED_NOREPLACE (Linux) or MAP_FIXED (macOS).
        let ptr = unsafe {
            libc::mmap(
                new_base as *mut libc::c_void,
                growth,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED,
                -1,
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            // mmap failed — can't grow. Abort.
            drop(registry);
            let msg = b"kodo: failed to grow green thread stack (mmap failed)\n";
            unsafe {
                libc::write(2, msg.as_ptr().cast(), msg.len());
            }
            reraise();
            return;
        }

        // Set new guard page at the very bottom.
        let ret = unsafe { libc::mprotect(new_base as *mut libc::c_void, ps, libc::PROT_NONE) };
        if ret != 0 {
            drop(registry);
            reraise();
            return;
        }

        // Register the new guard page.
        registry.insert(
            new_base,
            GuardEntry {
                stack_base: new_base,
                stack_size: new_size,
            },
        );

        // Return from signal handler — execution resumes at the instruction
        // that caused the fault, which will now succeed because the guard
        // page is writable and new space exists below.
    } else {
        // Not our guard page — re-raise for default handling.
        drop(registry);
        reraise();
    }
}

/// Re-raises SIGSEGV with default handler (causes process abort).
fn reraise() {
    unsafe {
        libc::signal(libc::SIGSEGV, libc::SIG_DFL);
        libc::raise(libc::SIGSEGV);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_installs_once() {
        HANDLER_INSTALLED.store(false, Ordering::SeqCst);
        install_signal_handler();
        assert!(HANDLER_INSTALLED.load(Ordering::SeqCst));
        // Second call is a no-op
        install_signal_handler();
    }

    #[test]
    fn register_unregister_guard() {
        register_guard(0x1000, 0x1000, 1024 * 1024);
        {
            let registry = GUARD_REGISTRY.lock().unwrap();
            assert!(registry.contains_key(&0x1000));
        }
        unregister_guard(0x1000);
        {
            let registry = GUARD_REGISTRY.lock().unwrap();
            assert!(!registry.contains_key(&0x1000));
        }
    }
}
