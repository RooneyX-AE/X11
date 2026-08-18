//! x86_64-specific CPU and platform initialization.

pub mod acpi;
pub mod activation;
pub mod apic;
pub mod context_switch;
pub mod dispatch;
pub mod execution;
pub mod execution_registry;
pub mod interrupt_frame;
pub mod ioapic;
pub mod irq_routing;
pub mod kernel_task;
pub mod lapic_timer;
pub mod local_apic;
pub mod preemption;
pub mod runtime;
pub mod tsc;
pub mod yield;
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
