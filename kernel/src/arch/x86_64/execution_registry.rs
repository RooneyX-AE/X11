//! Stable ownership registry for x86_64 task execution state.
//!
//! The registry owns one concrete execution binding per live task. Entries are
//! boxed so growing the registry cannot relocate a live kernel stack or saved
//! context. The registry itself remains single-CPU during bootstrap; SMP
//! synchronization belongs to a later per-CPU/locking layer.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::scheduler::{ExecutionBinding, ExecutionHandle, TaskId};

use super::context_switch::Context;
use super::execution::{ExecutionError, X86ExecutionBinding};
use super::interrupted_state::InterruptedState;
use super::interrupt_entry::{InterruptReturnFrame, SavedRegisters};
use super::preemption_plan::PreemptionPlan;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RegistryInsertError { AlreadyBound, Allocation }

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InterruptCaptureError { TaskNotFound, Execution(ExecutionError) }

#[derive(Debug, Default)]
pub struct ExecutionRegistry { entries: Vec<Option<Box<X86ExecutionBinding>>> }

impl ExecutionRegistry {
    pub const fn new() -> Self { Self { entries: Vec::new() } }

    pub fn insert(&mut self, handle: ExecutionHandle, entry: extern "C" fn() -> !) -> Result<(), RegistryInsertError> {
        let task_id = handle.task_id();
        if self.get(task_id).is_some() { return Err(RegistryInsertError::AlreadyBound); }
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
        let Some(slot) = self.entries.iter_mut().find(|slot| {
            slot.as_deref().is_some_and(|binding| binding.task_id() == task_id)
        }) else { return false; };
        *slot = None;
        true
    }

    pub fn get(&self, task_id: TaskId) -> Option<&X86ExecutionBinding> {
        self.entries.iter().filter_map(Option::as_deref).find(|binding| binding.task_id() == task_id)
    }

    pub fn get_mut(&mut self, task_id: TaskId) -> Option<&mut X86ExecutionBinding> {
        self.entries.iter_mut().filter_map(Option::as_deref_mut).find(|binding| binding.task_id() == task_id)
    }

    pub fn kernel_stack_top(&self, task_id: TaskId) -> Option<u64> {
        self.get(task_id)?.stack_top()
    }

    pub unsafe fn capture_interrupted(
        &mut self,
        task_id: TaskId,
        registers: *const SavedRegisters,
        return_frame: InterruptReturnFrame,
        resume_rsp: u64,
    ) -> Result<InterruptedState, InterruptCaptureError> {
        let binding = self.get_mut(task_id).ok_or(InterruptCaptureError::TaskNotFound)?;
        unsafe { binding.capture_interrupted(registers, return_frame, resume_rsp) }
            .map_err(InterruptCaptureError::Execution)?;
        binding.interrupted().ok_or(InterruptCaptureError::Execution(ExecutionError::InvalidStack))
    }

    /// Peeks at the architecture-level return path without changing ownership.
    pub fn preemption_plan(&self, task_id: TaskId) -> Option<PreemptionPlan> {
        let binding = self.get(task_id)?;
        if let Some(state) = binding.interrupted() {
            if state.return_state().kernel_iret_words().is_some() {
                return Some(PreemptionPlan::IretKernel { task_id, state });
            }
        }

        let context = *binding.context();
        context.is_initialized().then_some(PreemptionPlan::ReturnToContext { task_id, context })
    }

    /// Consumes a previously interrupted kernel snapshot when the scheduler has
    /// committed the task as the next CPU owner. Validation happens before the
    /// destructive `take`, so a malformed/non-kernel snapshot cannot be lost.
    pub fn take_kernel_preempt_state(&mut self, task_id: TaskId) -> Option<InterruptedState> {
        let binding = self.get_mut(task_id)?;
        let state = binding.interrupted()?;
        if state.return_state().kernel_iret_words().is_none() {
            return None;
        }
        binding.take_interrupted()
    }

    pub fn context_pair_mut(&mut self, current: TaskId, next: TaskId) -> Option<(&mut Context, &Context)> {
        if current == next { return None; }
        let current_index = self.index_of(current)?;
        let next_index = self.index_of(next)?;
        if current_index < next_index {
            let (left, right) = self.entries.split_at_mut(next_index);
            let current_binding = left[current_index].as_deref_mut()?;
            let next_binding = right[0].as_deref()?;
            Some((current_binding.context_mut(), next_binding.context()))
        } else {
            let (left, right) = self.entries.split_at_mut(current_index);
            let next_binding = left[next_index].as_deref()?;
            let current_binding = right[0].as_deref_mut()?;
            Some((current_binding.context_mut(), next_binding.context()))
        }
    }

    pub fn contains(&self, handle: ExecutionHandle) -> bool { self.get(handle.task_id()).is_some() }
    pub fn count(&self) -> usize { self.entries.iter().filter(|entry| entry.is_some()).count() }
    pub fn is_valid(&self, handle: ExecutionHandle) -> bool {
        self.get(handle.task_id()).map(|binding| binding.validate().is_ok()).unwrap_or(false)
    }

    fn index_of(&self, task_id: TaskId) -> Option<usize> {
        self.entries.iter().position(|entry| entry.as_deref().is_some_and(|binding| binding.task_id() == task_id))
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionRegistry, RegistryInsertError};
    use crate::arch::x86_64::interrupted_state::InterruptedState;
    use crate::arch::x86_64::interrupt_entry::{InterruptReturnFrame, SavedRegisters};
    use crate::scheduler::{ExecutionHandle, TaskId};

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
    fn registry_owns_one_execution_binding_per_handle() {
        let mut registry = ExecutionRegistry::new();
        let handle = ExecutionHandle::for_task(TaskId::new(1, 1));
        registry.insert(handle, never_returns).unwrap();
        assert_eq!(registry.count(), 1);
        assert!(registry.contains(handle));
        assert!(registry.is_valid(handle));
        assert!(registry.kernel_stack_top(handle.task_id()).is_some());
    }

    #[test]
    fn duplicate_handle_is_rejected() {
        let mut registry = ExecutionRegistry::new();
        let handle = ExecutionHandle::for_task(TaskId::new(1, 1));
        registry.insert(handle, never_returns).unwrap();
        assert_eq!(registry.insert(handle, never_returns), Err(RegistryInsertError::AlreadyBound));
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
            registry.insert(ExecutionHandle::for_task(TaskId::new(index, 1)), never_returns).unwrap();
        }
        let after = registry.get(first.task_id()).unwrap() as *const _;
        assert_eq!(before, after);
    }

    #[test]
    fn context_pair_rejects_same_task() {
        let mut registry = ExecutionRegistry::new();
        let handle = ExecutionHandle::for_task(TaskId::new(5, 1));
        registry.insert(handle, never_returns).unwrap();
        assert!(registry.context_pair_mut(handle.task_id(), handle.task_id()).is_none());
    }

    #[test]
    fn non_kernel_preempt_state_is_not_consumed() {
        let mut registry = ExecutionRegistry::new();
        let handle = ExecutionHandle::for_task(TaskId::new(6, 1));
        registry.insert(handle, never_returns).unwrap();
        let binding = registry.get_mut(handle.task_id()).unwrap();
        let registers = SavedRegisters::default();
        let mut raw = [0u64; 5];
        raw[0] = 0x1000;
        raw[1] = 0x1b;
        raw[2] = 0x202;
        raw[3] = 0x8000;
        raw[4] = 0x23;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        let snapshot = unsafe { InterruptedState::capture(&registers, frame) };
        binding.install_interrupted(snapshot).unwrap();
        assert!(registry.take_kernel_preempt_state(handle.task_id()).is_none());
        assert!(registry.get(handle.task_id()).unwrap().interrupted().is_some());
    }
}
