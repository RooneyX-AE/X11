//! Explicit userspace execution binding.
//!
//! Userspace execution is intentionally not represented by the kernel
//! `Context` used by voluntary kernel switches. A ring3 launch owns an
//! `iretq` return frame and an address-space identity instead.

use crate::process::{AddressSpaceId, ProcessId};
use crate::scheduler::TaskId;

use super::user_launch::PreparedUserLaunch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserExecutionBinding {
    process: ProcessId,
    task: TaskId,
    address_space: AddressSpaceId,
    launch: PreparedUserLaunch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserExecutionBindingError {
    AddressSpaceMismatch,
}

impl UserExecutionBinding {
    pub const fn new(
        process: ProcessId,
        task: TaskId,
        address_space: AddressSpaceId,
        launch: PreparedUserLaunch,
    ) -> Result<Self, UserExecutionBindingError> {
        if launch.address_space() != address_space {
            return Err(UserExecutionBindingError::AddressSpaceMismatch);
        }
        Ok(Self { process, task, address_space, launch })
    }

    pub const fn process(self) -> ProcessId { self.process }
    pub const fn task(self) -> TaskId { self.task }
    pub const fn address_space(self) -> AddressSpaceId { self.address_space }
    pub const fn launch(self) -> PreparedUserLaunch { self.launch }
}

#[cfg(test)]
mod tests {
    use super::{UserExecutionBinding, UserExecutionBindingError};
    use crate::arch::x86_64::address_space::AddressSpaceRoot;
    use crate::arch::x86_64::user_launch::prepare_launch;
    use crate::memory::{user_stack_range, USER_SPACE_START};
    use crate::process::{AddressSpaceId, ProcessId, UserLaunchPlan};
    use crate::scheduler::TaskId;

    fn launch() -> (AddressSpaceId, super::PreparedUserLaunch) {
        let id = AddressSpaceId::new(7).unwrap();
        let root = AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap();
        let stack = user_stack_range().unwrap();
        let plan = UserLaunchPlan { address_space: id, entry: USER_SPACE_START + 0x1000, stack_pointer: stack.end() };
        (id, prepare_launch(root, plan).unwrap())
    }

    #[test]
    fn binding_keeps_user_execution_identity_together() {
        let (id, launch) = launch();
        let binding = UserExecutionBinding::new(ProcessId::new(1, 2), TaskId::new(3, 4), id, launch);
        assert!(binding.is_ok());
    }

    #[test]
    fn binding_rejects_address_space_mismatch() {
        let (_, launch) = launch();
        assert_eq!(
            UserExecutionBinding::new(
                ProcessId::new(1, 2),
                TaskId::new(3, 4),
                AddressSpaceId::new(8).unwrap(),
                launch,
            ),
            Err(UserExecutionBindingError::AddressSpaceMismatch)
        );
    }
}
