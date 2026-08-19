//! Architecture-independent launch contract for a populated userspace image.
//!
//! A launch plan is only constructible after the address-space image has been
//! populated. It contains the validated address-space identity, entry point,
//! and initial stack pointer, but no architecture-specific selector or
//! assembly state.

use super::{AddressSpaceId, PopulatedAddressSpace};
use crate::memory::PAGE_SIZE_4K;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserLaunchError {
    EntryOutsideImage,
    EntryNotExecutable,
    StackOutsideImage,
    StackNotWritable,
    StackExecutable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserLaunchPlan {
    address_space: AddressSpaceId,
    entry: u64,
    stack_pointer: u64,
}

impl UserLaunchPlan {
    pub fn from_populated(image: PopulatedAddressSpace) -> Result<Self, UserLaunchError> {
        let context = image.image().context();
        let entry = context.entry();
        if !image.built().load().contains_page(entry) {
            return Err(UserLaunchError::EntryOutsideImage);
        }
        if !image.built().entry_executable() {
            return Err(UserLaunchError::EntryNotExecutable);
        }

        let stack_pointer = context.stack_pointer();
        let stack_ok = stack_pointer != 0 && (0..image.built().stack_count()).any(|index| {
            image.built().stack_page(index).is_some_and(|page| {
                let start = page.virtual_address;
                let end = start.checked_add(PAGE_SIZE_4K).unwrap_or(u64::MAX);
                stack_pointer > start && stack_pointer <= end
            })
        });
        if !stack_ok {
            return Err(UserLaunchError::StackOutsideImage);
        }
        if !image.built().stack_writable() {
            return Err(UserLaunchError::StackNotWritable);
        }
        if image.built().stack_executable() {
            return Err(UserLaunchError::StackExecutable);
        }

        Ok(Self {
            address_space: image.image().address_space().id(),
            entry,
            stack_pointer,
        })
    }

    pub const fn address_space(self) -> AddressSpaceId { self.address_space }
    pub const fn entry(self) -> u64 { self.entry }
    pub const fn stack_pointer(self) -> u64 { self.stack_pointer }
}

#[cfg(test)]
mod tests {
    use super::UserLaunchPlan;
    use crate::process::AddressSpaceId;

    #[test]
    fn plan_accessors_are_stable() {
        let id = AddressSpaceId::new(7).unwrap();
        let plan = UserLaunchPlan { address_space: id, entry: 0x401000, stack_pointer: 0x7000_0000 };
        assert_eq!(plan.address_space(), id);
        assert_eq!(plan.entry(), 0x401000);
        assert_eq!(plan.stack_pointer(), 0x7000_0000);
    }
}
