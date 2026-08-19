//! Validated userspace successor selection.
//!
//! This module is intentionally side-effect free. It identifies the next
//! userspace task that could receive a terminal return, but performs no state
//! mutation. Terminal mutation belongs to the architectural return path.

use crate::process::{ProcessId, ProcessState};
use crate::scheduler::{TaskId, TaskKind, TaskState};

use super::system_runtime::SystemRuntime;
use super::user_execution::UserExecutionBinding;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserSuccessorPlan {
    process: ProcessId,
    task: TaskId,
    binding: UserExecutionBinding,
}

impl UserSuccessorPlan {
    pub const fn process(self) -> ProcessId { self.process }
    pub const fn task(self) -> TaskId { self.task }
    pub const fn binding(self) -> UserExecutionBinding { self.binding }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSuccessorError {
    NoReadyTask,
    NotUserspace,
    MissingBinding,
    ProcessBindingMismatch,
    TaskNotReady,
}

/// Selects, but does not mutate, the next userspace task.
pub fn select_userspace_successor(
    system: &SystemRuntime,
    current_task: Option<TaskId>,
) -> Result<UserSuccessorPlan, UserSuccessorError> {
    let task = system.runtime().manager().scheduler.next_ready()
        .ok_or(UserSuccessorError::NoReadyTask)?;

    if Some(task) == current_task {
        return Err(UserSuccessorError::TaskNotReady);
    }
    if system.runtime().manager().scheduler.task_kind(task) != Some(TaskKind::Userspace) {
        return Err(UserSuccessorError::NotUserspace);
    }
    if system.runtime().manager().scheduler.state(task) != Some(TaskState::Ready) {
        return Err(UserSuccessorError::TaskNotReady);
    }

    let binding = system.userspace().get(task).ok_or(UserSuccessorError::MissingBinding)?;
    let process_binding = system.processes().binding(binding.process())
        .map_err(|_| UserSuccessorError::ProcessBindingMismatch)?
        .ok_or(UserSuccessorError::ProcessBindingMismatch)?;

    if process_binding.task() != task
        || process_binding.address_space() != binding.address_space()
        || system.processes().state(binding.process()) != Ok(ProcessState::Ready)
    {
        return Err(UserSuccessorError::ProcessBindingMismatch);
    }

    Ok(UserSuccessorPlan { process: binding.process(), task, binding })
}

#[cfg(test)]
mod tests {
    use super::{select_userspace_successor, UserSuccessorError};

    #[test]
    fn selector_reports_empty_ready_queue_without_mutation() {
        let system = crate::arch::x86_64::system_runtime::SystemRuntime::new();
        assert_eq!(select_userspace_successor(&system, None), Err(UserSuccessorError::NoReadyTask));
    }
}
