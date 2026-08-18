//! Translation of bootloader memory metadata into kernel-owned accounting.

use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};

use super::region::PhysRange;

/// Summary of physical memory regions visible during early boot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MemorySummary {
    usable_bytes: u64,
    reserved_bytes: u64,
    malformed_regions: u64,
}

impl MemorySummary {
    /// Returns the amount of memory marked usable by the bootloader.
    pub const fn usable_bytes(self) -> u64 {
        self.usable_bytes
    }

    /// Returns the amount of non-usable memory described by the bootloader.
    pub const fn reserved_bytes(self) -> u64 {
        self.reserved_bytes
    }

    /// Returns the number of memory-map entries whose bounds were invalid.
    pub const fn malformed_regions(self) -> u64 {
        self.malformed_regions
    }
}

/// Summarizes a bootloader-provided physical memory map.
///
/// Only `Usable` regions are candidates for future allocation. Every other
/// kind stays reserved until a dedicated subsystem explicitly knows how to
/// use it. `MemoryRegionKind` is non-exhaustive, so unknown future variants
/// are conservatively treated as reserved.
pub fn summarize(regions: &MemoryRegions) -> MemorySummary {
    let mut summary = MemorySummary::default();

    for region in regions.iter() {
        account_region(&mut summary, region);
    }

    summary
}

fn account_region(summary: &mut MemorySummary, region: &MemoryRegion) {
    let Some(range) = PhysRange::new(region.start, region.end) else {
        summary.malformed_regions = summary.malformed_regions.saturating_add(1);
        return;
    };

    match region.kind {
        MemoryRegionKind::Usable => {
            summary.usable_bytes = summary.usable_bytes.saturating_add(range.len());
        }
        _ => {
            summary.reserved_bytes = summary.reserved_bytes.saturating_add(range.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_map_is_empty() {
        let regions: &'static mut [MemoryRegion] = Box::leak(Box::new([]));
        let regions = regions.into();
        assert_eq!(summarize(&regions), MemorySummary::default());
    }
}
