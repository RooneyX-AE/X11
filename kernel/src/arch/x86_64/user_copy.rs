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
