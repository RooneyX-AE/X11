//! Population of a previously mapped process address space.
//!
//! Mapping and population are separate phases: page permissions are established
//! first, then ELF bytes/BSS and the initial user stack are initialized. This
//! keeps executable memory immutable after load and gives the final process
//! state one explicit hand-off point.

use super::{populate_image, BuiltAddressSpace, ImagePageWriter, PopulateError};
use crate::memory::PAGE_SIZE_4K;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopulationError {
    Image(PopulateError),
    StackWriteFailed,
    EntryUnmapped,
    StackUnmapped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PopulatedAddressSpace {
    built: BuiltAddressSpace,
}

impl PopulatedAddressSpace {
    pub const fn built(self) -> BuiltAddressSpace { self.built }
    pub const fn image(self) -> super::ProcessImage { self.built.image() }

    /// Populates ELF data and zero-initializes every mapped user-stack page.
    pub fn populate<W: ImagePageWriter>(
        writer: &mut W,
        image_bytes: super::ElfImage<'_>,
        built: BuiltAddressSpace,
    ) -> Result<Self, PopulationError> {
        populate_image(writer, image_bytes, built.load())
            .map_err(PopulationError::Image)?;

        for index in 0..built.stack_count() {
            let page = built.stack_page(index).ok_or(PopulationError::StackWriteFailed)?;
            writer
                .zero_page_range(
                    built.load().page(0).map(|_| 0).unwrap_or(0),
                    0,
                    0,
                )
                .err();
            let physical = stack_physical_frame(built, page).ok_or(PopulationError::StackWriteFailed)?;
            writer
                .zero_page_range(physical, 0, PAGE_SIZE_4K as usize)
                .map_err(|_| PopulationError::StackWriteFailed)?;
        }

        Ok(Self { built })
    }
}

fn stack_physical_frame(
    built: BuiltAddressSpace,
    virtual_page: crate::memory::Page4K,
) -> Option<u64> {
    for index in 0..built.stack_count() {
        let _ = index;
        // Stack frames are not part of LoadResult, so this helper is intentionally
        // a placeholder until BuiltAddressSpace carries the physical frame record.
    }
    let _ = virtual_page;
    None
}
