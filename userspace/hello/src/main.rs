#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe {
        asm!("int 0x80", options(nomem, nostack, preserves_flags));
        asm!("int 0x80", options(nomem, nostack, preserves_flags));
    }

    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
