//! Process-facing kernel state.
//!
//! Process lifecycle and scheduling remain separate: a process owns an
//! address-space identity, while runnable execution stays under scheduler
//! ownership.

mod address_space;
mod image;
mod load_plan;
mod loader;

pub use address_space::{AddressSpaceError, AddressSpaceId, AddressSpaceSpec};
pub use image::{ElfError, ElfImage, LoadSegment};
pub use load_plan::{LoadPlan, LoadPlanError, SegmentMapping};
pub use loader::{map_load_plan, LoadError, LoadResult};
