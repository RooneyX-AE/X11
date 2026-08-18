//! Transactional userspace segment mapper.
//!
//! This layer allocates physical frames and installs page mappings, but does
//! not copy ELF bytes or enter ring 3. Any mapping failure rolls back every
//! page installed by this transaction.

use crate::memory::{EarlyFrameAllocator, MappingError, MappingFlags, Page4K, PageTableMapper, PAGE_SIZE_4K};

use super::{LoadPlan, LoadPlanError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadError {
    InvalidPage,
    Mapping(MappingError),
    TooManyPages,
    InvalidFlags,
    RollbackFailed,
}

const MAX_MAPPED_PAGES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadResult {
    pub mapped_pages: usize,
    pub entry: u64,
}

pub fn map_load_plan<M: PageTableMapper>(
    mapper: &mut M,
    allocator: &mut EarlyFrameAllocator<'_>,
    plan: LoadPlan,
) -> Result<LoadResult, LoadError> {
    let mut mapped = [None; MAX_MAPPED_PAGES];
    let mut count = 0usize;

    for index in 0..plan.count() {
        let segment = plan.segment(index).ok_or(LoadError::InvalidPage)?;
        let flags = flags_from_elf(segment.flags()).ok_or(LoadError::InvalidFlags)?;
        let range = segment.virtual_range();
        let mut address = range.start();
        while address < range.end() {
            if count == MAX_MAPPED_PAGES {
                rollback(mapper, &mapped, count)?;
                return Err(LoadError::TooManyPages);
            }

            let page = match Page4K::from_start_address(address) {
                Some(page) => page,
                None => {
                    rollback(mapper, &mapped, count)?;
                    return Err(LoadError::InvalidPage);
                }
            };
            let frame = match allocator.allocate_frame() {
                Some(frame) => frame,
                None => {
                    rollback(mapper, &mapped, count)?;
                    return Err(LoadError::Mapping(MappingError::FrameAllocationFailed));
                }
            };

            match mapper.map_page(page, frame.start_address(), flags) {
                Ok(flush) => flush.flush(),
                Err(error) => {
                    rollback(mapper, &mapped, count)?;
                    return Err(LoadError::Mapping(error));
                }
            }

            mapped[count] = Some(page);
            count += 1;
            address = address.checked_add(PAGE_SIZE_4K).ok_or_else(|| {
                let _ = rollback(mapper, &mapped, count);
                LoadError::InvalidPage
            })?;
        }
    }

    Ok(LoadResult { mapped_pages: count, entry: plan.entry() })
}

fn rollback<M: PageTableMapper>(mapper: &mut M, mapped: &[Option<Page4K>; MAX_MAPPED_PAGES], count: usize) -> Result<(), LoadError> {
    for index in (0..count).rev() {
        let Some(page) = mapped[index] else { continue; };
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
