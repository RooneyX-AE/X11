//! Process-facing address-space ownership contract.
//!
//! The process layer owns identity and lifetime. Architecture-specific page
//! table objects remain behind the memory subsystem, keeping process code from
//! depending on x86_64 paging details.

use crate::memory::UserAddressSpaceLayout;

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

impl AddressSpaceSpec {
    pub const fn new(id: AddressSpaceId) -> Self {
        Self { id, layout: UserAddressSpaceLayout::default() }
    }

    pub const fn id(self) -> AddressSpaceId { self.id }
    pub const fn layout(self) -> UserAddressSpaceLayout { self.layout }
}

#[cfg(test)]
mod tests {
    use super::{AddressSpaceId, AddressSpaceSpec};

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
}
