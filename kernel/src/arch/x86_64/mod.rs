//! x86_64-specific CPU and platform initialization.

pub mod acpi;
pub mod apic;
pub mod ioapic;
pub mod irq_routing;
pub mod local_apic;
mod gdt;
mod idt;
pub mod page_table;
pub mod paging;
pub mod pic;

pub fn init() {
    gdt::init();
    idt::init();
}
