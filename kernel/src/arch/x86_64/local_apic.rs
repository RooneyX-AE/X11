//! x86_64 Local APIC EOI backend.
//!
//! xAPIC exposes EOI through MMIO while x2APIC exposes it through an MSR.
//! Initialization stores only immutable hardware configuration so interrupt
//! handlers never need a mutex or mutable global state.

use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

use x86_64::registers::model_specific::{ApicBase, Msr};

use crate::interrupts::{InterruptEvent, InterruptSource};
use crate::memory::PhysicalMemoryMapping;

use super::apic::ApicMode;

const LOCAL_APIC_EOI_OFFSET: u64 = 0xB0;
const IA32_X2APIC_EOI: u32 = 0x80B;
const MODE_UNINITIALIZED: u8 = 0;
const MODE_XAPIC: u8 = 1;
const MODE_X2APIC: u8 = 2;

static MODE: AtomicU8 = AtomicU8::new(MODE_UNINITIALIZED);
static PHYSICAL_MAPPING_OFFSET: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalApicError {
    UnsupportedMode,
    InvalidMmioAddress,
    AlreadyInitialized,
}

/// Initializes the per-CPU-independent EOI configuration.
///
/// # Safety
///
/// The caller must ensure the selected APIC mode is active, the physical map
/// covers the Local APIC MMIO page for xAPIC mode, and interrupts stay disabled
/// until this configuration is complete.
pub unsafe fn initialize(
    mode: ApicMode,
    mapping: Option<PhysicalMemoryMapping>,
) -> Result<(), LocalApicError> {
    if MODE.load(Ordering::Acquire) != MODE_UNINITIALIZED {
        return Err(LocalApicError::AlreadyInitialized);
    }

    if matches!(mode, ApicMode::XApic) {
        let Some(mapping) = mapping else {
            return Err(LocalApicError::UnsupportedMode);
        };
        let (frame, _) = ApicBase::read();
        let physical = frame
            .start_address()
            .as_u64()
            .checked_add(LOCAL_APIC_EOI_OFFSET)
            .ok_or(LocalApicError::InvalidMmioAddress)?;
        mapping
            .translate(physical)
            .ok_or(LocalApicError::InvalidMmioAddress)?;
        PHYSICAL_MAPPING_OFFSET.store(mapping.offset(), Ordering::Release);
    }

    let mode_value = match mode {
        ApicMode::XApic => MODE_XAPIC,
        ApicMode::X2Apic => MODE_X2APIC,
    };
    MODE.store(mode_value, Ordering::Release);
    Ok(())
}

pub fn mode() -> Option<ApicMode> {
    match MODE.load(Ordering::Acquire) {
        MODE_XAPIC => Some(ApicMode::XApic),
        MODE_X2APIC => Some(ApicMode::X2Apic),
        _ => None,
    }
}

/// Signals completion of an APIC-delivered interrupt on the current CPU.
pub fn end_of_interrupt(event: InterruptEvent) {
    if !matches!(
        event.source(),
        InterruptSource::External(_)
            | InterruptSource::Timer
            | InterruptSource::InterProcessor(_)
            | InterruptSource::Spurious
    ) {
        return;
    }

    match MODE.load(Ordering::Acquire) {
        MODE_XAPIC => {
            let mapping_offset = PHYSICAL_MAPPING_OFFSET.load(Ordering::Acquire);
            let (frame, _) = ApicBase::read();
            let Some(physical) = frame
                .start_address()
                .as_u64()
                .checked_add(LOCAL_APIC_EOI_OFFSET)
            else {
                return;
            };
            let Some(virtual_address) = mapping_offset.checked_add(physical) else {
                return;
            };
            // SAFETY: initialization validated the direct-map translation for
            // the Local APIC EOI register and the configuration is immutable.
            unsafe { core::ptr::write_volatile(virtual_address as *mut u32, 0) };
        }
        MODE_X2APIC => {
            let eoi = Msr::new(IA32_X2APIC_EOI);
            // SAFETY: x2APIC mode was selected during initialization.
            unsafe { eoi.write(0) };
        }
        _ => {}
    }
}
