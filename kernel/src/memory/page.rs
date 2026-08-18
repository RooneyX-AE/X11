//! Virtual-page primitives independent of page-table implementation.

use super::address_space::VirtRange;

/// Base page size used by the initial x86_64 paging configuration.
pub const PAGE_SIZE_4K: u64 = 4096;

/// A single 4 KiB virtual page identified by its aligned start address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Page4K(u64);

impl Page4K {
    /// Creates a page from an aligned virtual address.
    pub const fn from_start_address(address: u64) -> Option<Self> {
        if address % PAGE_SIZE_4K == 0 {
            Some(Self(address))
        } else {
            None
        }
    }

    /// Returns the first byte covered by this page.
    pub const fn start_address(self) -> u64 {
        self.0
    }

    /// Returns the exclusive end address of this page when it does not overflow.
    pub const fn end_address(self) -> Option<u64> {
        self.0.checked_add(PAGE_SIZE_4K)
    }

    /// Returns the range represented by this page.
    pub const fn range(self) -> Option<VirtRange> {
        let Some(end) = self.end_address() else {
            return None;
        };
        VirtRange::new(self.start_address(), end)
    }
}

#[cfg(test)]
mod tests {
    use super::{PAGE_SIZE_4K, Page4K};

    #[test]
    fn accepts_aligned_address() {
        let page = Page4K::from_start_address(0x20_000).unwrap();
        assert_eq!(page.start_address(), 0x20_000);
        assert_eq!(page.end_address(), Some(0x21_000));
    }

    #[test]
    fn rejects_unaligned_address() {
        assert!(Page4K::from_start_address(0x20_001).is_none());
    }

    #[test]
    fn rejects_end_address_overflow() {
        let page = Page4K::from_start_address(u64::MAX - PAGE_SIZE_4K + 1).unwrap();
        assert_eq!(page.end_address(), None);
        assert_eq!(page.range(), None);
    }

    #[test]
    fn page_size_is_four_kib() {
        assert_eq!(PAGE_SIZE_4K, 4096);
    }
}
