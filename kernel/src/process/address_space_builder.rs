//! Transactional construction of a process address space.
//!
//! This layer coordinates policy and the generic page-table interface without
//! knowing the concrete architecture. It only returns a ready-to-activate
//! description after the ELF image and user stack are mapped and verified.

use crate::memory::{MappingFlags, Page4K, PageTableMapper};

use super::{map_load_plan, AddressSpaceSpec, LoadError, LoadResult, ProcessImage};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressSpaceBuildError {
    AddressSpaceMismatch,
    Image(LoadError),
    StackMappingFailed,
    EntryUnmapped,
    StackUnmapped,
    InvalidStackPage,
    RollbackFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MappedStackPage {
    pub virtual_page: Page4K,
    pub physical_address: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltAddressSpace {
    image: ProcessImage,
    load: LoadResult,
    stack_pages: [Option<MappedStackPage>; crate::memory::USER_STACK_PAGES as usize],
    stack_count: usize,
}

impl BuiltAddressSpace {
    pub const fn image(self) -> ProcessImage { self.image }
    pub const fn load(self) -> LoadResult { self.load }
    pub const fn stack_count(self) -> usize { self.stack_count }
    pub fn stack_page(self, index: usize) -> Option<MappedStackPage> {
        if index >= self.stack_count { None } else { self.stack_pages[index] }
    }
}

pub fn build_address_space<M: PageTableMapper>(
    mapper: &mut M,
    spec: AddressSpaceSpec,
    image: ProcessImage,
) -> Result<BuiltAddressSpace, AddressSpaceBuildError> {
    if image.address_space().id() != spec.id() {
        return Err(AddressSpaceBuildError::AddressSpaceMismatch);
    }

    let load = map_load_plan(mapper, image.load_plan())
        .map_err(AddressSpaceBuildError::Image)?;

    let stack = image.stack_plan();
    let mut stack_pages = [None; crate::memory::USER_STACK_PAGES as usize];
    let mut stack_count = 0usize;

    for index in 0..stack.count() {
        let page = stack.page(index).ok_or(AddressSpaceBuildError::InvalidStackPage)?;
        let physical_address = match mapper.allocate_frame() {
            Some(range) => range.start(),
            None => {
                rollback_stack(mapper, &stack_pages, stack_count)?;
                rollback_load(mapper, load)?;
                return Err(AddressSpaceBuildError::StackMappingFailed);
            }
        };
        match mapper.map_page(page, physical_address, MappingFlags::read_write()) {
            Ok(flush) => flush.flush(),
            Err(_) => {
                rollback_stack(mapper, &stack_pages, stack_count)?;
                rollback_load(mapper, load)?;
                return Err(AddressSpaceBuildError::StackMappingFailed);
            }
        }
        stack_pages[stack_count] = Some(MappedStackPage { virtual_page: page, physical_address });
        stack_count += 1;
    }

    if mapper.translate(load.entry()).is_none() {
        rollback_stack(mapper, &stack_pages, stack_count)?;
        rollback_load(mapper, load)?;
        return Err(AddressSpaceBuildError::EntryUnmapped);
    }

    let stack_top_minus_one = stack.initial_rsp().checked_sub(1).ok_or(AddressSpaceBuildError::StackUnmapped)?;
    if mapper.translate(stack_top_minus_one).is_none() {
        rollback_stack(mapper, &stack_pages, stack_count)?;
        rollback_load(mapper, load)?;
        return Err(AddressSpaceBuildError::StackUnmapped);
    }

    Ok(BuiltAddressSpace { image, load, stack_pages, stack_count })
}

fn rollback_load<M: PageTableMapper>(mapper: &mut M, load: LoadResult) -> Result<(), AddressSpaceBuildError> {
    for index in (0..load.mapped_pages).rev() {
        let mapped = load.page(index).ok_or(AddressSpaceBuildError::RollbackFailed)?;
        let page = Page4K::from_start_address(mapped.virtual_address).ok_or(AddressSpaceBuildError::RollbackFailed)?;
        mapper.unmap_page(page)
            .map(|(_, flush)| flush.flush())
            .map_err(|_| AddressSpaceBuildError::RollbackFailed)?;
    }
    Ok(())
}

fn rollback_stack<M: PageTableMapper>(
    mapper: &mut M,
    pages: &[Option<MappedStackPage>; crate::memory::USER_STACK_PAGES as usize],
    count: usize,
) -> Result<(), AddressSpaceBuildError> {
    for index in (0..count).rev() {
        let mapped = pages[index].ok_or(AddressSpaceBuildError::RollbackFailed)?;
        mapper.unmap_page(mapped.virtual_page)
            .map(|(_, flush)| flush.flush())
            .map_err(|_| AddressSpaceBuildError::RollbackFailed)?;
    }
    Ok(())
}
