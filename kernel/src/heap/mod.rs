//! Kernel heap boundary.
//!
//! The allocator algorithm is deliberately hidden behind this module. The
//! initial implementation uses a linked-list first-fit heap as a bootstrap
//! backend; higher layers must depend only on `KernelHeap` and `HeapRegion`.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{null_mut, NonNull};
use core::sync::atomic::{AtomicBool, Ordering};

use linked_list_allocator::Heap;
use spin::Mutex;

use crate::memory::PAGE_SIZE_4K;

/// Number of 4 KiB frames reserved for the initial kernel heap.
pub const INITIAL_HEAP_FRAMES: usize = 256;

/// Initial heap size: 1 MiB.
pub const INITIAL_HEAP_SIZE: usize = INITIAL_HEAP_FRAMES * PAGE_SIZE_4K as usize;

/// A page-aligned, exclusively-owned virtual memory region suitable for a heap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeapRegion {
    start: u64,
    size: usize,
}

impl HeapRegion {
    /// Creates a heap region with page-aligned boundaries.
    pub const fn new(start: u64, size: usize) -> Option<Self> {
        if start % PAGE_SIZE_4K != 0
            || size == 0
            || size % PAGE_SIZE_4K as usize != 0
            || start.checked_add(size as u64).is_none()
        {
            return None;
        }

        Some(Self { start, size })
    }

    pub const fn start(self) -> u64 {
        self.start
    }

    pub const fn size(self) -> usize {
        self.size
    }

    pub const fn end(self) -> Option<u64> {
        self.start.checked_add(self.size as u64)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapInitError {
    AlreadyInitialized,
}

/// A consistent point-in-time heap usage snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeapStats {
    capacity: usize,
    used: usize,
    free: usize,
}

impl HeapStats {
    pub const fn capacity(self) -> usize {
        self.capacity
    }

    pub const fn used(self) -> usize {
        self.used
    }

    pub const fn free(self) -> usize {
        self.free
    }
}

/// Global allocator wrapper owned by the kernel heap subsystem.
pub struct KernelHeap {
    heap: Mutex<Heap>,
    initialized: AtomicBool,
}

impl KernelHeap {
    pub const fn empty() -> Self {
        Self {
            heap: Mutex::new(Heap::empty()),
            initialized: AtomicBool::new(false),
        }
    }

    pub fn init(&self, region: HeapRegion) -> Result<(), HeapInitError> {
        let mut heap = self.heap.lock();
        if self.initialized.load(Ordering::Acquire) {
            return Err(HeapInitError::AlreadyInitialized);
        }

        // SAFETY: the caller guarantees that the region is mapped, unused by
        // every other subsystem, and will remain valid for the kernel lifetime.
        unsafe {
            heap.init(region.start() as *mut u8, region.size());
        }

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Returns usage statistics from one allocator lock acquisition.
    ///
    /// The snapshot is consistent with a single point in the allocator's
    /// critical section, unlike separate `used()` and `free()` calls.
    pub fn stats(&self) -> HeapStats {
        let heap = self.heap.lock();
        HeapStats {
            capacity: heap.size(),
            used: heap.used(),
            free: heap.free(),
        }
    }

    pub fn used(&self) -> usize {
        self.stats().used()
    }

    pub fn free(&self) -> usize {
        self.stats().free()
    }
}

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !self.initialized.load(Ordering::Acquire) {
            return null_mut();
        }

        self.heap
            .lock()
            .allocate_first_fit(layout)
            .map(NonNull::as_ptr)
            .unwrap_or_else(|_| null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() || !self.initialized.load(Ordering::Acquire) {
            return;
        }

        // SAFETY: GlobalAlloc requires `ptr` to have originated from this
        // allocator with the same layout for deallocation.
        unsafe {
            self.heap
                .lock()
                .deallocate(NonNull::new_unchecked(ptr), layout);
        }
    }
}

#[global_allocator]
pub static GLOBAL: KernelHeap = KernelHeap::empty();

#[cfg(test)]
mod tests {
    use super::{HeapRegion, INITIAL_HEAP_SIZE};
    use crate::memory::PAGE_SIZE_4K;

    #[test]
    fn accepts_page_aligned_region() {
        let region = HeapRegion::new(0x1000_0000, INITIAL_HEAP_SIZE).unwrap();
        assert_eq!(region.start(), 0x1000_0000);
        assert_eq!(region.size(), INITIAL_HEAP_SIZE);
    }

    #[test]
    fn rejects_unaligned_start() {
        assert!(HeapRegion::new(0x1001, PAGE_SIZE_4K as usize).is_none());
    }

    #[test]
    fn rejects_non_page_multiple_size() {
        assert!(HeapRegion::new(0x1000, PAGE_SIZE_4K as usize + 1).is_none());
    }

    #[test]
    fn rejects_overflowing_range() {
        assert!(HeapRegion::new(u64::MAX - 0x1000, PAGE_SIZE_4K as usize).is_none());
    }

    #[test]
    fn heap_stats_preserve_capacity_identity() {
        let stats = super::HeapStats {
            capacity: 4096,
            used: 1024,
            free: 3072,
        };
        assert_eq!(stats.used() + stats.free(), stats.capacity());
    }
}
