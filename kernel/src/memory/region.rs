//! Physical-memory range primitives.
//!
//! This module intentionally knows nothing about firmware or the bootloader.
//! It provides small, architecture-neutral types that later allocators and
//! page-table code can depend on without importing boot-time implementation
//! details.

/// A half-open physical address range: `[start, end)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysRange {
    start: u64,
    end: u64,
}

impl PhysRange {
    /// Creates a range if `start <= end`.
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start <= end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    /// Returns the first physical address in the range.
    pub const fn start(self) -> u64 {
        self.start
    }

    /// Returns the exclusive end address of the range.
    pub const fn end(self) -> u64 {
        self.end
    }

    /// Returns the range length in bytes.
    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    /// Returns whether the range contains the address.
    pub const fn contains(self, address: u64) -> bool {
        address >= self.start && address < self.end
    }

    /// Returns the number of 4 KiB pages touched by this aligned range.
    ///
    /// The range must already be page aligned. Failing that invariant is a
    /// programmer error and returns `None` instead of silently rounding.
    pub const fn page_count_4k(self) -> Option<u64> {
        const PAGE_SIZE: u64 = 4096;
        if self.start % PAGE_SIZE != 0 || self.end % PAGE_SIZE != 0 {
            return None;
        }
        Some(self.len() / PAGE_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::PhysRange;

    #[test]
    fn accepts_empty_range() {
        assert_eq!(PhysRange::new(0x1000, 0x1000).map(PhysRange::len), Some(0));
    }

    #[test]
    fn rejects_inverted_range() {
        assert_eq!(PhysRange::new(0x2000, 0x1000), None);
    }

    #[test]
    fn contains_uses_half_open_semantics() {
        let range = PhysRange::new(0x1000, 0x3000).unwrap();
        assert!(range.contains(0x1000));
        assert!(range.contains(0x2fff));
        assert!(!range.contains(0x3000));
    }

    #[test]
    fn counts_only_aligned_pages() {
        assert_eq!(PhysRange::new(0, 0x3000).unwrap().page_count_4k(), Some(3));
        assert_eq!(PhysRange::new(1, 0x3000).unwrap().page_count_4k(), None);
    }
}
