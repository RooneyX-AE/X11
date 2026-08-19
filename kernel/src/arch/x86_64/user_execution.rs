//! Explicit userspace execution binding.
//!
//! Userspace execution is intentionally not represented by the kernel
//! `Context` used by voluntary kernel switches. A ring3 launch owns an
//! `iretq` return frame and an address-space identity instead.

use alloc::vec::Vec;

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
    pub fn new(
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

#[derive(Debug, Default)]
pub struct UserExecutionRegistry {
    entries: Vec<Option<UserExecutionBinding>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserExecutionRegistryError {
    AlreadyBound,
    NotFound,
}

impl UserExecutionRegistry {
    pub const fn new() -> Self { Self { entries: Vec::new() } }

    pub fn insert(&mut self, binding: UserExecutionBinding) -> Result<(), UserExecutionRegistryError> {
        if self.get(binding.task()).is_some() { return Err(UserExecutionRegistryError::AlreadyBound); }
        self.entries.push(Some(binding));
        Ok(())
    }

    pub fn remove(&mut self, task: TaskId) -> Result<UserExecutionBinding, UserExecutionRegistryError> {
        let slot = self.entries.iter_mut().find(|entry| entry.as_ref().is_some_and(|binding| binding.task() == task))
            .ok_or(UserExecutionRegistryError::NotFound)?;
        slot.take().ok_or(UserExecutionRegistryError::NotFound)
    }

    pub fn get(&self, task: TaskId) -> Option<UserExecutionBinding> {
        self.entries.iter().flatten().copied().find(|binding| binding.task() == task)
    }

    pub fn count(&self) -> usize { self.entries.iter().filter(|entry| entry.is_some()).count() }
}

#[cfg(test)]
mod tests {
    use super::{UserExecutionBinding, UserExecutionBindingError, UserExecutionRegistry, UserExecutionRegistryError};
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

    fn binding() -> UserExecutionBinding {
        let (id, launch) = launch();
        UserExecutionBinding::new(ProcessId::new(1, 2), TaskId::new(3, 4), id, launch).unwrap()
    }

    #[test]
    fn binding_rejects_mismatched_address_space() {
        let (id, launch) = launch();
        assert_eq!(UserExecutionBinding::new(ProcessId::new(1, 2), TaskId::new(3, 4), AddressSpaceId::new(8).unwrap(), launch), Err(UserExecutionBindingError::AddressSpaceMismatch));
        assert_eq!(launch.address_space(), id);
    }

    #[test]
    fn registry_rejects_duplicate_task() {
        let mut registry = UserExecutionRegistry::new();
        let value = binding();
        registry.insert(value).unwrap();
        assert_eq!(registry.insert(value), Err(UserExecutionRegistryError::AlreadyBound));
    }

    #[test]
    fn registry_removes_exact_task() {
        let mut registry = UserExecutionRegistry::new();
        let value = binding();
        registry.insert(value).unwrap();
        assert_eq!(registry.remove(value.task()), Ok(value));
        assert_eq!(registry.remove(value.task()), Err(UserExecutionRegistryError::NotFound));
    }
}
