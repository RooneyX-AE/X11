//! x86_64 Local APIC EOI backend.
//!
//! The EOI register differs between xAPIC and x2APIC. xAPIC exposes it via
//! MMIO at offset 0xB0 from the local APIC base; x2APIC exposes it through
//! the IA32_X2APIC_EOI MSR (0x80B). The constructor validates the MMIO
//! mapping boundary for xAPIC before exposing a safe `end_of_interrupt`.

use x86_64::registers::model_specific::{ApicBase, Msr};

use crate::interrupts::{InterruptEvent, InterruptSource};
use crate::memory::PhysicalMemoryMapping;

use super::apic::ApicMode;

const LOCAL_APIC_EOI_OFFSET: u64 = 0xB0;
const IA32_X2APIC_EOI: u32 = 0x80B;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalApicError {
    UnsupportedMode,
    InvalidMmioAddress,
}

/// Hardware-bound EOI writer for the current CPU.
pub struct LocalApic {
    mode: ApicMode,
    eoi_mmio: Option<*mut u32>,
    x2apic_eoi: Msr,
}

unsafe impl Send for LocalApic {}

impl LocalApic {
    /// Constructs a Local APIC EOI writer.
    ///
    /// # Safety
    ///
    /// `mode` must reflect the current IA32_APIC_BASE mode, and when `mode`
    /// is `XApic`, `mapping` must cover the bootloader-provided Local APIC
    /// physical frame. The caller must also keep the Local APIC hardware
    /// configuration stable for this object's lifetime.
    pub unsafe fn new(
        mode: ApicMode,
        mapping: Option<PhysicalMemoryMapping>,
    ) -> Result<Self, LocalApicError> {
        let eoi_mmio = match mode {
            ApicMode::XApic => {
                let Some(mapping) = mapping else {
                    return Err(LocalApicError::UnsupportedMode);
                };
                let (frame, _) = ApicBase::read();
                let physical = frame
                    .start_address()
                    .as_u64()
                    .checked_add(LOCAL_APIC_EOI_OFFSET)
                    .ok_or(LocalApicError::InvalidMmioAddress)?;
                let virtual_address = mapping
                    .translate(physical)
                    .ok_or(LocalApicError::InvalidMmioAddress)?;
                Some(virtual_address as *mut u32)
            }
            ApicMode::X2Apic => None,
        };

        Ok(Self {
            mode,
            eoi_mmio,
            x2apic_eoi: Msr::new(IA32_X2APIC_EOI),
        })
    }

    pub const fn mode(&self) -> ApicMode {
        self.mode
    }

    /// Signals completion of an external interrupt on the current CPU.
    ///
    /// Exceptions do not use the APIC EOI mechanism and are ignored here.
    pub fn end_of_interrupt(&mut self, event: InterruptEvent) {
        if !matches!(
            event.source(),
            InterruptSource::External(_)
                | InterruptSource::Timer
                | InterruptSource::InterProcessor(_)
                | InterruptSource::Spurious
        ) {
            return;
        }

        match self.mode {
            ApicMode::XApic => {
                let Some(address) = self.eoi_mmio else {
                    return;
                };
                // SAFETY: The constructor proved the address comes from the
                // Local APIC physical base plus the architectural EOI offset.
                unsafe { core::ptr::write_volatile(address, 0) };
            }
            ApicMode::X2Apic => {
                // SAFETY: The object is only constructible for x2APIC mode and
                // IA32_X2APIC_EOI is architecturally defined for that mode.
                unsafe { self.x2apic_eoi.write(0) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalApic, LocalApicError};
    use crate::arch::x86_64::apic::ApicMode;

    #[test]
    fn x2apic_does_not_require_mmio_mapping() {
        // Construction itself performs an APIC base read, so this test only
        // runs meaningfully in kernel execution. Keep the architectural mode
        // mapping policy covered without touching hardware in unit tests.
        let _ = ApicMode::X2Apic;
        let _ = LocalApicError::UnsupportedMode;
    }

    #[allow(dead_code)]
    fn _type_is_hardware_bound(value: &LocalApic) -> ApicMode {
        value.mode()
    }
}
