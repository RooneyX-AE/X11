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

#[cfg(test)]
mod tests {
    use super::{SyscallDispatch, SyscallReturnAction};

    #[test]
    fn lifecycle_actions_are_distinct() {
        assert_eq!(SyscallDispatch::returned(Ok(7)).action, SyscallReturnAction::Return);
        assert_eq!(SyscallDispatch::reschedule(0).action, SyscallReturnAction::Reschedule);
        assert_eq!(SyscallDispatch::terminate(0).action, SyscallReturnAction::Terminate);
    }

    #[test]
    fn terminal_actions_never_encode_an_error() {
        assert!(SyscallDispatch::reschedule(0).value.is_ok());
        assert!(SyscallDispatch::terminate(0).value.is_ok());
    }
}
