#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod arch;
mod memory;
mod serial;

use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;

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

    if let Some(mapping) = memory::PhysicalMemoryMapping::from_boot_info(boot_info) {
        serial::write_str("X11-OS: physical memory mapping enabled at 0x");
        serial::write_hex(mapping.offset());
        serial::write_str("\r\n");
    } else {
        serial::write_str("X11-OS: physical memory mapping unavailable\r\n");
    }

    let mut frame_allocator = memory::EarlyFrameAllocator::new(&boot_info.memory_regions);
    match memory::FrameAllocator::allocate_frame(&mut frame_allocator) {
        Some(frame) => {
            serial::write_str("X11-OS: first free frame = 0x");
            serial::write_hex(frame.start_address());
            serial::write_str("\r\n");
        }
        None => {
            serial::write_str("X11-OS: no usable physical frame available\r\n");
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
