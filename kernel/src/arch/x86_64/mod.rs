//! x86_64-specific CPU initialization.

mod gdt;
mod idt;
pub mod page_table;
pub mod paging;

pub fn init() {
    gdt::init();
    idt::init();
}
