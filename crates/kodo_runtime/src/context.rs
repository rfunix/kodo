//! # CPU Context Switch for Green Threads
//!
//! Provides low-level context save/restore primitives for cooperative
//! multitasking.  Each [`Context`] stores the callee-saved registers of
//! one green thread so we can suspend and resume execution without OS
//! involvement.
//!
//! Platform support: `x86_64` (System V ABI) and `aarch64` (AAPCS64).
//!
//! ## Safety
//!
//! This module is inherently `unsafe` — it manipulates raw stack pointers
//! and CPU registers via inline assembly.  Callers must guarantee that:
//!
//! - Stack memory passed to [`init_context`] is valid, properly aligned,
//!   and large enough.
//! - [`switch_context`] is only called with pointers to live, pinned
//!   `Context` values whose stacks have not been freed.

// ---------------------------------------------------------------------------
// Context struct — must be #[repr(C)] so inline asm can rely on field offsets.
// ---------------------------------------------------------------------------

/// Saved CPU state for a single green thread.
///
/// On `x86_64` this stores RSP, RBP, RBX, R12-R15 (7 registers).
/// On `aarch64` this stores X19-X29, X30 (LR), SP (13 registers).
///
/// The struct is zero-initialised by [`Default`]; a zeroed context is
/// *not* valid for switching — you must call [`init_context`] first or
/// populate it via [`switch_context`] (which saves the *current* state).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Context {
    /// Saved registers.  We use a fixed-size array big enough for the
    /// platform with the most registers (aarch64 = 13).
    ///
    /// **`x86_64` layout** (indices 0-6):
    ///   0: RSP, 1: RBP, 2: RBX, 3: R12, 4: R13, 5: R14, 6: R15
    ///
    /// **aarch64 layout** (indices 0-12):
    ///   0: X19, 1: X20, …, 10: X29 (FP), 11: X30 (LR), 12: SP
    pub regs: [u64; 13],
}

// ---------------------------------------------------------------------------
// switch_context — save current registers into `old`, load from `new`.
// ---------------------------------------------------------------------------

/// Suspend the current green thread and resume another.
///
/// Saves all callee-saved registers of the *current* CPU state into
/// `old`, then loads the registers stored in `new` — including the
/// stack pointer and return address — so that execution continues
/// wherever `new` left off (or at the entry trampoline for a freshly
/// initialised context).
///
/// This is a naked function: the entire body is hand-written assembly
/// with no compiler-generated prologue/epilogue.  This is essential
/// because we manipulate the stack pointer and return address directly.
///
/// # Safety
///
/// - `old` and `new` must point to valid, pinned [`Context`] values.
/// - The stack referenced by `new` must still be alive and properly
///   sized.
/// - Must not be called from an interrupt or signal handler.
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(old: *mut Context, new: *const Context) {
    // SAFETY: Caller guarantees both pointers are valid.
    // rdi = old, rsi = new (System V ABI).
    // We save callee-saved registers into old, load from new, then ret.
    // The `ret` pops the return address from the new stack, transferring
    // control to wherever `new` was suspended (or to the trampoline for
    // a fresh context).
    core::arch::naked_asm!(
        // Save callee-saved registers into old context
        "mov [rdi + 0*8], rsp",
        "mov [rdi + 1*8], rbp",
        "mov [rdi + 2*8], rbx",
        "mov [rdi + 3*8], r12",
        "mov [rdi + 4*8], r13",
        "mov [rdi + 5*8], r14",
        "mov [rdi + 6*8], r15",
        // Load callee-saved registers from new context
        "mov rsp, [rsi + 0*8]",
        "mov rbp, [rsi + 1*8]",
        "mov rbx, [rsi + 2*8]",
        "mov r12, [rsi + 3*8]",
        "mov r13, [rsi + 4*8]",
        "mov r14, [rsi + 5*8]",
        "mov r15, [rsi + 6*8]",
        // Return into the new context (pops return address from new stack)
        "ret",
    )
}

/// Suspend the current green thread and resume another (aarch64).
///
/// See the `x86_64` variant for full documentation.
///
/// # Safety
///
/// Same requirements as the `x86_64` variant.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(old: *mut Context, new: *const Context) {
    // SAFETY: Caller guarantees both pointers are valid.
    // x0 = old, x1 = new (AAPCS64).
    core::arch::naked_asm!(
        // Save callee-saved registers into old context
        "stp x19, x20, [x0, #0*8]",
        "stp x21, x22, [x0, #2*8]",
        "stp x23, x24, [x0, #4*8]",
        "stp x25, x26, [x0, #6*8]",
        "stp x27, x28, [x0, #8*8]",
        "stp x29, x30, [x0, #10*8]",
        "mov x9, sp",
        "str x9,       [x0, #12*8]",
        // Load callee-saved registers from new context
        "ldp x19, x20, [x1, #0*8]",
        "ldp x21, x22, [x1, #2*8]",
        "ldp x23, x24, [x1, #4*8]",
        "ldp x25, x26, [x1, #6*8]",
        "ldp x27, x28, [x1, #8*8]",
        "ldp x29, x30, [x1, #10*8]",
        "ldr x9,       [x1, #12*8]",
        "mov sp, x9",
        // Return into the new context via LR (x30)
        "ret",
    )
}

// ---------------------------------------------------------------------------
// Trampoline — entry point for brand-new green threads.
// ---------------------------------------------------------------------------

/// Function signature for a green-thread entry point.
pub type EntryFn = unsafe fn(arg: usize);

/// Trampoline that bootstraps a new green thread (`x86_64`).
///
/// When a freshly initialised context is switched-to for the first time,
/// execution lands here.  The trampoline recovers the entry function
/// pointer and argument from callee-saved registers (set by
/// [`init_context`]), calls the entry, and then spins (a real scheduler
/// would mark the task as done and switch away).
///
/// # Platform register conventions
///
/// | arch     | entry fn  | arg    |
/// |----------|-----------|--------|
/// | `x86_64`   | R12       | R13    |
/// | aarch64  | X19       | X20    |
///
/// This is a naked function — it contains only assembly and no Rust
/// prologue/epilogue, so the callee-saved registers are exactly as
/// `switch_context` restored them.
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
unsafe extern "C" fn trampoline() -> ! {
    // SAFETY: R12 = entry fn pointer, R13 = argument, both set by init_context.
    // We move the argument into RDI (first arg, System V) and call R12.
    // After return we spin in a tight loop (placeholder for scheduler hook).
    core::arch::naked_asm!(
        "mov rdi, r13", // arg → first parameter
        "call r12",     // call entry(arg)
        "2:",
        "pause",
        "jmp 2b",
    )
}

/// Trampoline that bootstraps a new green thread (aarch64).
///
/// See the `x86_64` variant for full documentation.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
unsafe extern "C" fn trampoline() -> ! {
    // SAFETY: X19 = entry fn pointer, X20 = argument, both set by init_context.
    // Move argument into X0 (first arg, AAPCS64) and branch-link to X19.
    core::arch::naked_asm!(
        "mov x0, x20", // arg → first parameter
        "blr x19",     // call entry(arg)
        "2:",
        "wfe",
        "b 2b",
    )
}

// ---------------------------------------------------------------------------
// init_context — prepare a context for first switch.
// ---------------------------------------------------------------------------

/// Initialise a [`Context`] so that switching to it starts execution at
/// `entry(arg)` on the provided stack.
///
/// `stack_top` must point to the **highest** usable address of the
/// stack (stacks grow downward).  The stack must be at least 4 KiB and
/// 16-byte aligned.
///
/// # Safety
///
/// - `ctx` must be a valid, writable pointer to a [`Context`].
/// - `stack_top` must be the end (highest address) of a sufficiently
///   large, 16-byte aligned allocation that will outlive the context.
/// - `entry` must be a valid function pointer.
pub unsafe fn init_context(ctx: *mut Context, stack_top: *mut u8, entry: EntryFn, arg: usize) {
    // SAFETY: caller guarantees ctx is valid.
    let c = unsafe { &mut *ctx };
    *c = Context::default();

    #[cfg(target_arch = "x86_64")]
    {
        // Align the stack top downward to 16 bytes.
        let mut sp = stack_top as usize & !0xF;

        // Push the trampoline address as the "return address" that `ret`
        // in switch_context will pop.
        sp -= 8;
        // SAFETY: writing within the caller-owned stack.
        unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            (sp as *mut u64).write(trampoline as *const () as usize as u64);
        }

        c.regs[0] = sp as u64; // RSP
        c.regs[1] = 0; // RBP — trampoline doesn't use a frame pointer
        c.regs[2] = 0; // RBX
        c.regs[3] = entry as usize as u64; // R12 — entry function
        c.regs[4] = arg as u64; // R13 — argument
        c.regs[5] = 0; // R14
        c.regs[6] = 0; // R15
    }

    #[cfg(target_arch = "aarch64")]
    {
        // Align the stack top downward to 16 bytes.
        let sp = stack_top as usize & !0xF;

        c.regs[0] = entry as usize as u64; // X19 — entry function
        c.regs[1] = arg as u64; // X20 — argument
                                // X21-X28 = 0 (already zeroed by default)
        c.regs[10] = 0; // X29 (FP)
        c.regs[11] = trampoline as *const () as usize as u64; // X30 (LR)
        c.regs[12] = sp as u64; // SP
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn context_default_is_zeroed() {
        let ctx = Context::default();
        for (i, &r) in ctx.regs.iter().enumerate() {
            assert_eq!(r, 0, "register slot {i} should be zero");
        }
    }

    /// Allocate a stack suitable for a green thread test.
    /// Returns (allocation base, stack top pointer).
    fn alloc_stack(size: usize) -> (Vec<u8>, *mut u8) {
        let mut buf = vec![0u8; size];
        let top = unsafe { buf.as_mut_ptr().add(size) };
        (buf, top)
    }

    static SWITCH_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Static pointer used by the child entry to switch back to the main
    /// context.  Only accessed from single-threaded test code.
    static BACK_CTX: AtomicUsize = AtomicUsize::new(0);

    unsafe fn entry_and_return(arg: usize) {
        SWITCH_COUNTER.store(arg, Ordering::SeqCst);
        // Switch back to the main context.
        // SAFETY: BACK_CTX was set before the switch and points to a
        // valid Context on the test's stack frame.
        unsafe {
            let back = BACK_CTX.load(Ordering::SeqCst) as *mut Context;
            let mut dummy = Context::default();
            switch_context(&mut dummy, back);
        }
    }

    #[test]
    fn basic_switch_runs_entry() {
        const STACK_SIZE: usize = 64 * 1024; // 64 KiB
        const MAGIC: usize = 0xCAFE;

        SWITCH_COUNTER.store(0, Ordering::SeqCst);

        let mut main_ctx = Context::default();
        let mut child_ctx = Context::default();

        let (_stack_buf, stack_top) = alloc_stack(STACK_SIZE);

        unsafe {
            BACK_CTX.store(&mut main_ctx as *mut Context as usize, Ordering::SeqCst);
            init_context(&mut child_ctx, stack_top, entry_and_return, MAGIC);
            switch_context(&mut main_ctx, &child_ctx);
        }

        assert_eq!(
            SWITCH_COUNTER.load(Ordering::SeqCst),
            MAGIC,
            "child green thread should have stored the magic value"
        );
    }
}
