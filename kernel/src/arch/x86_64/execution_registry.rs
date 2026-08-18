//! Stable ownership registry for x86_64 task execution state.
//!
//! The registry owns one concrete execution binding per live task. Entries are
//! boxed so growing the registry cannot relocate a live kernel stack or saved
//! context. The registry itself remains single-CPU during bootstrap; SMP
//! synchronization belongs to a later per-CPU/locking layer.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::scheduler::{ExecutionBinding, TaskId};

use super::execution::{ExecutionError, X86ExecutionBinding};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RegistryError {
    TaskAlreadyBound,
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
        task_id: TaskId,
        entry: extern "C" fn() -> !,
    ) -> Result<(), RegistryInsertError> {
        if self.get(task_id).is_some() {
            return Err(RegistryInsertError::AlreadyBound);
        }

        let binding = Box::new(
            X86ExecutionBinding::new(task_id, entry)
                .map_err(RegistryInsertError::Allocation)?,
        );

        if let Some(slot) = self.entries.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(binding);
            return Ok(());
        }

        self.entries.push(Some(binding));
        Ok(())
    }

    pub fn remove(&mut self, task_id: TaskId) -> bool {
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

    pub fn count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.is_some()).count()
    }

    pub fn is_valid(&self, task_id: TaskId) -> bool {
        self.get(task_id)
            .map(|binding| binding.validate().is_ok())
            .unwrap_or(false)
    }
}

#[derive(Debug)]
pub enum RegistryInsertError {
    AlreadyBound,
    Allocation(ExecutionError),
}

#[cfg(test)]
mod tests {
    use super::{ExecutionRegistry, RegistryInsertError};
    use crate::scheduler::TaskId;

    extern "C" fn never_returns() -> ! {
        loop {}
    }

    #[test]
    fn registry_owns_one_execution_binding_per_task() {
        let mut registry = ExecutionRegistry::new();
        let task = TaskId::new(1, 1);
        registry.insert(task, never_returns).unwrap();
        assert_eq!(registry.count(), 1);
        assert!(registry.is_valid(task));
    }

    #[test]
    fn duplicate_task_binding_is_rejected() {
        let mut registry = ExecutionRegistry::new();
        let task = TaskId::new(1, 1);
        registry.insert(task, never_returns).unwrap();
        assert!(matches!(
            registry.insert(task, never_returns),
            Err(RegistryInsertError::AlreadyBound)
        ));
    }

    #[test]
    fn stale_generation_does_not_match_reused_index() {
        let mut registry = ExecutionRegistry::new();
        let old = TaskId::new(2, 1);
        let new = TaskId::new(2, 2);
        registry.insert(old, never_returns).unwrap();
        assert!(registry.get(old).is_some());
        assert!(registry.get(new).is_none());
    }

    #[test]
    fn removing_execution_is_terminal_for_that_handle() {
        let mut registry = ExecutionRegistry::new();
        let task = TaskId::new(3, 9);
        registry.insert(task, never_returns).unwrap();
        assert!(registry.remove(task));
        assert!(registry.get(task).is_none());
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn boxed_entry_address_survives_registry_growth() {
        let mut registry = ExecutionRegistry::new();
        let first = TaskId::new(4, 1);
        registry.insert(first, never_returns).unwrap();
        let before = registry.get(first).unwrap() as *const _;

        for index in 5..128u32 {
            registry.insert(TaskId::new(index, 1), never_returns).unwrap();
        }

        let after = registry.get(first).unwrap() as *const _;
        assert_eq!(before, after);
    }
}
