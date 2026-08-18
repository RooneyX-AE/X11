//! Minimal x86_64 kernel context-switch boundary.
//!
//! The switch preserves the SysV64 callee-saved state plus the stack and
//! continuation address. Address-space state, FPU/SIMD state, interrupt state,
//! and per-thread metadata are deliberately owned by higher layers.

use core::arch::asm;

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

const _: () = assert!(core::mem::size_of::<Context>() == 64);

#[inline(never)]
pub unsafe fn switch(current: *mut Context, next: *const Context) {
    unsafe {
        asm!(
            "mov [rdi + 0], rsp",
            "mov [rdi + 8], rbp",
            "mov [rdi + 16], rbx",
            "mov [rdi + 24], r12",
            "mov [rdi + 32], r13",
            "mov [rdi + 40], r14",
            "mov [rdi + 48], r15",
            "lea rax, [rip + 2f]",
            "mov [rdi + 56], rax",
            "mov rsp, [rsi + 0]",
            "mov rbp, [rsi + 8]",
            "mov rbx, [rsi + 16]",
            "mov r12, [rsi + 24]",
            "mov r13, [rsi + 32]",
            "mov r14, [rsi + 40]",
            "mov r15, [rsi + 48]",
            "jmp [rsi + 56]",
            "2:",
            in("rdi") current,
            in("rsi") next,
            lateout("rax") _,
            options(nostack, preserves_flags)
        );
    }
}

pub fn bootstrap_kernel_context(
    stack_top: u64,
    activation: &ActivationRecord,
) -> Option<Context> {
    if stack_top == 0 {
        return None;
    }
    let aligned_stack = stack_top & !15;
    Some(Context {
        rsp: aligned_stack,
        rbp: 0,
        rbx: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: activation.entry() as usize as u64,
    })
}
