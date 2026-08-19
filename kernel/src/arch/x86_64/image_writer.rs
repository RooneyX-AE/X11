//! x86_64 backend for populating already-mapped userspace pages.
//!
//! The backend is the only layer allowed to turn a physical frame address into
//! a raw pointer through the bootloader-provided physical-memory direct map.
//! All arithmetic is checked before the pointer is formed.

use crate::memory::PhysicalMemoryMapping;
use crate::process::ImagePageWriter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageWriteError {
    AddressOverflow,
    EmptyRange,
}

pub struct X86ImagePageWriter {
    mapping: PhysicalMemoryMapping,
}

impl X86ImagePageWriter {
    pub const fn new(mapping: PhysicalMemoryMapping) -> Self {
        Self { mapping }
    }

    fn mapped_address(&self, physical_address: u64, page_offset: usize, length: usize) -> Result<usize, ImageWriteError> {
        if length == 0 {
            return Err(ImageWriteError::EmptyRange);
        }

        let offset = page_offset as u64;
        let length_u64 = length as u64;
        let physical_start = physical_address
            .checked_add(offset)
            .ok_or(ImageWriteError::AddressOverflow)?;
        let physical_end = physical_start
            .checked_add(length_u64)
            .ok_or(ImageWriteError::AddressOverflow)?;
        let virtual_start = self.mapping.translate(physical_start).ok_or(ImageWriteError::AddressOverflow)?;
        let virtual_end = self.mapping.translate(physical_end).ok_or(ImageWriteError::AddressOverflow)?;

        if virtual_end < virtual_start
            || virtual_start > usize::MAX as u64
            || virtual_end > (usize::MAX as u64).saturating_add(1)
        {
            return Err(ImageWriteError::AddressOverflow);
        }

        Ok(virtual_start as usize)
    }
}

impl ImagePageWriter for X86ImagePageWriter {
    type Error = ImageWriteError;

    fn copy_into_page(
        &mut self,
        physical_address: u64,
        page_offset: usize,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        let destination = self.mapped_address(physical_address, page_offset, bytes.len())?;

        // SAFETY: The caller only supplies frames returned by the kernel frame
        // allocator and this backend is constructed from the bootloader's
        // physical-memory direct-map offset. The checked address arithmetic
        // guarantees the destination range does not wrap.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), destination as *mut u8, bytes.len());
        }
        Ok(())
    }

    fn zero_page_range(
        &mut self,
        physical_address: u64,
        page_offset: usize,
        length: usize,
    ) -> Result<(), Self::Error> {
        let destination = self.mapped_address(physical_address, page_offset, length)?;

        // SAFETY: Same direct-map and checked-address guarantees as
        // `copy_into_page` apply here.
        unsafe {
            core::ptr::write_bytes(destination as *mut u8, 0, length);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ImageWriteError, X86ImagePageWriter};
    use crate::memory::PhysicalMemoryMapping;

    #[test]
    fn rejects_empty_write() {
        let writer = X86ImagePageWriter::new(PhysicalMemoryMapping::new(0x8000_0000));
        assert_eq!(writer.mapped_address(0x1000, 0, 0), Err(ImageWriteError::EmptyRange));
    }

    #[test]
    fn translates_page_offset_with_checked_math() {
        let writer = X86ImagePageWriter::new(PhysicalMemoryMapping::new(0x8000_0000));
        assert_eq!(writer.mapped_address(0x3000, 128, 64), Ok(0x8000_3080usize));
    }

    #[test]
    fn rejects_translation_overflow() {
        let writer = X86ImagePageWriter::new(PhysicalMemoryMapping::new(u64::MAX - 0x10));
        assert_eq!(writer.mapped_address(0x20, 0, 1), Err(ImageWriteError::AddressOverflow));
    }
}
