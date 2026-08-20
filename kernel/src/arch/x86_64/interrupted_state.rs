//! Owned snapshot of an interrupted x86_64 task.
//!
//! The raw interrupt stack frame is temporary. This type copies the CPU state
//! into task-owned memory before the interrupt stack can be reclaimed.

use super::interrupt_entry::{InterruptReturnFrame, SavedRegisters};

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReturnState {
    rip: u64,
    cs: u64,
    rflags: u64,
    resume_rsp: u64,
    rsp: Option<u64>,
    ss: Option<u64>,
}

impl ReturnState {
    pub unsafe fn capture(frame: InterruptReturnFrame) -> Option<Self> {
        Some(Self {
            rip: unsafe { frame.rip() },
            cs: unsafe { frame.cs() },
            rflags: unsafe { frame.rflags() },
            resume_rsp: frame.resume_rsp()?,
            rsp: unsafe { frame.rsp() },
            ss: unsafe { frame.ss() },
        })
    }

    pub const fn rip(self) -> u64 { self.rip }
    pub const fn cs(self) -> u64 { self.cs }
    pub const fn rflags(self) -> u64 { self.rflags }
    pub const fn resume_rsp(self) -> u64 { self.resume_rsp }
    pub const fn rsp(self) -> Option<u64> { self.rsp }
    pub const fn ss(self) -> Option<u64> { self.ss }

    pub const fn is_kernel(self) -> bool { self.cs & 3 == 0 }
    pub const fn is_user(self) -> bool { self.cs & 3 == 3 }

    pub const fn kernel_iret_words(self) -> Option<[u64; 3]> {
        if !self.is_kernel() || self.rip == 0 || self.rflags & 2 == 0 || self.resume_rsp < 24 {
            return None;
        }
        Some([self.rip, self.cs, self.rflags])
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptedState {
    registers: SavedRegisters,
    return_state: ReturnState,
}

impl InterruptedState {
    pub unsafe fn capture(registers: *const SavedRegisters, frame: InterruptReturnFrame) -> Self {
        let registers = unsafe { *registers };
        let return_state = unsafe { ReturnState::capture(frame) }
            .expect("interrupt frame address must be representable as u64");
        Self { registers, return_state }
    }

    pub const fn registers(self) -> SavedRegisters { self.registers }
    pub const fn return_state(self) -> ReturnState { self.return_state }

    pub const fn is_valid(self) -> bool {
        self.return_state.rip() != 0
            && self.return_state.rflags() & 2 != 0
            && self.return_state.resume_rsp() != 0
            && (self.return_state.is_kernel()
                || (self.return_state.rsp().is_some() && self.return_state.ss().is_some()))
    }

    pub const fn is_user_valid(self) -> bool {
        if !self.is_valid() || !self.return_state.is_user() || self.return_state.rsp().is_none() {
            return false;
        }
        let Some(ss) = self.return_state.ss() else { return false; };
        self.return_state.cs() & 3 == 3 && ss & 3 == 3
    }
}

const _: () = assert!(core::mem::size_of::<SavedRegisters>() == 120);
const _: () = assert!(core::mem::align_of::<SavedRegisters>() == 8);
const _: () = assert!(core::mem::size_of::<ReturnState>() == 64);
const _: () = assert!(core::mem::align_of::<ReturnState>() == 8);
const _: () = assert!(core::mem::offset_of!(ReturnState, rip) == 0);
const _: () = assert!(core::mem::offset_of!(ReturnState, cs) == 8);
const _: () = assert!(core::mem::offset_of!(ReturnState, rflags) == 16);
const _: () = assert!(core::mem::offset_of!(ReturnState, resume_rsp) == 24);
const _: () = assert!(core::mem::offset_of!(ReturnState, rsp) == 32);
const _: () = assert!(core::mem::offset_of!(ReturnState, ss) == 48);
const _: () = assert!(core::mem::offset_of!(InterruptedState, registers) == 0);
const _: () = assert!(core::mem::offset_of!(InterruptedState, return_state) == 120);
const _: () = assert!(core::mem::size_of::<InterruptedState>() == 184);
const _: () = assert!(core::mem::align_of::<InterruptedState>() == 8);

#[cfg(test)]
mod tests {
    use super::{InterruptedState, ReturnState};
    use crate::arch::x86_64::interrupt_entry::{InterruptReturnFrame, SavedRegisters};

    #[test]
    fn kernel_return_snapshot_is_valid() {
        let registers = SavedRegisters::default();
        let mut raw = [0u64; 3];
        raw[0] = 0x1000; raw[1] = 0x10; raw[2] = 0x202;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        let snapshot = unsafe { InterruptedState::capture(&registers, frame) };
        assert!(snapshot.is_valid());
        assert!(!snapshot.is_user_valid());
        assert_eq!(snapshot.return_state().rsp(), None);
        assert_eq!(snapshot.return_state().ss(), None);
        assert_eq!(snapshot.return_state().resume_rsp(), raw.as_mut_ptr() as u64 + 24);
        assert_eq!(snapshot.return_state().kernel_iret_words(), Some([0x1000, 0x10, 0x202]));
    }

    #[test]
    fn rejects_kernel_return_with_insufficient_resume_space() {
        let state = ReturnState { rip: 0x1000, cs: 0x10, rflags: 0x202, resume_rsp: 23, rsp: None, ss: None };
        assert!(state.kernel_iret_words().is_none());
    }

    #[test]
    fn user_return_snapshot_is_valid_only_with_user_stack_pair() {
        let state = ReturnState { rip: 0x1000, cs: 0x1b, rflags: 0x202, resume_rsp: 0x8000, rsp: Some(0x8000), ss: Some(0x23) };
        let interrupted = InterruptedState { registers: SavedRegisters::default(), return_state: state };
        assert!(!state.is_kernel());
        assert!(interrupted.is_valid());
        assert!(interrupted.is_user_valid());
        assert!(state.kernel_iret_words().is_none());
    }
}
