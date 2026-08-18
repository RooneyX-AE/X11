//! Process-facing kernel state.
//!
//! Process lifecycle and scheduling remain separate: a process owns an
//! address-space identity, while runnable execution stays under scheduler
//! ownership.

mod address_space;
mod image;
mod initial_context;
mod load_plan;
mod loader;
mod populate;
mod stack;

pub use address_space::{AddressSpaceError, AddressSpaceId, AddressSpaceSpec};
pub use image::{ElfError, ElfImage, LoadSegment};
pub use initial_context::{InitialContext, InitialContextError};
pub use load_plan::{LoadPlan, LoadPlanError, SegmentMapping};
pub use loader::{map_load_plan, LoadError, LoadResult, MappedPage, MAX_MAPPED_PAGES};
pub use populate::{populate_image, ImagePageWriter, PopulateError};
pub use stack::{StackPlanError, UserStackPlan};
