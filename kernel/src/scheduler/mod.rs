//! Architecture-independent scheduler foundation.
//!
//! Task control blocks are individually heap allocated so growing the slot
//! table never relocates a live task object. Architecture-specific execution
//! state may therefore safely retain a stable pointer to its owning task.

mod execution;
mod reschedule;
mod run_queue;
mod sleep_queue;
mod task;
mod wait_queue;

use alloc::boxed::Box;
use alloc::vec::Vec;

pub use execution::{ExecutionBinding, ExecutionHandle};
pub use reschedule::{DisableGuard, PreemptionGate, RescheduleRequest};
pub use run_queue::RunQueue;
pub use sleep_queue::{SleepEntry, SleepQueue};
pub use task::{ExecutionAttachError, Priority, TaskControlBlock, TaskId, TaskState};
pub use wait_queue::{WaitQueue, WaitQueueError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchDecision { pub previous: Option<TaskId>, pub next: Option<TaskId> }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    TaskNotFound,
    ExecutionAlreadyAttached,
    TaskNotCreated,
    WaitQueueAlreadyContainsTask,
    SleepQueueAlreadyContainsTask,
}

#[derive(Debug, Default)]
pub struct Scheduler {
    tasks: Vec<Option<Box<TaskControlBlock>>>,
    generations: Vec<u32>,
    ready: RunQueue,
    current: Option<TaskId>,
}

impl Scheduler {
    pub const fn new() -> Self { Self { tasks: Vec::new(), generations: Vec::new(), ready: RunQueue::new(), current: None } }

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
        task.attach_execution().map_err(|error| match error { ExecutionAttachError::AlreadyAttached => SchedulerError::ExecutionAlreadyAttached })
    }
    pub fn execution(&self, id: TaskId) -> Option<ExecutionHandle> { self.task(id).and_then(TaskControlBlock::execution) }

    pub fn destroy_created(&mut self, id: TaskId) -> Result<ExecutionHandle, SchedulerError> {
        let index = id.index() as usize;
        let Some(slot) = self.tasks.get_mut(index) else { return Err(SchedulerError::TaskNotFound); };
        let Some(task) = slot.as_mut() else { return Err(SchedulerError::TaskNotFound); };
        if task.id() != id || task.state() != TaskState::Created { return Err(SchedulerError::TaskNotCreated); }
        let handle = task.detach_execution().ok_or(SchedulerError::TaskNotCreated)?;
        *slot = None;
        Ok(handle)
    }

    pub fn make_ready(&mut self, id: TaskId) -> bool {
        if self.ready.contains(id) { return false; }
        let transitioned = { let Some(task) = self.task_mut(id) else { return false; }; task.transition(TaskState::Ready) };
        transitioned && self.ready.push(id)
    }
    pub fn next_ready(&self) -> Option<TaskId> { self.ready.peek() }

    pub fn schedule_next(&mut self) -> DispatchDecision {
        let previous = self.current;
        if let Some(previous_id) = previous {
            if let Some(task) = self.task_mut(previous_id) {
                if task.state() == TaskState::Running { let _ = task.transition(TaskState::Ready); let _ = self.ready.push(previous_id); }
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
                if task.transition(TaskState::Running) { self.current = Some(next_id); } else { self.current = None; }
            } else { self.current = None; }
        } else { self.current = None; }
        DispatchDecision { previous, next: self.current }
    }

    pub fn block_current_on(&mut self, waiters: &mut WaitQueue) -> Result<TaskId, SchedulerError> {
        let id = self.current.ok_or(SchedulerError::TaskNotFound)?;
        if waiters.contains(id) { return Err(SchedulerError::WaitQueueAlreadyContainsTask); }
        let transitioned = { let Some(task) = self.task_mut(id) else { return Err(SchedulerError::TaskNotFound); }; task.transition(TaskState::Blocked) };
        if !transitioned { return Err(SchedulerError::TaskNotFound); }
        if let Err(WaitQueueError::AlreadyQueued) = waiters.enqueue(id) {
            if let Some(task) = self.task_mut(id) { let _ = task.transition(TaskState::Running); }
            return Err(SchedulerError::WaitQueueAlreadyContainsTask);
        }
        self.current = None;
        Ok(id)
    }

    pub fn block_current(&mut self) -> bool {
        let Some(id) = self.current else { return false; };
        let changed = { let Some(task) = self.task_mut(id) else { return false; }; task.transition(TaskState::Blocked) };
        if changed { self.current = None; }
        changed
    }

    pub fn wake_one(&mut self, waiters: &mut WaitQueue) -> Option<TaskId> {
        while let Some(id) = waiters.dequeue() {
            let ready = { let Some(task) = self.task_mut(id) else { continue; }; if task.state() != TaskState::Blocked { continue; } task.transition(TaskState::Ready) };
            if !ready { continue; }
            if !self.ready.push(id) {
                if let Some(task) = self.task_mut(id) { let _ = task.transition(TaskState::Blocked); }
                let _ = waiters.enqueue(id);
                return None;
            }
            return Some(id);
        }
        None
    }

    pub fn sleep_current_until(&mut self, deadline: u64, sleepers: &mut SleepQueue) -> Result<TaskId, SchedulerError> {
        let id = self.current.ok_or(SchedulerError::TaskNotFound)?;
        if sleepers.contains(id) { return Err(SchedulerError::SleepQueueAlreadyContainsTask); }
        let transitioned = { let Some(task) = self.task_mut(id) else { return Err(SchedulerError::TaskNotFound); }; task.transition(TaskState::Blocked) };
        if !transitioned { return Err(SchedulerError::TaskNotFound); }
        if !sleepers.insert(SleepEntry::new(deadline, id)) {
            if let Some(task) = self.task_mut(id) { let _ = task.transition(TaskState::Running); }
            return Err(SchedulerError::SleepQueueAlreadyContainsTask);
        }
        self.current = None;
        Ok(id)
    }

    pub fn expire_sleepers(&mut self, now: u64, sleepers: &mut SleepQueue) -> Vec<TaskId> {
        let expired = sleepers.expire_until(now);
        let mut woken = Vec::with_capacity(expired.len());
        for id in expired {
            let transitioned = { let Some(task) = self.task_mut(id) else { continue; }; if task.state() != TaskState::Blocked { continue; } task.transition(TaskState::Ready) };
            if transitioned && self.ready.push(id) { woken.push(id); }
        }
        woken
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
    #[cfg(test)] fn task_ptr(&self, id: TaskId) -> Option<*const TaskControlBlock> { self.task(id).map(|task| task as *const TaskControlBlock) }
    fn task(&self, id: TaskId) -> Option<&TaskControlBlock> { let task = self.tasks.get(id.index() as usize)?.as_deref()?; (task.id() == id).then_some(task) }
    fn task_mut(&mut self, id: TaskId) -> Option<&mut TaskControlBlock> { let task = self.tasks.get_mut(id.index() as usize)?.as_deref_mut()?; if task.id() == id { Some(task) } else { None } }
}

#[cfg(test)]
mod tests {
    use super::{Priority, Scheduler, SleepEntry, SleepQueue, TaskState, WaitQueue};

    #[test]
    fn scheduler_round_robin_cycles_a_b_a_b() {
        let mut scheduler = Scheduler::new();
        let a = scheduler.create_task(Priority::DEFAULT);
        let b = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(a));
        assert!(scheduler.make_ready(b));

        assert_eq!(scheduler.schedule_next().next, Some(a));
        assert_eq!(scheduler.schedule_next().next, Some(b));
        assert_eq!(scheduler.schedule_next().next, Some(a));
        assert_eq!(scheduler.schedule_next().next, Some(b));

        assert_eq!(scheduler.state(a), Some(TaskState::Ready));
        assert_eq!(scheduler.state(b), Some(TaskState::Running));
        assert_eq!(scheduler.current(), Some(b));
    }

    #[test]
    fn scheduler_does_not_self_dispatch_single_task() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(task));
        assert_eq!(scheduler.schedule_next().next, Some(task));
        assert_eq!(scheduler.schedule_next().next, None);
        assert_eq!(scheduler.state(task), Some(TaskState::Running));
        assert_eq!(scheduler.ready_len(), 0);
    }

    #[test]
    fn sleep_blocks_current_then_expiry_makes_task_ready() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(task));
        assert_eq!(scheduler.schedule_next().next, Some(task));

        let mut sleepers = SleepQueue::new();
        assert_eq!(scheduler.sleep_current_until(100, &mut sleepers), Ok(task));
        assert_eq!(scheduler.current(), None);
        assert_eq!(scheduler.state(task), Some(TaskState::Blocked));

        assert!(scheduler.expire_sleepers(99, &mut sleepers).is_empty());
        assert_eq!(scheduler.state(task), Some(TaskState::Blocked));
        assert_eq!(scheduler.expire_sleepers(100, &mut sleepers), alloc::vec![task]);
        assert_eq!(scheduler.state(task), Some(TaskState::Ready));
        assert_eq!(scheduler.next_ready(), Some(task));
    }

    #[test]
    fn duplicate_sleep_rolls_back_state() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(task));
        assert_eq!(scheduler.schedule_next().next, Some(task));

        let mut sleepers = SleepQueue::new();
        assert!(sleepers.insert(SleepEntry::new(50, task)));
        assert_eq!(scheduler.sleep_current_until(100, &mut sleepers), Err(super::SchedulerError::SleepQueueAlreadyContainsTask));
        assert_eq!(scheduler.current(), Some(task));
        assert_eq!(scheduler.state(task), Some(TaskState::Running));
    }

    #[test]
    fn block_and_wake_preserve_fifo_and_ready_state() {
        let mut scheduler = Scheduler::new();
        let a = scheduler.create_task(Priority::DEFAULT);
        let b = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(a));
        assert!(scheduler.make_ready(b));
        assert_eq!(scheduler.schedule_next().next, Some(a));

        let mut waiters = WaitQueue::new();
        assert_eq!(scheduler.block_current_on(&mut waiters), Ok(a));
        assert_eq!(scheduler.current(), None);
        assert_eq!(scheduler.state(a), Some(TaskState::Blocked));
        assert_eq!(waiters.peek(), Some(a));

        assert_eq!(scheduler.schedule_next().next, Some(b));
        assert_eq!(scheduler.wake_one(&mut waiters), Some(a));
        assert_eq!(scheduler.state(a), Some(TaskState::Ready));
        assert_eq!(scheduler.next_ready(), Some(a));
    }

    #[test]
    fn exit_current_removes_task_and_clears_cpu_owner() {
        let mut scheduler = Scheduler::new();
        let task = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(task));
        assert_eq!(scheduler.schedule_next().next, Some(task));
        assert!(scheduler.exit_current());
        assert_eq!(scheduler.current(), None);
        assert_eq!(scheduler.state(task), None);
        assert_eq!(scheduler.next_ready(), None);
        assert_eq!(scheduler.task_count(), 0);
    }

    #[test]
    fn stale_waiter_id_is_ignored_after_task_exit() {
        let mut scheduler = Scheduler::new();
        let old = scheduler.create_task(Priority::DEFAULT);
        let mut waiters = WaitQueue::new();
        assert!(waiters.enqueue(old).is_ok());
        let replacement = scheduler.create_task(Priority::DEFAULT);
        assert_ne!(old, replacement);
        assert_eq!(scheduler.wake_one(&mut waiters), None);
        assert_eq!(scheduler.state(replacement), Some(TaskState::Created));
        assert!(waiters.is_empty());
    }
}
