//! x86_64 kernel-task bootstrap coordinator.
//!
//! This layer is intentionally voluntary/non-preemptive. It creates the
//! generic task first, binds an execution handle, installs the concrete x86
//! execution state, and only then exposes the task to the ready queue.

use crate::scheduler::{
    DispatchDecision, Priority, Scheduler, SchedulerError, SleepQueue, TaskId,
};

use super::dispatch::{self, DispatchError};
use super::execution_registry::{ExecutionRegistry, RegistryInsertError};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KernelTaskError {
    Scheduler(SchedulerError),
    Registry(RegistryInsertError),
    Dispatch(DispatchError),
    DispatchMismatch,
    KernelStackUnavailable,
}

impl From<SchedulerError> for KernelTaskError {
    fn from(error: SchedulerError) -> Self {
        Self::Scheduler(error)
    }
}

impl From<RegistryInsertError> for KernelTaskError {
    fn from(error: RegistryInsertError) -> Self {
        Self::Registry(error)
    }
}

impl From<DispatchError> for KernelTaskError {
    fn from(error: DispatchError) -> Self {
        Self::Dispatch(error)
    }
}

#[derive(Debug, Default)]
pub struct KernelTaskManager {
    pub scheduler: Scheduler,
    pub executions: ExecutionRegistry,
    pub sleepers: SleepQueue,
}

impl KernelTaskManager {
    pub const fn new() -> Self {
        Self {
            scheduler: Scheduler::new(),
            executions: ExecutionRegistry::new(),
            sleepers: SleepQueue::new(),
        }
    }

    pub fn spawn(
        &mut self,
        priority: Priority,
        entry: extern "C" fn() -> !,
    ) -> Result<TaskId, KernelTaskError> {
        let task_id = self.scheduler.create_task(priority);
        let handle = match self.scheduler.attach_execution(task_id) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = self.scheduler.destroy_created(task_id);
                return Err(KernelTaskError::Scheduler(error));
            }
        };

        if let Err(error) = self.executions.insert(handle, entry) {
            let _ = self.scheduler.destroy_created(task_id);
            return Err(KernelTaskError::Registry(error));
        }

        if !self.scheduler.make_ready(task_id) {
            let _ = self.executions.remove(handle);
            let _ = self.scheduler.destroy_created(task_id);
            return Err(KernelTaskError::Scheduler(SchedulerError::TaskNotCreated));
        }

        Ok(task_id)
    }

    pub fn prepare_dispatch(&mut self) -> Result<DispatchDecision, KernelTaskError> {
        let previous = self.scheduler.current();
        let Some(candidate) = self.scheduler.next_ready() else {
            return Ok(DispatchDecision { previous, next: None });
        };

        if Some(candidate) == previous {
            return Ok(DispatchDecision { previous, next: None });
        }

        dispatch::validate_transition(&self.executions, previous, candidate)?;
        let decision = self.scheduler.schedule_next();
        if decision.next != Some(candidate) {
            return Err(KernelTaskError::DispatchMismatch);
        }

        let stack_top = self.executions.kernel_stack_top(candidate)
            .ok_or(KernelTaskError::KernelStackUnavailable)?;
        // SAFETY: dispatch is a single-CPU kernel operation; interrupts/preemption
        // are excluded by the caller while the current task's TSS entry stack is
        // switched to the candidate's live kernel stack.
        unsafe { crate::arch::x86_64::gdt::set_kernel_stack_top(stack_top); }
        Ok(decision)
    }

    pub fn sleep_current_until(&mut self, deadline: u64) -> Result<TaskId, KernelTaskError> {
        self.scheduler
            .sleep_current_until(deadline, &mut self.sleepers)
            .map_err(KernelTaskError::Scheduler)
    }

    /// Expires blocked sleep deadlines without selecting or switching a task.
    /// The caller may perform scheduler dispatch after returning to normal
    /// kernel context.
    pub fn expire_sleepers(&mut self, now: u64) -> usize {
        self.scheduler.expire_sleepers(now, &mut self.sleepers).len()
    }

    pub fn next_sleep_deadline(&self) -> Option<u64> {
        self.sleepers.next_deadline()
    }

    pub fn is_executable(&self, task_id: TaskId) -> bool {
        let Some(handle) = self.scheduler.execution(task_id) else {
            return false;
        };
        self.executions.is_valid(handle)
    }

    pub fn exit_current(&mut self) -> bool {
        let Some(task_id) = self.scheduler.current() else {
            return false;
        };
        let Some(handle) = self.scheduler.execution(task_id) else {
            return false;
        };
        self.sleepers.remove(task_id);
        if !self.scheduler.exit_current() {
            return false;
        }
        self.executions.remove(handle)
    }

    pub fn task_count(&self) -> usize {
        self.scheduler.task_count()
    }
}

#[cfg(test)]
mod tests {
    use super::{KernelTaskError, KernelTaskManager};
    use crate::scheduler::{DispatchDecision, Priority, TaskState};

    extern "C" fn never_returns() -> ! {
        loop {}
    }

    #[test]
    fn spawn_installs_execution_before_ready_state() {
        let mut manager = KernelTaskManager::new();
        let task = manager.spawn(Priority::DEFAULT, never_returns).unwrap();
        assert!(manager.is_executable(task));
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Ready));
        assert_eq!(manager.task_count(), 1);
    }

    #[test]
    fn prepare_dispatch_validates_registry_backed_execution() {
        let mut manager = KernelTaskManager::new();
        let task = manager.spawn(Priority::DEFAULT, never_returns).unwrap();
        let decision = manager.prepare_dispatch().unwrap();
        assert_eq!(decision.previous, None);
        assert_eq!(decision.next, Some(task));
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Running));
        assert!(manager.executions.kernel_stack_top(task).is_some());
    }

    #[test]
    fn prepare_dispatch_returns_noop_without_another_ready_task() {
        let mut manager = KernelTaskManager::new();
        let task = manager.spawn(Priority::DEFAULT, never_returns).unwrap();
        assert_eq!(manager.prepare_dispatch().unwrap(), DispatchDecision { previous: None, next: Some(task) });
        assert_eq!(manager.prepare_dispatch().unwrap(), DispatchDecision { previous: Some(task), next: None });
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Running));
    }

    #[test]
    fn dispatch_mismatch_is_a_runtime_error_contract() {
        assert_eq!(KernelTaskError::DispatchMismatch, KernelTaskError::DispatchMismatch);
    }

    #[test]
    fn exit_current_releases_execution_registry_entry() {
        let mut manager = KernelTaskManager::new();
        let task = manager.spawn(Priority::DEFAULT, never_returns).unwrap();
        let handle = manager.scheduler.execution(task).unwrap();
        assert!(manager.prepare_dispatch().is_ok());
        assert!(manager.exit_current());
        assert!(!manager.executions.contains(handle));
        assert_eq!(manager.scheduler.state(task), None);
    }

    #[test]
    fn sleeping_task_expires_without_switching_in_timer_service() {
        let mut manager = KernelTaskManager::new();
        let task = manager.spawn(Priority::DEFAULT, never_returns).unwrap();
        assert!(manager.prepare_dispatch().is_ok());
        assert_eq!(manager.sleep_current_until(100).unwrap(), task);
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Blocked));
        assert_eq!(manager.scheduler.current(), None);
        assert_eq!(manager.next_sleep_deadline(), Some(100));
        assert_eq!(manager.expire_sleepers(99), 0);
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Blocked));
        assert_eq!(manager.expire_sleepers(100), 1);
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Ready));
        assert_eq!(manager.scheduler.current(), None);
    }

    #[test]
    fn sleep_deadline_remains_owned_while_task_is_blocked() {
        let mut manager = KernelTaskManager::new();
        let task = manager.spawn(Priority::DEFAULT, never_returns).unwrap();
        assert!(manager.prepare_dispatch().is_ok());
        assert_eq!(manager.sleep_current_until(100).unwrap(), task);
        assert_eq!(manager.next_sleep_deadline(), Some(100));
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Blocked));
    }
}
