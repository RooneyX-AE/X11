//! Process-facing kernel state.
//!
//! Process lifecycle and scheduling remain separate: a process owns an
//! address-space identity, while runnable execution stays under scheduler
//! ownership.

mod address_space;
mod address_space_builder;
mod identity;
mod image;
mod image_state;
mod initial_context;
mod load_plan;
mod loaded_address_space;
mod loader;
mod manager;
mod populate;
mod scheduler_binding;
mod stack;
mod state;

pub use address_space::{AddressSpaceError, AddressSpaceId, AddressSpaceSpec};
pub use address_space_builder::{build_address_space, AddressSpaceBuildError, BuiltAddressSpace};
pub use identity::{ProcessExecutionBinding, ProcessId};
pub use image::{ElfError, ElfImage, LoadSegment};
pub use image_state::{ProcessImage, ProcessImageError};
pub use initial_context::{InitialContext, InitialContextError};
pub use load_plan::{LoadPlan, LoadPlanError, SegmentMapping};
pub use loaded_address_space::{LoadedAddressSpace, LoadedAddressSpaceError};
pub use loader::{map_load_plan, LoadError, LoadResult, MappedPage, MAX_MAPPED_PAGES};
pub use manager::{ProcessManager, ProcessManagerError, SpawnedProcess, MAX_PROCESSES};
pub use populate::{populate_image, ImagePageWriter, PopulateError};
pub use scheduler_binding::{bind_and_ready, ProcessSchedulerBindError};
pub use stack::{StackPlanError, UserStackPlan};
pub use state::{ExitedProcess, ProcessState, ProcessTransitionError, ReadyProcess, RunningProcess};
