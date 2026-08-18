//! x86_64 preemption decision boundary.
//!
//! This layer observes a CPU-created timer return frame and separates the
//! scheduler decision from any later mutation of the live return state.
//! No assembly or `iretq` manipulation belongs here.

use super::interrupt_frame::KernelInterruptFrame;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptSnapshot {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
}

impl InterruptSnapshot {
    pub const fn from_frame(frame: KernelInterruptFrame) -> Self {
        Self {
            rip: frame.rip,
            cs: frame.cs,
            rflags: frame.rflags,
        }
    }

    pub const fn interrupt_enabled(self) -> bool {
        self.rflags & (1 << 9) != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreemptionDecision<T> {
    pub current: T,
    pub next: Option<T>,
}

impl<T: Copy + Eq> PreemptionDecision<T> {
    pub const fn should_switch(self) -> bool {
        match self.next {
            Some(next) => next != self.current,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InterruptSnapshot, PreemptionDecision};
    use crate::arch::x86_64::interrupt_frame::KernelInterruptFrame;

    #[test]
    fn snapshot_preserves_cpu_return_state() {
        let frame = KernelInterruptFrame::new(0x1234, 0x8, 1 << 9);
        let snapshot = InterruptSnapshot::from_frame(frame);
        assert_eq!(snapshot.rip, 0x1234);
        assert_eq!(snapshot.cs, 0x8);
        assert!(snapshot.interrupt_enabled());
    }

    #[test]
    fn decision_only_switches_to_a_different_task() {
        let keep = PreemptionDecision {
            current: 7u32,
            next: Some(7u32),
        };
        let change = PreemptionDecision {
            current: 7u32,
            next: Some(9u32),
        };
        assert!(!keep.should_switch());
        assert!(change.should_switch());
    }

    #[test]
    fn missing_next_task_keeps_current() {
        let decision = PreemptionDecision {
            current: 7u32,
            next: None,
        };
        assert!(!decision.should_switch());
    }
}
