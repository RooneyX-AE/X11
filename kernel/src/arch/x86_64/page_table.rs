//! x86_64 page-table backend.

use x86_64::structures::paging::{
    mapper::{MapToError, MapperFlush, Translate, UnmapError},
    FrameAllocator as X86FrameAllocator, Mapper, Page, PageTable, PageTableFlags,
    PhysFrame, Size4KiB,
};
use x86_64::PhysAddr;

use crate::arch::x86_64::paging;
use crate::memory::{
    EarlyFrameAllocator, FrameAllocator as X11FrameAllocator, MappingError, MappingFlags,
    MappingFlush, Page4K, PageAccess, PageTableMapper, PhysRange, VirtRange,
};

pub struct X86Flush(MapperFlush<Size4KiB>);
impl MappingFlush for X86Flush { fn flush(self) { self.0.flush(); } }

struct FrameAllocatorAdapter<'allocator, 'regions> { inner: &'allocator mut EarlyFrameAllocator<'regions> }
unsafe impl X86FrameAllocator<Size4KiB> for FrameAllocatorAdapter<'_, '_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.inner.allocate_frame().and_then(|frame| PhysFrame::from_start_address(PhysAddr::new(frame.start_address())).ok())
    }
}

fn map_to_error(error: MapToError<Size4KiB>) -> MappingError {
    match error {
        MapToError::FrameAllocationFailed => MappingError::FrameAllocationFailed,
        MapToError::ParentEntryHugePage => MappingError::ParentEntryHugePage,
        MapToError::PageAlreadyMapped(_) => MappingError::AlreadyMapped,
    }
}
fn unmap_error(error: UnmapError) -> MappingError {
    match error {
        UnmapError::ParentEntryHugePage => MappingError::ParentEntryHugePage,
        UnmapError::PageNotMapped => MappingError::NotMapped,
        UnmapError::InvalidFrameAddress(_) => MappingError::InvalidMappedFrame,
    }
}

pub struct X86PageTableMapper<'allocator, 'regions> {
    inner: x86_64::structures::paging::OffsetPageTable<'static>,
    frame_allocator: FrameAllocatorAdapter<'allocator, 'regions>,
    address_space: VirtRange,
}

impl<'allocator, 'regions> X86PageTableMapper<'allocator, 'regions> {
    /// # Safety
    /// `physical_memory_offset` must be the bootloader direct-map offset.
    pub unsafe fn new(
        physical_memory_offset: u64,
        frame_allocator: &'allocator mut EarlyFrameAllocator<'regions>,
        address_space: VirtRange,
    ) -> Self {
        let inner = unsafe { paging::init(physical_memory_offset) };
        Self { inner, frame_allocator: FrameAllocatorAdapter { inner: frame_allocator }, address_space }
    }

    /// Creates a mapper over an explicitly supplied level-4 root instead of
    /// the currently active CR3 root. This is required while constructing a
    /// process address space before it becomes the active address space.
    ///
    /// # Safety
    /// `physical_memory_offset` must be a valid direct map for `root`, the root
    /// frame must be exclusively owned by this address space, and no concurrent
    /// page-table mutation may race this mapper.
    pub unsafe fn new_for_root(
        physical_memory_offset: u64,
        root: PhysFrame<Size4KiB>,
        frame_allocator: &'allocator mut EarlyFrameAllocator<'regions>,
        address_space: VirtRange,
    ) -> Result<Self, MappingError> {
        let root_virtual = root
            .start_address()
            .as_u64()
            .checked_add(physical_memory_offset)
            .ok_or(MappingError::BackendFailure)?;
        let root_pointer = root_virtual as *mut PageTable;
        let level_4_table = unsafe { &mut *root_pointer };
        let inner = unsafe {
            x86_64::structures::paging::OffsetPageTable::new(
                level_4_table,
                x86_64::VirtAddr::new(physical_memory_offset),
            )
        };
        Ok(Self { inner, frame_allocator: FrameAllocatorAdapter { inner: frame_allocator }, address_space })
    }

    fn mapped_flags(&self, virtual_address: u64) -> Option<PageTableFlags> {
        if !self.address_space.contains(virtual_address) { return None; }
        let page = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(virtual_address));
        let mut table_ptr = self.inner.level_4_table() as *const PageTable;
        let mut effective_user = true;
        let mut effective_writable = true;
        let indexes = [page.p4_index(), page.p3_index(), page.p2_index(), page.p1_index()];
        for (level, index) in indexes.into_iter().enumerate() {
            let table = unsafe { &*table_ptr };
            let entry = &table[index];
            let flags = entry.flags();
            if !flags.contains(PageTableFlags::PRESENT) { return None; }
            effective_user &= flags.contains(PageTableFlags::USER_ACCESSIBLE);
            effective_writable &= flags.contains(PageTableFlags::WRITABLE);
            if level == 3 || ((level == 1 || level == 2) && flags.contains(PageTableFlags::HUGE_PAGE)) {
                let mut result = PageTableFlags::PRESENT;
                if effective_user { result |= PageTableFlags::USER_ACCESSIBLE; }
                if effective_writable { result |= PageTableFlags::WRITABLE; }
                if flags.contains(PageTableFlags::NO_EXECUTE) { result |= PageTableFlags::NO_EXECUTE; }
                return Some(result);
            }
            let next_table = self.inner.phys_offset().as_u64().checked_add(entry.addr().as_u64())?;
            table_ptr = next_table as *const PageTable;
        }
        None
    }

    fn translate_flags(flags: MappingFlags, user: bool) -> PageTableFlags {
        let mut result = PageTableFlags::PRESENT;
        if user { result |= PageTableFlags::USER_ACCESSIBLE; }
        if flags.writable() { result |= PageTableFlags::WRITABLE; }
        if !flags.executable() { result |= PageTableFlags::NO_EXECUTE; }
        result
    }
}

impl PageTableMapper for X86PageTableMapper<'_, '_> {
    type Flush = X86Flush;

    fn allocate_frame(&mut self) -> Option<PhysRange> {
        self.frame_allocator.inner.allocate_frame().and_then(|frame| {
            PhysRange::new(frame.start_address(), frame.start_address().checked_add(4096)?).ok()
        })
    }

    fn map_page(&mut self, page: Page4K, physical_address: u64, mapping_flags: MappingFlags) -> Result<Self::Flush, MappingError> {
        let range = page.range().ok_or(MappingError::InvalidVirtualAddress)?;
        if range.start() < self.address_space.start() || range.end() > self.address_space.end() {
            return Err(MappingError::OutsideAddressSpace);
        }
        if mapping_flags.is_writable_executable() { return Err(MappingError::BackendFailure); }
        let frame = PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(physical_address)).map_err(|_| MappingError::InvalidPhysicalAddress)?;
        let target = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));
        let flags = Self::translate_flags(mapping_flags, self.address_space.is_user());
        let flush = unsafe { self.inner.map_to(target, frame, flags, &mut self.frame_allocator) }.map_err(map_to_error)?;
        Ok(X86Flush(flush))
    }

    fn unmap_page(&mut self, page: Page4K) -> Result<(u64, Self::Flush), MappingError> {
        let range = page.range().ok_or(MappingError::InvalidVirtualAddress)?;
        if range.start() < self.address_space.start() || range.end() > self.address_space.end() {
            return Err(MappingError::OutsideAddressSpace);
        }
        let target = Page::<Size4KiB>::containing_address(x86_64::VirtAddr::new(page.start_address()));
        let (frame, flush) = self.inner.unmap(target).map_err(unmap_error)?;
        Ok((frame.start_address().as_u64(), X86Flush(flush)))
    }

    fn translate(&self, virtual_address: u64) -> Option<u64> {
        if !self.address_space.contains(virtual_address) { return None; }
        self.inner.translate_addr(x86_64::VirtAddr::new(virtual_address)).map(|address| address.as_u64())
    }

    fn page_access(&self, virtual_address: u64) -> PageAccess {
        let Some(flags) = self.mapped_flags(virtual_address) else { return PageAccess::unmapped(); };
        PageAccess {
            mapped: flags.contains(PageTableFlags::PRESENT),
            user: flags.contains(PageTableFlags::USER_ACCESSIBLE),
            readable: flags.contains(PageTableFlags::PRESENT),
            writable: flags.contains(PageTableFlags::WRITABLE),
        }
    }

    fn address_space(&self) -> VirtRange { self.address_space }
}

#[cfg(test)]
mod tests {
    use super::{map_to_error, unmap_error};
    use crate::memory::{MappingError, MappingFlags};
    use x86_64::structures::paging::mapper::{MapToError, UnmapError};
    use x86_64::PhysAddr;

    #[test]
    fn flags_reject_writable_executable_mapping() {
        assert!(MappingFlags::read_write_execute().is_writable_executable());
    }
    #[test]
    fn map_error_mapping_preserves_semantics() {
        assert_eq!(map_to_error(MapToError::FrameAllocationFailed), MappingError::FrameAllocationFailed);
        assert_eq!(map_to_error(MapToError::ParentEntryHugePage), MappingError::ParentEntryHugePage);
        assert_eq!(map_to_error(MapToError::PageAlreadyMapped(x86_64::structures::paging::PhysFrame::containing_address(PhysAddr::new(0x1000)))), MappingError::AlreadyMapped);
    }
    #[test]
    fn unmap_error_mapping_preserves_semantics() {
        assert_eq!(unmap_error(UnmapError::ParentEntryHugePage), MappingError::ParentEntryHugePage);
        assert_eq!(unmap_error(UnmapError::PageNotMapped), MappingError::NotMapped);
        assert_eq!(unmap_error(UnmapError::InvalidFrameAddress(PhysAddr::new(0x1234))), MappingError::InvalidMappedFrame);
    }
}
