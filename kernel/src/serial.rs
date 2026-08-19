use core::arch::asm;
use core::sync::atomic::{AtomicU8, Ordering};

const COM1: u16 = 0x3F8;
const UNINITIALIZED: u8 = 0;
const INITIALIZING: u8 = 1;
const READY: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(UNINITIALIZED);

pub fn init() {
    match STATE.compare_exchange(UNINITIALIZED, INITIALIZING, Ordering::Acquire, Ordering::Acquire) {
        Ok(_) => {}
        Err(READY) => return,
        Err(INITIALIZING) => { while STATE.load(Ordering::Acquire) == INITIALIZING { core::hint::spin_loop(); } return; }
        Err(_) => unreachable!(),
    }
    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1, 0x03);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xC7);
        outb(COM1 + 4, 0x0B);
    }
    STATE.store(READY, Ordering::Release);
}

pub fn write_byte(byte: u8) {
    if STATE.load(Ordering::Acquire) != READY { init(); }
    unsafe {
        while (inb(COM1 + 5) & 0x20) == 0 { core::hint::spin_loop(); }
        outb(COM1, byte);
    }
}

pub fn write_bytes(bytes: &[u8]) { for &byte in bytes { write_byte(byte); } }
pub fn write_str(message: &str) { write_bytes(message.as_bytes()); }

pub fn write_usize(mut value: usize) {
    let mut digits = [0u8; 20];
    let mut index = digits.len();
    if value == 0 { write_byte(b'0'); return; }
    while value != 0 { index -= 1; digits[index] = b'0' + (value % 10) as u8; value /= 10; }
    write_bytes(&digits[index..]);
}

pub fn write_hex(mut value: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut digits = [b'0'; 16];
    let mut index = digits.len();
    if value == 0 { write_byte(b'0'); return; }
    while value != 0 { index -= 1; digits[index] = DIGITS[(value & 0xf) as usize]; value >>= 4; }
    write_bytes(&digits[index..]);
}

unsafe fn outb(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe { asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags)); }
    value
}
