//! Architecture-independent syscall dispatch contract.

use x11_os_abi::{Syscall, UserSlice};

use crate::memory::{copy_from_user, validate_slice, UserCopyBackend, UserMemoryView, UserRangeError, UserReadError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyscallRequest {
    pub number: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
}

impl SyscallRequest {
    pub const fn new(number: u64, arg0: u64, arg1: u64, arg2: u64) -> Self { Self { number, arg0, arg1, arg2 } }
    pub const fn write(slice: UserSlice) -> Self { Self::new(Syscall::Write.number(), slice.ptr, slice.len, 0) }
    pub const fn syscall(self) -> Option<Syscall> { Syscall::from_number(self.number) }
    pub const fn user_slice(self) -> UserSlice { UserSlice { ptr: self.arg0, len: self.arg1 } }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallError {
    UnknownNumber,
    NotImplemented,
    InvalidArguments,
    InvalidUserRange(UserRangeError),
    InvalidUserMemory(UserReadError),
    WriteFailed,
}

pub type SyscallResult = Result<u64, SyscallError>;

pub trait WriteSink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), SyscallError>;
}

pub fn dispatch(request: SyscallRequest) -> SyscallResult {
    validate_slice(request.user_slice()).map_err(SyscallError::InvalidUserRange)?;
    match request.syscall().ok_or(SyscallError::UnknownNumber)? {
        Syscall::Write => Err(SyscallError::NotImplemented),
        Syscall::Exit | Syscall::Yield => Err(SyscallError::NotImplemented),
    }
}

pub fn dispatch_with_memory<M, B, S>(
    request: SyscallRequest,
    mapper: &M,
    backend: &B,
    sink: &mut S,
) -> SyscallResult
where
    M: UserMemoryView,
    B: UserCopyBackend,
    S: WriteSink,
{
    match request.syscall().ok_or(SyscallError::UnknownNumber)? {
        Syscall::Write => sys_write_with_memory(request.user_slice(), mapper, backend, sink),
        Syscall::Exit | Syscall::Yield => Err(SyscallError::NotImplemented),
    }
}

fn sys_write_with_memory<M, B, S>(
    slice: UserSlice,
    mapper: &M,
    backend: &B,
    sink: &mut S,
) -> SyscallResult
where
    M: UserMemoryView,
    B: UserCopyBackend,
    S: WriteSink,
{
    validate_slice(slice).map_err(SyscallError::InvalidUserRange)?;
    if slice.is_empty() { return Ok(0); }

    let mut buffer = [0u8; 256];
    let mut offset = 0u64;
    while offset < slice.len {
        let remaining = slice.len - offset;
        let chunk_len = core::cmp::min(remaining, buffer.len() as u64) as usize;
        let chunk_ptr = slice.ptr.checked_add(offset).ok_or(SyscallError::InvalidUserRange(UserRangeError::AddressOverflow))?;
        let chunk = UserSlice { ptr: chunk_ptr, len: chunk_len as u64 };
        copy_from_user(mapper, backend, chunk, &mut buffer[..chunk_len])
            .map_err(SyscallError::InvalidUserMemory)?;
        sink.write(&buffer[..chunk_len])?;
        offset = offset.checked_add(chunk_len as u64).ok_or(SyscallError::InvalidArguments)?;
    }

    Ok(slice.len)
}

#[cfg(test)]
mod tests {
    use super::{dispatch, dispatch_with_memory, SyscallError, SyscallRequest, WriteSink};
    use crate::memory::{KERNEL_SPACE_START, MappingFlags, MappingFlush, MappingError, Page4K, PageAccess, PageTableMapper, UserMemoryView, UserRangeError, UserReadError, VirtRange, USER_SPACE_START};
    use x11_os_abi::{Syscall, UserSlice};

    struct Flush;
    impl MappingFlush for Flush { fn flush(self) {} }

    struct FakeMapper { access: PageAccess }
    impl UserMemoryView for FakeMapper {
        fn translate(&self, _: u64) -> Option<u64> { Some(0x1000) }
        fn page_access(&self, _: u64) -> PageAccess { self.access }
        fn address_space(&self) -> VirtRange { VirtRange::new(USER_SPACE_START, KERNEL_SPACE_START).unwrap() }
    }
    impl PageTableMapper for FakeMapper {
        type Flush = Flush;
        fn allocate_frame(&mut self) -> Option<u64> { None }
        fn map_page(&mut self, _: Page4K, _: u64, _: MappingFlags) -> Result<Self::Flush, MappingError> { unreachable!() }
        fn unmap_page(&mut self, _: Page4K) -> Result<(u64, Self::Flush), MappingError> { unreachable!() }
    }

    struct FakeBackend;
    impl crate::memory::UserCopyBackend for FakeBackend {
        fn copy_from_user(&self, _: u64, dst: &mut [u8]) -> Result<(), UserReadError> {
            dst.fill(b'A');
            Ok(())
        }
    }

    #[derive(Default)]
    struct Sink(Vec<u8>);
    impl WriteSink for Sink {
        fn write(&mut self, bytes: &[u8]) -> Result<(), SyscallError> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
    }

    struct FailingSink;
    impl WriteSink for FailingSink {
        fn write(&mut self, _: &[u8]) -> Result<(), SyscallError> { Err(SyscallError::WriteFailed) }
    }

    #[test]
    fn decodes_shared_syscall_numbers() {
        assert_eq!(SyscallRequest::new(Syscall::Write.number(), 1, 2, 3).syscall(), Some(Syscall::Write));
        assert_eq!(SyscallRequest::new(Syscall::Exit.number(), 0, 0, 0).syscall(), Some(Syscall::Exit));
        assert_eq!(SyscallRequest::new(Syscall::Yield.number(), 0, 0, 0).syscall(), Some(Syscall::Yield));
    }

    #[test]
    fn rejects_unknown_syscall_numbers() {
        assert_eq!(dispatch(SyscallRequest::new(0xffff, 0, 0, 0)), Err(SyscallError::UnknownNumber));
    }

    #[test]
    fn write_rejects_kernel_address_before_dereference() {
        assert_eq!(dispatch(SyscallRequest::write(UserSlice { ptr: KERNEL_SPACE_START, len: 1 })), Err(SyscallError::InvalidUserRange(UserRangeError::OutsideUserSpace)));
    }

    #[test]
    fn write_executes_through_validated_copy_and_sink() {
        let mapper = FakeMapper { access: PageAccess::user_read_only() };
        let backend = FakeBackend;
        let mut sink = Sink::default();
        let result = dispatch_with_memory(SyscallRequest::write(UserSlice { ptr: USER_SPACE_START + 1, len: 300 }), &mapper, &backend, &mut sink);
        assert_eq!(result, Ok(300));
        assert_eq!(sink.0.len(), 300);
        assert!(sink.0.iter().all(|byte| *byte == b'A'));
    }

    #[test]
    fn write_propagates_sink_failure_without_claiming_success() {
        let mapper = FakeMapper { access: PageAccess::user_read_only() };
        let backend = FakeBackend;
        let mut sink = FailingSink;
        assert_eq!(
            dispatch_with_memory(SyscallRequest::write(UserSlice { ptr: USER_SPACE_START, len: 4 }), &mapper, &backend, &mut sink),
            Err(SyscallError::WriteFailed)
        );
    }

    #[test]
    fn write_rejects_unmapped_before_sink() {
        let mapper = FakeMapper { access: PageAccess::unmapped() };
        let backend = FakeBackend;
        let mut sink = Sink::default();
        assert_eq!(dispatch_with_memory(SyscallRequest::write(UserSlice { ptr: USER_SPACE_START, len: 4 }), &mapper, &backend, &mut sink), Err(SyscallError::InvalidUserMemory(UserReadError::Unmapped)));
        assert!(sink.0.is_empty());
    }
}
