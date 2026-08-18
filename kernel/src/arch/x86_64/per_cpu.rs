//! Per-CPU state contract for the x86_64 execution layer.
//!
//! APIC state, the currently running task, interrupt nesting, and scheduler
//! bookkeeping are logically owned by each logical processor. This module
//! provides the architectural boundary; SMP bootstrap will supply the real
//! CPU-local storage later.

use crate::scheduler::TaskId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuId(u32);

impl CpuId {
    pub const BOOTSTRAP: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PerCpuState {
    cpu_id: CpuId,
    current: Option<TaskId>,
    interrupt_depth: u32,
}

impl PerCpuState {
    pub const fn bootstrap() -> Self {
        Self {
            cpu_id: CpuId::BOOTSTRAP,
            current: None,
            interrupt_depth: 0,
        }
    }

    pub const fn cpu_id(self) -> CpuId {
        self.cpu_id
    }

    pub const fn current(self) -> Option<TaskId> {
        self.current
    }

    pub const fn interrupt_depth(self) -> u32 {
        self.interrupt_depth
    }

    pub fn set_current(&mut self, task: Option<TaskId>) {
        self.current = task;
    }

    pub fn enter_interrupt(&mut self) {
        self.interrupt_depth = self.interrupt_depth.saturating_add(1);
    }

    pub fn leave_interrupt(&mut self) -> bool {
        if self.interrupt_depth == 0 {
            return false;
        }
        self.interrupt_depth -= 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::PerCpuState;

    #[test]
    fn bootstrap_state_starts_idle() {
        let state = PerCpuState::bootstrap();
        assert_eq!(state.cpu_id().value(), 0);
        assert_eq!(state.current(), None);
        assert_eq!(state.interrupt_depth(), 0);
    }

    #[test]
    fn interrupt_depth_is_balanced() {
        let mut state = PerCpuState::bootstrap();
        state.enter_interrupt();
        state.enter_interrupt();
        assert_eq!(state.interrupt_depth(), 2);
        assert!(state.leave_interrupt());
        assert!(state.leave_interrupt());
        assert!(!state.leave_interrupt());
    }
}
