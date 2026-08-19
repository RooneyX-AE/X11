//! Physical and virtual memory policy.

mod address_space;
mod boot;
mod frame;
mod page;
mod page_table;
mod physical;
mod region;
mod user;
mod user_copy;
mod user_stack;

pub use address_space::{UserAddressSpaceLayout, VirtRange, KERNEL_SPACE_START, USER_SPACE_START};
pub use boot::MemorySummary;
pub use frame::{EarlyFrameAllocator, FrameAllocator};
pub use page::{Page4K, PAGE_SIZE_4K};
pub use page_table::{MappingError, MappingFlags, MappingFlush, PageAccess, PageTableMapper, UserMemoryView};
pub use physical::PhysicalMemoryMapping;
pub use user::{validate_slice, UserRangeError};
pub use user_copy::{copy_from_user, validate_readable_range, UserCopyBackend, UserReadError};
pub use user_stack::{
    is_valid_user_stack_pointer, user_stack_guard_range, user_stack_range, USER_STACK_GUARD_SIZE,
    USER_STACK_PAGES, USER_STACK_SIZE, USER_STACK_TOP,
};

pub fn summarize_boot_map(regions: &bootloader_api::info::MemoryRegions) -> MemorySummary {
    boot::summarize(regions)
}
