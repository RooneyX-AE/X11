//! x86_64 kernel-task bootstrap coordinator.
//!
//! This layer is intentionally voluntary/non-preemptive. It creates the
//! generic task first, binds an execution handle, installs the concrete x86
//! execution state, and only then exposes the task to the ready queue.

use crate::scheduler::{DispatchDecision, Priority, Scheduler, SchedulerError, TaskId};

use super::dispatch::{self, DispatchError};
use super::execution_registry::{ExecutionRegistry, RegistryInsertError};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KernelTaskError {
    Scheduler(SchedulerError),
    Registry(RegistryInsertError),
    Dispatch(DispatchError),
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
}

impl KernelTaskManager {
    pub const fn new() -> Self {
        Self {
            scheduler: Scheduler::new(),
            executions: ExecutionRegistry::new(),
        }
    }

    /// Create an executable kernel task and make it ready only after its
    /// concrete x86 execution state has been installed successfully.
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

    /// Select the next task and validate that its concrete execution state is
    /// present before any architecture-specific switch is attempted.
    pub fn prepare_dispatch(&mut self) -> Result<DispatchDecision, KernelTaskError> {
        let decision = self.scheduler.schedule_next();
        let Some(next) = decision.next else {
            return Ok(decision);
        };

        dispatch::validate_transition(&self.executions, decision.previous, next)?;
        Ok(decision)
    }

    pub fn is_executable(&self, task_id: TaskId) -> bool {
        let Some(handle) = self.scheduler.execution(task_id) else {
            return false;
        };
        self.executions.is_valid(handle)
    }

    pub fn task_count(&self) -> usize {
        self.scheduler.task_count()
    }
}

#[cfg(test)]
mod tests {
    use super::KernelTaskManager;
    use crate::scheduler::{Priority, TaskState};

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
    }

    #[test]
    fn scheduler_can_dispatch_bootstrapped_kernel_task() {
        let mut manager = KernelTaskManager::new();
        let task = manager.spawn(Priority::DEFAULT, never_returns).unwrap();
        let decision = manager.scheduler.schedule_next();
        assert_eq!(decision.next, Some(task));
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Running));
    }
}
