//! x86_64 I/O APIC redirection-table primitives.
//!
//! The ACPI MADT supplies each I/O APIC physical base and GSI range. This
//! module owns the MMIO register encoding but deliberately leaves IRQ policy
//! and unmasking to the interrupt-routing layer.

use crate::memory::PhysicalMemoryMapping;

const IOREGSEL: u64 = 0x00;
const IOWIN: u64 = 0x10;
const REDIRECTION_TABLE_BASE: u8 = 0x10;

/// A decoded I/O APIC redirection-table entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RedirectionEntry {
    pub vector: u8,
    pub delivery_mode: u8,
    pub destination_mode_logical: bool,
    pub delivery_pending: bool,
    pub active_low: bool,
    pub level_triggered: bool,
    pub masked: bool,
    pub destination: u8,
}

impl RedirectionEntry {
    /// Creates a masked fixed-delivery entry suitable for later routing.
    pub const fn masked(vector: u8, destination: u8) -> Self {
        Self {
            vector,
            delivery_mode: 0,
            destination_mode_logical: false,
            delivery_pending: false,
            active_low: false,
            level_triggered: false,
            masked: true,
            destination,
        }
    }

    pub const fn encode(self) -> (u32, u32) {
        let mut low = self.vector as u32;
        low |= ((self.delivery_mode as u32) & 0x7) << 8;
        if self.destination_mode_logical {
            low |= 1 << 11;
        }
        if self.delivery_pending {
            low |= 1 << 12;
        }
        if self.active_low {
            low |= 1 << 13;
        }
        if self.level_triggered {
            low |= 1 << 15;
        }
        if self.masked {
            low |= 1 << 16;
        }
        (low, (self.destination as u32) << 24)
    }
}

/// I/O APIC MMIO access over the bootloader's direct physical map.
pub struct IoApic {
    mapping: PhysicalMemoryMapping,
    physical_base: u64,
}

impl IoApic {
    pub const fn new(mapping: PhysicalMemoryMapping, physical_base: u64) -> Self {
        Self {
            mapping,
            physical_base,
        }
    }

    pub const fn physical_base(&self) -> u64 {
        self.physical_base
    }

    pub unsafe fn read_register(&self, register: u8) -> Option<u32> {
        let select = self.mapping.translate(self.physical_base + IOREGSEL)? as *mut u32;
        let data = self.mapping.translate(self.physical_base + IOWIN)? as *const u32;

        // SAFETY: The caller guarantees that the supplied physical base is the
        // I/O APIC MMIO region reported by ACPI. The direct mapping provides a
        // stable virtual address for the register window.
        unsafe {
            core::ptr::write_volatile(select, register as u32);
            Some(core::ptr::read_volatile(data))
        }
    }

    pub unsafe fn write_register(&self, register: u8, value: u32) -> bool {
        let Some(select) = self.mapping.translate(self.physical_base + IOREGSEL) else {
            return false;
        };
        let Some(data) = self.mapping.translate(self.physical_base + IOWIN) else {
            return false;
        };

        // SAFETY: Same MMIO invariant as `read_register`.
        unsafe {
            core::ptr::write_volatile(select as *mut u32, register as u32);
            core::ptr::write_volatile(data as *mut u32, value);
        }
        true
    }

    pub unsafe fn write_redirection(&self, index: u8, entry: RedirectionEntry) -> bool {
        let register = REDIRECTION_TABLE_BASE.wrapping_add(index.saturating_mul(2));
        let (low, high) = entry.encode();
        unsafe {
            self.write_register(register, low) && self.write_register(register + 1, high)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RedirectionEntry;

    #[test]
    fn masked_fixed_entry_encoding() {
        let entry = RedirectionEntry::masked(48, 2);
        let (low, high) = entry.encode();
        assert_eq!(low & 0xff, 48);
        assert_eq!((low >> 16) & 1, 1);
        assert_eq!(high >> 24, 2);
    }

    #[test]
    fn level_trigger_and_active_low_are_independent_bits() {
        let mut entry = RedirectionEntry::masked(40, 0);
        entry.active_low = true;
        entry.level_triggered = true;
        let (low, _) = entry.encode();
        assert_eq!((low >> 13) & 1, 1);
        assert_eq!((low >> 15) & 1, 1);
    }
}
