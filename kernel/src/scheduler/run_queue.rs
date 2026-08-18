//! Architecture-independent ready queue.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use super::task::TaskId;

/// Deterministic FIFO queue used by the initial round-robin scheduler policy.
#[derive(Debug, Default)]
pub struct RunQueue {
    queue: VecDeque<TaskId>,
}

impl RunQueue {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn push(&mut self, id: TaskId) -> bool {
        if self.queue.contains(&id) {
            return false;
        }
        self.queue.push_back(id);
        true
    }

    pub fn peek(&self) -> Option<TaskId> {
        self.queue.front().copied()
    }

    pub fn pop(&mut self) -> Option<TaskId> {
        self.queue.pop_front()
    }

    pub fn rotate(&mut self, running: TaskId) -> Option<TaskId> {
        if self.queue.front().copied() == Some(running) {
            let id = self.queue.pop_front()?;
            self.queue.push_back(id);
        }
        self.queue.front().copied()
    }

    pub fn remove(&mut self, id: TaskId) -> bool {
        let Some(position) = self.queue.iter().position(|candidate| *candidate == id) else {
            return false;
        };
        self.queue.remove(position).is_some()
    }

    pub fn contains(&self, id: TaskId) -> bool {
        self.queue.contains(&id)
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
    use super::{RunQueue, TaskId};

    #[test]
    fn fifo_order_is_stable() {
        let mut queue = RunQueue::new();
        let a = TaskId::new(0, 1);
        let b = TaskId::new(1, 1);
        assert!(queue.push(a));
        assert!(queue.push(b));
        assert_eq!(queue.pop(), Some(a));
        assert_eq!(queue.pop(), Some(b));
    }

    #[test]
    fn peek_does_not_mutate_queue() {
        let mut queue = RunQueue::new();
        let a = TaskId::new(0, 1);
        let b = TaskId::new(1, 1);
        queue.push(a);
        queue.push(b);
        assert_eq!(queue.peek(), Some(a));
        assert_eq!(queue.snapshot(), alloc::vec![a, b]);
    }

    #[test]
    fn duplicate_task_is_not_enqueued() {
        let mut queue = RunQueue::new();
        let id = TaskId::new(0, 1);
        assert!(queue.push(id));
        assert!(!queue.push(id));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn rotation_moves_current_task_to_back() {
        let mut queue = RunQueue::new();
        let a = TaskId::new(0, 1);
        let b = TaskId::new(1, 1);
        let c = TaskId::new(2, 1);
        queue.push(a);
        queue.push(b);
        queue.push(c);
        assert_eq!(queue.rotate(a), Some(b));
        assert_eq!(queue.snapshot(), alloc::vec![b, c, a]);
    }
}
