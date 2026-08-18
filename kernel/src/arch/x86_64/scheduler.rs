//! x86_64 execution state owned by the architecture scheduler adapter.
//!
//! The generic scheduler owns task identity and lifecycle. This module owns the
//! hardware-facing execution state: a kernel stack and the saved register
//! context required by `context_switch::switch`.

use alloc::vec::Vec;
use super::context_switch::{bootstrap_context, Context};
pub const KERNEL_STACK_SIZE: usize = 32 * 1024;

#[derive(Debug)]
pub enum ExecutionStateError { StackAllocationFailed, InvalidStack }

#[derive(Debug)]
pub struct TaskExecutionState { stack: Vec<u8>, context: Context }

impl TaskExecutionState {
    pub fn new(entry: extern "C" fn() -> !) -> Result<Self, ExecutionStateError> {
        let mut stack = Vec::new();
        stack.try_reserve_exact(KERNEL_STACK_SIZE).map_err(|_| ExecutionStateError::StackAllocationFailed)?;
        stack.resize(KERNEL_STACK_SIZE, 0);
        let stack_top = stack.as_ptr().cast::<u8>().addr().checked_add(stack.len()).ok_or(ExecutionStateError::InvalidStack)? as u64;
        let context = bootstrap_context(stack_top, entry).ok_or(ExecutionStateError::InvalidStack)?;
        Ok(Self { stack, context })
    }
    pub const fn context(&self) -> &Context { &self.context }
    pub fn context_mut(&mut self) -> &mut Context { &mut self.context }
    pub fn stack_size(&self) -> usize { self.stack.len() }
}

#[cfg(test)]
mod tests {
    use super::{TaskExecutionState, KERNEL_STACK_SIZE};
    extern "C" fn never_returns() -> ! { loop {} }
    #[test] fn execution_state_owns_a_kernel_stack() { let state = TaskExecutionState::new(never_returns).unwrap(); assert_eq!(state.stack_size(), KERNEL_STACK_SIZE); assert!(state.context().is_initialized()); }
}
