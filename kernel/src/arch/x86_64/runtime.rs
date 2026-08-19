//! Single-CPU kernel runtime owner.

use alloc::boxed::Box;

use crate::scheduler::{PreemptionGate, Priority, RescheduleRequest, TaskId, TaskKind};

use super::context_switch::Context;
use super::cpu_local::{self, RuntimeBindingError};
use super::execution::ExecutionError;
use super::interrupted_state::InterruptedState;
use super::kernel_task::{KernelTaskError, KernelTaskManager};
use super::preemption_plan::PreemptionPlan;
use super::yield_switch::{self, YieldError};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RuntimeError {
    Task(KernelTaskError),
    NoRunnableTask,
    MissingExecutionPair,
    UserspaceTaskRequiresSystemRuntime,
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
    ReturnToKernel(InterruptedState),
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
        let result = unsafe { cpu_local::local().bind_runtime(self as *mut Self as *mut ()) };
        result.map_err(RuntimeError::CpuBinding)
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
        let Some(task_id) = cpu_local::local().current_task() else {
            return if cpu_local::local().has_interrupted() {
                Err(RuntimeError::MissingExecutionPair)
            } else {
                Ok(())
            };
        };

        let Some(binding) = self.manager.executions.get_mut(task_id) else {
            return Err(RuntimeError::MissingExecutionPair);
        };

        if binding.interrupted().is_some() {
            return Err(RuntimeError::InterruptedState(ExecutionError::InterruptedStateAlreadyPresent));
        }

        let Some((captured_task, snapshot)) = cpu_local::local().take_interrupted() else {
            return Ok(());
        };
        debug_assert_eq!(captured_task, task_id);

        binding.install_interrupted(snapshot).map_err(RuntimeError::InterruptedState)
    }

    fn discard_interrupted_state(&self) { let _ = cpu_local::local().take_interrupted(); }

    pub fn spawn(&mut self, priority: Priority, entry: extern "C" fn() -> !) -> Result<TaskId, RuntimeError> {
        self.manager.spawn(priority, entry).map_err(RuntimeError::Task)
    }

    pub fn prepare_run(&mut self) -> Result<TaskId, RuntimeError> {
        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        let next = decision.next.ok_or(RuntimeError::NoRunnableTask)?;
        if self.manager.scheduler.task_kind(next) == Some(TaskKind::Userspace) {
            return Err(RuntimeError::UserspaceTaskRequiresSystemRuntime);
        }
        Ok(next)
    }

    pub fn prepare_preemption(&self) -> Result<PreemptionPlan, RuntimeError> {
        let next = self.manager.scheduler.next_ready().ok_or(RuntimeError::NoRunnableTask)?;
        if self.manager.scheduler.task_kind(next) == Some(TaskKind::Userspace) {
            return Err(RuntimeError::UserspaceTaskRequiresSystemRuntime);
        }
        self.manager.executions.preemption_plan(next).ok_or(RuntimeError::MissingExecutionPair)
    }

    pub unsafe fn handle_timer_preemption(&mut self) -> Result<InterruptPreemption, RuntimeError> {
        let _ = super::idt::take_timer_pending();

        if !self.preemption.is_enabled() {
            self.discard_interrupted_state();
            self.request_reschedule();
            return Ok(InterruptPreemption::ResumeCurrent);
        }

        let now = super::idt::timer_ticks();
        let _woken = self.manager.expire_sleepers(now);

        let current = self.manager.scheduler.current();
        let Some(candidate) = self.manager.scheduler.next_ready() else {
            self.discard_interrupted_state();
            self.request_reschedule();
            return Ok(InterruptPreemption::ResumeCurrent);
        };
        if self.manager.scheduler.task_kind(candidate) == Some(TaskKind::Userspace) {
            self.request_reschedule();
            return Err(RuntimeError::UserspaceTaskRequiresSystemRuntime);
        }
        if Some(candidate) == current {
            self.discard_interrupted_state();
            return Ok(InterruptPreemption::ResumeCurrent);
        }

        let plan = match self.manager.executions.preemption_plan(candidate) {
            Some(plan) => plan,
            None => {
                self.discard_interrupted_state();
                self.request_reschedule();
                return Err(RuntimeError::MissingExecutionPair);
            }
        };

        if let Err(error) = self.commit_interrupted_state() {
            self.request_reschedule();
            return Err(error);
        }

        let decision = match self.manager.prepare_dispatch() {
            Ok(decision) if decision.next == Some(candidate) => decision,
            Ok(_) => {
                self.request_reschedule();
                return Err(RuntimeError::MissingExecutionPair);
            }
            Err(error) => {
                self.request_reschedule();
                return Err(RuntimeError::Task(error));
            }
        };
        let _ = decision;

        match plan {
            PreemptionPlan::ReturnToContext { context, .. } => {
                unsafe { cpu_local::local().set_current_task(Some(candidate)); }
                let _ = self.reschedule.take();
                Ok(InterruptPreemption::ReturnToContext(context))
            }
            PreemptionPlan::IretKernel { .. } => {
                let state = match self.manager.executions.take_kernel_preempt_state(candidate) {
                    Some(state) => state,
                    None => {
                        self.request_reschedule();
                        return Err(RuntimeError::MissingExecutionPair);
                    }
                };
                unsafe { cpu_local::local().set_current_task(Some(candidate)); }
                let _ = self.reschedule.take();
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
            if self.manager.scheduler.task_kind(candidate) == Some(TaskKind::Userspace) {
                return Err(RuntimeError::UserspaceTaskRequiresSystemRuntime);
            }
            if matches!(self.manager.executions.preemption_plan(candidate), Some(PreemptionPlan::IretKernel { .. })) {
                return Err(RuntimeError::InterruptedTaskRequiresIret);
            }
        }

        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        let next = decision.next.ok_or(RuntimeError::NoRunnableTask)?;
        if self.manager.scheduler.task_kind(next) == Some(TaskKind::Userspace) {
            return Err(RuntimeError::UserspaceTaskRequiresSystemRuntime);
        }

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
