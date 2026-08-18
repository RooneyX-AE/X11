//! Kernel-facing page-table mapping contract.
//!
//! Higher-level memory code depends on this trait rather than on a specific
//! architecture crate. TLB flushes remain explicit so callers cannot mistake
//! a pending hardware update for a completed mapping operation.

use super::address_space::VirtRange;
use super::page::Page4K;

/// Errors returned while changing a virtual mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappingError {
    /// The virtual page could not be represented as a complete 4 KiB range.
    InvalidVirtualAddress,
    /// The backing physical address is not page aligned or is otherwise invalid.
    InvalidPhysicalAddress,
    /// The virtual page is outside the mapper's address-space policy.
    OutsideAddressSpace,
    /// A mapping already exists at the requested virtual address.
    AlreadyMapped,
    /// No mapping exists at the requested virtual address.
    NotMapped,
    /// A page-table entry above the requested page is a huge-page mapping.
    ParentEntryHugePage,
    /// A new lower-level page table could not be allocated.
    FrameAllocationFailed,
    /// The existing page-table entry contains an invalid physical frame address.
    InvalidMappedFrame,
    /// The architecture backend rejected an operation for an otherwise valid
    /// request and no more specific contract error is available.
    BackendFailure,
}

/// A single mapping change that still requires a hardware TLB update.
///
/// The architecture backend owns the exact flush mechanism. This token makes
/// the completion step explicit at the kernel API boundary.
pub trait MappingFlush {
    /// Applies the pending translation-cache invalidation.
    fn flush(self);
}

/// Result of mapping one page.
pub type MapResult<F> = Result<F, MappingError>;

/// Kernel-facing interface for page-sized mappings.
pub trait PageTableMapper {
    type Flush: MappingFlush;

    /// Maps one virtual page to one physical frame.
    fn map_page(&mut self, page: Page4K, physical_address: u64) -> MapResult<Self::Flush>;

    /// Removes one virtual-page mapping and returns its physical address plus
    /// the flush operation required to make the CPU observe the change.
    fn unmap_page(&mut self, page: Page4K) -> MapResult<(u64, Self::Flush)>;

    /// Translates a virtual address into its backing physical address.
    ///
    /// Addresses outside the mapper's supported address space are treated as
    /// unmapped by this contract.
    fn translate(&self, virtual_address: u64) -> Option<u64>;

    /// Returns the supported virtual-address policy.
    fn address_space(&self) -> VirtRange;
}

/// Initial kernel-only virtual address space policy.
pub const KERNEL_ADDRESS_SPACE: VirtRange = match VirtRange::new(
    super::address_space::KERNEL_SPACE_START,
    u64::MAX,
) {
    Some(range) => range,
    None => panic!("kernel virtual address range must be valid"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_address_space_is_kernel_only() {
        assert!(KERNEL_ADDRESS_SPACE.is_kernel());
        assert!(!KERNEL_ADDRESS_SPACE.is_user());
    }

    #[test]
    fn mapping_errors_are_distinct() {
        assert_ne!(MappingError::InvalidVirtualAddress, MappingError::InvalidPhysicalAddress);
        assert_ne!(MappingError::AlreadyMapped, MappingError::NotMapped);
        assert_ne!(MappingError::FrameAllocationFailed, MappingError::ParentEntryHugePage);
        assert_ne!(MappingError::InvalidMappedFrame, MappingError::BackendFailure);
    }
}
