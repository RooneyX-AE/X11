//! Physical and virtual memory policy.
//!
//! Memory is split into boot metadata translation, physical ranges, frame
//! allocation, direct physical mapping, virtual-address policy, and page-table
//! contracts. Higher layers depend on these kernel-owned interfaces instead
//! of bootloader or architecture implementation details.

mod address_space;
mod boot;
mod frame;
mod page;
mod page_table;
mod physical;
mod region;

pub use address_space::{KERNEL_SPACE_START, USER_SPACE_START, VirtRange};
pub use boot::MemorySummary;
pub use frame::{
    ContiguousFrames, EarlyFrameAllocator, Frame, FrameAllocator, FRAME_SIZE,
};
pub use page::{Page4K, PAGE_SIZE_4K};
pub use page_table::{KERNEL_ADDRESS_SPACE, MappingError, MappingFlush, PageTableMapper};
pub use physical::PhysicalMemoryMapping;
pub use region::PhysRange;

/// Produces a kernel-owned summary of the bootloader memory map.
pub fn summarize_boot_map(regions: &bootloader_api::info::MemoryRegions) -> MemorySummary {
    boot::summarize(regions)
}
