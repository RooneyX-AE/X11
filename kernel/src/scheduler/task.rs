//! Architecture-independent task identity and lifecycle state.

use super::execution::ExecutionHandle;

/// Generational identifier prevents stale handles from referring to reused task slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TaskId {
    index: u32,
    generation: u32,
}

impl TaskId {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    pub const fn index(self) -> u32 { self.index }
    pub const fn generation(self) -> u32 { self.generation }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState { Created, Ready, Running, Blocked, Exited }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Priority(u8);

impl Priority {
    pub const MIN: Self = Self(0);
    pub const DEFAULT: Self = Self(128);
    pub const MAX: Self = Self(u8::MAX);
    pub const fn new(value: u8) -> Self { Self(value) }
    pub const fn value(self) -> u8 { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionAttachError { AlreadyAttached }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskControlBlock {
    id: TaskId,
    state: TaskState,
    priority: Priority,
    execution: Option<ExecutionHandle>,
}

impl TaskControlBlock {
    pub const fn new(id: TaskId, priority: Priority) -> Self {
        Self { id, state: TaskState::Created, priority, execution: None }
    }
    pub const fn id(self) -> TaskId { self.id }
    pub const fn state(self) -> TaskState { self.state }
    pub const fn priority(self) -> Priority { self.priority }
    pub const fn execution(self) -> Option<ExecutionHandle> { self.execution }

    pub fn attach_execution(&mut self) -> Result<ExecutionHandle, ExecutionAttachError> {
        if self.execution.is_some() {
            return Err(ExecutionAttachError::AlreadyAttached);
        }
        let handle = ExecutionHandle::for_task(self.id);
        self.execution = Some(handle);
        Ok(handle)
    }

    pub fn detach_execution(&mut self) -> Option<ExecutionHandle> { self.execution.take() }

    pub fn transition(&mut self, state: TaskState) -> bool {
        let valid = matches!(
            (self.state, state),
            (TaskState::Created, TaskState::Ready)
                | (TaskState::Ready, TaskState::Running)
                | (TaskState::Running, TaskState::Ready)
                | (TaskState::Running, TaskState::Blocked)
                | (TaskState::Blocked, TaskState::Ready)
                | (TaskState::Running, TaskState::Exited)
                | (TaskState::Blocked, TaskState::Exited)
        );
        if valid { self.state = state; }
        valid
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionAttachError, Priority, TaskControlBlock, TaskId, TaskState};

    #[test]
    fn task_state_machine_rejects_illegal_transition() {
        let id = TaskId::new(3, 7);
        let mut task = TaskControlBlock::new(id, Priority::DEFAULT);
        assert!(!task.transition(TaskState::Running));
        assert_eq!(task.state(), TaskState::Created);
    }

    #[test]
    fn task_state_machine_accepts_ready_to_running() {
        let id = TaskId::new(3, 7);
        let mut task = TaskControlBlock::new(id, Priority::DEFAULT);
        assert!(task.transition(TaskState::Ready));
        assert!(task.transition(TaskState::Running));
        assert_eq!(task.state(), TaskState::Running);
    }

    #[test]
    fn execution_handle_can_only_be_attached_once() {
        let id = TaskId::new(3, 7);
        let mut task = TaskControlBlock::new(id, Priority::DEFAULT);
        let handle = task.attach_execution().unwrap();
        assert_eq!(task.execution(), Some(handle));
        assert_eq!(task.attach_execution(), Err(ExecutionAttachError::AlreadyAttached));
        assert_eq!(task.execution(), Some(handle));
    }

    #[test]
    fn priority_boundaries_are_stable() {
        assert_eq!(Priority::MIN.value(), 0);
        assert_eq!(Priority::DEFAULT.value(), 128);
        assert_eq!(Priority::MAX.value(), 255);
    }
}
