#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod arch;
mod memory;
mod serial;

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;

entry_point!(kernel_main);

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

    let mut frame_allocator = memory::EarlyFrameAllocator::new(&boot_info.memory_regions);
    match memory::FrameAllocator::allocate_frame(&mut frame_allocator) {
        Some(frame) => {
            serial::write_str("X11-OS: first free frame = ");
            serial::write_usize(frame.start_address() as usize);
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
