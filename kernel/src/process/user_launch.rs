//! Architecture-independent launch contract for a populated userspace image.
//!
//! A launch plan is only constructible after the address-space image has been
//! populated. It contains the validated entry point and initial stack pointer,
//! but no architecture-specific selector or assembly state.

use super::PopulatedAddressSpace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserLaunchError {
    EntryOutsideImage,
    StackOutsideImage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserLaunchPlan {
    entry: u64,
    stack_pointer: u64,
}

impl UserLaunchPlan {
    pub fn from_populated(image: PopulatedAddressSpace) -> Result<Self, UserLaunchError> {
        let context = image.image().context();
        let load = image.built().load();
        let entry = context.entry();
        let entry_loaded = (0..load.mapped_pages).any(|index| {
            load.page(index)
                .is_some_and(|page| page.virtual_address == (entry & !(crate::memory::PAGE_SIZE_4K - 1)))
        });
        if !entry_loaded {
            return Err(UserLaunchError::EntryOutsideImage);
        }

        let stack_pointer = context.stack_pointer();
        let stack_ok = (0..image.built().stack_count()).any(|index| {
            image
                .built()
                .stack_page(index)
                .is_some_and(|page| stack_pointer > page.virtual_address && stack_pointer <= page.virtual_address + crate::memory::PAGE_SIZE_4K)
        });
        if !stack_ok {
            return Err(UserLaunchError::StackOutsideImage);
        }

        Ok(Self { entry, stack_pointer })
    }

    pub const fn entry(self) -> u64 { self.entry }
    pub const fn stack_pointer(self) -> u64 { self.stack_pointer }
}

#[cfg(test)]
mod tests {
    use super::UserLaunchPlan;

    #[test]
    fn plan_accessors_are_stable() {
        let plan = UserLaunchPlan { entry: 0x401000, stack_pointer: 0x7000_0000 };
        assert_eq!(plan.entry(), 0x401000);
        assert_eq!(plan.stack_pointer(), 0x7000_0000);
    }
}
