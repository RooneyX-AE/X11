//! Architecture-independent syscall dispatch contract.
//!
//! This layer decodes the shared ABI and validates arguments that can be
//! checked without dereferencing user memory. Architecture-specific entry
//! code can translate its register ABI into `SyscallRequest` and call `dispatch`.

use x11_os_abi::{Syscall, UserSlice};

use crate::memory::{validate_readable_range, validate_slice, PageTableMapper, UserRangeError, UserReadError};

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
    InvalidUserMemory(UserReadError),
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
    // The active address-space mapper is not attached to the runtime yet.
    // Keep the syscall non-executing until page residency and permissions are
    // available instead of dereferencing an unproven user pointer.
    Err(SyscallError::NotImplemented)
}

/// Validates the complete memory contract required before `Write` can copy
/// bytes from userspace. Actual copying is deliberately a later operation.
pub fn sys_write_checked<M: PageTableMapper>(mapper: &M, slice: UserSlice) -> SyscallResult {
    validate_readable_range(mapper, slice).map_err(SyscallError::InvalidUserMemory)?;
    Err(SyscallError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::{dispatch, sys_write_checked, SyscallError, SyscallRequest};
    use crate::memory::{KERNEL_SPACE_START, Page4K, PageAccess, PageTableMapper, UserRangeError, UserReadError, VirtRange, USER_SPACE_START};
    use x11_os_abi::{Syscall, UserSlice};

    struct Flush;
    impl crate::memory::MappingFlush for Flush {
        fn flush(self) {}
    }

    struct FakeMapper {
        access: PageAccess,
    }

    impl PageTableMapper for FakeMapper {
        type Flush = Flush;
        fn map_page(&mut self, _: Page4K, _: u64) -> Result<Self::Flush, crate::memory::MappingError> { unreachable!() }
        fn unmap_page(&mut self, _: Page4K) -> Result<(u64, Self::Flush), crate::memory::MappingError> { unreachable!() }
        fn translate(&self, _: u64) -> Option<u64> { None }
        fn page_access(&self, _: u64) -> PageAccess { self.access }
        fn address_space(&self) -> VirtRange { VirtRange::new(USER_SPACE_START, KERNEL_SPACE_START).unwrap() }
    }

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
        let request = SyscallRequest::write(UserSlice { ptr: KERNEL_SPACE_START, len: 1 });
        assert_eq!(
            dispatch(request),
            Err(SyscallError::InvalidUserRange(UserRangeError::OutsideUserSpace))
        );
    }

    #[test]
    fn checked_write_rejects_unmapped_page() {
        let mapper = FakeMapper { access: PageAccess::unmapped() };
        assert_eq!(
            sys_write_checked(&mapper, UserSlice { ptr: USER_SPACE_START, len: 1 }),
            Err(SyscallError::InvalidUserMemory(UserReadError::Unmapped))
        );
    }

    #[test]
    fn checked_write_accepts_readable_user_pages_but_stays_unimplemented() {
        let mapper = FakeMapper { access: PageAccess::user_read_only() };
        assert_eq!(
            sys_write_checked(&mapper, UserSlice { ptr: USER_SPACE_START + 1, len: 8 }),
            Err(SyscallError::NotImplemented)
        );
    }
}
