//! Transactional userspace segment mapper.
//!
//! The early frame allocator is monotonic, so a failed load can roll back
//! virtual mappings but cannot return physical frames. That ownership rule is
//! explicit here rather than pretending this is a fully reclaiming transaction.

use crate::memory::{EarlyFrameAllocator, MappingError, MappingFlags, Page4K, PageTableMapper, PAGE_SIZE_4K};

use super::LoadPlan;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadError {
    InvalidPage,
    Mapping(MappingError),
    TooManyPages,
    InvalidFlags,
    RollbackFailed,
}

pub const MAX_MAPPED_PAGES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MappedPage {
    pub virtual_address: u64,
    pub physical_address: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadResult {
    pub mapped_pages: usize,
    pub entry: u64,
    pages: [Option<MappedPage>; MAX_MAPPED_PAGES],
}

impl LoadResult {
    pub(crate) const fn from_parts(
        mapped_pages: usize,
        entry: u64,
        pages: [Option<MappedPage>; MAX_MAPPED_PAGES],
    ) -> Self {
        Self { mapped_pages, entry, pages }
    }

    pub fn page(self, index: usize) -> Option<MappedPage> {
        if index >= self.mapped_pages { None } else { self.pages[index] }
    }

    pub fn contains_page(self, virtual_address: u64) -> bool {
        self.page_base(virtual_address)
            .is_some_and(|page| (0..self.mapped_pages).any(|index| {
                self.page(index).is_some_and(|mapped| mapped.virtual_address == page)
            }))
    }

    fn page_base(self, virtual_address: u64) -> Option<u64> {
        Some(virtual_address & !(PAGE_SIZE_4K - 1))
    }
}

pub fn map_load_plan<M: PageTableMapper>(
    mapper: &mut M,
    allocator: &mut EarlyFrameAllocator<'_>,
    plan: LoadPlan,
) -> Result<LoadResult, LoadError> {
    let mut pages = [None; MAX_MAPPED_PAGES];
    let mut count = 0usize;

    for index in 0..plan.count() {
        let segment = plan.segment(index).ok_or(LoadError::InvalidPage)?;
        let flags = flags_from_elf(segment.flags()).ok_or(LoadError::InvalidFlags)?;
        let range = segment.virtual_range();
        let mut address = range.start();
        while address < range.end() {
            if count == MAX_MAPPED_PAGES {
                rollback(mapper, &pages, count)?;
                return Err(LoadError::TooManyPages);
            }

            let page = match Page4K::from_start_address(address) {
                Some(page) => page,
                None => {
                    rollback(mapper, &pages, count)?;
                    return Err(LoadError::InvalidPage);
                }
            };
            let frame = match allocator.allocate_frame() {
                Some(frame) => frame,
                None => {
                    rollback(mapper, &pages, count)?;
                    return Err(LoadError::Mapping(MappingError::FrameAllocationFailed));
                }
            };
            let physical_address = frame.start_address();

            match mapper.map_page(page, physical_address, flags) {
                Ok(flush) => flush.flush(),
                Err(error) => {
                    rollback(mapper, &pages, count)?;
                    return Err(LoadError::Mapping(error));
                }
            }

            pages[count] = Some(MappedPage { virtual_address: address, physical_address });
            count += 1;
            address = match address.checked_add(PAGE_SIZE_4K) {
                Some(value) => value,
                None => {
                    rollback(mapper, &pages, count)?;
                    return Err(LoadError::InvalidPage);
                }
            };
        }
    }

    Ok(LoadResult::from_parts(count, plan.entry(), pages))
}

fn rollback<M: PageTableMapper>(mapper: &mut M, pages: &[Option<MappedPage>; MAX_MAPPED_PAGES], count: usize) -> Result<(), LoadError> {
    for index in (0..count).rev() {
        let Some(mapped) = pages[index] else { continue; };
        let page = Page4K::from_start_address(mapped.virtual_address).ok_or(LoadError::InvalidPage)?;
        let (_, flush) = mapper.unmap_page(page).map_err(|_| LoadError::RollbackFailed)?;
        flush.flush();
    }
    Ok(())
}

fn flags_from_elf(flags: u32) -> Option<MappingFlags> {
    let readable = flags & 4 != 0;
    let writable = flags & 2 != 0;
    let executable = flags & 1 != 0;
    if writable && executable { return None; }
    match (readable, writable, executable) {
        (true, false, false) => Some(MappingFlags::read_only()),
        (true, true, false) => Some(MappingFlags::read_write()),
        (true, false, true) => Some(MappingFlags::read_execute()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::flags_from_elf;
    use crate::memory::MappingFlags;

    #[test]
    fn translates_elf_permissions() {
        assert_eq!(flags_from_elf(4), Some(MappingFlags::read_only()));
        assert_eq!(flags_from_elf(6), Some(MappingFlags::read_write()));
        assert_eq!(flags_from_elf(5), Some(MappingFlags::read_execute()));
        assert_eq!(flags_from_elf(3), None);
    }
}
