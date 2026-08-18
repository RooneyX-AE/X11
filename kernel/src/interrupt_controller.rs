//! Architecture-independent interrupt-controller contract.
//!
//! The kernel depends on these semantics, not on APIC register layouts.

use crate::interrupts::{InterruptEvent, EXTERNAL_VECTOR_BASE, VECTOR_MAX};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptControllerError {
    Unsupported,
    InvalidVector,
    NotInitialized,
}

pub trait InterruptController {
    type Vector: Copy + Eq;

    fn initialize(&mut self) -> Result<(), InterruptControllerError>;
    fn allocate_vector(&mut self) -> Result<Self::Vector, InterruptControllerError>;
    fn end_of_interrupt(&mut self, event: InterruptEvent);
    fn supports_vector(vector: u8) -> bool {
        (EXTERNAL_VECTOR_BASE..VECTOR_MAX).contains(&vector)
    }
}

#[cfg(test)]
mod tests {
    use super::InterruptController;

    struct TestController;

    impl InterruptController for TestController {
        type Vector = u8;

        fn initialize(&mut self) -> Result<(), super::InterruptControllerError> {
            Ok(())
        }

        fn allocate_vector(&mut self) -> Result<Self::Vector, super::InterruptControllerError> {
            Ok(32)
        }

        fn end_of_interrupt(&mut self, _event: crate::interrupts::InterruptEvent) {}
    }

    #[test]
    fn vector_policy_excludes_exception_space() {
        assert!(!TestController::supports_vector(31));
        assert!(TestController::supports_vector(32));
    }
}
