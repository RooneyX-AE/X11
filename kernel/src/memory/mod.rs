//! Physical and virtual memory policy.
//!
//! Early memory is split into boot metadata translation, architecture-neutral
//! ranges, frame allocation, and virtual-address policy. Consumers depend on
//! these kernel-owned contracts rather than bootloader implementation details.

mod boot;
mod frame;
mod region;
mod virtual;

pub use boot::MemorySummary;
pub use frame::{EarlyFrameAllocator, Frame, FrameAllocator, FRAME_SIZE};
pub use region::PhysRange;
pub use virtual::{VirtRange, KERNEL_SPACE_START, USER_SPACE_START};

/// Produces a kernel-owned summary of the bootloader memory map.
pub fn summarize_boot_map(regions: &bootloader_api::info::MemoryRegions) -> MemorySummary {
    boot::summarize(regions)
}
