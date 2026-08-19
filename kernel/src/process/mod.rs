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
mod populated_address_space;
mod populate;
mod ramdisk_image;
mod scheduler_binding;
mod stack;
mod state;
mod user_launch;

pub use address_space::{AddressSpaceError, AddressSpaceId, AddressSpaceSpec};
#[allow(unused_imports)]
pub use address_space_builder::{build_address_space, AddressSpaceBuildError, BuiltAddressSpace, MappedStackPage};
pub use identity::{ProcessExecutionBinding, ProcessId};
pub use image::{ElfError, ElfImage, LoadSegment};
pub use image_state::{ProcessImage, ProcessImageError};
#[allow(unused_imports)]
pub use initial_context::{InitialContext, InitialContextError};
#[allow(unused_imports)]
pub use load_plan::{LoadPlan, LoadPlanError, SegmentMapping};
#[allow(unused_imports)]
pub use loaded_address_space::{LoadedAddressSpace, LoadedAddressSpaceError};
#[allow(unused_imports)]
pub use loader::{map_load_plan, LoadError, LoadResult, MappedPage, MAX_MAPPED_PAGES};
#[allow(unused_imports)]
pub use manager::{ProcessManager, ProcessManagerError, SpawnedProcess, MAX_PROCESSES};
pub use populated_address_space::{PopulatedAddressSpace, PopulationError};
pub use populate::{populate_image, ImagePageWriter, PopulateError};
#[allow(unused_imports)]
pub use ramdisk_image::{build_process_image, RamdiskImageError};
#[allow(unused_imports)]
pub use scheduler_binding::{bind_and_ready, ProcessSchedulerBindError};
pub use stack::{StackPlanError, UserStackPlan};
#[allow(unused_imports)]
pub use state::{ExitedProcess, ProcessState, ProcessTransitionError, ReadyProcess, RunningProcess};
#[allow(unused_imports)]
pub use user_launch::{UserLaunchError, UserLaunchPlan};