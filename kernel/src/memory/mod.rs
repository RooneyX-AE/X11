//! Physical-memory subsystem.
//!
//! Early memory code is deliberately split into boot metadata translation and
//! address-range primitives. Allocation and page-table manipulation will be
//! added only after these contracts are stable.

mod boot;
mod region;

pub use boot::MemorySummary;
pub use region::PhysRange;

/// Produces a kernel-owned summary of the bootloader memory map.
pub fn summarize_boot_map(regions: &bootloader_api::info::MemoryRegions) -> MemorySummary {
    boot::summarize(regions)
}
