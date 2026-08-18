//! Physical-to-virtual mapping contract.
//!
//! The bootloader can create a direct mapping of physical memory. This module
//! translates physical addresses through that offset without exposing the
//! bootloader's `BootInfo` representation to paging and allocator consumers.

use bootloader_api::BootInfo;

use super::address_space::VirtRange;
use super::region::PhysRange;

/// A kernel-owned physical-memory direct-map offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalMemoryMapping {
    offset: u64,
}

impl PhysicalMemoryMapping {
    /// Creates a mapping from a physical-to-virtual base offset.
    pub const fn new(offset: u64) -> Self {
        Self { offset }
    }

    /// Extracts the physical-to-virtual base offset.
    pub const fn offset(self) -> u64 {
        self.offset
    }

    /// Creates a mapping when the bootloader supplied one.
    pub fn from_boot_info(boot_info: &BootInfo) -> Option<Self> {
        Option::<u64>::from(boot_info.physical_memory_offset).map(Self::new)
    }

    /// Translates one physical address using checked arithmetic.
    pub const fn translate(self, physical_address: u64) -> Option<u64> {
        self.offset.checked_add(physical_address)
    }

    /// Translates a physical range without wrapping its end address.
    pub fn translate_range(self, range: PhysRange) -> Option<VirtRange> {
        let start = self.translate(range.start())?;
        let end = self.translate(range.end())?;
        VirtRange::new(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::PhysicalMemoryMapping;
    use crate::memory::PhysRange;

    #[test]
    fn translates_with_checked_addition() {
        let mapping = PhysicalMemoryMapping::new(0xffff_0000_0000_0000);
        assert_eq!(mapping.translate(0x4000), Some(0xffff_0000_0000_4000));
    }

    #[test]
    fn rejects_overflow() {
        let mapping = PhysicalMemoryMapping::new(u64::MAX - 0x1000);
        assert_eq!(mapping.translate(0x2000), None);
    }

    #[test]
    fn translates_ranges_without_wrapping() {
        let mapping = PhysicalMemoryMapping::new(0x8000_0000);
        let physical = PhysRange::new(0x1000, 0x3000).unwrap();
        let virtual_range = mapping.translate_range(physical).unwrap();
        assert_eq!(virtual_range.start(), 0x8000_1000);
        assert_eq!(virtual_range.end(), 0x8000_3000);
    }
}