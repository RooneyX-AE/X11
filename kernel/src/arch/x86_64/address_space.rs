//! x86_64 address-space root and CR3 activation boundary.
//!
//! Higher layers own address-space identity and page-table population. This
//! module only validates a level-4 physical frame and performs the hardware
//! switch when the caller has established that the frame contains a valid
//! complete address-space root.

use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::PhysAddr;
use x86_64::structures::paging::{PhysFrame, Size4KiB};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressSpaceRoot(PhysFrame<Size4KiB>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressSpaceRootError {
    Unaligned,
}

impl AddressSpaceRoot {
    pub fn from_physical_address(address: u64) -> Result<Self, AddressSpaceRootError> {
        let physical = PhysAddr::new(address);
        let frame = PhysFrame::<Size4KiB>::from_start_address(physical)
            .map_err(|_| AddressSpaceRootError::Unaligned)?;
        Ok(Self(frame))
    }

    pub const fn physical_address(self) -> u64 {
        self.0.start_address().as_u64()
    }

    pub const fn frame(self) -> PhysFrame<Size4KiB> {
        self.0
    }
}

/// Switches CR3 to a previously validated level-4 page-table frame.
///
/// # Safety
/// The physical frame must contain a valid x86_64 level-4 page table whose
/// mappings are safe to activate on the current CPU. Interrupt/preemption
/// coordination and address-space lifetime must be established by the caller.
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
        assert_eq!(
            AddressSpaceRoot::from_physical_address(0x1234_5001),
            Err(AddressSpaceRootError::Unaligned)
        );
    }
}
