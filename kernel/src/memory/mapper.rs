//! First concrete x86_64 page-table adapter.
//!
//! This adapter translates the kernel-owned mapping contract into the
//! `x86_64` crate's mapper API. Architecture-specific TLB and page-table
//! details remain contained here.

use x86_64::structures::paging::{Mapper, Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

use super::page::Page4K;
use super::page_table::{MappingError, MappingFlush, PageTableMapper};
use super::virtual::VirtRange;
use crate::arch::x86_64::paging;

/// TLB flush token returned by a concrete mapping operation.
pub struct X86Flush(x86_64::structures::paging::mapper::MapperFlush<Size4KiB>);

impl MappingFlush for X86Flush {
    fn flush(self) {
        self.0.flush();
    }
}

/// Kernel page-table adapter backed by the active x86_64 page table.
pub struct X86PageTableMapper {
    inner: x86_64::structures::paging::OffsetPageTable<'static>,
    address_space: VirtRange,
}

impl X86PageTableMapper {
    /// # Safety
    ///
    /// The supplied offset must come from `BootInfo.physical_memory_offset`
    /// after the bootloader established the complete physical-memory mapping.
    /// The mapper must be initialized only once for its underlying active page
    /// table.
    pub unsafe fn new(physical_memory_offset: u64, address_space: VirtRange) -> Self {
        // SAFETY: The caller upholds the direct-map and single-owner invariants
        // required by the architecture backend.
        let inner = unsafe { paging::init(physical_memory_offset) };
        Self {
            inner,
            address_space,
        }
    }
}

impl PageTableMapper for X86PageTableMapper {
    type Flush = X86Flush;

    fn map_page(&mut self, page: Page4K, physical_address: u64) -> Result<Self::Flush, MappingError> {
        let virtual_range = page.range().ok_or(MappingError::InvalidPhysicalAddress)?;
        if !self.address_space.contains(virtual_range.start()) {
            return Err(MappingError::OutsideAddressSpace);
        }
        let frame = PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(physical_address))
            .map_err(|_| MappingError::InvalidPhysicalAddress)?;
        let target = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));

        // SAFETY: `inner` owns the active page-table root and `frame` is a
        // validated 4 KiB physical frame. Flags intentionally remain minimal
        // until higher-level protection policy is introduced.
        let flush = unsafe {
            self.inner
                .map_to(target, frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE, &mut NullFrameAllocator)
        }
        .map_err(|_| MappingError::BackendFailure)?;
        Ok(X86Flush(flush))
    }

    fn unmap_page(&mut self, page: Page4K) -> Result<(u64, Self::Flush), MappingError> {
        let target = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));
        let (frame, flush) = self.inner.unmap(target).map_err(|_| MappingError::NotMapped)?;
        Ok((frame.start_address().as_u64(), X86Flush(flush)))
    }

    fn translate(&self, virtual_address: u64) -> Option<u64> {
        self.inner
            .translate(x86_64::VirtAddr::new(virtual_address))
            .map(|address| address.as_u64())
    }

    fn address_space(&self) -> VirtRange {
        self.address_space
    }
}

struct NullFrameAllocator;

impl x86_64::structures::paging::FrameAllocator<Size4KiB> for NullFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        None
    }
}
