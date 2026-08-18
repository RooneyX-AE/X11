//! Architecture-independent execution binding contract.
//!
//! The scheduler owns task policy; an architecture adapter owns the concrete
//! register context, kernel stack, address-space state, and switch primitive.

use super::TaskId;

/// Opaque token proving that a task has an execution binding.
///
/// The token does not expose architecture-specific storage. Its identity is
/// tied to the generational task ID, so a reused task slot cannot inherit the
/// execution binding of an older task instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ExecutionHandle(TaskId);

impl ExecutionHandle {
    pub const fn for_task(task_id: TaskId) -> Self {
        Self(task_id)
    }

    pub const fn task_id(self) -> TaskId {
        self.0
    }
}

/// Minimal execution binding owned by a task.
///
/// The binding is deliberately opaque to the generic scheduler. Architecture
/// implementations may associate a kernel stack, saved registers, address
/// space, or other execution metadata without leaking those details here.
pub trait ExecutionBinding {
    type Error;

    fn task_id(&self) -> TaskId;
    fn is_bootstrapped(&self) -> bool;
    fn validate(&self) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionState {
    task_id: TaskId,
    bootstrapped: bool,
}

impl ExecutionState {
    pub const fn new(task_id: TaskId) -> Self {
        Self {
            task_id,
            bootstrapped: false,
        }
    }

    pub const fn task_id(self) -> TaskId {
        self.task_id
    }

    pub const fn is_bootstrapped(self) -> bool {
        self.bootstrapped
    }

    pub fn mark_bootstrapped(&mut self) {
        self.bootstrapped = true;
    }
}

impl ExecutionBinding for ExecutionState {
    type Error = core::convert::Infallible;

    fn task_id(&self) -> TaskId {
        self.task_id
    }

    fn is_bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    fn validate(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionBinding, ExecutionHandle, ExecutionState};
    use crate::scheduler::TaskId;

    #[test]
    fn execution_handle_is_bound_to_generational_task_id() {
        let task = TaskId::new(4, 2);
        let replacement = TaskId::new(4, 3);
        let handle = ExecutionHandle::for_task(task);
        assert_eq!(handle.task_id(), task);
        assert_ne!(handle.task_id(), replacement);
    }

    #[test]
    fn execution_state_starts_unbootstrapped() {
        let state = ExecutionState::new(TaskId::new(4, 2));
        assert_eq!(state.task_id(), TaskId::new(4, 2));
        assert!(!state.is_bootstrapped());
        assert!(state.validate().is_ok());
    }

    #[test]
    fn execution_state_can_be_bootstrapped_once() {
        let mut state = ExecutionState::new(TaskId::new(4, 2));
        assert!(!state.is_bootstrapped());
        state.mark_bootstrapped();
        assert!(state.is_bootstrapped());
    }
}
