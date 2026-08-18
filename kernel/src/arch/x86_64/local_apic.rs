//! Local APIC EOI backend shared by timer and external interrupts.

use core::sync::atomic::{AtomicU8, Ordering};

use x86_64::registers::model_specific::Msr;

use crate::interrupts::InterruptEvent;
use crate::memory::PhysicalMemoryMapping;

use super::apic::ApicMode;

const LOCAL_APIC_EOI_OFFSET: u64 = 0xB0;
const IA32_X2APIC_EOI: u32 = 0x80B;

const MODE_UNINITIALIZED: u8 = 0;
const MODE_XAPIC: u8 = 1;
const MODE_X2APIC: u8 = 2;

static MODE: AtomicU8 = AtomicU8::new(MODE_UNINITIALIZED);
static XAPIC_OFFSET: AtomicU8 = AtomicU8::new(0);

/// Initializes the local APIC EOI backend.
///
/// # Safety
///
/// The caller must have configured the selected APIC mode and, for xAPIC,
/// provide a valid direct mapping to the LAPIC MMIO page.
pub unsafe fn initialize(
    mode: ApicMode,
    mapping: PhysicalMemoryMapping,
) -> Result<(), &'static str> {
    match mode {
        ApicMode::XApic => {
            let (_, offset) = x86_64::registers::model_specific::ApicBase::read();
            let physical = offset.apic_base() & 0xffff_ffff_f000;
            let Some(virtual_address) = mapping.translate(physical + LOCAL_APIC_EOI_OFFSET) else {
                return Err("invalid local APIC EOI mapping");
            };
            if !virtual_address.is_multiple_of(4) {
                return Err("unaligned local APIC EOI mapping");
            }
            XAPIC_OFFSET.store(0, Ordering::Release);
            MODE.store(MODE_XAPIC, Ordering::Release);
            Ok(())
        }
        ApicMode::X2Apic => {
            let _ = Msr::new(IA32_X2APIC_EOI);
            MODE.store(MODE_X2APIC, Ordering::Release);
            Ok(())
        }
    }
}

pub fn mode() -> Option<ApicMode> {
    match MODE.load(Ordering::Acquire) {
        MODE_XAPIC => Some(ApicMode::XApic),
        MODE_X2APIC => Some(ApicMode::X2Apic),
        _ => None,
    }
}

pub fn end_of_interrupt(event: InterruptEvent) {
    if event.vector() == crate::interrupts::TIMER_VECTOR {
        eoi();
    }
}

fn eoi() {
    match MODE.load(Ordering::Acquire) {
        MODE_XAPIC => {
            // SAFETY: `initialize` validated the direct-map EOI address and
            // the APIC mode is immutable after initialization.
            let mapping_offset = crate::memory::PhysicalMemoryMapping::new(0);
            let (frame, _) = x86_64::registers::model_specific::ApicBase::read();
            let physical = frame.start_address().as_u64().saturating_add(LOCAL_APIC_EOI_OFFSET);
            let Some(virtual_address) = mapping_offset.translate(physical) else {
                return;
            };
            let _ = XAPIC_OFFSET.load(Ordering::Acquire);
            // SAFETY: This path is only entered after successful APIC setup.
            unsafe { core::ptr::write_volatile(virtual_address as *mut u32, 0) };
        }
        MODE_X2APIC => {
            let mut eoi = Msr::new(IA32_X2APIC_EOI);
            // SAFETY: x2APIC mode was selected during initialization.
            unsafe { eoi.write(0) };
        }
        _ => {}
    }
}
