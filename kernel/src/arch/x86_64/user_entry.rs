//! x86_64 userspace privilege-transition primitive.
//!
//! The caller must provide a fully validated `UserReturnFrame`. This function
//! performs no address-space setup, permission checks, or policy decisions.
//! Those belong to the process/address-space layers.

use super::user_return::UserReturnFrame;

/// Enter CPL3 from a kernel-created `iretq` frame.
///
/// # Safety
/// `frame` must point to a readable, suitably aligned kernel memory region
/// containing exactly the five 64-bit fields consumed by `iretq`: RIP, CS,
/// RFLAGS, RSP, and SS. The selectors and addresses must already have been
/// validated, and the target address space must already be active.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn enter_user(frame: *const UserReturnFrame) -> ! {
    core::arch::naked_asm!(
        "mov rsp, rdi",
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
}
