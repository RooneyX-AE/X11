//! Owned snapshot of an interrupted x86_64 task.
//!
//! The raw interrupt stack frame is temporary. This type copies the CPU state
//! into task-owned memory before the interrupt stack can be reclaimed.

use super::interrupt_entry::{InterruptReturnFrame, SavedRegisters};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReturnState {
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: Option<u64>,
    ss: Option<u64>,
}

impl ReturnState {
    /// # Safety
    /// `frame` must reference a valid CPU interrupt frame for the interrupted
    /// task, and the frame must remain live for this call.
    pub unsafe fn capture(frame: InterruptReturnFrame) -> Self {
        Self {
            rip: unsafe { frame.rip() },
            cs: unsafe { frame.cs() },
            rflags: unsafe { frame.rflags() },
            rsp: unsafe { frame.rsp() },
            ss: unsafe { frame.ss() },
        }
    }

    pub const fn rip(self) -> u64 { self.rip }
    pub const fn cs(self) -> u64 { self.cs }
    pub const fn rflags(self) -> u64 { self.rflags }
    pub const fn rsp(self) -> Option<u64> { self.rsp }
    pub const fn ss(self) -> Option<u64> { self.ss }

    pub const fn is_kernel(self) -> bool { self.cs & 3 == 0 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptedState {
    registers: SavedRegisters,
    return_state: ReturnState,
}

impl InterruptedState {
    /// # Safety
    /// Both pointers must reference the live CPU-saved interrupt state.
    pub unsafe fn capture(
        registers: *const SavedRegisters,
        frame: InterruptReturnFrame,
    ) -> Self {
        let registers = unsafe { *registers };
        let return_state = unsafe { ReturnState::capture(frame) };
        Self { registers, return_state }
    }

    pub const fn registers(self) -> SavedRegisters { self.registers }
    pub const fn return_state(self) -> ReturnState { self.return_state }

    pub const fn is_valid(self) -> bool {
        self.return_state.rip() != 0
            && self.return_state.rflags() & 2 != 0
            && (self.return_state.is_kernel()
                || (self.return_state.rsp().is_some() && self.return_state.ss().is_some()))
    }
}

#[cfg(test)]
mod tests {
    use super::{InterruptedState, ReturnState};
    use crate::arch::x86_64::interrupt_entry::{InterruptReturnFrame, SavedRegisters};

    #[test]
    fn kernel_return_snapshot_is_valid() {
        let registers = SavedRegisters::default();
        let mut raw = [0u64; 3];
        raw[0] = 0x1000;
        raw[1] = 0x10;
        raw[2] = 0x202;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        let snapshot = unsafe { InterruptedState::capture(&registers, frame) };
        assert!(snapshot.is_valid());
        assert_eq!(snapshot.return_state().rsp(), None);
        assert_eq!(snapshot.return_state().ss(), None);
    }

    #[test]
    fn user_return_snapshot_requires_stack_pair() {
        let state = ReturnState {
            rip: 0x1000,
            cs: 0x1b,
            rflags: 0x202,
            rsp: Some(0x8000),
            ss: Some(0x23),
        };
        assert!(state.is_kernel() == false);
        assert!(state.rsp().is_some() && state.ss().is_some());
    }
}
