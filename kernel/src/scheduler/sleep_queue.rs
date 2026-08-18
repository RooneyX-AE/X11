//! Deadline queue for scheduler sleep/timeout state.
//!
//! The queue owns only `(deadline, TaskId)` membership. It does not mutate task
//! state and does not interact with CPU context switching. A timer backend can
//! later call `expire_until()` using its monotonic tick source.

use alloc::vec::Vec;

use super::TaskId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SleepEntry {
    deadline: u64,
    task: TaskId,
}

impl SleepEntry {
    pub const fn new(deadline: u64, task: TaskId) -> Self {
        Self { deadline, task }
    }

    pub const fn deadline(self) -> u64 {
        self.deadline
    }

    pub const fn task(self) -> TaskId {
        self.task
    }
}

#[derive(Debug, Default)]
pub struct SleepQueue {
    entries: Vec<SleepEntry>,
}

impl SleepQueue {
    pub const fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn insert(&mut self, entry: SleepEntry) -> bool {
        if self.entries.iter().any(|candidate| candidate.task == entry.task) {
            return false;
        }
        let position = self.entries.partition_point(|candidate| {
            (candidate.deadline, candidate.task) <= (entry.deadline, entry.task)
        });
        self.entries.insert(position, entry);
        true
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.entries.first().map(|entry| entry.deadline)
    }

    pub fn expire_until(&mut self, now: u64) -> Vec<TaskId> {
        let split = self.entries.partition_point(|entry| entry.deadline <= now);
        self.entries.drain(..split).map(SleepEntry::task).collect()
    }

    pub fn remove(&mut self, task: TaskId) -> bool {
        let Some(position) = self.entries.iter().position(|entry| entry.task == task) else {
            return false;
        };
        self.entries.remove(position);
        true
    }

    pub fn contains(&self, task: TaskId) -> bool {
        self.entries.iter().any(|entry| entry.task == task)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn snapshot(&self) -> Vec<SleepEntry> {
        self.entries.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{SleepEntry, SleepQueue};
    use crate::scheduler::TaskId;

    #[test]
    fn deadlines_are_monotonic() {
        let mut queue = SleepQueue::new();
        let a = TaskId::new(0, 1);
        let b = TaskId::new(1, 1);
        assert!(queue.insert(SleepEntry::new(30, a)));
        assert!(queue.insert(SleepEntry::new(10, b)));
        assert_eq!(queue.snapshot(), alloc::vec![SleepEntry::new(10, b), SleepEntry::new(30, a)]);
    }

    #[test]
    fn equal_deadlines_use_task_id_for_deterministic_order() {
        let mut queue = SleepQueue::new();
        let a = TaskId::new(2, 1);
        let b = TaskId::new(1, 1);
        assert!(queue.insert(SleepEntry::new(10, a)));
        assert!(queue.insert(SleepEntry::new(10, b)));
        assert_eq!(queue.snapshot(), alloc::vec![SleepEntry::new(10, b), SleepEntry::new(10, a)]);
    }

    #[test]
    fn duplicate_sleep_is_rejected() {
        let mut queue = SleepQueue::new();
        let task = TaskId::new(0, 1);
        assert!(queue.insert(SleepEntry::new(10, task)));
        assert!(!queue.insert(SleepEntry::new(20, task)));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn expiry_is_inclusive() {
        let mut queue = SleepQueue::new();
        let task = TaskId::new(0, 1);
        assert!(queue.insert(SleepEntry::new(10, task)));
        assert_eq!(queue.expire_until(9), alloc::vec![]);
        assert_eq!(queue.expire_until(10), alloc::vec![task]);
        assert!(queue.is_empty());
    }

    #[test]
    fn remove_deletes_only_the_requested_task() {
        let mut queue = SleepQueue::new();
        let a = TaskId::new(0, 1);
        let b = TaskId::new(1, 1);
        queue.insert(SleepEntry::new(10, a));
        queue.insert(SleepEntry::new(20, b));
        assert!(queue.remove(a));
        assert!(!queue.contains(a));
        assert!(queue.contains(b));
        assert_eq!(queue.next_deadline(), Some(20));
        assert!(!queue.remove(a));
    }
}
