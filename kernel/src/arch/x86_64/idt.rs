//! CPU exception handling.
//!
//! Only a small, explicit exception surface is installed at this stage.
//! Additional interrupt vectors will be introduced together with the APIC and
//! interrupt-controller design rather than being guessed here.

use core::sync::atomic::{AtomicUsize, Ordering};

use spin::Once;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

static IDT: Once<InterruptDescriptorTable> = Once::new();
static LAST_PAGE_FAULT_ADDRESS: AtomicUsize = AtomicUsize::new(0);

pub fn init() {
    let idt = IDT.call_once(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
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
