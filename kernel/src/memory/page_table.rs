//! Kernel-facing page-table mapping contract.

use super::address_space::VirtRange;
use super::page::Page4K;
use super::region::PhysRange;

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

pub trait MappingFlush { fn flush(self); }
pub type MapResult<F> = Result<F, MappingError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MappingFlags(u8);

impl MappingFlags {
    const READ: u8 = 1 << 0;
    const WRITE: u8 = 1 << 1;
    const EXECUTE: u8 = 1 << 2;

    pub const fn read_only() -> Self { Self(Self::READ) }
    pub const fn read_write() -> Self { Self(Self::READ | Self::WRITE) }
    pub const fn read_execute() -> Self { Self(Self::READ | Self::EXECUTE) }
    pub const fn read_write_execute() -> Self { Self(Self::READ | Self::WRITE | Self::EXECUTE) }
    pub const fn readable(self) -> bool { self.0 & Self::READ != 0 }
    pub const fn writable(self) -> bool { self.0 & Self::WRITE != 0 }
    pub const fn executable(self) -> bool { self.0 & Self::EXECUTE != 0 }
    pub const fn is_writable_executable(self) -> bool { self.writable() && self.executable() }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageAccess {
    pub mapped: bool,
    pub user: bool,
    pub readable: bool,
    pub writable: bool,
}

impl PageAccess {
    pub const fn unmapped() -> Self { Self { mapped: false, user: false, readable: false, writable: false } }
    pub const fn user_read_only() -> Self { Self { mapped: true, user: true, readable: true, writable: false } }
    pub const fn user_read_write() -> Self { Self { mapped: true, user: true, readable: true, writable: true } }
}

pub trait PageTableMapper {
    type Flush: MappingFlush;

    fn allocate_frame(&mut self) -> Option<PhysRange>;
    fn map_page(&mut self, page: Page4K, physical_address: u64, flags: MappingFlags) -> MapResult<Self::Flush>;
    fn unmap_page(&mut self, page: Page4K) -> MapResult<(u64, Self::Flush)>;
    fn translate(&self, virtual_address: u64) -> Option<u64>;
    fn page_access(&self, virtual_address: u64) -> PageAccess;
    fn address_space(&self) -> VirtRange;
}

pub const KERNEL_ADDRESS_SPACE: VirtRange = match VirtRange::new(super::address_space::KERNEL_SPACE_START, u64::MAX) {
    Some(range) => range,
    None => panic!("kernel virtual address range must be valid"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_flags_are_explicit() {
        assert!(MappingFlags::read_only().readable());
        assert!(!MappingFlags::read_only().writable());
        assert!(MappingFlags::read_execute().executable());
        assert!(MappingFlags::read_write().writable());
        assert!(MappingFlags::read_write_execute().is_writable_executable());
    }
}