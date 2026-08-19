//! Validated syscall return policy.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallReturnAction {
    Return,
    Reschedule,
    Terminate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyscallDispatch {
    pub value: Result<u64, crate::syscall::SyscallError>,
    pub action: SyscallReturnAction,
}

impl SyscallDispatch {
    pub const fn returned(value: Result<u64, crate::syscall::SyscallError>) -> Self {
        Self { value, action: SyscallReturnAction::Return }
    }

    pub const fn reschedule(value: u64) -> Self {
        Self { value: Ok(value), action: SyscallReturnAction::Reschedule }
    }

    pub const fn terminate(value: u64) -> Self {
        Self { value: Ok(value), action: SyscallReturnAction::Terminate }
    }
}
