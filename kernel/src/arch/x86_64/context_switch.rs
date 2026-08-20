//! Minimal x86_64 kernel context-switch boundary.
//!
//! The switch preserves the SysV64 callee-saved state plus the stack and
//! continuation address. Address-space state, FPU/SIMD state, interrupt state,
//! and per-thread metadata are deliberately owned by higher layers.

use super::activation::ActivationRecord;

/// Register state owned by the scheduler's kernel-thread context switch.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
        Self { rsp: 0, rbp: 0, rbx: 0, r12: 0, r13: 0, r14: 0, r15: 0, rip: 0 }
    }

    pub const fn is_initialized(self) -> bool {
        self.rsp != 0 && self.rip != 0
    }
}

const CONTEXT_RSP_OFFSET: usize = core::mem::offset_of!(Context, rsp);
const CONTEXT_RBP_OFFSET: usize = core::mem::offset_of!(Context, rbp);
const CONTEXT_RBX_OFFSET: usize = core::mem::offset_of!(Context, rbx);
const CONTEXT_R12_OFFSET: usize = core::mem::offset_of!(Context, r12);
const CONTEXT_R13_OFFSET: usize = core::mem::offset_of!(Context, r13);
const CONTEXT_R14_OFFSET: usize = core::mem::offset_of!(Context, r14);
const CONTEXT_R15_OFFSET: usize = core::mem::offset_of!(Context, r15);
const CONTEXT_RIP_OFFSET: usize = core::mem::offset_of!(Context, rip);

const _: () = assert!(core::mem::size_of::<Context>() == 64);
const _: () = assert!(core::mem::align_of::<Context>() == 8);
// These offsets are part of the switch/return assembly ABI below.
const _: () = assert!(CONTEXT_RSP_OFFSET == 0);
const _: () = assert!(CONTEXT_RBP_OFFSET == 8);
const _: () = assert!(CONTEXT_RBX_OFFSET == 16);
const _: () = assert!(CONTEXT_R12_OFFSET == 24);
const _: () = assert!(CONTEXT_R13_OFFSET == 32);
const _: () = assert!(CONTEXT_R14_OFFSET == 40);
const _: () = assert!(CONTEXT_R15_OFFSET == 48);
const _: () = assert!(CONTEXT_RIP_OFFSET == 56);

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

/// Builds an initial kernel-thread context and passes a stable activation
/// pointer through callee-saved `r12`.
pub fn bootstrap_context(
    stack_top: u64,
    activation: &ActivationRecord,
    trampoline: extern "C" fn() -> !,
) -> Option<Context> {
    let aligned = stack_top.checked_sub(8)? & !0xf;
    let entry_rsp = aligned.checked_add(8)?;
    if entry_rsp == 0 || entry_rsp > stack_top || entry_rsp & 0xf != 8 {
        return None;
    }

    Some(Context {
        rsp: entry_rsp,
        rbp: 0,
        rbx: 0,
        r12: activation.pointer(),
        r13: 0,
        r14: 0,
        r15: 0,
        rip: trampoline as *const () as usize as u64,
    })
}

/// Common naked entry trampoline for all newly created kernel tasks.
///
/// `switch` restores the activation pointer into `r12`. This trampoline moves
/// that value into the SysV64 first argument register and jumps across an
/// ordinary Rust ABI boundary, so the compiler never has to reason about an
/// undeclared implicit `r12` input.
///
/// # Safety
/// The restored `r12` value must point to a live `ActivationRecord` for the
/// task being entered, and the record must outlive the task's execution.
#[unsafe(naked)]
extern "C" fn task_trampoline() -> ! {
    core::arch::naked_asm!(
        "mov rdi, r12",
        "jmp {entry}",
        entry = sym task_entry_dispatch,
    );
}

#[inline(never)]
extern "C" fn task_entry_dispatch(activation: *const ActivationRecord) -> ! {
    if activation.is_null() {
        loop { core::hint::spin_loop(); }
    }

    unsafe { ((*activation).entry())() }
}

pub fn bootstrap_kernel_context(stack_top: u64, activation: &ActivationRecord) -> Option<Context> {
    bootstrap_context(stack_top, activation, task_trampoline)
}

/// Backward-compatible smoke-test entry used by the boot validation path.
/// The real A→B→A implementation lives in `voluntary_switch` so the context
/// primitive remains separate from its integration test.
pub fn smoke_test() -> bool {
    super::voluntary_switch::run()
}

#[cfg(test)]
mod tests {
    use super::{bootstrap_context, Context};
    use crate::arch::x86_64::activation::ActivationRecord;
    use crate::scheduler::TaskId;

    extern "C" fn never_returns() -> ! { loop {} }
    extern "C" fn trampoline() -> ! { loop {} }

    #[test]
    fn context_is_zero_before_initialization() {
        assert!(!Context::empty().is_initialized());
    }

    #[test]
    fn context_layout_is_stable() {
        assert_eq!(core::mem::size_of::<Context>(), 64);
        assert_eq!(core::mem::align_of::<Context>(), 8);
        assert_eq!(core::mem::offset_of!(Context, rsp), 0);
        assert_eq!(core::mem::offset_of!(Context, rip), 56);
    }

    #[test]
    fn bootstrap_context_carries_activation_pointer() {
        let activation = ActivationRecord::new(TaskId::new(1, 1), never_returns);
        let context = bootstrap_context(0x20_000, &activation, trampoline).unwrap();
        assert_eq!(context.rsp & 0xf, 0x8);
        assert_eq!(context.r12, activation.pointer());
        assert!(context.is_initialized());
    }

    #[test]
    fn bootstrap_rejects_invalid_stack() {
        let activation = ActivationRecord::new(TaskId::new(1, 1), never_returns);
        assert!(bootstrap_context(0, &activation, trampoline).is_none());
    }
}
