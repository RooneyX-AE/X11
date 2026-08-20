//! Read-only x86_64 page-table inspection for validation paths.
//!
//! This probe deliberately performs raw const reads of page-table entries. It
//! does not create an `&mut PageTable`, own an allocator, or expose mapping
//! mutation. It is intended for proving permissions before control transfer.

use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, Size4KiB};
use crate::memory::{MappingError, PageAccess, UserMemoryView, VirtRange};

const PAGE_SHIFT: u64 = 12;
const L2_SHIFT: u64 = 21;
const L3_SHIFT: u64 = 30;

pub struct X86PageTableProbe {
    root: PhysFrame<Size4KiB>,
    physical_memory_offset: u64,
    address_space: VirtRange,
}

impl X86PageTableProbe {
    pub unsafe fn new_for_root(
        physical_memory_offset: u64,
        root: PhysFrame<Size4KiB>,
        address_space: VirtRange,
    ) -> Result<Self, MappingError> {
        root.start_address()
            .as_u64()
            .checked_add(physical_memory_offset)
            .ok_or(MappingError::BackendFailure)?;
        Ok(Self { root, physical_memory_offset, address_space })
    }

    fn table_at(&self, physical_address: u64) -> Option<&PageTable> {
        let virtual_address = physical_address.checked_add(self.physical_memory_offset)?;
        Some(unsafe { &*(virtual_address as *const PageTable) })
    }

    fn page_indices(virtual_address: u64) -> [usize; 4] {
        [
            ((virtual_address >> 39) & 0x1ff) as usize,
            ((virtual_address >> 30) & 0x1ff) as usize,
            ((virtual_address >> 21) & 0x1ff) as usize,
            ((virtual_address >> 12) & 0x1ff) as usize,
        ]
    }

    fn terminal_flags(&self, virtual_address: u64) -> Option<PageTableFlags> {
        if !self.address_space.contains(virtual_address) { return None; }
        let indices = Self::page_indices(virtual_address);
        let mut table = self.table_at(self.root.start_address().as_u64())?;
        let mut effective_user = true;
        let mut effective_writable = true;
        let mut effective_nx = false;

        for (level, index) in indices.into_iter().enumerate() {
            let entry = &table[index];
            let flags = entry.flags();
            if !flags.contains(PageTableFlags::PRESENT) { return None; }
            effective_user &= flags.contains(PageTableFlags::USER_ACCESSIBLE);
            effective_writable &= flags.contains(PageTableFlags::WRITABLE);
            effective_nx |= flags.contains(PageTableFlags::NO_EXECUTE);

            let terminal = level == 3
                || ((level == 1 || level == 2) && flags.contains(PageTableFlags::HUGE_PAGE));
            if terminal {
                let mut result = PageTableFlags::PRESENT;
                if effective_user { result |= PageTableFlags::USER_ACCESSIBLE; }
                if effective_writable { result |= PageTableFlags::WRITABLE; }
                if effective_nx { result |= PageTableFlags::NO_EXECUTE; }
                return Some(result);
            }
            table = self.table_at(entry.addr().as_u64())?;
        }
        None
    }

    fn translate_address(&self, virtual_address: u64) -> Option<u64> {
        if !self.address_space.contains(virtual_address) { return None; }
        let indices = Self::page_indices(virtual_address);
        let mut table = self.table_at(self.root.start_address().as_u64())?;

        for (level, index) in indices.into_iter().enumerate() {
            let entry = &table[index];
            let flags = entry.flags();
            if !flags.contains(PageTableFlags::PRESENT) { return None; }

            if level == 1 && flags.contains(PageTableFlags::HUGE_PAGE) {
                return entry.addr().as_u64().checked_add(virtual_address & ((1u64 << L2_SHIFT) - 1));
            }
            if level == 2 && flags.contains(PageTableFlags::HUGE_PAGE) {
                return entry.addr().as_u64().checked_add(virtual_address & ((1u64 << L3_SHIFT) - 1));
            }
            if level == 3 {
                return entry.addr().as_u64().checked_add(virtual_address & ((1u64 << PAGE_SHIFT) - 1));
            }
            table = self.table_at(entry.addr().as_u64())?;
        }
        None
    }
}

impl UserMemoryView for X86PageTableProbe {
    fn translate(&self, virtual_address: u64) -> Option<u64> {
        self.translate_address(virtual_address)
    }

    fn page_access(&self, virtual_address: u64) -> PageAccess {
        let Some(flags) = self.terminal_flags(virtual_address) else { return PageAccess::unmapped(); };
        PageAccess {
            mapped: flags.contains(PageTableFlags::PRESENT),
            user: flags.contains(PageTableFlags::USER_ACCESSIBLE),
            readable: flags.contains(PageTableFlags::PRESENT),
            writable: flags.contains(PageTableFlags::WRITABLE),
            executable: !flags.contains(PageTableFlags::NO_EXECUTE),
        }
    }

    fn address_space(&self) -> VirtRange { self.address_space }
}

#[cfg(test)]
mod tests {
    use super::X86PageTableProbe;
    use crate::memory::{KERNEL_SPACE_START, USER_SPACE_START, VirtRange};
    use x86_64::structures::paging::{PhysFrame, Size4KiB};
    use x86_64::PhysAddr;

    #[test]
    fn rejects_overflowing_root_mapping() {
        let root = PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(u64::MAX - 0xfff)).unwrap();
        let range = VirtRange::new(USER_SPACE_START, KERNEL_SPACE_START).unwrap();
        assert!(unsafe { X86PageTableProbe::new_for_root(0x2000, root, range) }.is_err());
    }
}
