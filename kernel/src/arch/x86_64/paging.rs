//! x86_64 page-table backend boundary.
//!
//! Higher-level memory code must not depend directly on the `x86_64` crate's
//! mapper types. This module owns the architecture-specific bridge and keeps
//! unsafe page-table access close to the invariants that justify it.

use x86_64::VirtAddr;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{OffsetPageTable, PageTable};

/// Physical address of the active level-4 page table.
pub fn active_level_4_address() -> u64 {
    let (frame, _) = Cr3::read();
    frame.start_address().as_u64()
}

/// Initializes an x86_64 offset page-table mapper over the bootloader's
/// direct physical-memory mapping.
///
/// # Safety
///
/// The caller must provide the `physical_memory_offset` supplied by the
/// bootloader after enabling `Mapping::Dynamic`, and this function must be
/// called exactly once so the level-4 table is not aliased through multiple
/// mutable references.
pub unsafe fn init(physical_memory_offset: u64) -> OffsetPageTable<'static> {
    let offset = VirtAddr::new(physical_memory_offset);
    let level_4_table = unsafe { active_level_4_table(offset) };

    // SAFETY: `offset` identifies the bootloader's complete physical-memory
    // mapping and `level_4_table` is the active CR3 level-4 table.
    unsafe { OffsetPageTable::new(level_4_table, offset) }
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    let physical = active_level_4_address();
    let virtual_address = physical
        .checked_add(physical_memory_offset.as_u64())
        .expect("active page-table address overflow");
    let pointer = virtual_address as *mut PageTable;

    // SAFETY: The caller of `init` guarantees that the direct map resolves the
    // active level-4 frame and that this function is called only once.
    unsafe { &mut *pointer }
}
