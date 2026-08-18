//! x86_64-specific CPU and platform initialization.

pub mod acpi;
pub mod apic;
mod gdt;
mod idt;
pub mod local_apic;
pub mod page_table;
pub mod paging;

pub fn init() {
    gdt::init();
    idt::init();
}
