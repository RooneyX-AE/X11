//! Voluntary x86_64 dispatch boundary.
//!
//! The generic scheduler decides which task should run. This layer validates
//! the concrete execution bindings and is the only architecture-facing place
//! allowed to request a context switch during bootstrap.

use crate::scheduler::{ExecutionBinding, TaskId};

use super::context_switch;
use super::execution::X86ExecutionBinding;
use super::execution_registry::ExecutionRegistry;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DispatchError {
    MissingExecution,
    InvalidExecution,
    SameTask,
}

pub fn validate_transition(
    registry: &ExecutionRegistry,
    previous: Option<TaskId>,
    next: TaskId,
) -> Result<(), DispatchError> {
    let next_binding = registry
        .get(next)
        .ok_or(DispatchError::MissingExecution)?;

    if !next_binding.validate().is_ok() {
        return Err(DispatchError::InvalidExecution);
    }

    if previous == Some(next) {
        return Err(DispatchError::SameTask);
    }

    Ok(())
}

/// Execute a voluntary switch after the caller has established all scheduler
/// invariants. The current task's saved context is supplied by the caller;
/// this function only performs the architecture-specific primitive.
///
/// # Safety
/// The caller must guarantee that `current` and `next` point to live execution
/// contexts whose stacks remain allocated for the entire switch operation.
pub unsafe fn switch(
    current: &mut context_switch::Context,
    next: &context_switch::Context,
) {
    unsafe { context_switch::switch(current as *mut _, next as *const _) };
}

#[allow(dead_code)]
fn _type_check_binding(_: &X86ExecutionBinding) {}