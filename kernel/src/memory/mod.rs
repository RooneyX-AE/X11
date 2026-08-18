//! Physical-memory subsystem.
//!
//! Early memory code is deliberately split into boot metadata translation,
//! architecture-neutral ranges, and allocation. Consumers should depend on
//! these kernel-owned contracts rather than bootloader implementation details.

mod boot;
mod frame;
mod region;

pub use boot::MemorySummary;
pub use frame::{EarlyFrameAllocator, FRAME_SIZE, Frame, FrameAllocator};
pub use region::PhysRange;

/// Produces a kernel-owned summary of the bootloader memory map.
pub fn summarize_boot_map(regions: &bootloader_api::info::MemoryRegions) -> MemorySummary {
    boot::summarize(regions)
}
