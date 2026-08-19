//! Page-safe userspace load planning.
//!
//! This layer converts validated ELF segments into a deterministic mapping
//! plan. It performs no page-table writes and owns no physical frames.

use crate::memory::VirtRange;

use super::{AddressSpaceError, AddressSpaceSpec, ElfImage, LoadSegment};

const PAGE_SIZE: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadPlanError {
    AddressSpace(AddressSpaceError),
    SegmentOverlap,
    WritableExecutableSegment,
    InvalidEntry,
    TooManySegments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentMapping {
    virtual_range: VirtRange,
    flags: u32,
}

impl SegmentMapping {
    pub const fn virtual_range(self) -> VirtRange { self.virtual_range }
    pub const fn flags(self) -> u32 { self.flags }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadPlan {
    entry: u64,
    segments: [Option<SegmentMapping>; 8],
    count: usize,
}

impl LoadPlan {
    pub fn build(address_space: AddressSpaceSpec, image: ElfImage<'_>) -> Result<Self, LoadPlanError> {
        if image.segment_count() > 8 { return Err(LoadPlanError::TooManySegments); }

        let mut segments: [Option<SegmentMapping>; 8] = [None; 8];
        let mut count = 0;
        let mut entry_executable = false;

        for index in 0..image.segment_count() {
            let segment = image.segment(index).ok_or(LoadPlanError::TooManySegments)?;
            let mapped = map_span(segment)?;
            address_space.validate_image_range(mapped).map_err(LoadPlanError::AddressSpace)?;

            if segment.writable() && segment.executable() {
                return Err(LoadPlanError::WritableExecutableSegment);
            }
            if segment.executable() && mapped.contains(image.entry()) {
                entry_executable = true;
            }

            for prior in segments.iter().flatten() {
                if ranges_overlap(mapped, prior.virtual_range()) {
                    return Err(LoadPlanError::SegmentOverlap);
                }
            }

            segments[count] = Some(SegmentMapping { virtual_range: mapped, flags: segment.flags() });
            count += 1;
        }

        if !entry_executable { return Err(LoadPlanError::InvalidEntry); }
        Ok(Self { entry: image.entry(), segments, count })
    }

    pub const fn entry(self) -> u64 { self.entry }
    pub const fn count(self) -> usize { self.count }
    pub fn segment(self, index: usize) -> Option<SegmentMapping> {
        if index >= self.count { None } else { self.segments[index] }
    }
}

fn map_span(segment: LoadSegment) -> Result<VirtRange, LoadPlanError> {
    let range = segment.virtual_range();
    let start = range.start() & !(PAGE_SIZE - 1);
    let end = range.end().checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
        .ok_or(LoadPlanError::AddressSpace(AddressSpaceError::InvalidUserRange))?;
    VirtRange::new(start, end).ok_or(LoadPlanError::AddressSpace(AddressSpaceError::InvalidUserRange))
}

fn ranges_overlap(left: VirtRange, right: VirtRange) -> bool {
    left.start() < right.end() && right.start() < left.end()
}

#[cfg(test)]
mod tests {
    use super::{LoadPlan, LoadPlanError};
    use crate::memory::VirtRange;
    use crate::process::{AddressSpaceId, AddressSpaceSpec, ElfImage};

    fn image() -> [u8; 120] {
        let mut bytes = [0u8; 120];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[24..32].copy_from_slice(&0x401001u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        let p = 64usize;
        bytes[p..p + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[p + 4..p + 8].copy_from_slice(&5u32.to_le_bytes());
        bytes[p + 16..p + 24].copy_from_slice(&0x401001u64.to_le_bytes());
        bytes[p + 32..p + 40].copy_from_slice(&16u64.to_le_bytes());
        bytes[p + 40..p + 48].copy_from_slice(&0x1000u64.to_le_bytes());
        bytes
    }

    #[test]
    fn plans_page_aligned_segment() {
        let image = ElfImage::parse(&image()).unwrap();
        let spec = AddressSpaceSpec::new(AddressSpaceId::new(1).unwrap());
        let plan = LoadPlan::build(spec, image).unwrap();
        assert_eq!(plan.count(), 1);
        let range: VirtRange = plan.segment(0).unwrap().virtual_range();
        assert_eq!(range.start(), 0x401000);
        assert_eq!(range.end(), 0x402000);
        assert_eq!(plan.entry(), 0x401001);
    }

    #[test]
    fn entry_must_be_in_executable_segment() {
        let mut bytes = image();
        bytes[24..32].copy_from_slice(&0x401001u64.to_le_bytes());
        bytes[68..72].copy_from_slice(&6u32.to_le_bytes());
        let parsed = ElfImage::parse(&bytes).unwrap();
        let spec = AddressSpaceSpec::new(AddressSpaceId::new(2).unwrap());
        assert_eq!(LoadPlan::build(spec, parsed), Err(LoadPlanError::InvalidEntry));
    }
}
