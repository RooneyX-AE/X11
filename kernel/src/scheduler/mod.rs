//! Architecture-independent scheduler foundation.
//!
//! This layer owns task lifecycle and run-queue policy. Architecture-specific
//! register save/restore and address-space switching stay below this boundary.

mod run_queue;
mod task;

use alloc::vec::Vec;

pub use run_queue::RunQueue;
pub use task::{Priority, TaskControlBlock, TaskId, TaskState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchDecision {
    pub previous: Option<TaskId>,
    pub next: Option<TaskId>,
}

#[derive(Debug, Default)]
pub struct Scheduler {
    tasks: Vec<Option<TaskControlBlock>>,
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
            *slot = Some(TaskControlBlock::new(id, priority));
            return id;
        }

        let index = self.tasks.len();
        let generation = 1;
        self.tasks.push(Some(TaskControlBlock::new(
            TaskId::new(index as u32, generation),
            priority,
        )));
        self.generations.push(generation);
        TaskId::new(index as u32, generation)
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
        let Some(task) = self.task_mut(id) else {
            return false;
        };
        let changed = task.transition(TaskState::Exited);
        if changed {
            self.current = None;
        }
        changed
    }

    pub fn current(&self) -> Option<TaskId> {
        self.current
    }

    pub fn state(&self, id: TaskId) -> Option<TaskState> {
        self.task(id).map(TaskControlBlock::state)
    }

    pub fn ready_len(&self) -> usize {
        self.ready.len()
    }

    pub fn task_count(&self) -> usize {
        self.tasks.iter().filter(|task| task.is_some()).count()
    }

    fn task(&self, id: TaskId) -> Option<&TaskControlBlock> {
        let task = self.tasks.get(id.index() as usize)?.as_ref()?;
        (task.id() == id).then_some(task)
    }

    fn task_mut(&mut self, id: TaskId) -> Option<&mut TaskControlBlock> {
        let task = self.tasks.get_mut(id.index() as usize)?.as_mut()?;
        if task.id() == id {
            Some(task)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Priority, Scheduler, TaskState};

    #[test]
    fn scheduler_dispatches_ready_tasks_fifo() {
        let mut scheduler = Scheduler::new();
        let first = scheduler.create_task(Priority::DEFAULT);
        let second = scheduler.create_task(Priority::DEFAULT);
        assert!(scheduler.make_ready(first));
        assert!(scheduler.make_ready(second));

        let decision = scheduler.schedule_next();
        assert_eq!(decision.next, Some(first));
        assert_eq!(scheduler.state(first), Some(TaskState::Running));

        let decision = scheduler.schedule_next();
        assert_eq!(decision.next, Some(second));
        assert_eq!(scheduler.state(second), Some(TaskState::Running));
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
        assert_eq!(scheduler.state(first), None);
    }
}
