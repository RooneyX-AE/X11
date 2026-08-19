//! x86_64 execution binding for the architecture-independent scheduler.
//!
//! This adapter owns the kernel stack, activation metadata, voluntary context,
//! and an optional interrupted CPU snapshot. Interrupt frames are copied out of
//! transient IRQ stack memory before the handler returns.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::scheduler::{ExecutionBinding, TaskId};

use super::activation::ActivationRecord;
use super::context_switch::{bootstrap_kernel_context, Context};
use super::interrupted_state::InterruptedState;
use super::interrupt_entry::{InterruptReturnFrame, SavedRegisters};

pub const KERNEL_STACK_SIZE: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExecutionError {
    StackAllocationFailed,
    InvalidStack,
    InterruptedStateAlreadyPresent,
}

#[derive(Debug)]
pub struct X86ExecutionBinding {
    task_id: TaskId,
    stack: Vec<u8>,
    activation: Box<ActivationRecord>,
    context: Context,
    interrupted: Option<InterruptedState>,
}

impl X86ExecutionBinding {
    pub fn new(task_id: TaskId, entry: extern "C" fn() -> !) -> Result<Self, ExecutionError> {
        let mut stack = Vec::new();
        stack
            .try_reserve_exact(KERNEL_STACK_SIZE)
            .map_err(|_| ExecutionError::StackAllocationFailed)?;
        stack.resize(KERNEL_STACK_SIZE, 0);

        let activation = ActivationRecord::new(task_id, entry);
        let stack_top = stack
            .as_ptr()
            .cast::<u8>()
            .addr()
            .checked_add(stack.len())
            .ok_or(ExecutionError::InvalidStack)? as u64;
        let context = bootstrap_kernel_context(stack_top, &activation)
            .ok_or(ExecutionError::InvalidStack)?;

        Ok(Self { task_id, stack, activation, context, interrupted: None })
    }

    pub const fn context(&self) -> &Context { &self.context }
    pub fn context_mut(&mut self) -> &mut Context { &mut self.context }
    pub const fn activation(&self) -> &ActivationRecord { &self.activation }
    pub fn stack_size(&self) -> usize { self.stack.len() }
    pub const fn interrupted(&self) -> Option<InterruptedState> { self.interrupted }

    /// Copies the interrupted CPU state out of the transient IRQ stack.
    ///
    /// # Safety
    /// `registers` and `return_frame` must describe the live state created by
    /// the architecture interrupt entry stub for this exact task.
    pub unsafe fn capture_interrupted(
        &mut self,
        registers: *const SavedRegisters,
        return_frame: InterruptReturnFrame,
        _resume_rsp: u64,
    ) -> Result<(), ExecutionError> {
        if self.interrupted.is_some() {
            return Err(ExecutionError::InterruptedStateAlreadyPresent);
        }
        let snapshot = unsafe { InterruptedState::capture(registers, return_frame) };
        self.install_interrupted(snapshot)
    }

    pub fn install_interrupted(&mut self, snapshot: InterruptedState) -> Result<(), ExecutionError> {
        if self.interrupted.is_some() {
            return Err(ExecutionError::InterruptedStateAlreadyPresent);
        }
        if !snapshot.is_valid() {
            return Err(ExecutionError::InvalidStack);
        }
        self.interrupted = Some(snapshot);
        Ok(())
    }

    pub const fn take_interrupted(&mut self) -> Option<InterruptedState> { self.interrupted.take() }
}

impl ExecutionBinding for X86ExecutionBinding {
    type Error = ExecutionError;

    fn task_id(&self) -> TaskId { self.task_id }

    fn is_bootstrapped(&self) -> bool {
        self.context.is_initialized()
            && self.stack.len() == KERNEL_STACK_SIZE
            && self.activation.task_id() == self.task_id
    }

    fn validate(&self) -> Result<(), Self::Error> {
        if self.stack.len() != KERNEL_STACK_SIZE
            || !self.context.is_initialized()
            || self.activation.task_id() != self.task_id
            || self.context.r12 != self.activation.pointer()
            || self.interrupted.is_some_and(|state| !state.is_valid())
        {
            return Err(ExecutionError::InvalidStack);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{X86ExecutionBinding, KERNEL_STACK_SIZE};
    use crate::arch::x86_64::interrupted_state::InterruptedState;
    use crate::arch::x86_64::interrupt_entry::{InterruptReturnFrame, SavedRegisters};
    use crate::scheduler::{ExecutionBinding, TaskId};

    extern "C" fn never_returns() -> ! { loop {} }

    fn kernel_snapshot() -> InterruptedState {
        let registers = SavedRegisters::default();
        let mut raw = [0u64; 3];
        raw[0] = 0x1000;
        raw[1] = 0x10;
        raw[2] = 0x202;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        unsafe { InterruptedState::capture(&registers, frame) }
    }

    #[test]
    fn execution_binding_owns_stable_stack_and_activation() {
        let binding = X86ExecutionBinding::new(TaskId::new(1, 1), never_returns).unwrap();
        assert_eq!(binding.stack_size(), KERNEL_STACK_SIZE);
        assert_eq!(binding.activation().task_id(), TaskId::new(1, 1));
        assert_eq!(binding.context().r12, binding.activation().pointer());
        assert!(binding.is_bootstrapped());
        assert!(binding.validate().is_ok());
        assert!(binding.interrupted().is_none());
    }

    #[test]
    fn interrupted_snapshot_is_owned_once() {
        let mut binding = X86ExecutionBinding::new(TaskId::new(2, 1), never_returns).unwrap();
        assert!(binding.install_interrupted(kernel_snapshot()).is_ok());
        assert!(matches!(
            binding.install_interrupted(kernel_snapshot()),
            Err(super::ExecutionError::InterruptedStateAlreadyPresent)
        ));
        assert!(binding.take_interrupted().is_some());
        assert!(binding.interrupted().is_none());
    }
}
