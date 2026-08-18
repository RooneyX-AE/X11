//! Physical and virtual memory policy.
//!
//! Memory is split into boot metadata translation, physical ranges, frame
//! allocation, direct physical mapping, virtual-address policy, page-table
//! contracts, and user-address validation. Higher layers depend on these
//! kernel-owned interfaces instead of bootloader or architecture details.

mod address_space;
mod boot;
mod frame;
mod page;
mod page_table;
mod physical;
mod region;
mod user;
mod user_copy;

pub use address_space::{VirtRange, KERNEL_SPACE_START, USER_SPACE_START};
pub use boot::MemorySummary;
pub use frame::{EarlyFrameAllocator, FrameAllocator};
pub use page::{Page4K, PAGE_SIZE_4K};
pub use page_table::{MappingError, MappingFlush, PageAccess, PageTableMapper};
pub use physical::PhysicalMemoryMapping;
pub use user::{validate_slice, UserRangeError};
pub use user_copy::{validate_readable_range, UserReadError};

/// Produces a kernel-owned summary of the bootloader memory map.
pub fn summarize_boot_map(regions: &bootloader_api::info::MemoryRegions) -> MemorySummary {
    boot::summarize(regions)
}
