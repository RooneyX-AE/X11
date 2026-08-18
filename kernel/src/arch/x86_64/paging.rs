//! x86_64 page-table backend boundary.
//!
//! Higher-level memory code must not depend directly on the `x86_64` crate's
//! mapper types. This module owns the architecture-specific bridge and keeps
//! unsafe page-table access close to the invariants that justify it.

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::PageTable;

/// Physical address of the active level-4 page table.
pub fn active_level_4_address() -> u64 {
    let (frame, _) = Cr3::read();
    frame.start_address().as_u64()
}

/// Returns a mutable reference to the active level-4 table through a direct
/// physical-memory mapping.
///
/// # Safety
///
/// The caller must provide a direct-map offset established by the bootloader,
/// and that mapping must make the active level-4 physical frame accessible as
/// a valid, uniquely borrowed `PageTable` for the duration of the borrow.
pub unsafe fn active_level_4_table<'a>(physical_memory_offset: u64) -> &'a mut PageTable {
    let physical = active_level_4_address();
    let virtual_address = physical
        .checked_add(physical_memory_offset)
        .expect("active page-table address overflow");
    let pointer = virtual_address as *mut PageTable;

    // SAFETY: The caller guarantees that the direct map resolves this physical
    // page-table frame and that the returned borrow is uniquely held.
    unsafe { &mut *pointer }
}
