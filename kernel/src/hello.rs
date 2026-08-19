//! Minimal kernel hello-world application boundary.
//!
//! This is intentionally tiny: it provides a stable first observable output
//! without coupling the boot path to the scheduler or interrupt subsystem.

use crate::serial;

pub const MESSAGE: &str = "Hello World from X11-OS!";

pub fn run() {
    serial::write_str(MESSAGE);
    serial::write_str("\r\n");
}

/// Compatibility entrypoint for the early boot caller.
pub fn print() {
    run();
}

#[cfg(test)]
mod tests {
    use super::{print, MESSAGE};

    #[test]
    fn hello_message_is_stable() {
        assert_eq!(MESSAGE, "Hello World from X11-OS!");
    }

    #[test]
    fn compatibility_entrypoint_exists() {
        let _ = print as fn();
    }
}
