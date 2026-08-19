//! Validated userspace-to-userspace return transfer plan.
//!
//! This module intentionally performs no task or address-space mutation. It
//! packages the exact successor state that the terminal architectural return
//! path must activate before constructing a CPL3 `iretq` frame.

use crate::process::ProcessId;
use crate::scheduler::TaskId;

use super::user_execution::UserExecutionBinding;
use super::user_return::UserReturnFrame;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserReturnTransfer {
    process: ProcessId,
    task: TaskId,
    binding: UserExecutionBinding,
}

impl UserReturnTransfer {
    pub const fn new(binding: UserExecutionBinding) -> Self {
        Self { process: binding.process(), task: binding.task(), binding }
    }

    pub const fn process(self) -> ProcessId { self.process }
    pub const fn task(self) -> TaskId { self.task }
    pub const fn binding(self) -> UserExecutionBinding { self.binding }
    pub const fn root(self) -> super::address_space::AddressSpaceRoot { self.binding.launch().root() }
    pub const fn frame(self) -> UserReturnFrame { self.binding.launch().frame() }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserReturnTransferError {
    CurrentTask,
    InvalidSelectors,
    InvalidRflags,
}

impl UserReturnTransfer {
    pub fn validate(self, current_task: Option<TaskId>) -> Result<Self, UserReturnTransferError> {
        if current_task == Some(self.task) { return Err(UserReturnTransferError::CurrentTask); }
        let frame = self.frame();
        if frame.cs & 3 != 3 || frame.ss & 3 != 3 { return Err(UserReturnTransferError::InvalidSelectors); }
        if frame.rflags & 0x2 == 0 || frame.rflags & (1 << 9) == 0 { return Err(UserReturnTransferError::InvalidRflags); }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{UserReturnTransfer, UserReturnTransferError};
    use crate::arch::x86_64::address_space::AddressSpaceRoot;
    use crate::arch::x86_64::user_launch::prepare_launch;
    use crate::memory::{user_stack_range, USER_SPACE_START};
    use crate::process::{AddressSpaceId, ProcessId, UserLaunchPlan};
    use crate::scheduler::TaskId;

    fn transfer() -> UserReturnTransfer {
        let id = AddressSpaceId::new(7).unwrap();
        let root = AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap();
        let stack = user_stack_range().unwrap();
        let plan = UserLaunchPlan { address_space: id, entry: USER_SPACE_START + 0x1000, stack_pointer: stack.end() };
        let prepared = prepare_launch(root, plan).unwrap();
        let binding = crate::arch::x86_64::user_execution::UserExecutionBinding::new(
            ProcessId::new(1, 2), TaskId::new(3, 4), id, prepared,
        ).unwrap();
        UserReturnTransfer::new(binding)
    }

    #[test]
    fn transfer_rejects_current_task() {
        let transfer = transfer();
        assert_eq!(transfer.validate(Some(transfer.task())), Err(UserReturnTransferError::CurrentTask));
    }

    #[test]
    fn transfer_accepts_valid_successor_frame() {
        let transfer = transfer();
        assert_eq!(transfer.validate(None), Ok(transfer));
        assert_ne!(transfer.frame().rip, 0);
        assert_ne!(transfer.frame().rsp, 0);
    }
}
