//! Physical frame allocation over bootloader-reported usable memory.
//!
//! The first allocator is intentionally monotonic. It provides a deterministic
//! early-boot source of 4 KiB frames while the kernel is still single-threaded.
//! A reclaiming allocator can implement the same public trait later without
//! leaking allocator-specific details into page-table or process code.

use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};

use super::region::PhysRange;

/// The size of a base x86_64 page in bytes.
pub const FRAME_SIZE: u64 = 4096;

/// A single 4 KiB physical frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame(u64);

impl Frame {
    /// Creates a frame from an aligned physical address.
    pub const fn from_start_address(address: u64) -> Option<Self> {
        if address % FRAME_SIZE == 0 {
            Some(Self(address))
        } else {
            None
        }
    }

    /// Returns the physical start address of the frame.
    pub const fn start_address(self) -> u64 {
        self.0
    }
}

/// Interface used by page tables and other consumers that need physical frames.
pub trait FrameAllocator {
    /// Allocates one usable physical frame.
    fn allocate_frame(&mut self) -> Option<Frame>;
}

/// Deterministic early-boot allocator that walks usable memory regions once.
pub struct EarlyFrameAllocator<'a> {
    regions: &'a MemoryRegions,
    region_index: usize,
    next_address: u64,
}

impl<'a> EarlyFrameAllocator<'a> {
    /// Creates an allocator over a bootloader-provided memory map.
    pub fn new(regions: &'a MemoryRegions) -> Self {
        Self {
            regions,
            region_index: 0,
            next_address: 0,
        }
    }

    fn advance_to_usable_region(&mut self) -> Option<PhysRange> {
        while self.region_index < self.regions.len() {
            let region: &MemoryRegion = &self.regions[self.region_index];
            self.region_index += 1;

            if region.kind != MemoryRegionKind::Usable {
                continue;
            }

            let Some(range) = PhysRange::new(region.start, region.end) else {
                continue;
            };

            let start = align_up(range.start());
            if start >= range.end() {
                continue;
            }

            self.next_address = start;
            return Some(range);
        }

        None
    }
}

impl FrameAllocator for EarlyFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<Frame> {
        loop {
            let current = if self.region_index == 0 || self.next_address == 0 {
                self.advance_to_usable_region()?
            } else {
                let previous = self.region_index.saturating_sub(1);
                &self.regions[previous]
            };

            if self.next_address < current.end {
                let frame = Frame::from_start_address(self.next_address)?;
                self.next_address = self.next_address.checked_add(FRAME_SIZE)?;
                return Some(frame);
            }

            self.next_address = 0;
            if self.region_index >= self.regions.len() {
                return None;
            }
        }
    }
}

const fn align_up(address: u64) -> u64 {
    let mask = FRAME_SIZE - 1;
    address.checked_add(mask).map_or(u64::MAX, |value| value & !mask)
}

#[cfg(test)]
mod tests {
    use super::{FRAME_SIZE, Frame};

    #[test]
    fn aligned_address_constructs_frame() {
        let frame = Frame::from_start_address(0x4000).unwrap();
        assert_eq!(frame.start_address(), 0x4000);
    }

    #[test]
    fn unaligned_address_is_rejected() {
        assert!(Frame::from_start_address(0x4001).is_none());
    }

    #[test]
    fn frame_size_is_4k() {
        assert_eq!(FRAME_SIZE, 4096);
    }
}
