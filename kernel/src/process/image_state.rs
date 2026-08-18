//! Fully validated userspace image state.
//!
//! This type is created only after ELF metadata, virtual layout, and the initial
//! execution context have all passed their independent validation layers.

use super::{AddressSpaceSpec, InitialContext, LoadPlan, UserStackPlan};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessImage {
    address_space: AddressSpaceSpec,
    load_plan: LoadPlan,
    stack_plan: UserStackPlan,
    context: InitialContext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessImageError {
    InvalidContext,
}

impl ProcessImage {
    pub fn build(
        address_space: AddressSpaceSpec,
        load_plan: LoadPlan,
        stack_plan: UserStackPlan,
    ) -> Result<Self, ProcessImageError> {
        let context = InitialContext::new(load_plan.entry(), stack_plan.initial_rsp())
            .map_err(|_| ProcessImageError::InvalidContext)?;
        Ok(Self {
            address_space,
            load_plan,
            stack_plan,
            context,
        })
    }

    pub const fn address_space(self) -> AddressSpaceSpec { self.address_space }
    pub const fn load_plan(self) -> LoadPlan { self.load_plan }
    pub const fn stack_plan(self) -> UserStackPlan { self.stack_plan }
    pub const fn context(self) -> InitialContext { self.context }
}

#[cfg(test)]
mod tests {
    use super::ProcessImage;
    use crate::process::{AddressSpaceId, AddressSpaceSpec, ElfImage, LoadPlan, UserStackPlan};

    fn minimal_elf() -> [u8; 120] {
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
        bytes
    }

    #[test]
    fn validated_image_contains_consistent_context() {
        let image = ElfImage::parse(&minimal_elf()).unwrap();
        let address_space = AddressSpaceSpec::new(AddressSpaceId::new(1).unwrap());
        let load_plan = LoadPlan::build(address_space, image).unwrap();
        let stack_plan = UserStackPlan::build().unwrap();
        let process = ProcessImage::build(address_space, load_plan, stack_plan).unwrap();
        assert_eq!(process.context().entry(), process.load_plan().entry());
        assert_eq!(process.context().stack_pointer(), process.stack_plan().initial_rsp());
        assert_eq!(process.address_space().id(), address_space.id());
    }
}
