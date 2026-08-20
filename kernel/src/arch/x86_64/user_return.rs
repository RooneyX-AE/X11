//! x86_64 userspace return frame contract.
//!
//! `iretq` consumes a privilege-transition frame with RIP, CS, RFLAGS, RSP,
//! and SS. This module models the logical fields only. Entry assembly owns
//! the actual stack layout and instruction sequence.

use crate::process::InitialContext;

use super::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserReturnFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserReturnError {
    KernelSelector,
    InvalidSelectors,
    InvalidRflags,
}

impl UserReturnFrame {
    pub fn from_initial(
        context: InitialContext,
        user_code: u16,
        user_data: u16,
    ) -> Result<Self, UserReturnError> {
        if user_code & 3 != 3 || user_data & 3 != 3 {
            return Err(UserReturnError::KernelSelector);
        }
        if user_code != USER_CODE_SELECTOR || user_data != USER_DATA_SELECTOR {
            return Err(UserReturnError::InvalidSelectors);
        }

        let rflags = 0x202u64;
        if rflags & 0x2 == 0 || rflags & (1 << 9) == 0 {
            return Err(UserReturnError::InvalidRflags);
        }

        Ok(Self {
            rip: context.entry(),
            cs: user_code as u64,
            rflags,
            rsp: context.stack_pointer(),
            ss: user_data as u64,
        })
    }

    pub const fn size_bytes() -> usize {
        core::mem::size_of::<Self>()
    }
}

const _: () = {
    assert!(core::mem::size_of::<UserReturnFrame>() == 40);
    assert!(core::mem::align_of::<UserReturnFrame>() == 8);
};

#[cfg(test)]
mod tests {
    use super::{UserReturnError, UserReturnFrame};
    use crate::arch::x86_64::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};
    use crate::memory::{user_stack_range, USER_SPACE_START};
    use crate::process::InitialContext;

    #[test]
    fn builds_valid_ring3_iret_state() {
        let stack = user_stack_range().unwrap();
        let context = InitialContext::new(USER_SPACE_START + 0x1000, stack.end()).unwrap();
        let frame = UserReturnFrame::from_initial(context, USER_CODE_SELECTOR, USER_DATA_SELECTOR).unwrap();
        assert_eq!(frame.rip, context.entry());
        assert_eq!(frame.rsp, context.stack_pointer());
        assert_eq!(frame.cs, USER_CODE_SELECTOR as u64);
        assert_eq!(frame.ss, USER_DATA_SELECTOR as u64);
        assert_ne!(frame.rflags & 0x2, 0);
        assert_ne!(frame.rflags & (1 << 9), 0);
    }

    #[test]
    fn rejects_kernel_selectors() {
        let stack = user_stack_range().unwrap();
        let context = InitialContext::new(USER_SPACE_START + 0x1000, stack.end()).unwrap();
        assert_eq!(
            UserReturnFrame::from_initial(context, 0x08, 0x10),
            Err(UserReturnError::KernelSelector)
        );
    }

    #[test]
    fn rejects_unknown_ring3_selectors() {
        let stack = user_stack_range().unwrap();
        let context = InitialContext::new(USER_SPACE_START + 0x1000, stack.end()).unwrap();
        assert_eq!(
            UserReturnFrame::from_initial(context, USER_CODE_SELECTOR + 0x08, USER_DATA_SELECTOR + 0x08),
            Err(UserReturnError::InvalidSelectors)
        );
    }
}
