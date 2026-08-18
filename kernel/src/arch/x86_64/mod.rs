//! x86_64-specific CPU and platform initialization.

pub mod acpi;
pub mod apic;
pub mod context_switch;
pub mod execution;
pub mod interrupt_frame;
pub mod ioapic;
pub mod irq_routing;
pub mod lapic_timer;
pub mod local_apic;
pub mod tsc;
mod gdt;
mod idt;
pub mod page_table;
pub mod paging;
pub mod pic;

pub fn init() {
    gdt::init();
    idt::init();
}

pub fn timer_ticks() -> u64 {
    idt::timer_ticks()
}
