#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

mod arch;
mod heap;
mod interrupts;
mod memory;
mod serial;
mod timer;

use alloc::vec::Vec;
use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use memory::{FrameAllocator, PageTableMapper};

use arch::x86_64::page_table::X86PageTableMapper;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    serial::write_str("X11-OS: kernel entry reached\r\n");

    arch::x86_64::init();
    serial::write_str("X11-OS: CPU foundation initialized\r\n");
    serial::write_str("X11-OS: interrupt contracts initialized\r\n");

    let memory = memory::summarize_boot_map(&boot_info.memory_regions);
    serial::write_str("X11-OS: usable memory bytes = ");
    serial::write_usize(memory.usable_bytes() as usize);
    serial::write_str("\r\n");
    serial::write_str("X11-OS: reserved memory bytes = ");
    serial::write_usize(memory.reserved_bytes() as usize);
    serial::write_str("\r\n");
    serial::write_str("X11-OS: malformed memory regions = ");
    serial::write_usize(memory.malformed_regions() as usize);
    serial::write_str("\r\n");

    let physical_mapping = memory::PhysicalMemoryMapping::from_boot_info(boot_info);
    match physical_mapping {
        Some(mapping) => {
            serial::write_str("X11-OS: physical memory mapping enabled at 0x");
            serial::write_hex(mapping.offset());
            serial::write_str("\r\n");
        }
        None => {
            serial::write_str("X11-OS: physical memory mapping unavailable\r\n");
        }
    }

    let mut frame_allocator = memory::EarlyFrameAllocator::new(&boot_info.memory_regions);
    let first_frame = frame_allocator.allocate_frame();
    match first_frame {
        Some(frame) => {
            serial::write_str("X11-OS: first free frame = 0x");
            serial::write_hex(frame.start_address());
            serial::write_str("\r\n");
        }
        None => {
            serial::write_str("X11-OS: no usable physical frame available\r\n");
        }
    }

    if let (Some(mapping), Some(frame)) = (physical_mapping, first_frame) {
        let page_start = memory::KERNEL_SPACE_START + 0x20_0000;
        let page = memory::Page4K::from_start_address(page_start)
            .expect("kernel mapping test page must be aligned");

        // SAFETY: The bootloader established the complete physical-memory
        // direct mapping, and this mapper is initialized exactly once here.
        let mut mapper = unsafe {
            X86PageTableMapper::new(
                mapping.offset(),
                &mut frame_allocator,
                memory::KERNEL_ADDRESS_SPACE,
            )
        };

        if mapper.translate(page.start_address()).is_none() {
            match mapper.map_page(page, frame.start_address()) {
                Ok(flush) => {
                    flush.flush();
                    if mapper.translate(page.start_address()) == Some(frame.start_address()) {
                        serial::write_str("X11-OS: page mapping verified\r\n");
                    } else {
                        serial::write_str("X11-OS: page mapping verification failed\r\n");
                    }

                    match mapper.unmap_page(page) {
                        Ok((unmapped_frame, flush)) => {
                            flush.flush();
                            if unmapped_frame == frame.start_address()
                                && mapper.translate(page.start_address()).is_none()
                            {
                                serial::write_str("X11-OS: page unmapping verified\r\n");
                            } else {
                                serial::write_str("X11-OS: page unmapping verification failed\r\n");
                            }
                        }
                        Err(_) => serial::write_str("X11-OS: page unmapping failed\r\n"),
                    }
                }
                Err(_) => serial::write_str("X11-OS: page mapping failed\r\n"),
            }
        } else {
            serial::write_str("X11-OS: page mapping test skipped, page already mapped\r\n");
        }
    } else {
        serial::write_str("X11-OS: page-table integration test skipped\r\n");
    }

    if let Some(mapping) = physical_mapping {
        if let Some(frames) = frame_allocator.allocate_contiguous(heap::INITIAL_HEAP_FRAMES) {
            if let Some(physical_range) = frames.physical_range() {
                if let Some(virtual_range) = mapping.translate_range(physical_range) {
                    if let (Some(size), Some(region)) = (
                        frames.byte_len(),
                        heap::HeapRegion::new(virtual_range.start(), heap::INITIAL_HEAP_SIZE),
                    ) {
                        if size == heap::INITIAL_HEAP_SIZE && virtual_range.len() == size as u64 {
                            match heap::GLOBAL.init(region) {
                                Ok(()) => {
                                    serial::write_str("X11-OS: heap initialized\r\n");

                                    let mut probe = Vec::with_capacity(64);
                                    for value in 0..64u64 {
                                        probe.push(value * 3);
                                    }

                                    let valid = probe.len() == 64
                                        && probe.iter().enumerate().all(|(index, value)| {
                                            *value == index as u64 * 3
                                        });
                                    drop(probe);
                                    serial::write_str(if valid {
                                        "X11-OS: heap allocation verified\r\n"
                                    } else {
                                        "X11-OS: heap allocation verification failed\r\n"
                                    });

                                    let stats = heap::GLOBAL.stats();
                                    serial::write_str("X11-OS: heap used bytes = ");
                                    serial::write_usize(stats.used());
                                    serial::write_str("\r\n");
                                    serial::write_str("X11-OS: heap free bytes = ");
                                    serial::write_usize(stats.free());
                                    serial::write_str("\r\n");
                                }
                                Err(_) => {
                                    serial::write_str("X11-OS: heap initialization failed\r\n");
                                }
                            }
                        } else {
                            serial::write_str("X11-OS: heap region size mismatch\r\n");
                        }
                    } else {
                        serial::write_str("X11-OS: invalid heap region\r\n");
                    }
                } else {
                    serial::write_str("X11-OS: heap direct-map translation overflow\r\n");
                }
            } else {
                serial::write_str("X11-OS: heap physical range invalid\r\n");
            }
        } else {
            serial::write_str("X11-OS: insufficient contiguous frames for heap\r\n");
        }
    }

    serial::write_str("X11-OS: entering idle state\r\n");

    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial::write_str("X11-OS: KERNEL PANIC\r\n");

    loop {
        core::hint::spin_loop();
    }
}
