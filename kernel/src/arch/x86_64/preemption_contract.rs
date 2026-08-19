//! Architectural contract for the bootstrap kernel-preemption path.
//!
//! This module deliberately contains no scheduler mutation and no assembly.
//! It records the invariants that the boot fallback must satisfy before the
//! timer preemption path is enabled.

use super::context_switch::Context;
use super::interrupted_state::InterruptedState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapPreemptionContractError {
    MissingCurrentContext,
    InvalidInterruptedState,
}

pub fn validate_interrupted_state(
    state: Option<&InterruptedState>,
) -> Result<(), BootstrapPreemptionContractError> {
    let state = state.ok_or(BootstrapPreemptionContractError::MissingCurrentContext)?;
    if !state.is_valid() || state.return_state().kernel_iret_words().is_none() {
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
    use crate::arch::x86_64::interrupted_state::InterruptedState;
    use crate::arch::x86_64::interrupt_entry::{InterruptReturnFrame, SavedRegisters};

    fn state_from(raw: &mut [u64; 5]) -> InterruptedState {
        let registers = SavedRegisters::default();
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        unsafe { InterruptedState::capture(&registers, frame) }
    }

    #[test]
    fn rejects_missing_interrupted_state() {
        assert_eq!(
            validate_interrupted_state(None),
            Err(BootstrapPreemptionContractError::MissingCurrentContext)
        );
    }

    #[test]
    fn rejects_user_return_frame_for_kernel_preemption() {
        let mut raw = [0u64; 5];
        raw[0] = 0x1000;
        raw[1] = 0x1b;
        raw[2] = 0x202;
        raw[3] = 0x8000;
        raw[4] = 0x23;
        let state = state_from(&mut raw);
        assert_eq!(
            validate_interrupted_state(Some(&state)),
            Err(BootstrapPreemptionContractError::InvalidInterruptedState)
        );
    }

    #[test]
    fn accepts_valid_kernel_interrupted_state() {
        let mut raw = [0u64; 3];
        raw[0] = 0x1000;
        raw[1] = 0x08;
        raw[2] = 0x202;
        let state = unsafe {
            let registers = SavedRegisters::default();
            let frame = InterruptReturnFrame::from_raw(raw.as_mut_ptr());
            InterruptedState::capture(&registers, frame)
        };
        assert!(validate_interrupted_state(Some(&state)).is_ok());
    }

    #[test]
    fn rejects_uninitialized_context() {
        assert!(!context_is_initialized(&Context::empty()));
    }
}
