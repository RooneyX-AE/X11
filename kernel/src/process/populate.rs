//! ELF segment population over already-mapped user pages.
//!
//! This layer turns immutable ELF bytes into page-local copy/zero operations.
//! Every mapped userspace page is cleared before file bytes are copied so
//! unused leading/trailing bytes and BSS are deterministic rather than exposing
//! stale physical-frame contents.
//! It deliberately does not perform raw physical-memory dereferences. An
//! architecture backend supplies `ImagePageWriter`, keeping unsafe direct-map
//! access out of the process loader.

use super::{ElfImage, LoadResult};
use crate::memory::PAGE_SIZE_4K;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopulateError {
    MissingMappedPage,
    SegmentOutsideMapping,
    FileRangeInvalid,
    AddressOverflow,
}

pub trait ImagePageWriter {
    type Error;

    fn copy_into_page(&mut self, physical_address: u64, page_offset: usize, bytes: &[u8]) -> Result<(), Self::Error>;
    fn zero_page_range(&mut self, physical_address: u64, page_offset: usize, length: usize) -> Result<(), Self::Error>;
}

pub fn populate_image<W: ImagePageWriter>(
    writer: &mut W,
    image: ElfImage<'_>,
    loaded: LoadResult,
) -> Result<(), PopulateError> {
    for segment_index in 0..image.segment_count() {
        let segment = image.segment(segment_index).ok_or(PopulateError::SegmentOutsideMapping)?;
        let segment_start = segment.virtual_range().start();
        let segment_end = segment.virtual_range().end();
        let file_start = segment.file_offset();
        let file_size = segment.file_size();
        let memory_size = segment.memory_size();

        let file_end = file_start.checked_add(file_size).ok_or(PopulateError::FileRangeInvalid)?;
        let file_bytes = image.bytes().get(file_start as usize..file_end as usize).ok_or(PopulateError::FileRangeInvalid)?;
        let memory_end = segment_start.checked_add(memory_size).ok_or(PopulateError::AddressOverflow)?;

        let mut offset = 0u64;
        while offset < memory_size {
            let virtual_address = segment_start.checked_add(offset).ok_or(PopulateError::AddressOverflow)?;
            if virtual_address >= segment_end || virtual_address >= memory_end {
                return Err(PopulateError::SegmentOutsideMapping);
            }

            let page_base = virtual_address & !(PAGE_SIZE_4K - 1);
            let page_offset = (virtual_address - page_base) as usize;
            let remaining_memory = (memory_size - offset) as usize;
            let page_room = PAGE_SIZE_4K as usize - page_offset;
            let chunk = remaining_memory.min(page_room);
            let mapped_index = find_mapped_page(loaded, page_base).ok_or(PopulateError::MissingMappedPage)?;
            let physical = loaded.page(mapped_index).ok_or(PopulateError::MissingMappedPage)?.physical_address;

            // Early-boot frames are not guaranteed to be zeroed. Clear the
            // entire mapped page before writing segment bytes so prefix/suffix
            // padding and BSS cannot expose stale physical-frame contents.
            writer.zero_page_range(physical, 0, PAGE_SIZE_4K as usize)
                .map_err(|_| PopulateError::FileRangeInvalid)?;

            let file_chunk_start = offset.min(file_size) as usize;
            let file_remaining = file_bytes.len().saturating_sub(file_chunk_start);
            let copy_len = file_remaining.min(chunk);
            if copy_len != 0 {
                let source_end = file_chunk_start.checked_add(copy_len).ok_or(PopulateError::FileRangeInvalid)?;
                writer.copy_into_page(physical, page_offset, &file_bytes[file_chunk_start..source_end])
                    .map_err(|_| PopulateError::FileRangeInvalid)?;
            }

            offset = offset.checked_add(chunk as u64).ok_or(PopulateError::AddressOverflow)?;
        }
    }
    Ok(())
}

fn find_mapped_page(loaded: LoadResult, virtual_address: u64) -> Option<usize> {
    (0..loaded.mapped_pages).find(|&index| loaded.page(index).is_some_and(|page| page.virtual_address == virtual_address))
}

#[cfg(test)]
mod tests {
    use super::find_mapped_page;
    use super::PopulateError;
    use crate::process::{LoadResult, MappedPage};

    #[test]
    fn lookup_uses_exact_page_base() {
        let mut pages = [None; super::MAX_MAPPED_PAGES];
        pages[0] = Some(MappedPage { virtual_address: 0x401000, physical_address: 0x9000 });
        let load = LoadResult::from_parts(1, 0x401000, pages);
        assert_eq!(find_mapped_page(load, 0x401000), Some(0));
        assert_eq!(find_mapped_page(load, 0x401001), None);
    }

    #[test]
    fn zero_copy_errors_are_explicit() {
        assert_ne!(PopulateError::MissingMappedPage, PopulateError::FileRangeInvalid);
    }
}
