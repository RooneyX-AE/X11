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
    InvalidVirtualAddress,
    InvalidPhysicalAddress,
    OutsideAddressSpace,
    AlreadyMapped,
    NotMapped,
    ParentEntryHugePage,
    FrameAllocationFailed,
    InvalidMappedFrame,
    BackendFailure,
}

pub trait MappingFlush {
    fn flush(self);
}

pub type MapResult<F> = Result<F, MappingError>;

/// Permissions and residency exposed by an address-space backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageAccess {
    pub mapped: bool,
    pub user: bool,
    pub readable: bool,
    pub writable: bool,
}

impl PageAccess {
    pub const fn unmapped() -> Self {
        Self {
            mapped: false,
            user: false,
            readable: false,
            writable: false,
        }
    }

    pub const fn user_read_only() -> Self {
        Self {
            mapped: true,
            user: true,
            readable: true,
            writable: false,
        }
    }

    pub const fn user_read_write() -> Self {
        Self {
            mapped: true,
            user: true,
            readable: true,
            writable: true,
        }
    }
}

/// Kernel-facing interface for page-sized mappings.
pub trait PageTableMapper {
    type Flush: MappingFlush;

    fn map_page(&mut self, page: Page4K, physical_address: u64) -> MapResult<Self::Flush>;

    fn unmap_page(&mut self, page: Page4K) -> MapResult<(u64, Self::Flush)>;

    fn translate(&self, virtual_address: u64) -> Option<u64>;

    /// Returns access attributes for the mapping containing `virtual_address`.
    fn page_access(&self, virtual_address: u64) -> PageAccess;

    fn address_space(&self) -> VirtRange;
}

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

    #[test]
    fn page_access_defaults_to_unmapped() {
        assert_eq!(PageAccess::unmapped(), PageAccess {
            mapped: false,
            user: false,
            readable: false,
            writable: false,
        });
    }

    #[test]
    fn user_access_profiles_are_explicit() {
        assert_eq!(PageAccess::user_read_only().mapped, true);
        assert_eq!(PageAccess::user_read_only().user, true);
        assert_eq!(PageAccess::user_read_only().writable, false);
        assert_eq!(PageAccess::user_read_write().writable, true);
    }
}
