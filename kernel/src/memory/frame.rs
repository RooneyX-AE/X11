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

/// A physically contiguous run of 4 KiB frames already owned by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContiguousFrames {
    start: u64,
    frame_count: usize,
}

impl ContiguousFrames {
    pub const fn start_address(self) -> u64 {
        self.start
    }

    pub const fn frame_count(self) -> usize {
        self.frame_count
    }

    pub fn byte_len(self) -> Option<usize> {
        self.frame_count.checked_mul(FRAME_SIZE as usize)
    }

    pub fn byte_end(self) -> Option<u64> {
        let bytes = self.frame_count.checked_mul(FRAME_SIZE as usize)?;
        self.start.checked_add(bytes as u64)
    }

    pub fn physical_range(self) -> Option<PhysRange> {
        let end = self.byte_end()?;
        PhysRange::new(self.start, end)
    }
}

/// Interface used by page tables and other consumers that need physical frames.
pub trait FrameAllocator {
    /// Allocates one usable physical frame.
    fn allocate_frame(&mut self) -> Option<Frame>;
}

/// Deterministic early-boot allocator that walks usable memory regions once.
pub struct EarlyFrameAllocator<'a> {
    regions: core::slice::Iter<'a, MemoryRegion>,
    current_end: u64,
    next_address: u64,
}

impl<'a> EarlyFrameAllocator<'a> {
    /// Creates an allocator over a bootloader-provided memory map.
    pub fn new(regions: &'a MemoryRegions) -> Self {
        Self {
            regions: regions.iter(),
            current_end: 0,
            next_address: 0,
        }
    }

    fn select_next_usable_region(&mut self) -> bool {
        for region in &mut self.regions {
            if region.kind != MemoryRegionKind::Usable {
                continue;
            }
            let start = region.start;
            let end = region.end;
            if start >= end {
                continue;
            }
            let aligned_start = (start + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);
            if aligned_start >= end {
                continue;
            }
            self.current_end = end;
            self.next_address = aligned_start;
            return true;
        }
        false
    }
}

impl FrameAllocator for EarlyFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<Frame> {
        loop {
            if self.next_address >= self.current_end && !self.select_next_usable_region() {
                return None;
            }

            let address = self.next_address;
            let next = address.checked_add(FRAME_SIZE)?;
            if next > self.current_end {
                self.next_address = self.current_end;
                continue;
            }
            self.next_address = next;
            return Frame::from_start_address(address);
        }
    }
}

impl<'a> EarlyFrameAllocator<'a> {
    pub fn allocate_contiguous(&mut self, frame_count: usize) -> Option<ContiguousFrames> {
        if frame_count == 0 {
            return None;
        }
        let bytes = frame_count.checked_mul(FRAME_SIZE as usize)? as u64;
        loop {
            if self.next_address >= self.current_end && !self.select_next_usable_region() {
                return None;
            }
            let start = self.next_address;
            let end = start.checked_add(bytes)?;
            if end <= self.current_end {
                self.next_address = end;
                return Some(ContiguousFrames { start, frame_count });
            }
            self.next_address = self.current_end;
        }
    }
}

#[cfg(test)]
mod tests {
    use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};
    use alloc::boxed::Box;

    use super::{ContiguousFrames, EarlyFrameAllocator, Frame, FrameAllocator, FRAME_SIZE};

    fn region(start: u64, end: u64, kind: MemoryRegionKind) -> MemoryRegion {
        MemoryRegion { start, end, kind }
    }

    fn regions(items: &'static mut [MemoryRegion]) -> MemoryRegions {
        MemoryRegions::new(items)
    }

    #[test]
    fn frame_alignment_is_enforced() {
        assert_eq!(Frame::from_start_address(0x1000), Some(Frame(0x1000)));
        assert_eq!(Frame::from_start_address(0x1001), None);
        assert_eq!(FRAME_SIZE, 4096);
    }

    #[test]
    fn allocator_aligns_region_start_up() {
        let items = Box::leak(Box::new([region(
            0x1001,
            0x3000,
            MemoryRegionKind::Usable,
        )]));
        let regions = regions(items);
        let mut allocator = EarlyFrameAllocator::new(&regions);
        assert_eq!(allocator.allocate_frame(), Some(Frame(0x2000)));
    }

    #[test]
    fn contiguous_allocation_stays_inside_region() {
        let items = Box::leak(Box::new([region(
            0x1000,
            0x9000,
            MemoryRegionKind::Usable,
        )]));
        let regions = regions(items);
        let mut allocator = EarlyFrameAllocator::new(&regions);
        let contiguous = allocator.allocate_contiguous(2).unwrap();
        assert_eq!(contiguous.start_address(), 0x1000);
        assert_eq!(contiguous.frame_count(), 2);
        assert_eq!(contiguous.byte_len(), Some(8192));
        assert_eq!(contiguous.byte_end(), Some(0x3000));
    }

    #[test]
    fn contiguous_physical_range_is_half_open() {
        let frames = ContiguousFrames { start: 0x4000, frame_count: 3 };
        assert_eq!(frames.physical_range().unwrap().start(), 0x4000);
        assert_eq!(frames.physical_range().unwrap().end(), 0x7000);
    }
}
