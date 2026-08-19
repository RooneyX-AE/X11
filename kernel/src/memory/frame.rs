//! Physical frame allocation over bootloader-reported usable memory.
//!
//! The first allocator is intentionally monotonic. It provides a deterministic
//! early-boot source of 4 KiB frames while the kernel is still single-threaded.
//! Transaction checkpoints allow higher layers to abandon a failed bootstrap
//! construction without leaking the allocator cursor.

use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};

use super::region::PhysRange;

pub const FRAME_SIZE: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame(u64);

impl Frame {
    pub const fn from_start_address(address: u64) -> Option<Self> {
        if address % FRAME_SIZE == 0 { Some(Self(address)) } else { None }
    }
    pub const fn start_address(self) -> u64 { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContiguousFrames { start: u64, frame_count: usize }

impl ContiguousFrames {
    pub const fn start_address(self) -> u64 { self.start }
    pub const fn frame_count(self) -> usize { self.frame_count }
    pub fn byte_len(self) -> Option<usize> { self.frame_count.checked_mul(FRAME_SIZE as usize) }
    pub fn byte_end(self) -> Option<u64> {
        let bytes = self.frame_count.checked_mul(FRAME_SIZE as usize)? as u64;
        self.start.checked_add(bytes)
    }
    pub fn physical_range(self) -> Option<PhysRange> { PhysRange::new(self.start, self.byte_end()?) }
}

pub trait FrameAllocator { fn allocate_frame(&mut self) -> Option<Frame>; }

#[derive(Clone, Copy)]
pub struct EarlyFrameAllocator<'a> {
    regions: core::slice::Iter<'a, MemoryRegion>,
    current_end: u64,
    next_address: u64,
}

impl<'a> EarlyFrameAllocator<'a> {
    pub fn new(regions: &'a MemoryRegions) -> Self {
        Self { regions: regions.iter(), current_end: 0, next_address: 0 }
    }

    /// Captures the allocator cursor before a transactional construction.
    pub const fn checkpoint(&self) -> Self {
        *self
    }

    /// Restores a previously captured cursor. No frames may be concurrently
    /// allocated while restoring an early-boot checkpoint.
    pub const fn rollback(&mut self, checkpoint: Self) {
        *self = checkpoint;
    }

    fn select_next_usable_region(&mut self) -> bool {
        for region in &mut self.regions {
            if region.kind != MemoryRegionKind::Usable { continue; }
            let Some(range) = PhysRange::new(region.start, region.end) else { continue; };
            let Some(start) = align_up(range.start()) else { continue; };
            if start >= range.end() { continue; }
            self.next_address = start;
            self.current_end = range.end();
            return true;
        }
        false
    }

    pub fn allocate_contiguous(&mut self, frame_count: usize) -> Option<ContiguousFrames> {
        if frame_count == 0 { return None; }
        let bytes = frame_count.checked_mul(FRAME_SIZE as usize)? as u64;
        loop {
            if self.next_address < self.current_end {
                let end = self.next_address.checked_add(bytes)?;
                if end <= self.current_end {
                    let range = ContiguousFrames { start: self.next_address, frame_count };
                    self.next_address = end;
                    return Some(range);
                }
            }
            self.next_address = 0;
            self.current_end = 0;
            if !self.select_next_usable_region() { return None; }
        }
    }
}

impl FrameAllocator for EarlyFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<Frame> {
        loop {
            if self.next_address < self.current_end {
                let frame = Frame::from_start_address(self.next_address)?;
                self.next_address = self.next_address.checked_add(FRAME_SIZE)?;
                return Some(frame);
            }
            self.next_address = 0;
            self.current_end = 0;
            if !self.select_next_usable_region() { return None; }
        }
    }
}

fn align_up(address: u64) -> Option<u64> {
    let mask = FRAME_SIZE - 1;
    address.checked_add(mask).map(|value| value & !mask)
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use bootloader_api::info::{MemoryRegion, MemoryRegionKind};
    use super::{ContiguousFrames, EarlyFrameAllocator, Frame, FrameAllocator, FRAME_SIZE};

    fn regions(items: &'static mut [MemoryRegion]) -> bootloader_api::info::MemoryRegions { items.into() }
    fn region(start: u64, end: u64, kind: MemoryRegionKind) -> MemoryRegion { MemoryRegion { start, end, kind } }

    #[test]
    fn aligned_address_constructs_frame() {
        let frame = Frame::from_start_address(0x4000).unwrap();
        assert_eq!(frame.start_address(), 0x4000);
    }
    #[test]
    fn unaligned_address_is_rejected() { assert!(Frame::from_start_address(0x4001).is_none()); }
    #[test]
    fn frame_size_is_4k() { assert_eq!(FRAME_SIZE, 4096); }
    #[test]
    fn allocator_skips_reserved_regions() {
        let items = Box::leak(Box::new([
            region(0x0000, 0x2000, MemoryRegionKind::Reserved),
            region(0x3000, 0x6000, MemoryRegionKind::Usable),
        ]));
        let regions = regions(items);
        let mut allocator = EarlyFrameAllocator::new(&regions);
        assert_eq!(allocator.allocate_frame().unwrap().start_address(), 0x3000);
        assert_eq!(allocator.allocate_frame().unwrap().start_address(), 0x4000);
        assert_eq!(allocator.allocate_frame().unwrap().start_address(), 0x5000);
        assert!(allocator.allocate_frame().is_none());
    }
    #[test]
    fn allocator_aligns_region_start_up() {
        let items = Box::leak(Box::new([region(0x1001, 0x3000, MemoryRegionKind::Usable)]));
        let regions = regions(items);
        let mut allocator = EarlyFrameAllocator::new(&regions);
        assert_eq!(allocator.allocate_frame().unwrap().start_address(), 0x2000);
        assert!(allocator.allocate_frame().is_none());
    }
    #[test]
    fn contiguous_allocation_consumes_one_region() {
        let items = Box::leak(Box::new([region(0x4000, 0x10000, MemoryRegionKind::Usable)]));
        let regions = regions(items);
        let mut allocator = EarlyFrameAllocator::new(&regions);
        let frames = allocator.allocate_contiguous(4).unwrap();
        assert_eq!(frames.start_address(), 0x4000);
        assert_eq!(frames.frame_count(), 4);
        assert_eq!(frames.byte_len(), Some(4 * FRAME_SIZE as usize));
        assert_eq!(frames.byte_end(), Some(0x8000));
        assert_eq!(allocator.allocate_frame().unwrap().start_address(), 0x8000);
    }
    #[test]
    fn contiguous_allocation_does_not_cross_region_boundaries() {
        let items = Box::leak(Box::new([
            region(0x4000, 0x8000, MemoryRegionKind::Usable),
            region(0x9000, 0xB000, MemoryRegionKind::Usable),
        ]));
        let regions = regions(items);
        let mut allocator = EarlyFrameAllocator::new(&regions);
        assert_eq!(allocator.allocate_contiguous(2), Some(ContiguousFrames { start: 0x4000, frame_count: 2 }));
        assert_eq!(allocator.allocate_contiguous(2), Some(ContiguousFrames { start: 0x9000, frame_count: 2 }));
    }

    #[test]
    fn checkpoint_restores_allocator_cursor() {
        let items = Box::leak(Box::new([region(0x4000, 0x8000, MemoryRegionKind::Usable)]));
        let regions = regions(items);
        let mut allocator = EarlyFrameAllocator::new(&regions);
        let checkpoint = allocator.checkpoint();
        assert_eq!(allocator.allocate_frame().unwrap().start_address(), 0x4000);
        assert_eq!(allocator.allocate_frame().unwrap().start_address(), 0x5000);
        allocator.rollback(checkpoint);
        assert_eq!(allocator.allocate_frame().unwrap().start_address(), 0x4000);
    }
}
