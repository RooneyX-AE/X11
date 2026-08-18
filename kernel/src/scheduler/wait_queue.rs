//! Architecture-independent FIFO wait queue.
//!
//! Wait queues own blocked-task membership only. They never mutate task state;
//! the scheduler remains the sole owner of lifecycle transitions.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use super::TaskId;

#[derive(Debug, Default)]
pub struct WaitQueue {
    queue: VecDeque<TaskId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitQueueError {
    AlreadyQueued,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn enqueue(&mut self, task: TaskId) -> Result<(), WaitQueueError> {
        if self.queue.contains(&task) {
            return Err(WaitQueueError::AlreadyQueued);
        }
        self.queue.push_back(task);
        Ok(())
    }

    pub fn dequeue(&mut self) -> Option<TaskId> {
        self.queue.pop_front()
    }

    pub fn remove(&mut self, task: TaskId) -> bool {
        let Some(position) = self.queue.iter().position(|candidate| *candidate == task) else {
            return false;
        };
        self.queue.remove(position).is_some()
    }

    pub fn contains(&self, task: TaskId) -> bool {
        self.queue.contains(&task)
    }

    pub fn peek(&self) -> Option<TaskId> {
        self.queue.front().copied()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn snapshot(&self) -> Vec<TaskId> {
        self.queue.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{WaitQueue, WaitQueueError};
    use crate::scheduler::TaskId;

    #[test]
    fn preserves_fifo_order() {
        let mut queue = WaitQueue::new();
        let a = TaskId::new(0, 1);
        let b = TaskId::new(1, 1);
        queue.enqueue(a).unwrap();
        queue.enqueue(b).unwrap();
        assert_eq!(queue.dequeue(), Some(a));
        assert_eq!(queue.dequeue(), Some(b));
    }

    #[test]
    fn rejects_duplicate_membership() {
        let mut queue = WaitQueue::new();
        let task = TaskId::new(2, 4);
        queue.enqueue(task).unwrap();
        assert_eq!(queue.enqueue(task), Err(WaitQueueError::AlreadyQueued));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn removes_specific_waiter_without_reordering_remaining_tasks() {
        let mut queue = WaitQueue::new();
        let a = TaskId::new(0, 1);
        let b = TaskId::new(1, 1);
        let c = TaskId::new(2, 1);
        queue.enqueue(a).unwrap();
        queue.enqueue(b).unwrap();
        queue.enqueue(c).unwrap();
        assert!(queue.remove(b));
        assert_eq!(queue.snapshot(), alloc::vec![a, c]);
    }
}