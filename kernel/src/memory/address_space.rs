//! Virtual-address policy shared by paging and higher-level address-space code.
//!
//! This module intentionally contains no page-table writes. It defines the
//! invariants that later mapping code must preserve, keeping address-space
//! policy independent from a particular paging implementation.

/// Lowest canonical user-space virtual address used by X11-OS.
///
/// The initial policy keeps the lower half available for userspace and avoids
/// reserving arbitrary addresses in early kernel code. The exact userspace
/// layout can evolve behind the address-space API later.
pub const USER_SPACE_START: u64 = 0x0000_0000_0001_0000;

/// First virtual address reserved for the kernel half.
///
/// x86_64 canonical addresses are currently constrained by the architecture
/// implementation. This constant deliberately sits in the conventional high
/// half and is treated as policy, not as a hardware requirement.
pub const KERNEL_SPACE_START: u64 = 0xffff_8000_0000_0000;

/// A validated virtual-address range with half-open semantics: `[start, end)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtRange {
    start: u64,
    end: u64,
}

impl VirtRange {
    /// Creates a non-inverted virtual range.
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start <= end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    pub const fn start(self) -> u64 {
        self.start
    }

    pub const fn end(self) -> u64 {
        self.end
    }

    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    pub const fn contains(self, address: u64) -> bool {
        address >= self.start && address < self.end
    }

    pub const fn is_user(self) -> bool {
        self.start >= USER_SPACE_START && self.end <= KERNEL_SPACE_START
    }

    pub const fn is_kernel(self) -> bool {
        self.start >= KERNEL_SPACE_START
    }
}

#[cfg(test)]
mod tests {
    use super::{KERNEL_SPACE_START, USER_SPACE_START, VirtRange};

    #[test]
    fn rejects_inverted_range() {
        assert_eq!(VirtRange::new(2, 1), None);
    }

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
}
