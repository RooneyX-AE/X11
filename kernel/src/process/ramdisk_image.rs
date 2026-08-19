//! Construction of a validated process image from a bootloader ramdisk payload.
//!
//! This layer owns no physical frames and performs no page-table writes. It
//! deliberately stops at the immutable `ProcessImage` boundary.

use super::{AddressSpaceSpec, ElfError, ElfImage, LoadPlan, LoadPlanError, ProcessImage, ProcessImageError, UserStackPlan};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RamdiskImageError {
    Elf(ElfError),
    LoadPlan(LoadPlanError),
    Stack(super::StackPlanError),
    Image(ProcessImageError),
}

pub fn build_process_image(
    bytes: &[u8],
    address_space: AddressSpaceSpec,
) -> Result<ProcessImage, RamdiskImageError> {
    let image = ElfImage::parse(bytes).map_err(RamdiskImageError::Elf)?;
    let load_plan = LoadPlan::build(address_space, image).map_err(RamdiskImageError::LoadPlan)?;
    let stack_plan = UserStackPlan::build().map_err(RamdiskImageError::Stack)?;
    ProcessImage::build(address_space, load_plan, stack_plan)
        .map_err(RamdiskImageError::Image)
}

#[cfg(test)]
mod tests {
    use super::build_process_image;
    use crate::process::{AddressSpaceId, AddressSpaceSpec};

    fn minimal_elf() -> [u8; 120] {
        let mut bytes = [0u8; 120];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[24..32].copy_from_slice(&0x401001u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        let p = 64usize;
        bytes[p..p + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[p + 4..p + 8].copy_from_slice(&5u32.to_le_bytes());
        bytes[p + 16..p + 24].copy_from_slice(&0x401001u64.to_le_bytes());
        bytes[p + 32..p + 40].copy_from_slice(&16u64.to_le_bytes());
        bytes[p + 40..p + 48].copy_from_slice(&0x1000u64.to_le_bytes());
        bytes
    }

    #[test]
    fn ramdisk_payload_becomes_validated_process_image() {
        let spec = AddressSpaceSpec::new(AddressSpaceId::new(11).unwrap());
        let process = build_process_image(&minimal_elf(), spec).unwrap();
        assert_eq!(process.address_space().id(), spec.id());
        assert_eq!(process.load_plan().entry(), 0x401001);
        assert_eq!(process.stack_plan().count(), 16);
    }
}
