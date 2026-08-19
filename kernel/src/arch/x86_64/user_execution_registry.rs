//! Stable registry for userspace execution bindings.
//!
//! User tasks keep their ring3 `iretq` launch state separate from kernel task
//! context-switch state. The registry owns those bindings by TaskId.

use alloc::vec::Vec;

use crate::scheduler::TaskId;

use super::user_execution::UserExecutionBinding;

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
        if self.get(binding.task()).is_some() {
            return Err(UserExecutionRegistryError::AlreadyBound);
        }
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
    use super::{UserExecutionRegistry, UserExecutionRegistryError};
    use crate::arch::x86_64::address_space::AddressSpaceRoot;
    use crate::arch::x86_64::user_execution::UserExecutionBinding;
    use crate::arch::x86_64::user_launch::prepare_launch;
    use crate::memory::USER_SPACE_START;
    use crate::process::{AddressSpaceId, ProcessId, UserLaunchPlan};
    use crate::scheduler::TaskId;

    fn binding() -> UserExecutionBinding {
        let id = AddressSpaceId::new(7).unwrap();
        let root = AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap();
        let plan = UserLaunchPlan { address_space: id, entry: USER_SPACE_START + 0x1000, stack_pointer: 0x7000_0000 };
        let launch = prepare_launch(root, plan).unwrap();
        UserExecutionBinding::new(ProcessId::new(1, 1), TaskId::new(2, 1), id, launch).unwrap()
    }

    #[test]
    fn registry_rejects_duplicate_task_binding() {
        let mut registry = UserExecutionRegistry::new();
        let item = binding();
        assert!(registry.insert(item).is_ok());
        assert_eq!(registry.insert(item), Err(UserExecutionRegistryError::AlreadyBound));
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn registry_removes_binding_terminally() {
        let mut registry = UserExecutionRegistry::new();
        let item = binding();
        assert!(registry.insert(item).is_ok());
        assert_eq!(registry.remove(item.task()).unwrap(), item);
        assert!(registry.get(item.task()).is_none());
        assert_eq!(registry.count(), 0);
    }
}
