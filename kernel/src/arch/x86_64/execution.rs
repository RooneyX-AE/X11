//! x86_64 execution binding for the architecture-independent scheduler.
//!
//! This adapter owns the kernel stack and voluntary context for one task.
//! Interrupt/preemption state, CR3, FPU/SIMD state, and CPU-local ownership
//! are deliberately outside this binding until their contracts are defined.

use alloc::vec::Vec;

use crate::scheduler::{ExecutionBinding, TaskId};

use super::context_switch::{bootstrap_context, Context};

pub const KERNEL_STACK_SIZE: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExecutionError {
    StackAllocationFailed,
    InvalidStack,
    TaskMismatch,
}

#[derive(Debug)]
pub struct X86ExecutionBinding {
    task_id: TaskId,
    stack: Vec<u8>,
    context: Context,
}

impl X86ExecutionBinding {
    pub fn new(task_id: TaskId, entry: extern "C" fn() -> !) -> Result<Self, ExecutionError> {
        let mut stack = Vec::new();
        stack
            .try_reserve_exact(KERNEL_STACK_SIZE)
            .map_err(|_| ExecutionError::StackAllocationFailed)?;
        stack.resize(KERNEL_STACK_SIZE, 0);

        let stack_top = stack
            .as_ptr()
            .cast::<u8>()
            .addr()
            .checked_add(stack.len())
            .ok_or(ExecutionError::InvalidStack)? as u64;

        let context = bootstrap_context(stack_top, entry).ok_or(ExecutionError::InvalidStack)?;

        Ok(Self {
            task_id,
            stack,
            context,
        })
    }

    pub const fn context(&self) -> &Context {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }

    pub fn stack_size(&self) -> usize {
        self.stack.len()
    }
}

impl ExecutionBinding for X86ExecutionBinding {
    type Error = ExecutionError;

    fn task_id(&self) -> TaskId {
        self.task_id
    }

    fn is_bootstrapped(&self) -> bool {
        self.context.is_initialized() && self.stack.len() == KERNEL_STACK_SIZE
    }

    fn validate(&self) -> Result<(), Self::Error> {
        if self.stack.len() != KERNEL_STACK_SIZE || !self.context.is_initialized() {
            return Err(ExecutionError::InvalidStack);
        }
        Ok(())
    }

    fn validate_for(&self, task_id: TaskId) -> Result<(), Self::Error> {
        if self.task_id != task_id {
            return Err(ExecutionError::TaskMismatch);
        }
        self.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::{X86ExecutionBinding, KERNEL_STACK_SIZE};
    use crate::scheduler::ExecutionBinding;

    extern "C" fn never_returns() -> ! {
        loop {}
    }

    #[test]
    fn execution_binding_owns_stable_stack() {
        let binding = X86ExecutionBinding::new(crate::scheduler::TaskId::new(1, 1), never_returns)
            .unwrap();
        assert_eq!(binding.stack_size(), KERNEL_STACK_SIZE);
        assert!(binding.is_bootstrapped());
        assert!(binding.validate().is_ok());
    }
}
