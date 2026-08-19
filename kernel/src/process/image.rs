//! Minimal ELF64 image validation for the first userspace loader.
//!
//! This parser intentionally does not allocate or map memory. It validates the
//! immutable executable metadata first, leaving address-space mutation to the
//! process loader.

use crate::memory::VirtRange;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    UnsupportedClass,
    UnsupportedEndian,
    UnsupportedType,
    UnsupportedMachine,
    InvalidProgramTable,
    InvalidSegmentRange,
    InvalidEntry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadSegment {
    virtual_range: VirtRange,
    flags: u32,
    file_offset: u64,
    file_size: u64,
    memory_size: u64,
}

impl LoadSegment {
    pub const fn virtual_range(self) -> VirtRange { self.virtual_range }
    pub const fn flags(self) -> u32 { self.flags }
    pub const fn executable(self) -> bool { self.flags & PF_X != 0 }
    pub const fn writable(self) -> bool { self.flags & PF_W != 0 }
    pub const fn readable(self) -> bool { self.flags & PF_R != 0 }
    pub const fn file_offset(self) -> u64 { self.file_offset }
    pub const fn file_size(self) -> u64 { self.file_size }
    pub const fn memory_size(self) -> u64 { self.memory_size }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElfImage<'a> {
    bytes: &'a [u8],
    entry: u64,
    segments: [Option<LoadSegment>; 8],
    segment_count: usize,
}

impl<'a> ElfImage<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ElfError> {
        if bytes.len() < 64 { return Err(ElfError::TooSmall); }
        if bytes[0..4] != ELF_MAGIC { return Err(ElfError::BadMagic); }
        if bytes[4] != ELFCLASS64 { return Err(ElfError::UnsupportedClass); }
        if bytes[5] != ELFDATA2LSB { return Err(ElfError::UnsupportedEndian); }

        let kind = read_u16(bytes, 16).ok_or(ElfError::TooSmall)?;
        if kind != ET_EXEC { return Err(ElfError::UnsupportedType); }
        if read_u16(bytes, 18).ok_or(ElfError::TooSmall)? != EM_X86_64 {
            return Err(ElfError::UnsupportedMachine);
        }

        let entry = read_u64(bytes, 24).ok_or(ElfError::TooSmall)?;
        let phoff = read_u64(bytes, 32).ok_or(ElfError::TooSmall)? as usize;
        let phentsize = read_u16(bytes, 54).ok_or(ElfError::TooSmall)? as usize;
        let phnum = read_u16(bytes, 56).ok_or(ElfError::TooSmall)? as usize;

        if phentsize < 56 || phnum == 0 || phnum > 8 {
            return Err(ElfError::InvalidProgramTable);
        }
        let table_size = phentsize.checked_mul(phnum).ok_or(ElfError::InvalidProgramTable)?;
        let table_end = phoff.checked_add(table_size).ok_or(ElfError::InvalidProgramTable)?;
        if table_end > bytes.len() { return Err(ElfError::InvalidProgramTable); }

        let mut segments = [None; 8];
        let mut segment_count = 0;
        let mut entry_is_loaded = false;
        for index in 0..phnum {
            let base = phoff + index * phentsize;
            let kind = read_u32(bytes, base).ok_or(ElfError::InvalidProgramTable)?;
            if kind != PT_LOAD { continue; }

            let flags = read_u32(bytes, base + 4).ok_or(ElfError::InvalidProgramTable)?;
            let file_offset = read_u64(bytes, base + 8).ok_or(ElfError::InvalidSegmentRange)?;
            let virtual_address = read_u64(bytes, base + 16).ok_or(ElfError::InvalidSegmentRange)?;
            let file_size = read_u64(bytes, base + 32).ok_or(ElfError::InvalidSegmentRange)?;
            let memory_size = read_u64(bytes, base + 40).ok_or(ElfError::InvalidSegmentRange)?;

            if memory_size < file_size { return Err(ElfError::InvalidSegmentRange); }
            let file_end = file_offset.checked_add(file_size).ok_or(ElfError::InvalidSegmentRange)?;
            if file_end > bytes.len() as u64 { return Err(ElfError::InvalidSegmentRange); }
            let virtual_end = virtual_address.checked_add(memory_size).ok_or(ElfError::InvalidSegmentRange)?;
            let range = VirtRange::new(virtual_address, virtual_end).ok_or(ElfError::InvalidSegmentRange)?;
            if range.len() == 0 || !range.is_user() { return Err(ElfError::InvalidSegmentRange); }

            if range.contains(entry) { entry_is_loaded = true; }
            segments[segment_count] = Some(LoadSegment { virtual_range: range, flags, file_offset, file_size, memory_size });
            segment_count += 1;
        }

        if segment_count == 0 || !entry_is_loaded { return Err(ElfError::InvalidEntry); }
        Ok(Self { bytes, entry, segments, segment_count })
    }

    pub const fn entry(self) -> u64 { self.entry }
    pub const fn bytes(self) -> &'a [u8] { self.bytes }
    pub const fn segment_count(self) -> usize { self.segment_count }
    pub fn segment(self, index: usize) -> Option<LoadSegment> {
        if index >= self.segment_count { None } else { self.segments[index] }
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    Some(u64::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::{ElfError, ElfImage};

    fn minimal_elf() -> [u8; 120] {
        let mut bytes = [0u8; 120];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        let p = 64usize;
        bytes[p..p + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[p + 4..p + 8].copy_from_slice(&5u32.to_le_bytes());
        bytes[p + 8..p + 16].copy_from_slice(&0u64.to_le_bytes());
        bytes[p + 16..p + 24].copy_from_slice(&0x401000u64.to_le_bytes());
        bytes[p + 32..p + 40].copy_from_slice(&16u64.to_le_bytes());
        bytes[p + 40..p + 48].copy_from_slice(&0x1000u64.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_minimal_x86_64_image() {
        let image = ElfImage::parse(&minimal_elf()).unwrap();
        assert_eq!(image.entry(), 0x401000);
        assert_eq!(image.segment_count(), 1);
        assert!(image.segment(0).unwrap().executable());
    }

    #[test]
    fn rejects_non_elf_input() {
        assert_eq!(ElfImage::parse(b"not elf"), Err(ElfError::TooSmall));
    }

    #[test]
    fn rejects_dynamic_elf_until_load_bias_exists() {
        let mut bytes = minimal_elf();
        bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::UnsupportedType));
    }

    #[test]
    fn rejects_entry_outside_loaded_segment() {
        let mut bytes = minimal_elf();
        bytes[24..32].copy_from_slice(&0x900000u64.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::InvalidEntry));
    }

    #[test]
    fn rejects_segment_larger_than_file() {
        let mut bytes = minimal_elf();
        bytes[32..40].copy_from_slice(&80u64.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::InvalidSegmentRange));
    }
}
