//! x86_64 kernel-mode preemption return primitive.
//!
//! This primitive deliberately supports kernel-to-kernel returns only. User
//! returns require a separate privilege-transition frame contract and address
//! space restoration path.

use core::arch::asm;

use super::interrupted_state::KernelPreemptState;

/// Restores a previously interrupted kernel task and returns through `iretq`.
///
/// # Safety
/// `state` must point to a valid, live `KernelPreemptState`. Its `resume_rsp`
/// must identify writable memory in the target task's kernel stack with at
/// least 24 bytes available below it. `cs`, `rip`, and `rflags` must be valid
/// for a same-CPL 64-bit `iretq` return.
#[unsafe(naked)]
pub unsafe extern "C" fn return_to_kernel(state: *const KernelPreemptState) -> ! {
    core::arch::naked_asm!(
        "mov r11, rdi",
        // Build the CPU-owned same-CPL return frame at resume_rsp - 24.
        "mov r10, [r11 + 144]",
        "sub r10, 24",
        "mov rax, [r11 + 120]",
        "mov [r10 + 0], rax",
        "mov rax, [r11 + 128]",
        "mov [r10 + 8], rax",
        "mov rax, [r11 + 136]",
        "mov [r10 + 16], rax",
        "mov rsp, r10",
        // Restore all general-purpose registers from the stable state packet.
        "mov rax, [r11 + 0]",
        "mov rcx, [r11 + 8]",
        "mov rdx, [r11 + 16]",
        "mov rbx, [r11 + 24]",
        "mov rbp, [r11 + 32]",
        "mov rsi, [r11 + 40]",
        "mov rdi, [r11 + 48]",
        "mov r8,  [r11 + 56]",
        "mov r9,  [r11 + 64]",
        "mov r10, [r11 + 72]",
        "mov r12, [r11 + 88]",
        "mov r13, [r11 + 96]",
        "mov r14, [r11 + 104]",
        "mov r15, [r11 + 112]",
        "mov r11, [r11 + 80]",
        "iretq",
    );
}

/// Validates a kernel preemption packet before handing it to the naked return
/// primitive. Keeping validation in Rust prevents the assembly boundary from
/// becoming a second scheduler policy engine.
pub fn validate_kernel_state(state: &KernelPreemptState) -> bool {
    state.rip != 0
        && state.resume_rsp >= 24
        && state.cs & 3 == 0
        && state.rflags & 2 != 0
}

#[cfg(test)]
mod tests {
    use super::validate_kernel_state;
    use crate::arch::x86_64::interrupted_state::KernelPreemptState;
    use crate::arch::x86_64::interrupt_entry::SavedRegisters;

    #[test]
    fn validates_kernel_return_packet() {
        let state = KernelPreemptState {
            registers: SavedRegisters::default(),
            rip: 0x1000,
            cs: 0x8,
            rflags: 0x202,
            resume_rsp: 0x8000,
        };
        assert!(validate_kernel_state(&state));
    }

    #[test]
    fn rejects_user_code_selector() {
        let state = KernelPreemptState {
            registers: SavedRegisters::default(),
            rip: 0x1000,
            cs: 0x1b,
            rflags: 0x202,
            resume_rsp: 0x8000,
        };
        assert!(!validate_kernel_state(&state));
    }
}

#[allow(dead_code)]
fn _keep_asm_import_live() {
    let _ = asm as unsafe fn();
}
