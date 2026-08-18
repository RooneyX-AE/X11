//! x86_64 user-mode execution context contract.
//!
//! This is data only. Entry/return assembly remains separate so process
//! construction cannot accidentally perform a privilege transition itself.

use crate::process::InitialContext;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserContext {
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub cs: u16,
    pub ss: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserContextError {
    InvalidSelectors,
}

impl UserContext {
    pub const fn new(rip: u64, rsp: u64, cs: u16, ss: u16) -> Self {
        Self {
            rip,
            rsp,
            rflags: 0x202,
            cs,
            ss,
        }
    }

    pub fn from_initial(
        context: InitialContext,
        user_code: u16,
        user_data: u16,
    ) -> Result<Self, UserContextError> {
        if user_code & 3 != 3 || user_data & 3 != 3 {
            return Err(UserContextError::InvalidSelectors);
        }
        Ok(Self::new(context.entry(), context.stack_pointer(), user_code, user_data))
    }

    pub const fn is_user_selectors(self, user_code: u16, user_data: u16) -> bool {
        self.cs == user_code && self.ss == user_data && (self.cs & 3) == 3 && (self.ss & 3) == 3
    }
}

const _: () = {
    assert!(core::mem::size_of::<UserContext>() == 32);
    assert!(core::mem::align_of::<UserContext>() == 8);
};

#[cfg(test)]
mod tests {
    use super::{UserContext, UserContextError};
    use crate::memory::{user_stack_range, USER_SPACE_START};
    use crate::process::InitialContext;

    #[test]
    fn builds_ring3_context_from_process_context() {
        let stack = user_stack_range().unwrap();
        let initial = InitialContext::new(USER_SPACE_START + 0x1000, stack.end()).unwrap();
        let context = UserContext::from_initial(initial, 0x23, 0x1b).unwrap();
        assert_eq!(context.rip, USER_SPACE_START + 0x1000);
        assert_eq!(context.rsp, stack.end());
        assert_eq!(context.rflags & (1 << 9), 1 << 9);
        assert!(context.is_user_selectors(0x23, 0x1b));
    }

    #[test]
    fn rejects_kernel_selectors() {
        let stack = user_stack_range().unwrap();
        let initial = InitialContext::new(USER_SPACE_START + 0x1000, stack.end()).unwrap();
        assert_eq!(
            UserContext::from_initial(initial, 0x08, 0x10),
            Err(UserContextError::InvalidSelectors)
        );
    }
}
