//! Virtual-address policy and process-independent address-space layout.
//!
//! This module owns policy only. Page-table mutation belongs to the mapper
//! backend, while process lifecycle belongs to the scheduler/process layer.

pub const USER_SPACE_START: u64 = 0x0000_0000_0001_0000;
pub const KERNEL_SPACE_START: u64 = 0xffff_8000_0000_0000;
pub const USER_STACK_SIZE: u64 = 16 * 4096;
pub const USER_STACK_GUARD_SIZE: u64 = 4096;
pub const USER_STACK_TOP: u64 = KERNEL_SPACE_START - 0x10_0000;
pub const USER_IMAGE_BASE: u64 = USER_SPACE_START + 0x10_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtRange { start: u64, end: u64 }

impl VirtRange {
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start <= end { Some(Self { start, end }) } else { None }
    }
    pub const fn start(self) -> u64 { self.start }
    pub const fn end(self) -> u64 { self.end }
    pub const fn len(self) -> u64 { self.end - self.start }
    pub const fn is_empty(self) -> bool { self.start == self.end }
    pub const fn contains(self, address: u64) -> bool { address >= self.start && address < self.end }
    pub const fn is_user(self) -> bool { self.start >= USER_SPACE_START && self.end <= KERNEL_SPACE_START }
    pub const fn is_kernel(self) -> bool { self.start >= KERNEL_SPACE_START }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserAddressSpaceLayout {
    user: VirtRange,
    stack: VirtRange,
    guard: VirtRange,
    image_base: u64,
}

impl UserAddressSpaceLayout {
    pub const fn default() -> Self {
        let stack_start = USER_STACK_TOP - USER_STACK_SIZE;
        let guard_start = stack_start - USER_STACK_GUARD_SIZE;
        Self {
            user: VirtRange { start: USER_SPACE_START, end: KERNEL_SPACE_START },
            stack: VirtRange { start: stack_start, end: USER_STACK_TOP },
            guard: VirtRange { start: guard_start, end: stack_start },
            image_base: USER_IMAGE_BASE,
        }
    }
    pub const fn user_range(self) -> VirtRange { self.user }
    pub const fn stack_range(self) -> VirtRange { self.stack }
    pub const fn guard_range(self) -> VirtRange { self.guard }
    pub const fn image_base(self) -> u64 { self.image_base }
    pub const fn stack_top(self) -> u64 { self.stack.end }
    pub const fn is_stack_address(self, address: u64) -> bool { self.stack.contains(address) }
    pub const fn is_guard_address(self, address: u64) -> bool { self.guard.contains(address) }
}

#[cfg(test)]
mod tests {
    use super::{KERNEL_SPACE_START, USER_IMAGE_BASE, USER_SPACE_START, UserAddressSpaceLayout, VirtRange};

    #[test]
    fn rejects_inverted_range() { assert_eq!(VirtRange::new(2, 1), None); }
    #[test]
    fn recognizes_user_range() {
        let range = VirtRange::new(USER_SPACE_START, USER_SPACE_START + 0x4000).unwrap();
        assert!(range.is_user());
        assert!(!range.is_kernel());
    }
    #[test]
    fn recognizes_kernel_range() {
        let range = VirtRange::new(KERNEL_SPACE_START, KERNEL_SPACE_START + 0x4000).unwrap();
        assert!(range.is_kernel());
        assert!(!range.is_user());
    }
    #[test]
    fn default_user_layout_is_disjoint() {
        let layout = UserAddressSpaceLayout::default();
        assert!(layout.user_range().is_user());
        assert!(layout.stack_range().is_user());
        assert!(layout.guard_range().is_user());
        assert!(!layout.is_guard_address(layout.stack_top() - 1));
        assert!(layout.is_guard_address(layout.guard_range().start()));
        assert_eq!(layout.image_base(), USER_IMAGE_BASE);
        assert_eq!(layout.stack_range().len(), 16 * 4096);
        assert_eq!(layout.guard_range().len(), 4096);
    }
    #[test]
    fn stack_is_below_kernel_boundary() {
        let layout = UserAddressSpaceLayout::default();
        assert!(layout.stack_top() < KERNEL_SPACE_START);
    }
}
