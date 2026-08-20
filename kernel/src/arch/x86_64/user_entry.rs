//! x86_64 userspace privilege-transition primitives.

use super::interrupted_state::InterruptedState;
use super::user_return::UserReturnFrame;

/// Enter CPL3 from a kernel-created `iretq` frame.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn enter_user(frame: *const UserReturnFrame, kernel_stack_top: u64) -> ! {
    core::arch::naked_asm!(
        "mov r11, rdi",
        "mov rsp, rsi",
        // Copy the five-word CPL3 frame onto the target task's kernel stack.
        "push qword ptr [r11 + 32]",
        "push qword ptr [r11 + 24]",
        "push qword ptr [r11 + 16]",
        "push qword ptr [r11 + 8]",
        "push qword ptr [r11 + 0]",
        "iretq",
    )
}

/// Restore a task-owned userspace interrupt snapshot and resume through `iretq`.
///
/// The snapshot is copied into a fresh five-word privilege-transition frame on
/// the target task's kernel stack before any GPR is restored. The target CR3
/// must already be active.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn resume_user(state: *const InterruptedState, kernel_stack_top: u64) -> ! {
    core::arch::naked_asm!(
        // Keep the snapshot pointer until every frame word has been copied.
        "mov r11, rdi",
        "mov rsp, rsi",
        // iretq frame is pushed in reverse pop order: SS, RSP, RFLAGS, CS, RIP.
        "mov rax, [r11 + 168]",
        "push rax",
        "mov rax, [r11 + 152]",
        "push rax",
        "mov rax, [r11 + 136]",
        "push rax",
        "mov rax, [r11 + 128]",
        "push rax",
        "mov rax, [r11 + 120]",
        "push rax",
        // Restore the complete user GPR set after the frame has been built.
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
    )
}

#[cfg(test)]
mod tests {
    use super::UserReturnFrame;
    use crate::arch::x86_64::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};
    use crate::memory::{user_stack_range, USER_SPACE_START};
    use crate::process::InitialContext;

    #[test]
    fn return_frame_matches_iretq_field_order() {
        let stack = user_stack_range().unwrap();
        let context = InitialContext::new(USER_SPACE_START + 0x1000, stack.end()).unwrap();
        let frame = UserReturnFrame::from_initial(context, USER_CODE_SELECTOR, USER_DATA_SELECTOR).unwrap();
        assert_eq!(core::mem::size_of::<UserReturnFrame>(), 40);
        assert_eq!(frame.rip, context.entry());
        assert_eq!(frame.cs, USER_CODE_SELECTOR as u64);
        assert_eq!(frame.rflags, 0x202);
        assert_eq!(frame.rsp, context.stack_pointer());
        assert_eq!(frame.ss, USER_DATA_SELECTOR as u64);
    }

    #[test]
    fn resume_user_symbol_has_never_returning_abi() {
        let _ = super::resume_user as unsafe extern "sysv64" fn(*const crate::arch::x86_64::interrupted_state::InterruptedState, u64) -> !;
    }
}
