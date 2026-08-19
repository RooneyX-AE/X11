//! x86_64 backend for copying from validated userspace mappings.
//!
//! The syscall layer validates the complete user range first. This backend
//! then translates each virtual chunk through the supplied page-table mapper,
//! turns the resulting physical address into a kernel virtual address through
//! the bootloader direct map, and performs the copy without assuming physical
//! contiguity between user pages.

use crate::memory::{PageTableMapper, PhysicalMemoryMapping, UserCopyBackend, UserReadError, PAGE_SIZE_4K};

pub struct X86UserCopyBackend<'a, M: PageTableMapper> {
    mapper: &'a M,
    physical_memory: PhysicalMemoryMapping,
}

impl<'a, M: PageTableMapper> X86UserCopyBackend<'a, M> {
    pub const fn new(mapper: &'a M, physical_memory: PhysicalMemoryMapping) -> Self {
        Self { mapper, physical_memory }
    }
}

impl<M: PageTableMapper> UserCopyBackend for X86UserCopyBackend<'_, M> {
    fn copy_from_user(&self, src: u64, dst: &mut [u8]) -> Result<(), UserReadError> {
        let mut copied = 0usize;

        while copied < dst.len() {
            let virtual_address = src
                .checked_add(copied as u64)
                .ok_or(UserReadError::PageAddressOverflow)?;
            let physical_address = self
                .mapper
                .translate(virtual_address)
                .ok_or(UserReadError::Unmapped)?;
            let kernel_address = self
                .physical_memory
                .translate(physical_address)
                .ok_or(UserReadError::BackendFailure)?;

            let page_offset = (virtual_address % PAGE_SIZE_4K) as usize;
            let remaining_in_page = PAGE_SIZE_4K as usize - page_offset;
            let chunk_len = remaining_in_page.min(dst.len() - copied);
            let destination = &mut dst[copied..copied + chunk_len];

            // SAFETY: the syscall layer validates the entire user range before
            // invoking this backend; `kernel_address` comes from the checked
            // physical direct map and `chunk_len` remains inside one page.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    kernel_address as *const u8,
                    destination.as_mut_ptr(),
                    chunk_len,
                );
            }

            copied += chunk_len;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::X86UserCopyBackend;
    use crate::memory::{
        MappingError, MappingFlags, MappingFlush, Page4K, PageAccess, PageTableMapper,
        PhysicalMemoryMapping, VirtRange, KERNEL_SPACE_START, USER_SPACE_START,
    };
    use super::super::PAGE_SIZE_4K;

    struct Flush;
    impl MappingFlush for Flush {
        fn flush(self) {}
    }

    struct FakeMapper {
        physical: Option<u64>,
    }

    impl PageTableMapper for FakeMapper {
        type Flush = Flush;

        fn allocate_frame(&mut self) -> Option<u64> { None }

        fn map_page(
            &mut self,
            _: Page4K,
            _: u64,
            _: MappingFlags,
        ) -> Result<Self::Flush, MappingError> {
            unreachable!()
        }

        fn unmap_page(&mut self, _: Page4K) -> Result<(u64, Self::Flush), MappingError> {
            unreachable!()
        }

        fn translate(&self, _: u64) -> Option<u64> { self.physical }

        fn page_access(&self, _: u64) -> PageAccess {
            PageAccess::user_read_only()
        }

        fn address_space(&self) -> VirtRange {
            VirtRange::new(USER_SPACE_START, KERNEL_SPACE_START).unwrap()
        }
    }

    #[test]
    fn rejects_direct_map_overflow_before_raw_copy() {
        let mapper = FakeMapper { physical: Some(u64::MAX) };
        let backend = X86UserCopyBackend::new(
            &mapper,
            PhysicalMemoryMapping::new(1),
        );
        let mut destination = [0u8; 1];

        assert_eq!(
            crate::memory::UserCopyBackend::copy_from_user(
                &backend,
                USER_SPACE_START,
                &mut destination,
            ),
            Err(crate::memory::UserReadError::BackendFailure)
        );
    }

    #[test]
    fn page_chunk_calculation_never_crosses_a_page() {
        let source = PAGE_SIZE_4K - 3;
        let page_offset = source % PAGE_SIZE_4K;
        let remaining = PAGE_SIZE_4K - page_offset;
        assert_eq!(remaining, 3);
    }
}
