//! Diverging userspace-to-userspace architectural transfer primitive.
//!
//! Policy and lifecycle mutation stay outside this module. The caller must
//! provide a fully validated successor transfer and may supply either the
//! initial launch frame or an owned interrupted userspace snapshot.

use x86_64::instructions::interrupts;

use super::address_space::activate;
use super::user_entry::{enter_user, resume_user};
use super::user_return_transfer::UserReturnTransfer;

pub unsafe fn execute_user_transfer(transfer: UserReturnTransfer) -> ! {
    transfer.validate(None).expect("validated user transfer required");
    interrupts::disable();
    unsafe { activate(transfer.root()) };
    match transfer.resume() {
        Some(state) => unsafe { resume_user(&state) },
        None => {
            let frame = transfer.frame();
            unsafe { enter_user(&frame) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::execute_user_transfer;
    use crate::arch::x86_64::address_space::AddressSpaceRoot;
    use crate::arch::x86_64::user_launch::prepare_launch;
    use crate::arch::x86_64::user_return_transfer::UserReturnTransfer;
    use crate::memory::{user_stack_range, USER_SPACE_START};
    use crate::process::{AddressSpaceId, ProcessId, UserLaunchPlan};
    use crate::scheduler::TaskId;

    #[test]
    fn transfer_uses_the_validated_initial_user_frame() {
        let id = AddressSpaceId::new(9).unwrap();
        let root = AddressSpaceRoot::from_physical_address(0x1234_7000).unwrap();
        let stack = user_stack_range().unwrap();
        let prepared = prepare_launch(root, UserLaunchPlan {
            address_space: id,
            entry: USER_SPACE_START + 0x2000,
            stack_pointer: stack.end(),
        }).unwrap();
        let binding = crate::arch::x86_64::user_execution::UserExecutionBinding::new(
            ProcessId::new(4, 5), TaskId::new(6, 7), id, prepared,
        ).unwrap();
        let transfer = UserReturnTransfer::new(binding);
        assert_eq!(transfer.root(), root);
        assert_eq!(transfer.frame().rip, USER_SPACE_START + 0x2000);
        assert_eq!(transfer.frame().rsp, stack.end());
        assert!(transfer.resume().is_none());
        let _ = execute_user_transfer as unsafe fn(UserReturnTransfer) -> !;
    }
}
