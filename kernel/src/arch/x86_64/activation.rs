//! Stable kernel-task activation metadata.
//!
//! The activation object is separate from the execution binding so the initial
//! context can carry a stable pointer to task entry metadata without becoming
//! self-referential. The pointer is installed only after the object is boxed.

use alloc::boxed::Box;

use crate::scheduler::TaskId;

use super::context_switch::Context;

#[derive(Debug)]
pub struct ActivationRecord {
    task_id: TaskId,
    entry: extern "C" fn() -> !,
    boot_context: Context,
}

impl ActivationRecord {
    pub fn new(task_id: TaskId, entry: extern "C" fn() -> !) -> Box<Self> {
        Box::new(Self {
            task_id,
            entry,
            boot_context: Context::empty(),
        })
    }

    pub const fn task_id(&self) -> TaskId {
        self.task_id
    }

    pub const fn entry(&self) -> extern "C" fn() -> ! {
        self.entry
    }

    pub const fn boot_context(&self) -> &Context {
        &self.boot_context
    }

    pub fn boot_context_mut(&mut self) -> &mut Context {
        &mut self.boot_context
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
