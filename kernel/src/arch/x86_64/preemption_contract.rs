//! Architectural contract for the bootstrap kernel-preemption path.
//!
//! This module deliberately contains no scheduler mutation and no assembly.
//! It records the invariants that the boot fallback must satisfy before the
//! timer preemption path is enabled.

use super::context_switch::Context;
use super::interrupted_state::KernelPreemptState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapPreemptionContractError {
    MissingCurrentContext,
    InvalidInterruptedState,
}

pub fn validate_interrupted_state(
    state: Option<&KernelPreemptState>,
) -> Result<(), BootstrapPreemptionContractError> {
    let state = state.ok_or(BootstrapPreemptionContractError::MissingCurrentContext)?;
    if state.rip == 0 || state.resume_rsp < 24 || state.cs & 3 != 0 || state.rflags & 2 == 0 {
        return Err(BootstrapPreemptionContractError::InvalidInterruptedState);
    }
    Ok(())
}

pub fn context_is_initialized(context: &Context) -> bool {
    context.rip != 0 && context.rsp != 0
}

#[cfg(test)]
mod tests {
    use super::{context_is_initialized, validate_interrupted_state, BootstrapPreemptionContractError};
    use crate::arch::x86_64::context_switch::Context;
    use crate::arch::x86_64::interrupted_state::KernelPreemptState;
    use crate::arch::x86_64::interrupt_entry::SavedRegisters;

    #[test]
    fn rejects_missing_interrupted_state() {
        assert_eq!(
            validate_interrupted_state(None),
            Err(BootstrapPreemptionContractError::MissingCurrentContext)
        );
    }

    #[test]
    fn rejects_user_return_frame_for_kernel_preemption() {
        let state = KernelPreemptState {
            registers: SavedRegisters::default(),
            rip: 0x1000,
            cs: 0x1b,
            rflags: 0x202,
            resume_rsp: 0x8000,
        };
        assert_eq!(
            validate_interrupted_state(Some(&state)),
            Err(BootstrapPreemptionContractError::InvalidInterruptedState)
        );
    }

    #[test]
    fn accepts_valid_kernel_interrupted_state() {
        let state = KernelPreemptState {
            registers: SavedRegisters::default(),
            rip: 0x1000,
            cs: 0x08,
            rflags: 0x202,
            resume_rsp: 0x8000,
        };
        assert!(validate_interrupted_state(Some(&state)).is_ok());
    }

    #[test]
    fn rejects_uninitialized_context() {
        assert!(!context_is_initialized(&Context::empty()));
    }
}
