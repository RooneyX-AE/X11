#![no_std]

//! First userspace payload boundary for X11-OS.
//!
//! The payload contains no hardware access. It consumes only the shared ABI
//! crate so syscall numbers cannot silently diverge from the kernel.

use x11_os_abi::Syscall;

pub const MESSAGE: &[u8] = b"Hello World from X11-OS userspace!\n";
pub const WRITE_SYSCALL: u64 = Syscall::Write.number();

pub const fn message() -> &'static [u8] {
    MESSAGE
}

#[cfg(test)]
mod tests {
    use super::{message, MESSAGE, WRITE_SYSCALL};
    use x11_os_abi::Syscall;

    #[test]
    fn payload_message_is_stable() {
        assert_eq!(MESSAGE, b"Hello World from X11-OS userspace!\n");
        assert_eq!(message(), MESSAGE);
    }

    #[test]
    fn payload_uses_shared_write_syscall_number() {
        assert_eq!(WRITE_SYSCALL, Syscall::Write.number());
    }
}
