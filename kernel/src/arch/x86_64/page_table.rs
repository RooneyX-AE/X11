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

impl X86FrameAllocator<Size4KiB> for FrameAllocatorAdapter<'_, '_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.inner.allocate_frame().and_then(|frame| {
            PhysFrame::from_start_address(PhysAddr::new(frame.start_address())).ok()
        })
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

        if self.inner.translate(target.start_address()).is_ok() {
            return Err(MappingError::AlreadyMapped);
        }

        // SAFETY: The target page is validated by the kernel address-space
        // policy, the backing frame is page-aligned, and the allocator only
        // yields usable 4 KiB frames.
        let result = unsafe {
            self.inner.map_to(
                target,
                frame,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                &mut self.frame_allocator,
            )
        };

        let flush = match result {
            Ok(flush) => flush,
            Err(MapToError::FrameAllocationFailed) => {
                return Err(MappingError::FrameAllocationFailed)
            }
            Err(MapToError::ParentEntryHugePage) => {
                return Err(MappingError::ParentEntryHugePage)
            }
            Err(MapToError::PageAlreadyMapped(_)) => return Err(MappingError::AlreadyMapped),
        };

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
        let result = self.inner.unmap(target);

        let (frame, flush) = match result {
            Ok(result) => result,
            Err(UnmapError::PageNotMapped) => return Err(MappingError::NotMapped),
            Err(UnmapError::ParentEntryHugePage) => {
                return Err(MappingError::ParentEntryHugePage)
            }
            Err(UnmapError::InvalidFrameAddress(_)) => {
                return Err(MappingError::InvalidMappedFrame)
            }
        };

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

    fn address_space(&self) -> VirtRange {
        self.address_space
    }
}
