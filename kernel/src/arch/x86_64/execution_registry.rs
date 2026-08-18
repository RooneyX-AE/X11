//! Stable ownership registry for x86_64 task execution state.
//!
//! The registry owns one concrete execution binding per live task. Entries are
//! boxed so growing the registry cannot relocate a live kernel stack or saved
//! context. The registry itself remains single-CPU during bootstrap; SMP
//! synchronization belongs to a later per-CPU/locking layer.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::scheduler::{ExecutionBinding, ExecutionHandle, TaskId};

use super::execution::{ExecutionError, X86ExecutionBinding};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RegistryInsertError {
    AlreadyBound,
    Allocation,
}

#[derive(Debug, Default)]
pub struct ExecutionRegistry {
    entries: Vec<Option<Box<X86ExecutionBinding>>>,
}

impl ExecutionRegistry {
    pub const fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn insert(
        &mut self,
        handle: ExecutionHandle,
        entry: extern "C" fn() -> !,
    ) -> Result<(), RegistryInsertError> {
        let task_id = handle.task_id();
        if self.get(task_id).is_some() {
            return Err(RegistryInsertError::AlreadyBound);
        }

        let binding = Box::new(
            X86ExecutionBinding::new(task_id, entry)
                .map_err(|_: ExecutionError| RegistryInsertError::Allocation)?,
        );

        if let Some(slot) = self.entries.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(binding);
            return Ok(());
        }

        self.entries.push(Some(binding));
        Ok(())
    }

    pub fn remove(&mut self, handle: ExecutionHandle) -> bool {
        let task_id = handle.task_id();
        let Some(slot) = self
            .entries
            .iter_mut()
            .find(|slot| slot.as_deref().is_some_and(|binding| binding.task_id() == task_id))
        else {
            return false;
        };
        *slot = None;
        true
    }

    pub fn get(&self, task_id: TaskId) -> Option<&X86ExecutionBinding> {
        self.entries
            .iter()
            .filter_map(Option::as_deref)
            .find(|binding| binding.task_id() == task_id)
    }

    pub fn get_mut(&mut self, task_id: TaskId) -> Option<&mut X86ExecutionBinding> {
        self.entries
            .iter_mut()
            .filter_map(Option::as_deref_mut)
            .find(|binding| binding.task_id() == task_id)
    }

    pub fn contains(&self, handle: ExecutionHandle) -> bool {
        self.get(handle.task_id()).is_some()
    }

    pub fn count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.is_some()).count()
    }

    pub fn is_valid(&self, handle: ExecutionHandle) -> bool {
        self.get(handle.task_id())
            .map(|binding| binding.validate().is_ok())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionRegistry, RegistryInsertError};
    use crate::scheduler::{ExecutionHandle, TaskId};

    extern "C" fn never_returns() -> ! {
        loop {}
    }

    #[test]
    fn registry_owns_one_execution_binding_per_handle() {
        let mut registry = ExecutionRegistry::new();
        let handle = ExecutionHandle::for_task(TaskId::new(1, 1));
        registry.insert(handle, never_returns).unwrap();
        assert_eq!(registry.count(), 1);
        assert!(registry.contains(handle));
        assert!(registry.is_valid(handle));
    }

    #[test]
    fn duplicate_handle_is_rejected() {
        let mut registry = ExecutionRegistry::new();
        let handle = ExecutionHandle::for_task(TaskId::new(1, 1));
        registry.insert(handle, never_returns).unwrap();
        assert_eq!(
            registry.insert(handle, never_returns),
            Err(RegistryInsertError::AlreadyBound)
        );
    }

    #[test]
    fn stale_generation_does_not_match_reused_index() {
        let mut registry = ExecutionRegistry::new();
        let old = ExecutionHandle::for_task(TaskId::new(2, 1));
        let new = ExecutionHandle::for_task(TaskId::new(2, 2));
        registry.insert(old, never_returns).unwrap();
        assert!(registry.contains(old));
        assert!(!registry.contains(new));
    }

    #[test]
    fn removing_execution_is_terminal_for_that_handle() {
        let mut registry = ExecutionRegistry::new();
        let handle = ExecutionHandle::for_task(TaskId::new(3, 9));
        registry.insert(handle, never_returns).unwrap();
        assert!(registry.remove(handle));
        assert!(!registry.contains(handle));
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn boxed_entry_address_survives_registry_growth() {
        let mut registry = ExecutionRegistry::new();
        let first = ExecutionHandle::for_task(TaskId::new(4, 1));
        registry.insert(first, never_returns).unwrap();
        let before = registry.get(first.task_id()).unwrap() as *const _;

        for index in 5..128u32 {
            let handle = ExecutionHandle::for_task(TaskId::new(index, 1));
            registry.insert(handle, never_returns).unwrap();
        }

        let after = registry.get(first.task_id()).unwrap() as *const _;
        assert_eq!(before, after);
    }
}
