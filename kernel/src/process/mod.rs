//! Process-facing kernel state.
//!
//! Process lifecycle and scheduling remain separate: a process owns an
//! address-space identity, while runnable execution stays under scheduler
//! ownership.

mod address_space;
mod image;

pub use address_space::{AddressSpaceError, AddressSpaceId, AddressSpaceSpec};
pub use image::{ElfError, ElfImage, LoadSegment};
