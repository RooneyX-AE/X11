//! Architecture-independent syscall dispatch contract.
//!
//! This layer deliberately stops before touching CPU register frames or raw
//! user pointers. Architecture-specific entry code can later translate its
//! register ABI into `SyscallRequest` and call `dispatch` here.

use x11_os_abi::{Syscall, UserSlice};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyscallRequest {
    pub number: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
}

impl SyscallRequest {
    pub const fn new(number: u64, arg0: u64, arg1: u64, arg2: u64) -> Self {
        Self { number, arg0, arg1, arg2 }
    }

    pub const fn write(slice: UserSlice) -> Self {
        Self::new(Syscall::Write.number(), slice.ptr, slice.len, 0)
    }

    pub const fn syscall(self) -> Option<Syscall> {
        match self.number {
            0 => Some(Syscall::Write),
            1 => Some(Syscall::Exit),
            2 => Some(Syscall::Yield),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallError {
    UnknownNumber,
    NotImplemented,
    InvalidArguments,
}

pub type SyscallResult = Result<u64, SyscallError>;

pub fn dispatch(request: SyscallRequest) -> SyscallResult {
    match request.syscall().ok_or(SyscallError::UnknownNumber)? {
        Syscall::Write | Syscall::Exit | Syscall::Yield => Err(SyscallError::NotImplemented),
    }
}

#[cfg(test)]
mod tests {
    use super::{dispatch, SyscallError, SyscallRequest};
    use x11_os_abi::Syscall;

    #[test]
    fn decodes_shared_syscall_numbers() {
        assert_eq!(
            SyscallRequest::new(Syscall::Write.number(), 1, 2, 3).syscall(),
            Some(Syscall::Write)
        );
        assert_eq!(
            SyscallRequest::new(Syscall::Exit.number(), 0, 0, 0).syscall(),
            Some(Syscall::Exit)
        );
        assert_eq!(
            SyscallRequest::new(Syscall::Yield.number(), 0, 0, 0).syscall(),
            Some(Syscall::Yield)
        );
    }

    #[test]
    fn rejects_unknown_syscall_numbers() {
        assert_eq!(
            SyscallRequest::new(0xffff, 0, 0, 0).syscall(),
            None
        );
        assert_eq!(
            dispatch(SyscallRequest::new(0xffff, 0, 0, 0)),
            Err(SyscallError::UnknownNumber)
        );
    }

    #[test]
    fn known_syscalls_are_explicitly_not_implemented_yet() {
        assert_eq!(
            dispatch(SyscallRequest::new(Syscall::Write.number(), 0, 0, 0)),
            Err(SyscallError::NotImplemented)
        );
    }
}
