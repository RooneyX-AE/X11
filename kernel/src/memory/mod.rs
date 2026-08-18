//! Physical and virtual memory policy.
//!
//! Memory is split into boot metadata translation, physical ranges, frame
//! allocation, direct physical mapping, virtual-address policy, and page-table
//! contracts. Higher layers depend on these kernel-owned interfaces instead
//! of bootloader or architecture implementation details.

mod boot;
mod frame;
mod mapper;
mod page;
mod page_table;
mod physical;
mod region;
mod virtual;

pub use boot::MemorySummary;
pub use frame::{EarlyFrameAllocator, Frame, FrameAllocator, FRAME_SIZE};
pub use mapper::{X86Flush, X86PageTableMapper};
pub use page::{Page4K, PAGE_SIZE_4K};
pub use page_table::{KERNEL_ADDRESS_SPACE, MappingError, MappingFlush, PageTableMapper};
pub use physical::PhysicalMemoryMapping;
pub use region::PhysRange;
pub use virtual::{VirtRange, KERNEL_SPACE_START, USER_SPACE_START};

/// Produces a kernel-owned summary of the bootloader memory map.
pub fn summarize_boot_map(regions: &bootloader_api::info::MemoryRegions) -> MemorySummary {
    boot::summarize(regions)
}
