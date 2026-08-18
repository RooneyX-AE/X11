//! User-stack virtual layout policy.
//!
//! Allocation is deliberately separate from this module. This file defines
//! only the addresses and invariants that a process builder must obey.

use super::{VirtRange, KERNEL_SPACE_START, USER_SPACE_START};

pub const USER_STACK_PAGES: u64 = 16;
pub const USER_STACK_SIZE: u64 = USER_STACK_PAGES * super::PAGE_SIZE_4K;
pub const USER_STACK_TOP: u64 = KERNEL_SPACE_START - 0x1000;
pub const USER_STACK_GUARD_SIZE: u64 = super::PAGE_SIZE_4K;

pub const fn user_stack_range() -> Option<VirtRange> {
    let top = USER_STACK_TOP;
    let data_start = top.checked_sub(USER_STACK_SIZE)?;
    VirtRange::new(data_start, top)
}

pub const fn user_stack_guard_range() -> Option<VirtRange> {
    let range = user_stack_range()?;
    let guard_start = range.start().checked_sub(USER_STACK_GUARD_SIZE)?;
    VirtRange::new(guard_start, range.start())
}

pub const fn is_valid_user_stack_pointer(rsp: u64) -> bool {
    let Some(range) = user_stack_range() else {
        return false;
    };
    rsp >= range.start() && rsp <= range.end() && rsp % 16 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_is_inside_user_half() {
        let range = user_stack_range().unwrap();
        assert!(range.start() >= USER_SPACE_START);
        assert!(range.end() <= KERNEL_SPACE_START);
        assert_eq!(range.len(), USER_STACK_SIZE);
    }

    #[test]
    fn guard_page_sits_below_stack() {
        let stack = user_stack_range().unwrap();
        let guard = user_stack_guard_range().unwrap();
        assert_eq!(guard.end(), stack.start());
        assert_eq!(guard.len(), USER_STACK_GUARD_SIZE);
    }

    #[test]
    fn stack_pointer_requires_alignment_and_range() {
        let stack = user_stack_range().unwrap();
        assert!(is_valid_user_stack_pointer(stack.end()));
        assert!(!is_valid_user_stack_pointer(stack.end() - 8));
        assert!(!is_valid_user_stack_pointer(stack.start() - 16));
    }
}
