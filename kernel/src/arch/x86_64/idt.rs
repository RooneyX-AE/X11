//! CPU exception and early interrupt handling.
//!
//! Interrupt handlers stay intentionally small. The raw timer entry records a
//! pending event and acknowledges the local APIC; scheduler policy is serviced
//! outside interrupt context.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use spin::Once;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::VirtAddr;

use crate::interrupts::{InterruptEvent, InterruptSource, TIMER_VECTOR};

use super::gdt::DOUBLE_FAULT_IST_INDEX;
use super::interrupt_entry;

static IDT: Once<InterruptDescriptorTable> = Once::new();
static LAST_PAGE_FAULT_ADDRESS: AtomicUsize = AtomicUsize::new(0);
static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);
static TIMER_PENDING: AtomicBool = AtomicBool::new(false);

pub fn init() {
    let idt = IDT.call_once(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(DOUBLE_FAULT_IST_INDEX);
            idt[TIMER_VECTOR].set_handler_addr(VirtAddr::new(
                interrupt_entry::timer_entry as *const () as usize as u64,
            ));
        }
        idt
    });

    idt.load();
}

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    crate::serial::write_str("[x11-os] breakpoint exception\n");
}

extern "x86-interrupt" fn page_fault_handler(
    _stack_frame: InterruptStackFrame,
    _error_code: PageFaultErrorCode,
) {
    let address = Cr2::read_raw();
    LAST_PAGE_FAULT_ADDRESS.store(address as usize, Ordering::Release);
    crate::serial::write_str("[x11-os] page fault\n");

    loop {
        core::hint::spin_loop();
    }
}

extern "x86-interrupt" fn double_fault_handler(_stack_frame: InterruptStackFrame, _error_code: u64) -> ! {
    crate::serial::write_str("[x11-os] double fault\n");

    loop {
        core::hint::spin_loop();
    }
}

/// Shared timer bookkeeping for the raw timer entry.
///
/// This is the sole timer-event accounting path. It owns no scheduler policy
/// and never performs a context switch.
pub fn record_timer_interrupt() {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    TIMER_PENDING.store(true, Ordering::Release);

    if let Some(event) = InterruptEvent::new(InterruptSource::Timer) {
        crate::arch::x86_64::local_apic::end_of_interrupt(event);
    }
}

pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Acquire)
}

pub fn take_timer_pending() -> bool {
    TIMER_PENDING.swap(false, Ordering::AcqRel)
}
