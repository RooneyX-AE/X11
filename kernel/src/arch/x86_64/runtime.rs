//! Single-CPU kernel runtime owner.

use alloc::boxed::Box;

use crate::scheduler::{PreemptionGate, Priority, RescheduleRequest, TaskId};

use super::context_switch::Context;
use super::cpu_local::{self, RuntimeBindingError};
use super::execution::ExecutionError;
use super::interrupted_state::KernelPreemptState;
use super::kernel_task::{KernelTaskError, KernelTaskManager};
use super::preemption_plan::PreemptionPlan;
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
    InterruptedTaskRequiresIret,
}

impl From<YieldError> for RuntimeError {
    fn from(error: YieldError) -> Self { Self::Yield(error) }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptPreemption {
    ResumeCurrent,
    ReturnToContext(Context),
    ReturnToKernel(KernelPreemptState),
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

    pub unsafe fn bind_cpu(&mut self) -> Result<(), RuntimeError> {
        cpu_local::local().bind_runtime(self as *mut Self as *mut ()).map_err(RuntimeError::CpuBinding)
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
        match unsafe { self.dispatch_once() } {
            Ok(()) => Ok(true),
            Err(error @ RuntimeError::InterruptedTaskRequiresIret) => {
                self.request_reschedule();
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    fn commit_interrupted_state(&mut self) -> Result<(), RuntimeError> {
        let Some((task_id, snapshot)) = cpu_local::local().take_interrupted() else { return Ok(()); };
        let binding = self.manager.executions.get_mut(task_id).ok_or(RuntimeError::MissingExecutionPair)?;
        binding.install_interrupted(snapshot).map_err(RuntimeError::InterruptedState)
    }

    pub fn spawn(&mut self, priority: Priority, entry: extern "C" fn() -> !) -> Result<TaskId, RuntimeError> {
        self.manager.spawn(priority, entry).map_err(RuntimeError::Task)
    }

    pub fn prepare_run(&mut self) -> Result<TaskId, RuntimeError> {
        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        decision.next.ok_or(RuntimeError::NoRunnableTask)
    }

    pub fn prepare_preemption(&self) -> Result<PreemptionPlan, RuntimeError> {
        let next = self.manager.scheduler.next_ready().ok_or(RuntimeError::NoRunnableTask)?;
        self.manager.executions.preemption_plan(next).ok_or(RuntimeError::MissingExecutionPair)
    }

    /// Handles timer-triggered preemption at the interrupt-return boundary.
    /// The scheduler is mutated only after the target return path has been
    /// classified, preventing a task with no initialized execution state from
    /// being marked Running while the CPU is still executing the interrupted task.
    pub unsafe fn handle_timer_preemption(&mut self) -> Result<InterruptPreemption, RuntimeError> {
        self.commit_interrupted_state()?;
        self.request_reschedule();

        if !self.preemption.is_enabled() { return Ok(InterruptPreemption::ResumeCurrent); }
        let Some(candidate) = self.manager.scheduler.next_ready() else { return Ok(InterruptPreemption::ResumeCurrent); };
        let plan = self.manager.executions.preemption_plan(candidate).ok_or(RuntimeError::MissingExecutionPair)?;

        match plan {
            PreemptionPlan::ReturnToContext { context, .. } => {
                let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
                if decision.next != Some(candidate) { return Err(RuntimeError::MissingExecutionPair); }
                unsafe { cpu_local::local().set_current_task(Some(candidate)); }
                Ok(InterruptPreemption::ReturnToContext(context))
            }
            PreemptionPlan::IretKernel { .. } => {
                let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
                if decision.next != Some(candidate) { return Err(RuntimeError::MissingExecutionPair); }
                let state = self.manager.executions.take_kernel_preempt_state(candidate).ok_or(RuntimeError::MissingExecutionPair)?;
                unsafe { cpu_local::local().set_current_task(Some(candidate)); }
                Ok(InterruptPreemption::ReturnToKernel(state))
            }
        }
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

    pub unsafe fn dispatch_once(&mut self) -> Result<(), RuntimeError> {
        if let Some(candidate) = self.manager.scheduler.next_ready() {
            if matches!(self.manager.executions.preemption_plan(candidate), Some(PreemptionPlan::IretKernel { .. })) {
                return Err(RuntimeError::InterruptedTaskRequiresIret);
            }
        }

        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        let next = decision.next.ok_or(RuntimeError::NoRunnableTask)?;

        match decision.previous {
            None => {
                let next_context = self.manager.executions.get(next).ok_or(RuntimeError::MissingExecutionPair)?.context();
                unsafe { cpu_local::local().set_current_task(Some(next)); }
                let result = unsafe { yield_switch::activate_first(&mut self.boot_context as *mut Context, next_context as *const Context) };
                unsafe { cpu_local::local().set_current_task(None); }
                result.map_err(RuntimeError::Yield)?;
            }
            Some(previous) => {
                let (current, next_context) = self.manager.executions.context_pair_mut(previous, next).ok_or(RuntimeError::MissingExecutionPair)?;
                unsafe { cpu_local::local().set_current_task(Some(next)); }
                let result = unsafe { yield_switch::switch(current as *mut Context, next_context as *const Context) };
                unsafe { cpu_local::local().set_current_task(Some(previous)); }
                result.map_err(RuntimeError::Yield)?;
            }
        }
        Ok(())
    }

    pub fn execution_ready(&self, task_id: TaskId) -> bool { self.manager.is_executable(task_id) }
}

pub unsafe fn yield_current() -> Result<(), RuntimeError> {
    let Some(runtime) = cpu_local::local().runtime_ptr() else { return Err(RuntimeError::CpuBinding(RuntimeBindingError::Null)); };
    let runtime = unsafe { &mut *(runtime as *mut KernelRuntime) };
    runtime.request_reschedule();
    unsafe { runtime.dispatch_once() }
}

pub struct PreemptionDisableGuard<'a> { inner: Option<crate::scheduler::DisableGuard<'a>> }
impl Drop for PreemptionDisableGuard<'_> { fn drop(&mut self) { let _ = self.inner.take(); } }
