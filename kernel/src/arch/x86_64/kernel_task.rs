//! x86_64 kernel-task bootstrap coordinator.
//!
//! This layer is intentionally voluntary/non-preemptive. It creates the
//! generic task first, binds an execution handle, installs the concrete x86
//! execution state, and only then exposes the task to the ready queue.

use crate::scheduler::{Priority, Scheduler, SchedulerError, TaskId};

use super::execution::execution_registry::{ExecutionRegistry, RegistryInsertError};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KernelTaskError {
    Scheduler(SchedulerError),
    Registry(RegistryInsertError),
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
    fn scheduler_can_dispatch_bootstrapped_kernel_task() {
        let mut manager = KernelTaskManager::new();
        let task = manager.spawn(Priority::DEFAULT, never_returns).unwrap();
        let decision = manager.scheduler.schedule_next();
        assert_eq!(decision.next, Some(task));
        assert_eq!(manager.scheduler.state(task), Some(TaskState::Running));
    }
}
