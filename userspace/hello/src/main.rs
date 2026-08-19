#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let message = b"Hello from X11-OS userspace!\r\n";
    unsafe {
        asm!(
            "int 0x80",
            in("rax") x11_os_abi::Syscall::Write.number(),
            in("rdi") message.as_ptr() as u64,
            in("rsi") message.len() as u64,
            in("rdx") 0u64,
            options(nomem, nostack)
        );
        asm!(
            "int 0x80",
            in("rax") x11_os_abi::Syscall::Write.number(),
            in("rdi") message.as_ptr() as u64,
            in("rsi") message.len() as u64,
            in("rdx") 0u64,
            options(nomem, nostack)
        );
    }

    loop { core::hint::spin_loop(); }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop { core::hint::spin_loop(); }
}
