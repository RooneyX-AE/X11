#![no_std]

//! First userspace payload boundary for X11-OS.
//!
//! The payload contains no kernel dependencies and no direct hardware access.
//! A future userspace loader can map this crate into a user address space and
//! provide the syscall/console ABI separately.

pub const MESSAGE: &[u8] = b"Hello World from X11-OS userspace!\n";

pub fn message() -> &'static [u8] {
    MESSAGE
}

#[cfg(test)]
mod tests {
    use super::MESSAGE;

    #[test]
    fn payload_message_is_stable() {
        assert_eq!(MESSAGE, b"Hello World from X11-OS userspace!\n");
    }
}
