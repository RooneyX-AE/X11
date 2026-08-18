//! Stable kernel-task activation metadata.
//!
//! The activation object is separate from the execution binding so the initial
//! context can carry a stable pointer to task entry metadata without becoming
//! self-referential.

use alloc::boxed::Box;

use crate::scheduler::TaskId;

#[derive(Debug)]
pub struct ActivationRecord {
    task_id: TaskId,
    entry: extern "C" fn() -> !,
}

impl ActivationRecord {
    pub fn new(task_id: TaskId, entry: extern "C" fn() -> !) -> Box<Self> {
        Box::new(Self { task_id, entry })
    }

    pub const fn task_id(&self) -> TaskId {
        self.task_id
    }

    pub const fn entry(&self) -> extern "C" fn() -> ! {
        self.entry
    }

    pub fn pointer(&self) -> u64 {
        self as *const Self as usize as u64
    }
}

#[cfg(test)]
mod tests {
    use super::ActivationRecord;
    use crate::scheduler::TaskId;

    extern "C" fn never_returns() -> ! {
        loop {}
    }

    #[test]
    fn boxed_activation_has_stable_address() {
        let record = ActivationRecord::new(TaskId::new(1, 1), never_returns);
        let before = record.pointer();
        let moved = record;
        assert_eq!(before, moved.pointer());
    }
}
