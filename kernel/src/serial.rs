use core::arch::asm;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

const COM1: u16 = 0x3F8;
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init() {
    if INITIALIZED.swap(true, Ordering::AcqRel) {
        return;
    }

    unsafe {
        outb(COM1 + 1, 0x00); // Disable interrupts.
        outb(COM1 + 3, 0x80); // Enable DLAB.
        outb(COM1, 0x03); // 38400 baud divisor low byte.
        outb(COM1 + 1, 0x00); // Divisor high byte.
        outb(COM1 + 3, 0x03); // 8 bits, no parity, one stop bit.
        outb(COM1 + 2, 0xC7); // Enable FIFO, clear them, 14-byte threshold.
        outb(COM1 + 4, 0x0B); // IRQs enabled, RTS/DSR set.
    }
}

pub fn write_byte(byte: u8) {
    if !INITIALIZED.load(Ordering::Acquire) {
        init();
    }

    unsafe {
        while (inb(COM1 + 5) & 0x20) == 0 {
            core::hint::spin_loop();
        }
        outb(COM1, byte);
    }
}

pub fn write_str(message: &str) {
    for byte in message.bytes() {
        write_byte(byte);
    }
}

pub fn _print(args: fmt::Arguments<'_>) {
    struct SerialWriter;

    impl fmt::Write for SerialWriter {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            super::serial::write_str(s);
            Ok(())
        }
    }

    let mut writer = SerialWriter;
    let _ = writer.write_fmt(args);
}

unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}
