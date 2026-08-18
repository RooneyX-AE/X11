//! Single-CPU architecture-local handoff state.
//!
//! This is deliberately a small bootstrap abstraction. It will become a
//! per-CPU array when SMP is introduced, but the ownership contract remains:
//! interrupt entry records state here, while normal kernel context consumes it.

use core::cell::UnsafeCell;

use crate::scheduler::TaskId;

use super::interrupted_state::InterruptedState;
use super::interrupt_entry::{InterruptReturnFrame, SavedRegisters};

#[derive(Debug)]
pub struct CpuLocalState {
    current_task: UnsafeCell<Option<TaskId>>,
    runtime: UnsafeCell<Option<usize>>,
    interrupted: UnsafeCell<Option<(TaskId, InterruptedState)>>,
}

unsafe impl Sync for CpuLocalState {}

impl CpuLocalState {
    pub const fn new() -> Self {
        Self {
            current_task: UnsafeCell::new(None),
            runtime: UnsafeCell::new(None),
            interrupted: UnsafeCell::new(None),
        }
    }

    pub unsafe fn set_current_task(&self, task: Option<TaskId>) {
        unsafe { *self.current_task.get() = task };
    }

    pub fn current_task(&self) -> Option<TaskId> {
        unsafe { *self.current_task.get() }
    }

    /// Binds the single runtime instance owned by this CPU during bootstrap.
    ///
    /// # Safety
    /// `runtime` must remain allocated at a stable address for the lifetime of
    /// the CPU binding, and callers must serialize binding with interrupt and
    /// preemption activity.
    pub unsafe fn bind_runtime(&self, runtime: *mut ()) -> Result<(), RuntimeBindingError> {
        let slot = unsafe { &mut *self.runtime.get() };
        if slot.is_some() {
            return Err(RuntimeBindingError::AlreadyBound);
        }
        if runtime.is_null() {
            return Err(RuntimeBindingError::Null);
        }
        *slot = Some(runtime as usize);
        Ok(())
    }

    pub fn runtime_ptr(&self) -> Option<*mut ()> {
        unsafe { (*self.runtime.get()).map(|address| address as *mut ()) }
    }

    pub unsafe fn unbind_runtime(&self) {
        unsafe { *self.runtime.get() = None };
    }

    /// Captures the interrupted state for the current CPU task.
    ///
    /// # Safety
    /// `registers` and `return_frame` must point to the live CPU interrupt
    /// frame owned by the current interrupt entry. `resume_rsp` must be the
    /// task stack pointer captured immediately before interrupt entry. Only the
    /// owning CPU may call this method, and at most one snapshot may be pending.
    pub unsafe fn capture_interrupted(
        &self,
        registers: *const SavedRegisters,
        return_frame: InterruptReturnFrame,
        resume_rsp: u64,
    ) -> Result<TaskId, CaptureError> {
        let task = self.current_task().ok_or(CaptureError::NoCurrentTask)?;
        let slot = unsafe { &mut *self.interrupted.get() };
        if slot.is_some() {
            return Err(CaptureError::AlreadyPending);
        }
        let snapshot = unsafe { InterruptedState::capture(registers, return_frame, resume_rsp) };
        if !snapshot.is_valid() {
            return Err(CaptureError::InvalidState);
        }
        *slot = Some((task, snapshot));
        Ok(task)
    }

    pub fn take_interrupted(&self) -> Option<(TaskId, InterruptedState)> {
        unsafe { (*self.interrupted.get()).take() }
    }

    pub fn has_interrupted(&self) -> bool {
        unsafe { (*self.interrupted.get()).is_some() }
    }
}

static CPU_LOCAL: CpuLocalState = CpuLocalState::new();

pub fn local() -> &'static CpuLocalState {
    &CPU_LOCAL
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeBindingError {
    Null,
    AlreadyBound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureError {
    NoCurrentTask,
    AlreadyPending,
    InvalidState,
}

#[cfg(test)]
mod tests {
    use super::{CaptureError, CpuLocalState, RuntimeBindingError};
    use crate::arch::x86_64::interrupt_entry::{InterruptReturnFrame, SavedRegisters};
    use crate::scheduler::TaskId;

    #[test]
    fn cpu_local_current_task_round_trips() {
        let cpu = CpuLocalState::new();
        unsafe { cpu.set_current_task(Some(TaskId::new(7, 3))) };
        assert_eq!(cpu.current_task(), Some(TaskId::new(7, 3)));
        unsafe { cpu.set_current_task(None) };
        assert_eq!(cpu.current_task(), None);
    }

    #[test]
    fn runtime_binding_is_stable_and_single_owner() {
        let cpu = CpuLocalState::new();
        let mut marker = 0u8;
        let ptr = core::ptr::addr_of_mut!(marker).cast::<()>();
        unsafe { cpu.bind_runtime(ptr).unwrap() };
        assert_eq!(cpu.runtime_ptr(), Some(ptr));
        assert_eq!(unsafe { cpu.bind_runtime(ptr) }, Err(RuntimeBindingError::AlreadyBound));
        unsafe { cpu.unbind_runtime() };
        assert_eq!(cpu.runtime_ptr(), None);
    }

    #[test]
    fn capture_requires_current_task() {
        let cpu = CpuLocalState::new();
        let registers = SavedRegisters::default();
        let mut raw = [0u64; 3];
        raw[0] = 0x1000;
        raw[1] = 0x10;
        raw[2] = 0x202;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        assert_eq!(
            unsafe { cpu.capture_interrupted(&registers, frame, 0x8000) },
            Err(CaptureError::NoCurrentTask)
        );
    }
}
