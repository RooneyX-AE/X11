//! CPU exception handling.
//!
//! Exceptions remain separate from external IRQ routing. The double-fault
//! handler uses the dedicated TSS IST entry so stack corruption in an earlier
//! exception path does not immediately destroy the diagnostic handler.

use core::sync::atomic::{AtomicUsize, Ordering};

use spin::Once;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode,
};

use super::gdt::DOUBLE_FAULT_IST_INDEX;

static IDT: Once<InterruptDescriptorTable> = Once::new();
static LAST_PAGE_FAULT_ADDRESS: AtomicUsize = AtomicUsize::new(0);

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

extern "x86-interrupt" fn double_fault_handler(
    _stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    crate::serial::write_str("[x11-os] double fault\n");

    loop {
        core::hint::spin_loop();
    }
}
