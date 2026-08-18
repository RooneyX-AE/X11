//! Process-facing address-space ownership contract.
//!
//! The process layer owns identity and lifetime. Architecture-specific page
//! table objects remain behind the memory subsystem, keeping process code from
//! depending on x86_64 paging details.

use crate::memory::{UserAddressSpaceLayout, VirtRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct AddressSpaceId(u64);

impl AddressSpaceId {
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    pub const fn raw(self) -> u64 { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressSpaceSpec {
    id: AddressSpaceId,
    layout: UserAddressSpaceLayout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressSpaceError {
    InvalidUserRange,
    ImageOutsideUserSpace,
    StackOutsideUserSpace,
    StackOverlapsGuard,
}

impl AddressSpaceSpec {
    pub const fn new(id: AddressSpaceId) -> Self {
        Self { id, layout: UserAddressSpaceLayout::default() }
    }

    pub const fn id(self) -> AddressSpaceId { self.id }
    pub const fn layout(self) -> UserAddressSpaceLayout { self.layout }

    /// Validate a range before any architecture-specific page-table mutation.
    pub fn validate_user_range(self, range: VirtRange) -> Result<(), AddressSpaceError> {
        if !range.is_user() {
            return Err(AddressSpaceError::InvalidUserRange);
        }
        if range.start() < self.layout.user_range().start()
            || range.end() > self.layout.user_range().end()
        {
            return Err(AddressSpaceError::InvalidUserRange);
        }
        if range.end() > self.layout.guard_range().start()
            && range.start() < self.layout.guard_range().end()
        {
            return Err(AddressSpaceError::StackOverlapsGuard);
        }
        Ok(())
    }

    pub fn validate_image_range(self, range: VirtRange) -> Result<(), AddressSpaceError> {
        self.validate_user_range(range)?;
        if range.start() < self.layout.image_base()
            || range.start() >= self.layout.stack_range().start()
        {
            return Err(AddressSpaceError::ImageOutsideUserSpace);
        }
        Ok(())
    }

    pub fn validate_stack_range(self, range: VirtRange) -> Result<(), AddressSpaceError> {
        if !range.is_user()
            || range.start() < self.layout.stack_range().start()
            || range.end() > self.layout.stack_range().end()
        {
            return Err(AddressSpaceError::StackOutsideUserSpace);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AddressSpaceError, AddressSpaceId, AddressSpaceSpec};
    use crate::memory::VirtRange;

    #[test]
    fn zero_is_not_a_valid_address_space_id() {
        assert_eq!(AddressSpaceId::new(0), None);
    }

    #[test]
    fn ids_are_stable_and_specs_use_default_layout() {
        let id = AddressSpaceId::new(7).unwrap();
        let spec = AddressSpaceSpec::new(id);
        assert_eq!(spec.id().raw(), 7);
        assert_eq!(spec.layout().stack_top(), spec.layout().stack_range().end());
    }

    #[test]
    fn user_range_must_stay_in_user_half() {
        let spec = AddressSpaceSpec::new(AddressSpaceId::new(1).unwrap());
        let range = VirtRange::new(spec.layout().user_range().start(), spec.layout().user_range().start() + 0x2000).unwrap();
        assert_eq!(spec.validate_user_range(range), Ok(()));
        let bad = VirtRange::new(spec.layout().user_range().end() - 0x1000, spec.layout().user_range().end() + 0x1000).unwrap();
        assert_eq!(spec.validate_user_range(bad), Err(AddressSpaceError::InvalidUserRange));
    }

    #[test]
    fn stack_range_is_valid_but_guard_page_is_not() {
        let spec = AddressSpaceSpec::new(AddressSpaceId::new(2).unwrap());
        assert_eq!(spec.validate_stack_range(spec.layout().stack_range()), Ok(()));
        assert_eq!(spec.validate_user_range(spec.layout().guard_range()), Err(AddressSpaceError::StackOverlapsGuard));
    }

    #[test]
    fn image_must_not_reach_stack() {
        let spec = AddressSpaceSpec::new(AddressSpaceId::new(3).unwrap());
        let image = VirtRange::new(spec.layout().image_base(), spec.layout().image_base() + 0x4000).unwrap();
        assert_eq!(spec.validate_image_range(image), Ok(()));
        let bad = VirtRange::new(spec.layout().stack_range().start(), spec.layout().stack_range().end()).unwrap();
        assert_eq!(spec.validate_image_range(bad), Err(AddressSpaceError::ImageOutsideUserSpace));
    }
}
