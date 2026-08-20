//! Validated userspace-to-userspace return transfer plan.
//!
//! This module intentionally performs no task or address-space mutation. It
//! packages the exact successor state that the terminal architectural return
//! path must activate before constructing a CPL3 `iretq` frame.

use crate::process::ProcessId;
use crate::scheduler::TaskId;

use super::address_space::AddressSpaceRoot;
use super::interrupted_state::InterruptedState;
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
    pub const fn root(self) -> AddressSpaceRoot { self.binding.launch().root() }
    pub const fn frame(self) -> UserReturnFrame { self.binding.launch().frame() }
    pub const fn resume(self) -> Option<InterruptedState> { self.binding.resume() }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserReturnTransferError {
    CurrentTask,
    InvalidSelectors,
    InvalidRflags,
    InvalidResumeState,
}

impl UserReturnTransfer {
    pub fn validate(self, current_task: Option<TaskId>) -> Result<Self, UserReturnTransferError> {
        if current_task == Some(self.task) { return Err(UserReturnTransferError::CurrentTask); }

        if let Some(state) = self.resume() {
            if !state.is_user_valid() {
                return Err(UserReturnTransferError::InvalidResumeState);
            }
            return Ok(self);
        }

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
    use crate::arch::x86_64::interrupted_state::InterruptedState;
    use crate::arch::x86_64::interrupt_entry::{InterruptReturnFrame, SavedRegisters};
    use crate::arch::x86_64::user_launch::prepare_launch;
    use crate::memory::{user_stack_range, USER_SPACE_START};
    use crate::process::{AddressSpaceId, ProcessId, UserLaunchPlan};
    use crate::scheduler::TaskId;

    fn transfer(task: TaskId, process: ProcessId) -> UserReturnTransfer {
        let id = AddressSpaceId::new(task.index().wrapping_add(1).max(1)).unwrap();
        let root = AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap();
        let stack = user_stack_range().unwrap();
        let plan = UserLaunchPlan { address_space: id, entry: USER_SPACE_START + 0x1000, stack_pointer: stack.end() };
        let prepared = prepare_launch(root, plan).unwrap();
        let binding = crate::arch::x86_64::user_execution::UserExecutionBinding::new(process, task, id, prepared).unwrap();
        UserReturnTransfer::new(binding)
    }

    fn user_state(rip: u64, rax: u64) -> InterruptedState {
        let mut registers = SavedRegisters::default();
        registers.rax = rax;
        let stack = user_stack_range().unwrap();
        let mut raw = [0u64; 5];
        raw[0] = rip;
        raw[1] = 0x1b;
        raw[2] = 0x202;
        raw[3] = stack.end();
        raw[4] = 0x13;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        unsafe { InterruptedState::capture(&registers, frame) }
    }

    #[test]
    fn transfer_rejects_current_task() {
        let transfer = transfer(TaskId::new(3, 4), ProcessId::new(1, 2));
        assert_eq!(transfer.validate(Some(transfer.task())), Err(UserReturnTransferError::CurrentTask));
    }

    #[test]
    fn transfer_accepts_valid_initial_frame() {
        let transfer = transfer(TaskId::new(3, 4), ProcessId::new(1, 2));
        assert_eq!(transfer.validate(None), Ok(transfer));
        assert_ne!(transfer.frame().rip, 0);
        assert_ne!(transfer.frame().rsp, 0);
    }

    #[test]
    fn transfer_accepts_valid_resume_snapshot() {
        let transfer = transfer(TaskId::new(3, 4), ProcessId::new(1, 2));
        let mut binding = transfer.binding();
        binding.install_resume(user_state(USER_SPACE_START + 0x5000, 7)).unwrap();
        let transfer = UserReturnTransfer::new(binding);
        assert_eq!(transfer.validate(None), Ok(transfer));
        assert_eq!(transfer.resume().unwrap().registers().rax, 7);
    }

    #[test]
    fn userspace_a_b_a_preserves_a_resume_state() {
        let a = transfer(TaskId::new(10, 1), ProcessId::new(10, 1));
        let b = transfer(TaskId::new(11, 1), ProcessId::new(11, 1));

        let mut a_binding = a.binding();
        let a_state = user_state(USER_SPACE_START + 0x7000, 0xA5);
        a_binding.install_resume(a_state).unwrap();
        let a_resume = UserReturnTransfer::new(a_binding);

        // A -> B: successor B has never run, so it must use its initial launch frame.
        let b_transfer = b;
        assert!(b_transfer.resume().is_none());
        assert_eq!(b_transfer.validate(Some(a_transfer_task(a_resume))).is_ok(), true);

        // B -> A: A must return through its saved snapshot, not its original entry point.
        assert_eq!(a_resume.resume().unwrap().return_state().rip(), USER_SPACE_START + 0x7000);
        assert_eq!(a_resume.resume().unwrap().registers().rax, 0xA5);
        assert!(a_resume.validate(Some(b_transfer.task())).is_ok());
    }

    const fn a_transfer_task(transfer: UserReturnTransfer) -> TaskId { transfer.task() }
}
