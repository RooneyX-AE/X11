//! x86_64-specific CPU initialization.

mod gdt;
mod idt;
pub mod paging;

pub fn init() {
    gdt::init();
    idt::init();
}
