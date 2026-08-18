//! x86_64-specific CPU initialization.

mod gdt;
mod idt;

pub fn init() {
    gdt::init();
    idt::init();
}
