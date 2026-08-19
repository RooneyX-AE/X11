#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let message = b"Hello from X11-OS userspace!\r\n";
    let passed = b"X11-OS: syscall ABI return contract verified\r\n";

    let first_write = unsafe {
        let mut result: u64;
        asm!(
            "int 0x80",
            inout("rax") x11_os_abi::Syscall::Write.number() => result,
            in("rdi") message.as_ptr() as u64,
            in("rsi") message.len() as u64,
            in("rdx") 0u64,
            options(nostack)
        );
        result
    };

    if first_write != message.len() as u64 {
        loop { core::hint::spin_loop(); }
    }

    let second_write = unsafe {
        let mut result: u64;
        asm!(
            "int 0x80",
            inout("rax") x11_os_abi::Syscall::Write.number() => result,
            in("rdi") message.as_ptr() as u64,
            in("rsi") message.len() as u64,
            in("rdx") 0u64,
            options(nostack)
        );
        result
    };

    if second_write != message.len() as u64 {
        loop { core::hint::spin_loop(); }
    }

    let invalid_syscall = unsafe {
        let mut result: u64;
        asm!(
            "int 0x80",
            inout("rax") u64::MAX => result,
            in("rdi") 0u64,
            in("rsi") 0u64,
            in("rdx") 0u64,
            options(nostack)
        );
        result
    };

    if invalid_syscall != x11_os_abi::SyscallError::UnknownSyscall.return_value() {
        loop { core::hint::spin_loop(); }
    }

    let abi_check_write = unsafe {
        let mut result: u64;
        asm!(
            "int 0x80",
            inout("rax") x11_os_abi::Syscall::Write.number() => result,
            in("rdi") passed.as_ptr() as u64,
            in("rsi") passed.len() as u64,
            in("rdx") 0u64,
            options(nostack)
        );
        result
    };

    if abi_check_write != passed.len() as u64 {
        loop { core::hint::spin_loop(); }
    }

    loop { core::hint::spin_loop(); }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop { core::hint::spin_loop(); }
}
