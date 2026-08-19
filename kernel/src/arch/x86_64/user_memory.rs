//! Read-only view of the currently active x86_64 address space.

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{Page, PageTable, PageTableFlags, Size4KiB};
use x86_64::VirtAddr;

use crate::memory::{PageAccess, UserMemoryView, VirtRange, KERNEL_SPACE_START, USER_SPACE_START};

pub struct X86ActiveUserMemory {
    table: &'static PageTable,
    physical_offset: u64,
    address_space: VirtRange,
}

impl X86ActiveUserMemory {
    /// # Safety
    /// The supplied direct-map offset must remain valid while the view is used.
    /// The active CR3 must remain unchanged for the lifetime of the view.
    pub unsafe fn current(physical_offset: u64) -> Self {
        let (frame, _) = Cr3::read();
        let table_address = frame
            .start_address()
            .as_u64()
            .checked_add(physical_offset)
            .expect("active page-table direct-map address overflow");
        let table = unsafe { &*(table_address as *const PageTable) };
        Self {
            table,
            physical_offset,
            address_space: VirtRange::new(USER_SPACE_START, KERNEL_SPACE_START).expect("user address space must be valid"),
        }
    }

    fn flags(&self, virtual_address: u64) -> Option<PageTableFlags> {
        if !self.address_space.contains(virtual_address) { return None; }
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virtual_address));
        let mut table = self.table;
        let indexes = [page.p4_index(), page.p3_index(), page.p2_index(), page.p1_index()];
        let mut user = true;
        let mut writable = true;
        let mut nx = false;

        for (level, index) in indexes.into_iter().enumerate() {
            let entry = &table[index];
            let flags = entry.flags();
            if !flags.contains(PageTableFlags::PRESENT) { return None; }
            user &= flags.contains(PageTableFlags::USER_ACCESSIBLE);
            writable &= flags.contains(PageTableFlags::WRITABLE);
            nx |= flags.contains(PageTableFlags::NO_EXECUTE);
            if level == 3 || ((level == 1 || level == 2) && flags.contains(PageTableFlags::HUGE_PAGE)) {
                let mut result = PageTableFlags::PRESENT;
                if user { result |= PageTableFlags::USER_ACCESSIBLE; }
                if writable { result |= PageTableFlags::WRITABLE; }
                if nx { result |= PageTableFlags::NO_EXECUTE; }
                return Some(result);
            }
            let next = self.physical_offset.checked_add(entry.addr().as_u64())?;
            table = unsafe { &*(next as *const PageTable) };
        }
        None
    }
}

impl UserMemoryView for X86ActiveUserMemory {
    fn translate(&self, virtual_address: u64) -> Option<u64> {
        if !self.address_space.contains(virtual_address) { return None; }
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virtual_address));
        let indexes = [page.p4_index(), page.p3_index(), page.p2_index(), page.p1_index()];
        let mut table = self.table;
        for (level, index) in indexes.into_iter().enumerate() {
            let entry = &table[index];
            if !entry.flags().contains(PageTableFlags::PRESENT) { return None; }
            if level == 3 {
                return entry.addr().as_u64().checked_add(virtual_address & 0xfff);
            }
            if (level == 1 || level == 2) && entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                let page_size = if level == 1 { 1u64 << 30 } else { 1u64 << 21 };
                let mask = page_size - 1;
                return entry.addr().as_u64().checked_add(virtual_address & mask);
            }
            let next = self.physical_offset.checked_add(entry.addr().as_u64())?;
            table = unsafe { &*(next as *const PageTable) };
        }
        None
    }

    fn page_access(&self, virtual_address: u64) -> PageAccess {
        let Some(flags) = self.flags(virtual_address) else { return PageAccess::unmapped(); };
        PageAccess {
            mapped: true,
            user: flags.contains(PageTableFlags::USER_ACCESSIBLE),
            readable: true,
            writable: flags.contains(PageTableFlags::WRITABLE),
            executable: !flags.contains(PageTableFlags::NO_EXECUTE),
        }
    }

    fn address_space(&self) -> VirtRange { self.address_space }
}
