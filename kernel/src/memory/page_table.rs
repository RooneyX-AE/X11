//! Page-table mapping contract.
//!
//! This module owns the kernel-facing mapping abstraction while leaving the
//! actual page-table implementation behind the architecture boundary. The
//! first implementation will use x86_64's mapper types without exposing them
//! to higher-level memory consumers.

use super::page::Page4K;
use super::virtual::VirtRange;

/// Errors returned while changing a virtual mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappingError {
    /// The requested virtual address is not page aligned.
    Unaligned,
    /// The address range is outside the supported virtual-address policy.
    OutsideAddressSpace,
    /// A mapping already exists at the requested virtual address.
    AlreadyMapped,
    /// No mapping exists at the requested virtual address.
    NotMapped,
    /// The backing physical address is invalid or unavailable.
    InvalidPhysicalAddress,
    /// The hardware page-table operation failed.
    BackendFailure,
}

/// Kernel-facing interface for page-sized mappings.
pub trait PageTableMapper {
    /// Maps one virtual page to one physical frame.
    fn map_page(&mut self, page: Page4K, physical_address: u64) -> Result<(), MappingError>;

    /// Removes one virtual-page mapping and returns its physical address.
    fn unmap_page(&mut self, page: Page4K) -> Result<u64, MappingError>;

    /// Translates a virtual address into its backing physical address.
    fn translate(&self, virtual_address: u64) -> Option<u64>;

    /// Returns the supported virtual-address policy.
    fn address_space(&self) -> VirtRange;
}

/// The initial kernel virtual address space policy.
pub const KERNEL_ADDRESS_SPACE: VirtRange = match VirtRange::new(
    super::virtual::KERNEL_SPACE_START,
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
        assert_ne!(MappingError::AlreadyMapped, MappingError::NotMapped);
    }
}
