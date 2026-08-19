//! Page-granular user-memory validation and copy contract.
//!
//! Validation is separated from the architecture-specific read operation.
//! This keeps unsafe address translation in one backend instead of spreading
//! raw user-pointer dereferences across syscall handlers.

use super::{validate_slice, Page4K, PageAccess, UserMemoryView, UserRangeError, PAGE_SIZE_4K};
use x11_os_abi::UserSlice;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserReadError {
    InvalidRange(UserRangeError),
    PageAddressOverflow,
    InvalidPageAddress,
    Unmapped,
    NotUserAccessible,
    NotReadable,
    BackendFailure,
}

pub trait UserCopyBackend {
    fn copy_from_user(&self, src: u64, dst: &mut [u8]) -> Result<(), UserReadError>;
}

pub fn validate_readable_range<M: UserMemoryView>(
    mapper: &M,
    slice: UserSlice,
) -> Result<(), UserReadError> {
    let range = validate_slice(slice).map_err(UserReadError::InvalidRange)?;
    if range.is_empty() {
        return Ok(());
    }

    let mut page_start = range.start() / PAGE_SIZE_4K * PAGE_SIZE_4K;
    loop {
        let page = Page4K::from_start_address(page_start)
            .ok_or(UserReadError::InvalidPageAddress)?;
        match mapper.page_access(page.start_address()) {
            PageAccess { mapped: false, .. } => return Err(UserReadError::Unmapped),
            PageAccess { mapped: true, user: false, .. } => return Err(UserReadError::NotUserAccessible),
            PageAccess { mapped: true, user: true, readable: false, .. } => return Err(UserReadError::NotReadable),
            PageAccess { mapped: true, user: true, readable: true, .. } => {}
        }

        let next = page_start
            .checked_add(PAGE_SIZE_4K)
            .ok_or(UserReadError::PageAddressOverflow)?;
        if next >= range.end() { break; }
        page_start = next;
    }

    Ok(())
}

pub fn copy_from_user<M: UserMemoryView, B: UserCopyBackend>(
    mapper: &M,
    backend: &B,
    slice: UserSlice,
    dst: &mut [u8],
) -> Result<(), UserReadError> {
    if slice.len != dst.len() as u64 {
        return Err(UserReadError::InvalidRange(UserRangeError::AddressOverflow));
    }
    validate_readable_range(mapper, slice)?;
    backend.copy_from_user(slice.ptr, dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{KERNEL_SPACE_START, PageTableMapper, MappingFlags, MappingFlush, MappingError, USER_SPACE_START, VirtRange};

    struct Flush;
    impl MappingFlush for Flush { fn flush(self) {} }

    struct FakeMapper { access: PageAccess }
    impl UserMemoryView for FakeMapper {
        fn translate(&self, _: u64) -> Option<u64> { None }
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
    impl UserCopyBackend for FakeBackend {
        fn copy_from_user(&self, _: u64, dst: &mut [u8]) -> Result<(), UserReadError> {
            dst.fill(0x41);
            Ok(())
        }
    }

    #[test]
    fn accepts_readable_user_page() {
        let mapper = FakeMapper { access: PageAccess::user_read_only() };
        assert!(validate_readable_range(&mapper, UserSlice { ptr: USER_SPACE_START + 1, len: 8 }).is_ok());
    }
    #[test]
    fn rejects_unmapped_page() {
        let mapper = FakeMapper { access: PageAccess::unmapped() };
        assert_eq!(validate_readable_range(&mapper, UserSlice { ptr: USER_SPACE_START, len: 1 }), Err(UserReadError::Unmapped));
    }
    #[test]
    fn rejects_kernel_only_mapping() {
        let mapper = FakeMapper { access: PageAccess { mapped: true, user: false, readable: true, writable: true, executable: false } };
        assert_eq!(validate_readable_range(&mapper, UserSlice { ptr: USER_SPACE_START, len: 1 }), Err(UserReadError::NotUserAccessible));
    }
    #[test]
    fn rejects_non_readable_user_mapping() {
        let mapper = FakeMapper { access: PageAccess { mapped: true, user: true, readable: false, writable: true, executable: false } };
        assert_eq!(validate_readable_range(&mapper, UserSlice { ptr: USER_SPACE_START, len: 1 }), Err(UserReadError::NotReadable));
    }
    #[test]
    fn empty_slice_requires_no_mapping() {
        let mapper = FakeMapper { access: PageAccess::unmapped() };
        assert!(validate_readable_range(&mapper, UserSlice::empty()).is_ok());
    }
    #[test]
    fn copy_requires_destination_length_match() {
        let mapper = FakeMapper { access: PageAccess::user_read_only() };
        let backend = FakeBackend;
        let mut dst = [0u8; 4];
        assert!(matches!(copy_from_user(&mapper, &backend, UserSlice { ptr: USER_SPACE_START, len: 8 }, &mut dst), Err(UserReadError::InvalidRange(_))));
    }
    #[test]
    fn copy_uses_backend_only_after_validation() {
        let mapper = FakeMapper { access: PageAccess::user_read_only() };
        let backend = FakeBackend;
        let mut dst = [0u8; 4];
        copy_from_user(&mapper, &backend, UserSlice { ptr: USER_SPACE_START, len: 4 }, &mut dst).unwrap();
        assert_eq!(dst, [0x41; 4]);
    }
    #[test]
    fn copy_rejects_unmapped_before_backend() {
        let mapper = FakeMapper { access: PageAccess::unmapped() };
        let backend = FakeBackend;
        let mut dst = [0u8; 4];
        assert_eq!(copy_from_user(&mapper, &backend, UserSlice { ptr: USER_SPACE_START, len: 4 }, &mut dst), Err(UserReadError::Unmapped));
    }
}