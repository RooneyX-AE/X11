//! Process-to-scheduler construction boundary.
//!
//! This module creates the scheduler task that represents a process without
//! moving scheduler policy into the process manager. Every intermediate step
//! is explicit so a failed bind cannot leave an orphan task behind.

use crate::scheduler::{Priority, Scheduler, SchedulerError, TaskId};

use super::{AddressSpaceId, ProcessId, ProcessManager, ProcessManagerError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessSchedulerBindError {
    Process(ProcessManagerError),
    Scheduler(SchedulerError),
    CleanupFailed,
}

/// Creates and queues the first scheduler task for a process.
///
/// The process remains `Ready` in the process registry; the scheduler owns the
/// runnable state of the corresponding task. No CPU execution transition is
/// performed here.
pub fn bind_and_ready(
    manager: &mut ProcessManager,
    scheduler: &mut Scheduler,
    process: ProcessId,
    priority: Priority,
) -> Result<TaskId, ProcessSchedulerBindError> {
    let image = manager.image(process).map_err(ProcessSchedulerBindError::Process)?;
    let address_space: AddressSpaceId = image.address_space().id();

    let task = scheduler.create_task(priority);
    let execution = match scheduler.attach_execution(task) {
        Ok(handle) => handle,
        Err(error) => {
            cleanup_created_task(scheduler, task)?;
            return Err(ProcessSchedulerBindError::Scheduler(error));
        }
    };

    if let Err(error) = manager.attach_execution(process, task, execution, address_space) {
        cleanup_created_task(scheduler, task)?;
        return Err(ProcessSchedulerBindError::Process(error));
    }

    if !scheduler.make_ready(task) {
        let _ = manager.exit(process);
        cleanup_created_task(scheduler, task)?;
        return Err(ProcessSchedulerBindError::CleanupFailed);
    }

    Ok(task)
}

fn cleanup_created_task(scheduler: &mut Scheduler, task: TaskId) -> Result<(), ProcessSchedulerBindError> {
    scheduler
        .destroy_created(task)
        .map(|_| ())
        .map_err(|_| ProcessSchedulerBindError::CleanupFailed)
}

#[cfg(test)]
mod tests {
    use super::bind_and_ready;
    use crate::memory::AddressSpaceId;
    use crate::process::{AddressSpaceSpec, ElfImage, LoadPlan, ProcessImage, ProcessManager, UserStackPlan};
    use crate::scheduler::{Priority, Scheduler, TaskState};

    fn image(address_space: AddressSpaceId) -> ProcessImage {
        let mut bytes = [0u8; 120];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        let p = 64usize;
        bytes[p..p + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[p + 4..p + 8].copy_from_slice(&5u32.to_le_bytes());
        bytes[p + 16..p + 24].copy_from_slice(&0x401000u64.to_le_bytes());
        bytes[p + 32..p + 40].copy_from_slice(&16u64.to_le_bytes());
        bytes[p + 40..p + 48].copy_from_slice(&0x1000u64.to_le_bytes());
        let parsed = ElfImage::parse(&bytes).unwrap();
        let spec = AddressSpaceSpec::new(address_space);
        let plan = LoadPlan::build(spec, parsed).unwrap();
        ProcessImage::build(spec, plan, UserStackPlan::build().unwrap()).unwrap()
    }

    #[test]
    fn binds_process_to_created_ready_task() {
        let address_space = AddressSpaceId::new(7).unwrap();
        let mut manager = ProcessManager::new();
        let process = manager.register_ready(image(address_space)).unwrap();
        let mut scheduler = Scheduler::new();

        let task = bind_and_ready(&mut manager, &mut scheduler, process, Priority::DEFAULT).unwrap();
        assert_eq!(scheduler.state(task), Some(TaskState::Ready));
        assert_eq!(manager.binding(process).unwrap().unwrap().task(), task);
    }

    #[test]
    fn binding_failure_does_not_leave_task_ready() {
        let address_space = AddressSpaceId::new(7).unwrap();
        let wrong_process_space = AddressSpaceId::new(8).unwrap();
        let mut manager = ProcessManager::new();
        let process = manager.register_ready(image(address_space)).unwrap();
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        let execution = scheduler.attach_execution(task).unwrap();

        let _ = manager.attach_execution(process, task, execution, wrong_process_space);
        assert_eq!(scheduler.task_count(), 1);
        assert_eq!(scheduler.state(task), Some(TaskState::Created));
        assert!(scheduler.destroy_created(task).is_ok());
        assert_eq!(scheduler.task_count(), 0);
    }
}
