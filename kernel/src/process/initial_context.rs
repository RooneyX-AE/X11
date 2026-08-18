//! Initial userspace execution context.
//!
//! Process construction chooses only entry and stack. Architecture code owns
//! privilege-level selectors and machine-specific return state.

use crate::memory::is_valid_user_stack_pointer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitialContext {
    entry: u64,
    stack_pointer: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialContextError {
    InvalidEntry,
    InvalidStackPointer,
}

impl InitialContext {
    pub fn new(entry: u64, stack_pointer: u64) -> Result<Self, InitialContextError> {
        if entry < crate::memory::USER_SPACE_START || entry >= crate::memory::KERNEL_SPACE_START {
            return Err(InitialContextError::InvalidEntry);
        }
        if !is_valid_user_stack_pointer(stack_pointer) {
            return Err(InitialContextError::InvalidStackPointer);
        }
        Ok(Self { entry, stack_pointer })
    }

    pub const fn entry(self) -> u64 { self.entry }
    pub const fn stack_pointer(self) -> u64 { self.stack_pointer }
}

#[cfg(test)]
mod tests {
    use super::{InitialContext, InitialContextError};
    use crate::memory::USER_SPACE_START;
    use crate::memory::user_stack_range;

    #[test]
    fn accepts_valid_user_entry_and_stack() {
        let stack = user_stack_range().unwrap();
        let context = InitialContext::new(USER_SPACE_START + 0x1000, stack.end()).unwrap();
        assert_eq!(context.entry(), USER_SPACE_START + 0x1000);
        assert_eq!(context.stack_pointer(), stack.end());
    }

    #[test]
    fn rejects_kernel_entry() {
        let stack = user_stack_range().unwrap();
        assert_eq!(
            InitialContext::new(crate::memory::KERNEL_SPACE_START, stack.end()),
            Err(InitialContextError::InvalidEntry)
        );
    }

    #[test]
    fn rejects_misaligned_stack() {
        let stack = user_stack_range().unwrap();
        assert_eq!(
            InitialContext::new(USER_SPACE_START + 0x1000, stack.end() - 8),
            Err(InitialContextError::InvalidStackPointer)
        );
    }
}
