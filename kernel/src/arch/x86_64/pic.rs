//! Legacy dual-8259 PIC control.
//!
//! ACPI APIC mode requires legacy 8259 IRQ delivery to be masked. The PIC is
//! kept behind this small boundary so future compatibility or fallback modes
//! can restore it without leaking port I/O into interrupt policy.

use core::arch::asm;

const MASTER_DATA: u16 = 0x21;
const SLAVE_DATA: u16 = 0xA1;

/// Masks every legacy 8259 IRQ line.
pub fn mask_all() {
    unsafe {
        outb(MASTER_DATA, 0xFF);
        outb(SLAVE_DATA, 0xFF);
    }
}

#[inline]
unsafe fn outb(port: u16, value: u8) {
    // SAFETY: Caller provides the architectural 8259 data ports.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_pic_ports_are_stable() {
        assert_eq!(super::MASTER_DATA, 0x21);
        assert_eq!(super::SLAVE_DATA, 0xA1);
    }
}
