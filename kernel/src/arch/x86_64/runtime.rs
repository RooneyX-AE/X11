//! Single-CPU kernel runtime owner.
//!
//! The runtime owns the scheduler, execution registry, sleep queue, and boot
//! continuation. Tasks never access these components through global mutable
//! state. The runtime is boxed so its address remains stable while a spawned
//! task is executing.

use alloc::boxed::Box;

use crate::scheduler::{PreemptionGate, Priority, RescheduleRequest, TaskId};

use super::context_switch::Context;
use super::cpu_local::{self, RuntimeBindingError};
use super::execution::ExecutionError;
use super::kernel_task::{KernelTaskError, KernelTaskManager};
use super::yield_switch::{self, YieldError};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RuntimeError {
    Task(KernelTaskError),
    NoRunnableTask,
    MissingExecutionPair,
    Yield(YieldError),
    PreemptionDisabled,
    InterruptedState(ExecutionError),
    CpuBinding(RuntimeBindingError),
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

    /// Binds this stable runtime object to the current CPU. Bootstrap currently
    /// has one CPU; SMP will replace this with a per-CPU runtime owner table.
    pub unsafe fn bind_cpu(&mut self) -> Result<(), RuntimeError> {
        cpu_local::local()
            .bind_runtime(self as *mut Self as *mut ())
            .map_err(RuntimeError::CpuBinding)
    }

    pub fn request_reschedule(&self) { self.reschedule.request(); }
    pub fn is_reschedule_pending(&self) -> bool { self.reschedule.is_pending() }

    pub fn preemption_disable(&mut self) -> PreemptionDisableGuard<'_> {
        PreemptionDisableGuard { inner: Some(self.preemption.disable()) }
    }

    pub fn safe_reschedule_point(&mut self) -> Result<bool, RuntimeError> {
        if !self.preemption.is_enabled() { return Err(RuntimeError::PreemptionDisabled); }
        if !self.reschedule.take() { return Ok(false); }
        self.commit_interrupted_state()?;
        self.dispatch_once()?;
        Ok(true)
    }

    fn commit_interrupted_state(&mut self) -> Result<(), RuntimeError> {
        let Some((task_id, snapshot)) = cpu_local::local().take_interrupted() else {
            return Ok(());
        };

        let binding = self
            .manager
            .executions
            .get_mut(task_id)
            .ok_or(RuntimeError::MissingExecutionPair)?;
        binding.install_interrupted(snapshot).map_err(RuntimeError::InterruptedState)
    }

    pub fn spawn(&mut self, priority: Priority, entry: extern "C" fn() -> !) -> Result<TaskId, RuntimeError> {
        self.manager.spawn(priority, entry).map_err(RuntimeError::Task)
    }

    pub fn prepare_run(&mut self) -> Result<TaskId, RuntimeError> {
        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        decision.next.ok_or(RuntimeError::NoRunnableTask)
    }

    pub fn service_timer(&mut self, now: u64) -> Result<usize, RuntimeError> {
        let woken = self.manager.expire_sleepers(now);
        self.request_reschedule();
        Ok(woken)
    }

    pub fn service_pending_timer(&mut self, now: u64) -> Result<usize, RuntimeError> {
        if !super::idt::take_timer_pending() { return Ok(0); }
        self.service_timer(now)
    }

    /// Performs one scheduler dispatch and synchronizes the CPU-local current
    /// task identity on both sides of the context-switch continuation.
    ///
    /// # Safety
    /// The caller must exclude interrupt/preemption races around the switch
    /// boundary and the task execution bindings must have valid contexts.
    pub unsafe fn dispatch_once(&mut self) -> Result<(), RuntimeError> {
        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        let next = decision.next.ok_or(RuntimeError::NoRunnableTask)?;

        match decision.previous {
            None => {
                let next_context = self
                    .manager
                    .executions
                    .get(next)
                    .ok_or(RuntimeError::MissingExecutionPair)?
                    .context();

                unsafe { cpu_local::local().set_current_task(Some(next)); }
                let result = unsafe {
                    yield_switch::activate_first(
                        &mut self.boot_context as *mut Context,
                        next_context as *const Context,
                    )
                };
                unsafe { cpu_local::local().set_current_task(None); }
                result.map_err(RuntimeError::Yield)?;
            }
            Some(previous) => {
                let (current, next_context) = self
                    .manager
                    .executions
                    .context_pair_mut(previous, next)
                    .ok_or(RuntimeError::MissingExecutionPair)?;

                unsafe { cpu_local::local().set_current_task(Some(next)); }
                let result = unsafe {
                    yield_switch::switch(current as *mut Context, next_context as *const Context)
                };
                unsafe { cpu_local::local().set_current_task(Some(previous)); }
                result.map_err(RuntimeError::Yield)?;
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
    use crate::arch::x86_64::cpu_local::CpuLocalState;
    use crate::scheduler::{Priority, TaskState};

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

    #[test]
    fn timer_service_requests_reschedule_after_each_tick() {
        let mut runtime = KernelRuntime::new();
        let task = runtime.spawn(Priority::DEFAULT, never_returns).unwrap();
        assert!(!runtime.is_reschedule_pending());
        assert_eq!(runtime.service_timer(100).unwrap(), 0);
        assert!(runtime.is_reschedule_pending());
        assert_eq!(runtime.manager().scheduler.state(task), Some(TaskState::Ready));
    }

    #[test]
    fn cpu_local_identity_is_clear_before_dispatch() {
        let cpu = CpuLocalState::new();
        unsafe { cpu.set_current_task(None); }
        assert_eq!(cpu.current_task(), None);
    }
}
