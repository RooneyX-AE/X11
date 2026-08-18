#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod serial;

use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    serial::write_str("X11-OS: kernel entry reached\r\n");
    serial::write_str("X11-OS: BootInfo received\r\n");
    serial::write_str("X11-OS: memory regions = ");
    serial::write_usize(boot_info.memory_regions.len());
    serial::write_str("\r\n");
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
