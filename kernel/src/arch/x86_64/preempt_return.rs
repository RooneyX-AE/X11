//! x86_64 kernel return primitives for scheduler dispatch.

use super::context_switch::Context;
use super::interrupted_state::InterruptedState;

/// Restores a previously interrupted kernel task and returns through `iretq`.
///
/// # Safety
/// `state` must point to a valid, live `InterruptedState`. Its
/// `return_state.resume_rsp` must identify writable memory in the target task's
/// kernel stack with at least 24 bytes available below it. The saved return
/// state must be valid for a same-CPL 64-bit `iretq` return.
#[unsafe(naked)]
pub unsafe extern "C" fn return_to_kernel(state: *const InterruptedState) -> ! {
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

/// Starts a task from its existing cooperative kernel context without
/// returning to the current interrupt frame. This is used when a timer
/// interrupt preempts the current task and selects a task that has a valid
/// cooperative context but has never been interrupted itself.
///
/// # Safety
/// `context` must point to a live initialized kernel context whose stack and
/// continuation remain valid for the target task.
#[unsafe(naked)]
pub unsafe extern "C" fn return_to_context(context: *const Context) -> ! {
    core::arch::naked_asm!(
        "mov rsp, [rdi + 0]",
        "mov rbp, [rdi + 8]",
        "mov rbx, [rdi + 16]",
        "mov r12, [rdi + 24]",
        "mov r13, [rdi + 32]",
        "mov r14, [rdi + 40]",
        "mov r15, [rdi + 48]",
        "push qword ptr [rdi + 56]",
        "ret",
    );
}

pub fn validate_kernel_state(state: &InterruptedState) -> bool {
    state.is_valid() && state.return_state().kernel_iret_words().is_some()
}

#[cfg(test)]
mod tests {
    use super::validate_kernel_state;
    use crate::arch::x86_64::interrupted_state::InterruptedState;
    use crate::arch::x86_64::interrupt_entry::{InterruptReturnFrame, SavedRegisters};

    #[test]
    fn validates_kernel_return_packet() {
        let registers = SavedRegisters::default();
        let mut raw = [0u64; 3];
        raw[0] = 0x1000;
        raw[1] = 0x8;
        raw[2] = 0x202;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        let state = unsafe { InterruptedState::capture(&registers, frame) };
        assert!(validate_kernel_state(&state));
    }

    #[test]
    fn rejects_user_return_packet() {
        let registers = SavedRegisters::default();
        let mut raw = [0u64; 5];
        raw[0] = 0x1000;
        raw[1] = 0x1b;
        raw[2] = 0x202;
        raw[3] = 0x8000;
        raw[4] = 0x23;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        let state = unsafe { InterruptedState::capture(&registers, frame) };
        assert!(!validate_kernel_state(&state));
    }
}
