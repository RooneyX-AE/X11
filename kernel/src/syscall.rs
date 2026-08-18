//! Architecture-independent syscall dispatch contract.
//!
//! This layer decodes the shared ABI and validates arguments that can be
//! checked without dereferencing user memory. Architecture-specific entry
//! code can translate its register ABI into `SyscallRequest` and call `dispatch`.

use x11_os_abi::{Syscall, UserSlice};

use crate::memory::{validate_slice, UserRangeError};

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

    pub const fn user_slice(self) -> UserSlice {
        UserSlice { ptr: self.arg0, len: self.arg1 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallError {
    UnknownNumber,
    NotImplemented,
    InvalidArguments,
    InvalidUserRange(UserRangeError),
}

pub type SyscallResult = Result<u64, SyscallError>;

pub fn dispatch(request: SyscallRequest) -> SyscallResult {
    match request.syscall().ok_or(SyscallError::UnknownNumber)? {
        Syscall::Write => sys_write(request.user_slice()),
        Syscall::Exit | Syscall::Yield => Err(SyscallError::NotImplemented),
    }
}

fn sys_write(slice: UserSlice) -> SyscallResult {
    validate_slice(slice).map_err(SyscallError::InvalidUserRange)?;
    // The range is validated, but the active address space is not yet able to
    // prove that every page is mapped and readable. Do not dereference it here.
    Err(SyscallError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::{dispatch, SyscallError, SyscallRequest};
    use crate::memory::UserRangeError;
    use x11_os_abi::{Syscall, UserSlice};

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
    fn write_rejects_kernel_address_before_dereference() {
        let request = SyscallRequest::write(UserSlice { ptr: crate::memory::KERNEL_SPACE_START, len: 1 });
        assert_eq!(
            dispatch(request),
            Err(SyscallError::InvalidUserRange(UserRangeError::OutsideUserSpace))
        );
    }

    #[test]
    fn known_write_in_user_range_is_not_implemented_until_page_validation_exists() {
        let request = SyscallRequest::write(UserSlice { ptr: crate::memory::USER_SPACE_START, len: 1 });
        assert_eq!(dispatch(request), Err(SyscallError::NotImplemented));
    }
}
