//! x86_64 page-table backend.
//!
//! This module owns the architecture-specific translation from the kernel
//! memory contract to the `x86_64` crate's mapper API. The generic memory
//! subsystem does not depend on the architecture crate.

use x86_64::structures::paging::{
    mapper::{MapToError, MapperFlush, Translate, UnmapError},
    FrameAllocator as X86FrameAllocator,
    Mapper,
    Page,
    PageTable,
    PageTableFlags,
    PhysFrame,
    Size4KiB,
};
use x86_64::PhysAddr;

use crate::arch::x86_64::paging;
use crate::memory::{
    EarlyFrameAllocator,
    FrameAllocator as X11FrameAllocator,
    MappingError,
    MappingFlush,
    Page4K,
    PageAccess,
    PageTableMapper,
    VirtRange,
};

/// TLB flush token produced by an x86_64 mapping operation.
pub struct X86Flush(MapperFlush<Size4KiB>);

impl MappingFlush for X86Flush {
    fn flush(self) {
        self.0.flush();
    }
}

struct FrameAllocatorAdapter<'allocator, 'regions> {
    inner: &'allocator mut EarlyFrameAllocator<'regions>,
}

unsafe impl X86FrameAllocator<Size4KiB> for FrameAllocatorAdapter<'_, '_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.inner.allocate_frame().and_then(|frame| {
            PhysFrame::from_start_address(PhysAddr::new(frame.start_address())).ok()
        })
    }
}

/// Converts an architecture mapping error into the kernel mapping contract.
fn map_to_error(error: MapToError<Size4KiB>) -> MappingError {
    match error {
        MapToError::FrameAllocationFailed => MappingError::FrameAllocationFailed,
        MapToError::ParentEntryHugePage => MappingError::ParentEntryHugePage,
        MapToError::PageAlreadyMapped(_) => MappingError::AlreadyMapped,
    }
}

/// Converts an architecture unmapping error into the kernel mapping contract.
fn unmap_error(error: UnmapError) -> MappingError {
    match error {
        UnmapError::ParentEntryHugePage => MappingError::ParentEntryHugePage,
        UnmapError::PageNotMapped => MappingError::NotMapped,
        UnmapError::InvalidFrameAddress(_) => MappingError::InvalidMappedFrame,
    }
}

/// Page-table adapter backed by the active x86_64 page table.
pub struct X86PageTableMapper<'allocator, 'regions> {
    inner: x86_64::structures::paging::OffsetPageTable<'static>,
    frame_allocator: FrameAllocatorAdapter<'allocator, 'regions>,
    address_space: VirtRange,
}

impl<'allocator, 'regions> X86PageTableMapper<'allocator, 'regions> {
    /// # Safety
    ///
    /// `physical_memory_offset` must be the bootloader-provided direct-map
    /// offset, and the active page table must remain uniquely owned by this
    /// mapper for the lifetime of the returned value.
    pub unsafe fn new(
        physical_memory_offset: u64,
        frame_allocator: &'allocator mut EarlyFrameAllocator<'regions>,
        address_space: VirtRange,
    ) -> Self {
        // SAFETY: The caller provides the bootloader-established direct map and
        // guarantees this is the unique mutable owner of the active table.
        let inner = unsafe { paging::init(physical_memory_offset) };
        Self {
            inner,
            frame_allocator: FrameAllocatorAdapter { inner: frame_allocator },
            address_space,
        }
    }

    fn mapped_flags(&self, virtual_address: u64) -> Option<PageTableFlags> {
        if !self.address_space.contains(virtual_address) {
            return None;
        }

        let page = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(virtual_address));
        let mut table_ptr = self.inner.level_4_table() as *const PageTable;
        let mut effective_user = true;
        let mut effective_writable = true;
        let indexes = [page.p4_index(), page.p3_index(), page.p2_index(), page.p1_index()];

        for (level, index) in indexes.into_iter().enumerate() {
            // SAFETY: `OffsetPageTable` was constructed from a valid root table
            // and a bootloader-established complete physical mapping. Every
            // next-level address comes from a PRESENT page-table entry, so the
            // direct-map conversion below points at the corresponding table.
            let table = unsafe { &*table_ptr };
            let entry = &table[index];
            let flags = entry.flags();
            if !flags.contains(PageTableFlags::PRESENT) {
                return None;
            }

            effective_user &= flags.contains(PageTableFlags::USER_ACCESSIBLE);
            effective_writable &= flags.contains(PageTableFlags::WRITABLE);

            if level == 3 || ((level == 1 || level == 2) && flags.contains(PageTableFlags::HUGE_PAGE)) {
                let mut result = PageTableFlags::PRESENT;
                if effective_user {
                    result |= PageTableFlags::USER_ACCESSIBLE;
                }
                if effective_writable {
                    result |= PageTableFlags::WRITABLE;
                }
                return Some(result);
            }

            let next_table = self.inner.phys_offset().as_u64().checked_add(entry.addr().as_u64())?;
            table_ptr = next_table as *const PageTable;
        }

        let mut result = PageTableFlags::PRESENT;
        if effective_user {
            result |= PageTableFlags::USER_ACCESSIBLE;
        }
        if effective_writable {
            result |= PageTableFlags::WRITABLE;
        }
        Some(result)
    }
}

impl PageTableMapper for X86PageTableMapper<'_, '_> {
    type Flush = X86Flush;

    fn map_page(
        &mut self,
        page: Page4K,
        physical_address: u64,
    ) -> Result<Self::Flush, MappingError> {
        let virtual_range = page.range().ok_or(MappingError::InvalidVirtualAddress)?;
        if virtual_range.start() < self.address_space.start()
            || virtual_range.end() > self.address_space.end()
        {
            return Err(MappingError::OutsideAddressSpace);
        }

        let frame = PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(physical_address))
            .map_err(|_| MappingError::InvalidPhysicalAddress)?;
        let target =
            Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        if self.address_space.is_user() {
            flags |= PageTableFlags::USER_ACCESSIBLE;
        }

        // SAFETY: The target page is validated by the kernel address-space
        // policy, the backing frame is page-aligned, and the allocator only
        // yields usable 4 KiB frames.
        let flush = unsafe {
            self.inner.map_to(
                target,
                frame,
                flags,
                &mut self.frame_allocator,
            )
        }
        .map_err(map_to_error)?;

        Ok(X86Flush(flush))
    }

    fn unmap_page(&mut self, page: Page4K) -> Result<(u64, Self::Flush), MappingError> {
        let virtual_range = page.range().ok_or(MappingError::InvalidVirtualAddress)?;
        if virtual_range.start() < self.address_space.start()
            || virtual_range.end() > self.address_space.end()
        {
            return Err(MappingError::OutsideAddressSpace);
        }

        let target =
            Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));
        let (frame, flush) = self.inner.unmap(target).map_err(unmap_error)?;
        Ok((frame.start_address().as_u64(), X86Flush(flush)))
    }

    fn translate(&self, virtual_address: u64) -> Option<u64> {
        if !self.address_space.contains(virtual_address) {
            return None;
        }

        self.inner
            .translate_addr(x86_64::VirtAddr::new(virtual_address))
            .map(|address| address.as_u64())
    }

    fn page_access(&self, virtual_address: u64) -> PageAccess {
        let Some(flags) = self.mapped_flags(virtual_address) else {
            return PageAccess::unmapped();
        };
        PageAccess {
            mapped: flags.contains(PageTableFlags::PRESENT),
            user: flags.contains(PageTableFlags::USER_ACCESSIBLE),
            readable: flags.contains(PageTableFlags::PRESENT),
            writable: flags.contains(PageTableFlags::WRITABLE),
        }
    }

    fn address_space(&self) -> VirtRange {
        self.address_space
    }
}

#[cfg(test)]
mod tests {
    use x86_64::structures::paging::mapper::{MapToError, UnmapError};
    use x86_64::PhysAddr;

    use super::{map_to_error, unmap_error};
    use crate::memory::MappingError;

    #[test]
    fn map_error_mapping_preserves_semantics() {
        assert_eq!(
            map_to_error(MapToError::FrameAllocationFailed),
            MappingError::FrameAllocationFailed
        );
        assert_eq!(
            map_to_error(MapToError::ParentEntryHugePage),
            MappingError::ParentEntryHugePage
        );
        assert_eq!(
            map_to_error(MapToError::PageAlreadyMapped(
                x86_64::structures::paging::PhysFrame::containing_address(PhysAddr::new(0x1000))
            )),
            MappingError::AlreadyMapped
        );
    }

    #[test]
    fn unmap_error_mapping_preserves_semantics() {
        assert_eq!(
            unmap_error(UnmapError::ParentEntryHugePage),
            MappingError::ParentEntryHugePage
        );
        assert_eq!(
            unmap_error(UnmapError::PageNotMapped),
            MappingError::NotMapped
        );
        assert_eq!(
            unmap_error(UnmapError::InvalidFrameAddress(PhysAddr::new(0x1234))),
            MappingError::InvalidMappedFrame
        );
    }
}
