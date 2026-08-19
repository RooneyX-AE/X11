//! x86_64 address-space root and CR3 activation boundary.
//!
//! Higher layers own address-space identity and page-table population. This
//! module allocates an isolated level-4 root for a user address space, keeps
//! the kernel-half mappings shared, and leaves user-half entries empty until
//! explicit process mappings are installed.

use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::PhysAddr;
use x86_64::structures::paging::{PageTable, PhysFrame, Size4KiB};

use crate::memory::{EarlyFrameAllocator, FrameAllocator};

const KERNEL_P4_START: usize = 256;
const KERNEL_P4_END: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressSpaceRoot(PhysFrame<Size4KiB>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressSpaceRootError {
    Unaligned,
    AllocationFailed,
    PhysicalAddressOverflow,
}

impl AddressSpaceRoot {
    pub fn from_physical_address(address: u64) -> Result<Self, AddressSpaceRootError> {
        let physical = PhysAddr::new(address);
        let frame = PhysFrame::<Size4KiB>::from_start_address(physical)
            .map_err(|_| AddressSpaceRootError::Unaligned)?;
        Ok(Self(frame))
    }

    pub unsafe fn new_user_root(
        physical_memory_offset: u64,
        allocator: &mut EarlyFrameAllocator<'_>,
    ) -> Result<Self, AddressSpaceRootError> {
        let frame = allocator
            .allocate_frame()
            .and_then(|raw| PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(raw.start_address())).ok())
            .ok_or(AddressSpaceRootError::AllocationFailed)?;
        let root = Self(frame);
        let root_virtual = frame
            .start_address()
            .as_u64()
            .checked_add(physical_memory_offset)
            .ok_or(AddressSpaceRootError::PhysicalAddressOverflow)? as *mut PageTable;

        let (active_frame, _) = Cr3::read();
        let active_virtual = active_frame
            .start_address()
            .as_u64()
            .checked_add(physical_memory_offset)
            .ok_or(AddressSpaceRootError::PhysicalAddressOverflow)? as *const PageTable;

        unsafe {
            core::ptr::write_bytes(root_virtual.cast::<u8>(), 0, core::mem::size_of::<PageTable>());
            for index in KERNEL_P4_START..KERNEL_P4_END {
                (*root_virtual)[index] = (*active_virtual)[index].clone();
            }
        }

        Ok(root)
    }

    pub const fn physical_address(self) -> u64 { self.0.start_address().as_u64() }
    pub const fn frame(self) -> PhysFrame<Size4KiB> { self.0 }
}

pub unsafe fn activate(root: AddressSpaceRoot) {
    unsafe { Cr3::write(root.frame(), Cr3Flags::empty()) };
}

pub fn active_root() -> AddressSpaceRoot {
    let (frame, _) = Cr3::read();
    AddressSpaceRoot(frame)
}

#[cfg(test)]
mod tests {
    use super::{AddressSpaceRoot, AddressSpaceRootError};

    #[test]
    fn accepts_aligned_level4_frame() {
        let root = AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap();
        assert_eq!(root.physical_address(), 0x1234_5000);
    }

    #[test]
    fn rejects_unaligned_level4_frame() {
        assert_eq!(AddressSpaceRoot::from_physical_address(0x1234_5001), Err(AddressSpaceRootError::Unaligned));
    }

    #[test]
    fn root_error_is_explicit() {
        assert_eq!(AddressSpaceRootError::AllocationFailed, AddressSpaceRootError::AllocationFailed);
        assert_eq!(AddressSpaceRootError::PhysicalAddressOverflow, AddressSpaceRootError::PhysicalAddressOverflow);
    }
}
