//! Architecture-independent scheduler foundation.
//!
//! Task control blocks are individually heap allocated so growing the slot
//! table never relocates a live task object. Architecture-specific execution
//! state may therefore safely retain a stable pointer to its owning task.

mod execution;
mod run_queue;
mod task;
mod wait_queue;

use alloc::boxed::Box;
use alloc::vec::Vec;

pub use execution::{ExecutionBinding, ExecutionHandle, ExecutionState};
pub use run_queue::RunQueue;
pub use task::{ExecutionAttachError, Priority, TaskControlBlock, TaskId, TaskState};
pub use wait_queue::{WaitQueue, WaitQueueError};

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
    WaitQueueAlreadyContainsTask,
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
        Self { tasks: Vec::new(), generations: Vec::new(), ready: RunQueue::new(), current: None }
    }

    pub fn create_task(&mut self, priority: Priority) -> TaskId {
        if let Some((index, slot)) = self.tasks.iter_mut().enumerate().find(|(_, slot)| slot.is_none()) {
            let generation = self.generations[index].wrapping_add(1).max(1);
            self.generations[index] = generation;
            let id = TaskId::new(index as u32, generation);
            *slot = Some(Box::new(TaskControlBlock::new(id, priority)));
            return id;
        }
        let index = self.tasks.len();
        let id = TaskId::new(index as u32, 1);
        self.tasks.push(Some(Box::new(TaskControlBlock::new(id, priority))));
        self.generations.push(1);
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

    pub fn destroy_created(&mut self, id: TaskId) -> Result<ExecutionHandle, SchedulerError> {
        let index = id.index() as usize;
        let Some(slot) = self.tasks.get_mut(index) else { return Err(SchedulerError::TaskNotFound); };
        let Some(task) = slot.as_mut() else { return Err(SchedulerError::TaskNotFound); };
        if task.id() != id { return Err(SchedulerError::TaskNotFound); }
        if task.state() != TaskState::Created { return Err(SchedulerError::TaskNotCreated); }
        let handle = task.detach_execution().ok_or(SchedulerError::TaskNotCreated)?;
        *slot = None;
        Ok(handle)
    }

    pub fn make_ready(&mut self, id: TaskId) -> bool {
        if self.ready.contains(id) { return false; }
        let Some(task) = self.task_mut(id) else { return false; };
        task.transition(TaskState::Ready) && self.ready.push(id)
    }

    pub fn next_ready(&self) -> Option<TaskId> { self.ready.peek() }

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

        if let Some(previous_id) = previous {
            if self.ready.len() == 1 && self.ready.peek() == Some(previous_id) {
                let _ = self.ready.pop();
                if let Some(task) = self.task_mut(previous_id) { let _ = task.transition(TaskState::Running); }
                self.current = Some(previous_id);
                return DispatchDecision { previous, next: None };
            }
        }

        let next = self.ready.pop();
        if let Some(next_id) = next {
            if let Some(task) = self.task_mut(next_id) {
                if task.transition(TaskState::Running) { self.current = Some(next_id); }
                else { self.current = None; }
            } else { self.current = None; }
        } else { self.current = None; }
        DispatchDecision { previous, next: self.current }
    }

    pub fn block_current_on(&mut self, waiters: &mut WaitQueue) -> Result<TaskId, SchedulerError> {
        let id = self.current.ok_or(SchedulerError::TaskNotFound)?;
        if waiters.contains(id) { return Err(SchedulerError::WaitQueueAlreadyContainsTask); }
        let Some(task) = self.task_mut(id) else { return Err(SchedulerError::TaskNotFound); };
        if !task.transition(TaskState::Blocked) { return Err(SchedulerError::TaskNotFound); }
        if waiters.enqueue(id).is_err() {
            let _ = task.transition(TaskState::Running);
            return Err(SchedulerError::WaitQueueAlreadyContainsTask);
        }
        self.current = None;
        Ok(id)
    }

    pub fn block_current(&mut self) -> bool {
        let Some(id) = self.current else { return false; };
        let Some(task) = self.task_mut(id) else { return false; };
        let changed = task.transition(TaskState::Blocked);
        if changed { self.current = None; }
        changed
    }

    pub fn wake_one(&mut self, waiters: &mut WaitQueue) -> Option<TaskId> {
        while let Some(id) = waiters.dequeue() {
            let Some(task) = self.task_mut(id) else { continue; };
            if task.state() != TaskState::Blocked { continue; }
            if !task.transition(TaskState::Ready) { continue; }
            if !self.ready.push(id) {
                let _ = task.transition(TaskState::Blocked);
                let _ = waiters.enqueue(id);
                return None;
            }
            return Some(id);
        }
        None
    }

    pub fn exit_current(&mut self) -> bool {
        let Some(id) = self.current else { return false; };
        let index = id.index() as usize;
        let Some(task) = self.tasks.get_mut(index).and_then(Option::as_mut) else { return false; };
        if task.id() != id || !task.transition(TaskState::Exited) { return false; }
        let _ = task.detach_execution();
        self.tasks[index] = None;
        self.current = None;
        true
    }

    pub fn current(&self) -> Option<TaskId> { self.current }
    pub fn state(&self, id: TaskId) -> Option<TaskState> { self.task(id).map(TaskControlBlock::state) }
    pub fn ready_len(&self) -> usize { self.ready.len() }
    pub fn task_count(&self) -> usize { self.tasks.iter().filter(|task| task.is_some()).count() }

    #[cfg(test)]
    fn task_ptr(&self, id: TaskId) -> Option<*const TaskControlBlock> { self.task(id).map(|task| task as *const TaskControlBlock) }

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
    use super::{Priority, Scheduler, SchedulerError, TaskState, WaitQueue};

    #[test]
    fn scheduler_dispatches_ready_tasks_fifo() {
        let mut scheduler = Scheduler::new();
        let first = scheduler.create_task(Priority::DEFAULT);
        let second = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(first));
        assert!(scheduler.make_ready(second));
        assert_eq!(scheduler.next_ready(), Some(first));
        assert_eq!(scheduler.schedule_next().next, Some(first));
        assert_eq!(scheduler.schedule_next().next, Some(second));
    }

    #[test]
    fn scheduler_single_task_does_not_self_dispatch() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(task));
        assert_eq!(scheduler.schedule_next().next, Some(task));
        assert_eq!(scheduler.schedule_next(), super::DispatchDecision { previous: Some(task), next: None });
        assert_eq!(scheduler.state(task), Some(TaskState::Running));
    }

    #[test]
    fn duplicate_ready_membership_is_rejected_before_state_change() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(task));
        assert!(!scheduler.make_ready(task));
        assert_eq!(scheduler.ready_len(), 1);
    }

    #[test]
    fn block_and_wake_preserve_lifecycle_invariants() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        let mut waiters = WaitQueue::new();
        assert!(scheduler.make_ready(task));
        assert_eq!(scheduler.schedule_next().next, Some(task));
        assert_eq!(scheduler.block_current_on(&mut waiters), Ok(task));
        assert_eq!(scheduler.state(task), Some(TaskState::Blocked));
        assert_eq!(scheduler.current(), None);
        assert_eq!(scheduler.wake_one(&mut waiters), Some(task));
        assert_eq!(scheduler.state(task), Some(TaskState::Ready));
        assert_eq!(scheduler.ready_len(), 1);
    }

    #[test]
    fn duplicate_wait_membership_does_not_block_twice() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        let mut waiters = WaitQueue::new();
        assert!(scheduler.make_ready(task));
        assert_eq!(scheduler.schedule_next().next, Some(task));
        assert_eq!(scheduler.block_current_on(&mut waiters), Ok(task));
        assert_eq!(scheduler.block_current_on(&mut waiters), Err(SchedulerError::TaskNotFound));
        assert_eq!(waiters.len(), 1);
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
    }

    #[test]
    fn task_address_is_stable_when_slot_table_grows() {
        let mut scheduler = Scheduler::new();
        let first = scheduler.create_task(Priority::DEFAULT);
        let before = scheduler.task_ptr(first).unwrap();
        for _ in 0..128 { let _ = scheduler.create_task(Priority::DEFAULT); }
        let after = scheduler.task_ptr(first).unwrap();
        assert_eq!(before, after);
    }
}