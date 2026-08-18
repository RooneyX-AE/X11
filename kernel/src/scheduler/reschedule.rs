//! Deferred scheduler request state.
//!
//! Interrupt handlers may request rescheduling, but they must not perform a
//! context switch themselves. The request is consumed only at an explicit
//! safe-return boundary.

use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Default)]
pub struct RescheduleRequest {
    pending: AtomicBool,
}

impl RescheduleRequest {
    pub const fn new() -> Self {
        Self { pending: AtomicBool::new(false) }
    }

    pub fn request(&self) {
        self.pending.store(true, Ordering::Release);
    }

    pub fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }

    pub fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
pub struct PreemptionGate {
    disabled_depth: usize,
}

impl PreemptionGate {
    pub const fn new() -> Self {
        Self { disabled_depth: 0 }
    }

    pub fn disable(&mut self) -> DisableGuard<'_> {
        self.disabled_depth = self.disabled_depth.saturating_add(1);
        DisableGuard { gate: self }
    }

    pub const fn is_enabled(&self) -> bool {
        self.disabled_depth == 0
    }

    pub const fn depth(&self) -> usize {
        self.disabled_depth
    }

    fn enable_one(&mut self) {
        debug_assert!(self.disabled_depth > 0);
        self.disabled_depth -= 1;
    }
}

pub struct DisableGuard<'a> {
    gate: &'a mut PreemptionGate,
}

impl Drop for DisableGuard<'_> {
    fn drop(&mut self) {
        self.gate.enable_one();
    }
}

#[cfg(test)]
mod tests {
    use super::{PreemptionGate, RescheduleRequest};

    #[test]
    fn request_is_edge_consumable() {
        let request = RescheduleRequest::new();
        assert!(!request.take());
        request.request();
        assert!(request.is_pending());
        assert!(request.take());
        assert!(!request.take());
    }

    #[test]
    fn nested_preemption_disable_is_balanced() {
        let mut gate = PreemptionGate::new();
        assert!(gate.is_enabled());
        {
            let outer = gate.disable();
            assert!(!gate.is_enabled());
            assert_eq!(gate.depth(), 1);
            {
                let inner = gate.disable();
                assert_eq!(gate.depth(), 2);
                drop(inner);
            }
            assert_eq!(gate.depth(), 1);
            drop(outer);
        }
        assert!(gate.is_enabled());
        assert_eq!(gate.depth(), 0);
    }
}
