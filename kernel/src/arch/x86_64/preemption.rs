//! x86_64 preemption boundary.
//!
//! Timer interrupts only request a reschedule. This module defines the
//! architecture-side hand-off contract without performing a context switch
//! from interrupt context. The actual switch remains owned by the runtime's
//! safe-return boundary.

use crate::scheduler::RescheduleRequest;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PreemptionError {
    Disabled,
    NoRequest,
    NoRunnableTask,
}

#[derive(Debug, Default)]
pub struct PreemptionController {
    request: RescheduleRequest,
}

impl PreemptionController {
    pub const fn new() -> Self {
        Self { request: RescheduleRequest::new() }
    }

    /// Records a preemption request from an interrupt or deferred event.
    pub fn request(&self) {
        self.request.request();
    }

    pub const fn request_state(&self) -> &RescheduleRequest {
        &self.request
    }

    /// Consumes a pending request only after the caller has reached a safe
    /// return point. No CPU context switch occurs here.
    pub fn take_request(&self, enabled: bool) -> Result<(), PreemptionError> {
        if !enabled {
            return Err(PreemptionError::Disabled);
        }
        if self.request.take() {
            Ok(())
        } else {
            Err(PreemptionError::NoRequest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PreemptionController, PreemptionError};

    #[test]
    fn request_is_deferred_until_safe_point() {
        let controller = PreemptionController::new();
        controller.request();
        assert_eq!(controller.take_request(false), Err(PreemptionError::Disabled));
        assert_eq!(controller.take_request(true), Ok(()));
        assert_eq!(controller.take_request(true), Err(PreemptionError::NoRequest));
    }
}
