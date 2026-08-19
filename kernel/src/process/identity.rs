//! Stable process identity and execution binding.

use super::AddressSpaceId;
use crate::scheduler::{ExecutionHandle, TaskId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ProcessId {
    index: u32,
    generation: u32,
}

impl ProcessId {
    pub const fn new(index: u32, generation: u32) -> Self { Self { index, generation } }
    pub const fn index(self) -> u32 { self.index }
    pub const fn generation(self) -> u32 { self.generation }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ProcessExecutionBinding {
    process: ProcessId,
    task: TaskId,
    execution: ExecutionHandle,
    address_space: AddressSpaceId,
}

impl ProcessExecutionBinding {
    pub const fn new(
        process: ProcessId,
        task: TaskId,
        execution: ExecutionHandle,
        address_space: AddressSpaceId,
    ) -> Option<Self> {
        let execution_task = execution.task_id();
        if execution_task.index() != task.index() || execution_task.generation() != task.generation() {
            return None;
        }
        Some(Self { process, task, execution, address_space })
    }

    pub const fn process(self) -> ProcessId { self.process }
    pub const fn task(self) -> TaskId { self.task }
    pub const fn execution(self) -> ExecutionHandle { self.execution }
    pub const fn address_space(self) -> AddressSpaceId { self.address_space }
}

#[cfg(test)]
mod tests {
    use super::{ProcessExecutionBinding, ProcessId};
    use crate::memory::AddressSpaceId;
    use crate::scheduler::{ExecutionHandle, TaskId};

    #[test]
    fn binding_keeps_identity_domains_distinct() {
        let process = ProcessId::new(1, 4);
        let task = TaskId::new(7, 9);
        let execution = ExecutionHandle::for_task(task);
        let address_space = AddressSpaceId::new(11).unwrap();
        let binding = ProcessExecutionBinding::new(process, task, execution, address_space).unwrap();
        assert_eq!(binding.process(), process);
        assert_eq!(binding.task(), task);
        assert_eq!(binding.execution(), execution);
        assert_eq!(binding.address_space(), address_space);
    }

    #[test]
    fn rejects_execution_handle_from_another_task() {
        let process = ProcessId::new(1, 4);
        let task = TaskId::new(7, 9);
        let other = TaskId::new(7, 10);
        assert!(ProcessExecutionBinding::new(
            process,
            task,
            ExecutionHandle::for_task(other),
            AddressSpaceId::new(11).unwrap(),
        ).is_none());
    }
}
