//! x86_64 kernel return primitives for scheduler dispatch.

use super::context_switch::Context;
use super::interrupted_state::KernelPreemptState;

#[unsafe(naked)]
pub unsafe extern "C" fn return_to_kernel(state: *const KernelPreemptState) -> ! {
    core::arch::naked_asm!(
        "mov r11, rdi",
        "mov r10, [r11 + 144]",
        "sub r10, 24",
        "mov rax, [r11 + 120]",
        "mov [r10 + 0], rax",
        "mov rax, [r11 + 128]",
        "mov [r10 + 8], rax",
        "mov rax, [r11 + 136]",
        "mov [r10 + 16], rax",
        "mov rsp, r10",
        "mov rax, [r11 + 0]",
        "mov rcx, [r11 + 8]",
        "mov rdx, [r11 + 16]",
        "mov rbx, [r11 + 24]",
        "mov rbp, [r11 + 32]",
        "mov rsi, [r11 + 40]",
        "mov rdi, [r11 + 48]",
        "mov r8, [r11 + 56]",
        "mov r9, [r11 + 64]",
        "mov r10, [r11 + 72]",
        "mov r12, [r11 + 88]",
        "mov r13, [r11 + 96]",
        "mov r14, [r11 + 104]",
        "mov r15, [r11 + 112]",
        "mov r11, [r11 + 80]",
        "iretq",
    );
}

/// Starts a task from an existing cooperative kernel context. The interrupted
/// frame is intentionally abandoned because the interrupted task has already
/// been copied into task-owned state. The task continuation is installed before
/// interrupts are re-enabled, so a timer can never observe a half-built stack.
#[unsafe(naked)]
pub unsafe extern "C" fn return_to_context(context: *const Context) -> ! {
    core::arch::naked_asm!(
        "mov rsp, [rdi + 0]",
        "push qword ptr [rdi + 56]",
        "mov rbp, [rdi + 8]",
        "mov rbx, [rdi + 16]",
        "mov r12, [rdi + 24]",
        "mov r13, [rdi + 32]",
        "mov r14, [rdi + 40]",
        "mov r15, [rdi + 48]",
        "mov rdi, [rdi + 48]",
        "sti",
        "ret",
    );
}

pub fn validate_kernel_state(state: &KernelPreemptState) -> bool {
    state.rip != 0 && state.resume_rsp >= 24 && state.cs & 3 == 0 && state.rflags & 2 != 0
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
