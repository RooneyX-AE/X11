//! x86_64 process-image construction over an isolated page-table root.
//!
//! The adapter owns the architecture-specific sequence: allocate a root,
//! construct a mapper for that root, map the generic process image, and leave
//! activation to the dedicated user-activation boundary.

use crate::memory::PhysicalMemoryMapping;
use crate::process::{build_address_space, ProcessImage, PopulatedAddressSpace};

use super::address_space::{AddressSpaceRoot, AddressSpaceRootError};
use super::image_writer::X86ImagePageWriter;
use super::page_table::X86PageTableMapper;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessLoadError {
    Root(AddressSpaceRootError),
    Mapping(crate::memory::MappingError),
    Image(crate::process::PopulationError),
}

/// Builds a process address space without activating CR3.
///
/// # Safety
/// The physical-memory mapping must remain valid for the lifetime of the
/// returned root and populated image. No concurrent page-table mutation may
/// occur while the root is being constructed.
pub unsafe fn load_process_image<'regions>(
    physical_memory: PhysicalMemoryMapping,
    allocator: &mut crate::memory::EarlyFrameAllocator<'regions>,
    image: ProcessImage,
    elf_bytes: crate::process::ElfImage<'_>,
) -> Result<(AddressSpaceRoot, PopulatedAddressSpace), ProcessLoadError> {
    let checkpoint = allocator.checkpoint();

    let result = (|| {
        let root = unsafe { AddressSpaceRoot::new_user_root(physical_memory.offset(), allocator) }
            .map_err(ProcessLoadError::Root)?;

        let spec = image.address_space();
        let mut mapper = unsafe {
            X86PageTableMapper::new_for_root(
                physical_memory.offset(),
                root.frame(),
                allocator,
                spec.layout().user_range(),
            )
        }
        .map_err(ProcessLoadError::Mapping)?;

        let built = build_address_space(&mut mapper, spec, image)
            .map_err(|_| ProcessLoadError::Mapping(crate::memory::MappingError::BackendFailure))?;

        let mut writer = X86ImagePageWriter::new(physical_memory);
        let populated = crate::process::PopulatedAddressSpace::populate(&mut writer, elf_bytes, built)
            .map_err(ProcessLoadError::Image)?;

        Ok((root, populated))
    })();

    if result.is_err() {
        allocator.rollback(checkpoint);
    }

    result
}
