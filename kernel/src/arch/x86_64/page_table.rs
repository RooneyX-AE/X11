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
        let target = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));

        // SAFETY: `frame_allocator` yields owned, 4 KiB-aligned frames and
        // `inner` uniquely owns the active page-table mapping for this adapter.
        let flush = unsafe {
            self.inner
                .map_to(target, frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE, &mut self.frame_allocator)
                .map_err(map_to_error)?
        };
        Ok(X86Flush(flush))
    }

    fn unmap_page(&mut self, page: Page4K) -> Result<Self::Flush, MappingError> {
        let target = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));
        // SAFETY: The caller owns this mapper and the target page is within the
        // mapper's kernel address-space contract.
        let (frame, flush) = unsafe { self.inner.unmap(target).map_err(unmap_error)? };
        let _ = frame;
        Ok(X86Flush(flush))
    }

    fn translate_page(&self, page: Page4K) -> Option<u64> {
        let target = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));
        self.inner.translate_addr(target.start_address()).map(|address| address.as_u64())
    }
}
