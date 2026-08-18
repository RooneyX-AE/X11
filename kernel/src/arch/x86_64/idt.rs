//! CPU exception and early interrupt handling.
//!
//! Exception handlers remain explicit, while the timer vector is installed as
//! the first hardware-interrupt entry. Timer handling only updates an atomic
//! counter and completes the APIC interrupt, keeping scheduling policy out of
//! the architecture layer.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use spin::Once;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::interrupts::{InterruptEvent, InterruptSource, TIMER_VECTOR};

use super::gdt::DOUBLE_FAULT_IST_INDEX;

static IDT: Once<InterruptDescriptorTable> = Once::new();
static LAST_PAGE_FAULT_ADDRESS: AtomicUsize = AtomicUsize::new(0);
static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let idt = IDT.call_once(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(DOUBLE_FAULT_IST_INDEX);
        }
        idt[TIMER_VECTOR as usize].set_handler_fn(timer_handler);
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
    let address = Cr2::read();
    LAST_PAGE_FAULT_ADDRESS.store(address.as_u64() as usize, Ordering::Release);
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

extern "x86-interrupt" fn timer_handler(_stack_frame: InterruptStackFrame) {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    if let Some(event) = InterruptEvent::new(InterruptSource::Timer) {
        crate::arch::x86_64::local_apic::end_of_interrupt(event);
    }
}

pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}
