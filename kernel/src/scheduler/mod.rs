//! Architecture-independent scheduler foundation.
//!
//! Task control blocks are individually heap allocated so growing the slot
//! table never relocates a live task object. Architecture-specific execution
//! state may therefore safely retain a stable pointer to its owning task.

mod execution;
mod run_queue;
mod task;

use alloc::boxed::Box;
use alloc::vec::Vec;

pub use execution::{ExecutionBinding, ExecutionHandle, ExecutionState};
pub use run_queue::RunQueue;
pub use task::{ExecutionAttachError, Priority, TaskControlBlock, TaskId, TaskState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchDecision {
    pub previous: Option<TaskId>,
    pub next: Option<TaskId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    TaskNotFound,
    ExecutionAlreadyAttached,
    TaskNotCreated,
}

#[derive(Debug, Default)]
pub struct Scheduler {
    tasks: Vec<Option<Box<TaskControlBlock>>>,
    generations: Vec<u32>,
    ready: RunQueue,
    current: Option<TaskId>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            tasks: Vec::new(),
            generations: Vec::new(),
            ready: RunQueue::new(),
            current: None,
        }
    }

    pub fn create_task(&mut self, priority: Priority) -> TaskId {
        if let Some((index, slot)) = self
            .tasks
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
        {
            let generation = self.generations[index].wrapping_add(1).max(1);
            self.generations[index] = generation;
            let id = TaskId::new(index as u32, generation);
            *slot = Some(Box::new(TaskControlBlock::new(id, priority)));
            return id;
        }

        let index = self.tasks.len();
        let generation = 1;
        let id = TaskId::new(index as u32, generation);
        self.tasks.push(Some(Box::new(TaskControlBlock::new(id, priority))));
        self.generations.push(generation);
        id
    }

    pub fn attach_execution(&mut self, id: TaskId) -> Result<ExecutionHandle, SchedulerError> {
        let task = self.task_mut(id).ok_or(SchedulerError::TaskNotFound)?;
        task.attach_execution().map_err(|error| match error {
            ExecutionAttachError::AlreadyAttached => SchedulerError::ExecutionAlreadyAttached,
        })
    }

    pub fn execution(&self, id: TaskId) -> Option<ExecutionHandle> {
        self.task(id).and_then(TaskControlBlock::execution)
    }

    /// Roll back a task that has not entered the scheduler lifecycle yet.
    pub fn destroy_created(&mut self, id: TaskId) -> Result<ExecutionHandle, SchedulerError> {
        let index = id.index() as usize;
        let Some(slot) = self.tasks.get_mut(index) else {
            return Err(SchedulerError::TaskNotFound);
        };
        let Some(task) = slot.as_mut() else {
            return Err(SchedulerError::TaskNotFound);
        };
        if task.id() != id {
            return Err(SchedulerError::TaskNotFound);
        }
        if task.state() != TaskState::Created {
            return Err(SchedulerError::TaskNotCreated);
        }
        let handle = task
            .detach_execution()
            .ok_or(SchedulerError::TaskNotCreated)?;
        *slot = None;
        Ok(handle)
    }

    pub fn make_ready(&mut self, id: TaskId) -> bool {
        let Some(task) = self.task_mut(id) else {
            return false;
        };
        if !task.transition(TaskState::Ready) {
            return false;
        }
        self.ready.push(id)
    }

    pub fn schedule_next(&mut self) -> DispatchDecision {
        let previous = self.current;

        if let Some(previous_id) = previous {
            if let Some(task) = self.task_mut(previous_id) {
                if task.state() == TaskState::Running {
                    let _ = task.transition(TaskState::Ready);
                    let _ = self.ready.push(previous_id);
                }
            }
        }

        let next = self.ready.pop();
        if let Some(next_id) = next {
            if let Some(task) = self.task_mut(next_id) {
                if task.transition(TaskState::Running) {
                    self.current = Some(next_id);
                } else {
                    self.current = None;
                }
            } else {
                self.current = None;
            }
        } else {
            self.current = None;
        }

        DispatchDecision {
            previous,
            next: self.current,
        }
    }

    pub fn block_current(&mut self) -> bool {
        let Some(id) = self.current else {
            return false;
        };
        let Some(task) = self.task_mut(id) else {
            return false;
        };
        let changed = task.transition(TaskState::Blocked);
        if changed {
            self.current = None;
        }
        changed
    }

    pub fn exit_current(&mut self) -> bool {
        let Some(id) = self.current else {
            return false;
        };
        let index = id.index() as usize;
        let Some(task) = self.tasks.get_mut(index).and_then(Option::as_mut) else {
            return false;
        };
        if task.id() != id || !task.transition(TaskState::Exited) {
            return false;
        }
        let _ = task.detach_execution();
        self.tasks[index] = None;
        self.current = None;
        true
    }

    pub fn current(&self) -> Option<TaskId> { self.current }

    pub fn state(&self, id: TaskId) -> Option<TaskState> {
        self.task(id).map(TaskControlBlock::state)
    }

    pub fn ready_len(&self) -> usize { self.ready.len() }
    pub fn task_count(&self) -> usize { self.tasks.iter().filter(|task| task.is_some()).count() }

    #[cfg(test)]
    fn task_ptr(&self, id: TaskId) -> Option<*const TaskControlBlock> {
        self.task(id).map(|task| task as *const TaskControlBlock)
    }

    fn task(&self, id: TaskId) -> Option<&TaskControlBlock> {
        let task = self.tasks.get(id.index() as usize)?.as_deref()?;
        (task.id() == id).then_some(task)
    }

    fn task_mut(&mut self, id: TaskId) -> Option<&mut TaskControlBlock> {
        let task = self.tasks.get_mut(id.index() as usize)?.as_deref_mut()?;
        if task.id() == id { Some(task) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::{Priority, Scheduler, SchedulerError, TaskState};

    #[test]
    fn scheduler_dispatches_ready_tasks_fifo() {
        let mut scheduler = Scheduler::new();
        let first = scheduler.create_task(Priority::DEFAULT);
        let second = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(first));
        assert!(scheduler.make_ready(second));
        assert_eq!(scheduler.schedule_next().next, Some(first));
        assert_eq!(scheduler.state(first), Some(TaskState::Running));
        assert_eq!(scheduler.schedule_next().next, Some(second));
        assert_eq!(scheduler.state(second), Some(TaskState::Running));
    }

    #[test]
    fn scheduler_tracks_execution_handle() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        let handle = scheduler.attach_execution(task).unwrap();
        assert_eq!(scheduler.execution(task), Some(handle));
        assert_eq!(scheduler.attach_execution(task), Err(SchedulerError::ExecutionAlreadyAttached));
    }

    #[test]
    fn scheduler_can_rollback_unstarted_task() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        let handle = scheduler.attach_execution(task).unwrap();
        assert_eq!(scheduler.destroy_created(task), Ok(handle));
        assert_eq!(scheduler.state(task), None);
        assert_eq!(scheduler.execution(task), None);
    }

    #[test]
    fn scheduler_rejects_rollback_after_task_enters_lifecycle() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        let _ = scheduler.attach_execution(task).unwrap();
        assert!(scheduler.make_ready(task));
        assert_eq!(scheduler.destroy_created(task), Err(SchedulerError::TaskNotCreated));
    }

    #[test]
    fn scheduler_rejects_execution_for_unknown_task() {
        let mut scheduler = Scheduler::new();
        assert_eq!(scheduler.attach_execution(super::TaskId::new(99, 1)), Err(SchedulerError::TaskNotFound));
    }

    #[test]
    fn stale_task_ids_are_rejected_after_slot_reuse() {
        let mut scheduler = Scheduler::new();
        let first = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(first));
        assert_eq!(scheduler.schedule_next().next, Some(first));
        assert!(scheduler.exit_current());
        let replacement = scheduler.create_task(Priority::DEFAULT);
        assert_ne!(first, replacement);
        assert_eq!(replacement.index(), first.index());
        assert_eq!(scheduler.state(first), None);
        assert_eq!(scheduler.execution(first), None);
    }

    #[test]
    fn task_address_is_stable_when_slot_table_grows() {
        let mut scheduler = Scheduler::new();
        let first = scheduler.create_task(Priority::DEFAULT);
        let before = scheduler.task_ptr(first).expect("task must exist");
        for _ in 0..128 { let _ = scheduler.create_task(Priority::DEFAULT); }
        let after = scheduler.task_ptr(first).expect("task must survive growth");
        assert_eq!(before, after);
    }
}
