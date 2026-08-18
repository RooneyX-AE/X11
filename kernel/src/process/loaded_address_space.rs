//! Architecture-independent lifecycle marker for a fully built process address space.
//!
//! Construction is intentionally separate from activation. A process may own a
//! complete address-space root while another address space is currently active.

use super::{AddressSpaceId, AddressSpaceSpec, ProcessImage};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadedAddressSpaceError {
    AddressSpaceMismatch,
    ImageNotValidated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadedAddressSpace {
    spec: AddressSpaceSpec,
    image: ProcessImage,
}

impl LoadedAddressSpace {
    pub fn build(
        spec: AddressSpaceSpec,
        image: ProcessImage,
    ) -> Result<Self, LoadedAddressSpaceError> {
        if image.address_space().id() != spec.id() {
            return Err(LoadedAddressSpaceError::AddressSpaceMismatch);
        }
        Ok(Self { spec, image })
    }

    pub const fn id(self) -> AddressSpaceId {
        self.spec.id()
    }

    pub const fn spec(self) -> AddressSpaceSpec {
        self.spec
    }

    pub const fn image(self) -> ProcessImage {
        self.image
    }
}

#[cfg(test)]
mod tests {
    use super::{LoadedAddressSpace, LoadedAddressSpaceError};
    use crate::memory::AddressSpaceId;
    use crate::process::{AddressSpaceSpec, ElfImage, LoadPlan, ProcessImage, UserStackPlan};

    fn image(id: AddressSpaceId) -> ProcessImage {
        let mut bytes = [0u8; 120];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        let p = 64usize;
        bytes[p..p + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[p + 4..p + 8].copy_from_slice(&5u32.to_le_bytes());
        bytes[p + 16..p + 24].copy_from_slice(&0x401000u64.to_le_bytes());
        bytes[p + 32..p + 40].copy_from_slice(&16u64.to_le_bytes());
        bytes[p + 40..p + 48].copy_from_slice(&0x1000u64.to_le_bytes());
        let elf = ElfImage::parse(&bytes).unwrap();
        let spec = AddressSpaceSpec::new(id);
        let plan = LoadPlan::build(spec, elf).unwrap();
        ProcessImage::build(spec, plan, UserStackPlan::build().unwrap()).unwrap()
    }

    #[test]
    fn accepts_matching_address_space_identity() {
        let id = AddressSpaceId::new(7).unwrap();
        let spec = AddressSpaceSpec::new(id);
        let loaded = LoadedAddressSpace::build(spec, image(id)).unwrap();
        assert_eq!(loaded.id(), id);
    }

    #[test]
    fn rejects_mismatched_address_space_identity() {
        let spec = AddressSpaceSpec::new(AddressSpaceId::new(7).unwrap());
        let image = image(AddressSpaceId::new(8).unwrap());
        assert_eq!(
            LoadedAddressSpace::build(spec, image),
            Err(LoadedAddressSpaceError::AddressSpaceMismatch)
        );
    }
}
