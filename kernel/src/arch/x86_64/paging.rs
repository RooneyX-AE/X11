//! x86_64 page-table backend boundary.
//!
//! Higher-level memory code must not depend directly on the `x86_64` crate's
//! mapper types. This module owns the architecture-specific bridge and keeps
//! unsafe page-table access close to the invariants that justify it.

use core::arch::x86_64::{__cpuid, __cpuid_count};
use x86_64::VirtAddr;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{OffsetPageTable, PageTable};

/// Hardware MMU capabilities discovered from architectural CPUID leaves.
///
/// Detection is intentionally separate from policy and from enabling features.
/// This keeps old x86_64 CPUs on the conservative path while allowing later
/// TLB/CR3 optimizations to select a fast strategy from one immutable profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuFeatures {
    pcid: bool,
    invpcid: bool,
    one_gib_pages: bool,
    nx: bool,
}

/// TLB strategy that hardware can support once the corresponding control state
/// is enabled. This enum deliberately describes capability, not current state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TlbStrategy {
    Cr3Flush,
    Pcid,
    Invpcid,
}

impl CpuFeatures {
    pub fn detect() -> Self {
        let mut features = Self { pcid: false, invpcid: false, one_gib_pages: false, nx: false };

        let leaf0 = unsafe { __cpuid(0) };
        if leaf0.eax >= 1 {
            let leaf1 = unsafe { __cpuid(1) };
            features.pcid = (leaf1.ecx & (1 << 17)) != 0;
        }

        if leaf0.eax >= 7 {
            let leaf7 = unsafe { __cpuid_count(7, 0) };
            features.invpcid = (leaf7.ebx & (1 << 10)) != 0;
        }

        let extended_max = unsafe { __cpuid(0x8000_0000) }.eax;
        if extended_max >= 0x8000_0001 {
            let extended = unsafe { __cpuid(0x8000_0001) };
            features.one_gib_pages = (extended.edx & (1 << 26)) != 0;
            features.nx = (extended.edx & (1 << 20)) != 0;
        }

        features
    }

    pub const fn pcid(self) -> bool { self.pcid }
    pub const fn invpcid(self) -> bool { self.invpcid }
    pub const fn one_gib_pages(self) -> bool { self.one_gib_pages }
    pub const fn nx(self) -> bool { self.nx }
    pub const fn pcid_fast_path(self) -> bool { self.pcid }
    pub const fn targeted_invalidation(self) -> bool { self.invpcid }

    pub const fn tlb_strategy(self) -> TlbStrategy {
        if self.invpcid && self.pcid {
            TlbStrategy::Invpcid
        } else if self.pcid {
            TlbStrategy::Pcid
        } else {
            TlbStrategy::Cr3Flush
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::{CpuFeatures, TlbStrategy};

    #[test]
    fn feature_profile_defaults_are_conservative() {
        let features = CpuFeatures { pcid: false, invpcid: false, one_gib_pages: false, nx: false };
        assert!(!features.pcid());
        assert!(!features.invpcid());
        assert!(!features.one_gib_pages());
        assert!(!features.nx());
        assert!(!features.pcid_fast_path());
        assert!(!features.targeted_invalidation());
        assert_eq!(features.tlb_strategy(), TlbStrategy::Cr3Flush);
    }

    #[test]
    fn pcid_does_not_depend_on_nx() {
        let features = CpuFeatures { pcid: true, invpcid: false, one_gib_pages: false, nx: false };
        assert!(features.pcid_fast_path());
        assert!(!features.targeted_invalidation());
        assert_eq!(features.tlb_strategy(), TlbStrategy::Pcid);
    }

    #[test]
    fn invpcid_is_the_targeted_invalidation_capability() {
        let features = CpuFeatures { pcid: true, invpcid: true, one_gib_pages: false, nx: true };
        assert!(features.pcid_fast_path());
        assert!(features.targeted_invalidation());
        assert_eq!(features.tlb_strategy(), TlbStrategy::Invpcid);
    }

    #[test]
    fn invpcid_without_pcid_does_not_select_pcid_strategy() {
        let features = CpuFeatures { pcid: false, invpcid: true, one_gib_pages: false, nx: true };
        assert_eq!(features.tlb_strategy(), TlbStrategy::Cr3Flush);
    }
}
