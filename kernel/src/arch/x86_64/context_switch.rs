//! Minimal x86_64 kernel context-switch boundary.
//!
//! The switch preserves the SysV64 callee-saved state plus the stack and
//! continuation address. Address-space state, FPU/SIMD state, interrupt state,
//! and per-thread metadata are deliberately owned by higher layers.

/// Register state owned by the scheduler's kernel-thread context switch.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Context {
    pub rsp: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
}

impl Context {
    pub const fn empty() -> Self {
        Self {
            rsp: 0,
            rbp: 0,
            rbx: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0,
        }
    }

    pub const fn is_initialized(self) -> bool {
        self.rsp != 0 && self.rip != 0
    }
}

/// Switches from `current` to `next` and resumes the saved continuation.
///
/// # Safety
///
/// Both contexts must be valid kernel contexts for the same x86_64 execution
/// domain. Their stacks must be writable and live for the duration of the
/// switch, obey the SysV64 stack-alignment contract, and the next context must
/// point at executable kernel code. State not represented by `Context` must be
/// managed by higher layers.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn switch(current: *mut Context, next: *const Context) {
    core::arch::naked_asm!(
        "mov [rdi + 0], rsp",
        "mov [rdi + 8], rbp",
        "mov [rdi + 16], rbx",
        "mov [rdi + 24], r12",
        "mov [rdi + 32], r13",
        "mov [rdi + 40], r14",
        "mov [rdi + 48], r15",
        "mov rax, [rsp]",
        "mov [rdi + 56], rax",
        "mov rsp, [rsi + 0]",
        "mov rbp, [rsi + 8]",
        "mov rbx, [rsi + 16]",
        "mov r12, [rsi + 24]",
        "mov r13, [rsi + 32]",
        "mov r14, [rsi + 40]",
        "mov r15, [rsi + 48]",
        "push qword ptr [rsi + 56]",
        "ret",
    );
}

/// Builds an initial kernel-thread context.
///
/// `stack_top` points one byte past the writable allocation. The first switch
/// resumes at `entry`; the caller owns the stack storage for the thread.
pub fn bootstrap_context(stack_top: u64, entry: extern "C" fn() -> !) -> Option<Context> {
    let aligned = stack_top.checked_sub(8)? & !0xf;
    let entry_rsp = aligned.checked_add(8)?;
    if entry_rsp == 0 || entry_rsp > stack_top || entry_rsp & 0xf != 8 {
        return None;
    }

    Some(Context {
        rsp: entry_rsp,
        rbp: 0,
        rbx: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: entry as *const () as usize as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::{bootstrap_context, Context};

    extern "C" fn never_returns() -> ! {
        loop {}
    }

    #[test]
    fn context_is_zero_before_initialization() {
        assert!(!Context::empty().is_initialized());
    }

    #[test]
    fn bootstrap_stack_keeps_sysv_entry_alignment() {
        let context = bootstrap_context(0x20_000, never_returns).unwrap();
        assert_eq!(context.rsp & 0xf, 0x8);
        assert!(context.is_initialized());
    }

    #[test]
    fn bootstrap_rejects_invalid_stack() {
        assert!(bootstrap_context(0, never_returns).is_none());
    }
}
