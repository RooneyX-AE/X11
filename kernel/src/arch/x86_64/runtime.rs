//! Single-CPU kernel runtime owner.
//!
//! The runtime owns the scheduler, execution registry, sleep queue, and boot
//! continuation. Tasks never access these components through global mutable
//! state. The runtime is boxed so its address remains stable while a spawned
//! task is executing.

use alloc::boxed::Box;

use crate::scheduler::{PreemptionGate, Priority, RescheduleRequest, TaskId};

use super::context_switch::Context;
use super::kernel_task::{KernelTaskError, KernelTaskManager};
use super::yield_switch::{self, YieldError};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RuntimeError {
    Task(KernelTaskError),
    NoRunnableTask,
    MissingExecutionPair,
    Yield(YieldError),
    PreemptionDisabled,
}

impl From<YieldError> for RuntimeError {
    fn from(error: YieldError) -> Self { Self::Yield(error) }
}

#[derive(Debug)]
pub struct KernelRuntime {
    manager: KernelTaskManager,
    boot_context: Context,
    reschedule: RescheduleRequest,
    preemption: PreemptionGate,
}

impl KernelRuntime {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            manager: KernelTaskManager::new(),
            boot_context: Context::empty(),
            reschedule: RescheduleRequest::new(),
            preemption: PreemptionGate::new(),
        })
    }

    pub const fn manager(&self) -> &KernelTaskManager { &self.manager }
    pub const fn manager_mut(&mut self) -> &mut KernelTaskManager { &mut self.manager }
    pub const fn boot_context(&self) -> &Context { &self.boot_context }
    pub fn boot_context_mut(&mut self) -> &mut Context { &mut self.boot_context }
    pub fn address(&self) -> u64 { self as *const Self as usize as u64 }

    pub fn request_reschedule(&self) { self.reschedule.request(); }
    pub fn is_reschedule_pending(&self) -> bool { self.reschedule.is_pending() }

    pub fn preemption_disable(&mut self) -> PreemptionDisableGuard<'_> {
        PreemptionDisableGuard { inner: Some(self.preemption.disable()) }
    }

    pub fn safe_reschedule_point(&mut self) -> Result<bool, RuntimeError> {
        if !self.preemption.is_enabled() { return Err(RuntimeError::PreemptionDisabled); }
        if !self.reschedule.take() { return Ok(false); }
        self.dispatch_once()?;
        Ok(true)
    }

    pub fn spawn(&mut self, priority: Priority, entry: extern "C" fn() -> !) -> Result<TaskId, RuntimeError> {
        self.manager.spawn(priority, entry).map_err(RuntimeError::Task)
    }

    pub fn prepare_run(&mut self) -> Result<TaskId, RuntimeError> {
        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        decision.next.ok_or(RuntimeError::NoRunnableTask)
    }

    pub fn service_timer(&mut self, now: u64) -> Result<usize, RuntimeError> {
        Ok(self.manager.expire_sleepers(now))
    }

    pub fn service_pending_timer(&mut self, now: u64) -> Result<usize, RuntimeError> {
        if !super::idt::take_timer_pending() { return Ok(0); }
        self.service_timer(now)
    }

    pub unsafe fn dispatch_once(&mut self) -> Result<(), RuntimeError> {
        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        let next = decision.next.ok_or(RuntimeError::NoRunnableTask)?;
        match decision.previous {
            None => {
                let next_context = self.manager.executions.get(next).ok_or(RuntimeError::MissingExecutionPair)?.context();
                unsafe { yield_switch::activate_first(&mut self.boot_context as *mut Context, next_context as *const Context)?; }
            }
            Some(previous) => {
                let (current, next_context) = self.manager.executions.context_pair_mut(previous, next).ok_or(RuntimeError::MissingExecutionPair)?;
                unsafe { yield_switch::switch(current as *mut Context, next_context as *const Context)?; }
            }
        }
        Ok(())
    }

    pub fn execution_ready(&self, task_id: TaskId) -> bool { self.manager.is_executable(task_id) }
}

pub struct PreemptionDisableGuard<'a> {
    inner: Option<crate::scheduler::DisableGuard<'a>>,
}

impl Drop for PreemptionDisableGuard<'_> {
    fn drop(&mut self) { let _ = self.inner.take(); }
}

#[cfg(test)]
mod tests {
    use super::{KernelRuntime, RuntimeError};
    use crate::scheduler::Priority;

    extern "C" fn never_returns() -> ! { loop {} }

    #[test]
    fn pending_reschedule_waits_for_enabled_safe_point() {
        let mut runtime = KernelRuntime::new();
        runtime.request_reschedule();
        {
            let _guard = runtime.preemption_disable();
            assert_eq!(runtime.safe_reschedule_point(), Err(RuntimeError::PreemptionDisabled));
            assert!(runtime.is_reschedule_pending());
        }
        assert!(runtime.is_reschedule_pending());
        let _ = runtime.spawn(Priority::DEFAULT, never_returns);
        assert!(runtime.is_reschedule_pending());
    }
}
