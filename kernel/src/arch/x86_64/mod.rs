//! x86_64-specific CPU and platform initialization.

pub mod acpi;
pub mod activation;
pub mod address_space;
pub mod apic;
pub mod context_switch;
pub mod cpu_local;
pub mod dispatch;
pub mod execution;
pub mod execution_registry;
pub mod image_writer;
pub mod interrupt_entry;
pub mod interrupt_frame;
pub mod interrupted_state;
pub mod ioapic;
pub mod irq_routing;
pub mod kernel_task;
pub mod lapic_timer;
pub mod local_apic;
pub mod preemption;
pub mod preempt_return;
pub mod preemption_contract;
pub mod preemption_plan;
pub mod process_loader;
pub mod process_runtime;
pub mod runtime;
pub mod tsc;
pub mod user_activation;
pub mod user_context;
pub mod user_entry;
pub mod user_execution;
pub mod user_launch;
pub mod user_memory;
pub mod user_return;
pub mod user_copy;
pub mod voluntary_switch;
pub mod yield_switch;
mod gdt;
mod idt;
pub mod page_table;
pub mod paging;
pub mod pic;

fn enable_nxe() {
    let max_extended = unsafe { core::arch::x86_64::__cpuid(0x8000_0000) };
    if max_extended.eax < 0x8000_0001 { panic!("x86_64 extended CPUID leaf is unavailable"); }
    let features = unsafe { core::arch::x86_64::__cpuid(0x8000_0001) };
    if features.edx & (1 << 20) == 0 { panic!("CPU does not support NX page protection"); }
    use x86_64::registers::model_specific::{Efer, EferFlags};
    if !Efer::read().contains(EferFlags::NO_EXECUTE_ENABLE) {
        unsafe { Efer::update(|flags| flags.insert(EferFlags::NO_EXECUTE_ENABLE)); }
    }
}

pub fn init() {
    enable_nxe();
    gdt::init();
    idt::init();
}

pub fn timer_ticks() -> u64 { idt::timer_ticks() }
