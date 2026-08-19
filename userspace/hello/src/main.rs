#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use x11_os_abi::Syscall;

const TEST_READABLE_USER_ADDRESS: u64 = 0x401000;
const TEST_LENGTH: u64 = 1;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let syscall = Syscall::Write.number();
    unsafe {
        asm!(
            "int 0x80",
            in("rax") syscall,
            in("rdi") TEST_READABLE_USER_ADDRESS,
            in("rsi") TEST_LENGTH,
            in("rdx") 0u64,
            options(nostack, preserves_flags),
        );
        asm!(
            "int 0x80",
            in("rax") syscall,
            in("rdi") TEST_READABLE_USER_ADDRESS,
            in("rsi") TEST_LENGTH,
            in("rdx") 0u64,
            options(nostack, preserves_flags),
        );
    }

    loop { core::hint::spin_loop(); }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop { core::hint::spin_loop(); }
}
