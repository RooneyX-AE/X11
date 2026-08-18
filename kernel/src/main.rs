#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

mod arch;
mod heap;
mod interrupt_controller;
mod interrupts;
mod memory;
mod scheduler;
mod serial;
mod timer;

use alloc::vec::Vec;
use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};
use timer::TimerDevice;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

pub static PREEMPT_IRET_RETURNED: AtomicBool = AtomicBool::new(false);

extern "C" fn preemption_task_a() -> ! {
    serial::write_str("X11-OS: task A started\r\n");
    let result = unsafe { arch::x86_64::runtime::yield_current() };
    if result.is_err() {
        serial::write_str("X11-OS: task A initial yield failed\r\n");
        loop { core::hint::spin_loop(); }
    }
    serial::write_str("X11-OS: task A resumed after timer preemption\r\n");
    loop { x86_64::instructions::hlt(); }
}

extern "C" fn preemption_task_b() -> ! {
    serial::write_str("X11-OS: task B started\r\n");
    x86_64::instructions::interrupts::enable();
    serial::write_str("X11-OS: interrupts enabled in task B\r\n");
    loop {
        if PREEMPT_IRET_RETURNED.swap(false, Ordering::AcqRel) {
            serial::write_str("X11-OS: task B resumed through kernel iretq\r\n");
        }
        x86_64::instructions::hlt();
    }
}

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    serial::write_str("X11-OS: kernel entry reached\r\n");

    arch::x86_64::init();
    serial::write_str("X11-OS: CPU foundation initialized\r\n");

    let tsc_clock = match arch::x86_64::tsc::TscClocksource::try_new() {
        Ok(clock) => {
            serial::write_str("X11-OS: invariant TSC clocksource available\r\n");
            serial::write_str("X11-OS: TSC frequency Hz = ");
            serial::write_usize(clock.frequency().hz() as usize);
            serial::write_str("\r\n");
            Some(clock)
        }
        Err(_) => {
            serial::write_str("X11-OS: invariant TSC clocksource unavailable\r\n");
            None
        }
    };

    let apic = arch::x86_64::apic::ApicCapabilities::detect();
    serial::write_str("X11-OS: local APIC = ");
    serial::write_str(if apic.apic { "supported\r\n" } else { "unsupported\r\n" });
    serial::write_str("X11-OS: x2APIC = ");
    serial::write_str(if apic.x2apic { "supported\r\n" } else { "unsupported\r\n" });

    let memory = memory::summarize_boot_map(&boot_info.memory_regions);
    serial::write_str("X11-OS: usable memory bytes = ");
    serial::write_usize(memory.usable_bytes() as usize);
    serial::write_str("\r\n");
    serial::write_str("X11-OS: reserved memory bytes = ");
    serial::write_usize(memory.reserved_bytes() as usize);
    serial::write_str("\r\n");
    serial::write_str("X11-OS: malformed memory regions = ");
    serial::write_usize(memory.malformed_regions() as usize);
    serial::write_str("\r\n");

    let physical_mapping = memory::PhysicalMemoryMapping::from_boot_info(boot_info);
    let mut apic_ready = false;
    if let (Some(mapping), Some(rsdp_address)) = (physical_mapping, Option::<u64>::from(boot_info.rsdp_addr)) {
        // SAFETY: the bootloader supplied the RSDP and validated direct map.
        match unsafe { arch::x86_64::acpi::discover(rsdp_address, mapping) } {
            Ok(topology) => {
                serial::write_str("X11-OS: ACPI MADT discovered\r\n");
                serial::write_str("X11-OS: local APIC records = ");
                serial::write_usize(topology.local_apic_count);
                serial::write_str("\r\n");
                serial::write_str("X11-OS: local x2APIC records = ");
                serial::write_usize(topology.local_x2apic_count);
                serial::write_str("\r\n");
                serial::write_str("X11-OS: I/O APIC records = ");
                serial::write_usize(topology.io_apic_count);
                serial::write_str("\r\n");
                if topology.io_apic_count > 0 {
                    arch::x86_64::pic::mask_all();
                    serial::write_str("X11-OS: legacy 8259 masked\r\n");
                    if let Some(mode) = apic.preferred_mode() {
                        // SAFETY: APIC capabilities came from CPUID and IRQs are disabled.
                        if unsafe { arch::x86_64::apic::enable_preferred_mode(apic) } == Some(mode) {
                            serial::write_str("X11-OS: local APIC mode = ");
                            serial::write_str(match mode {
                                arch::x86_64::apic::ApicMode::XApic => "xAPIC\r\n",
                                arch::x86_64::apic::ApicMode::X2Apic => "x2APIC\r\n",
                            });
                            // SAFETY: direct map is available for xAPIC mode.
                            apic_ready = unsafe { arch::x86_64::local_apic::initialize(mode, Some(mapping)).is_ok() };
                            serial::write_str(if apic_ready { "X11-OS: Local APIC EOI initialized\r\n" } else { "X11-OS: Local APIC EOI initialization failed\r\n" });
                        }
                    }
                }
            }
            Err(_) => serial::write_str("X11-OS: ACPI MADT discovery failed\r\n"),
        }
    } else {
        serial::write_str("X11-OS: ACPI RSDP unavailable\r\n");
    }

    if let (true, Some(mapping), Some(tsc)) = (apic_ready, physical_mapping, tsc_clock.as_ref()) {
        // SAFETY: APIC mode and direct physical mapping were initialized above.
        match unsafe { arch::x86_64::lapic_timer::LapicTimer::new(apic.preferred_mode().unwrap(), Some(mapping)) } {
            Ok(mut timer_device) => match timer_device.calibrate(tsc) {
                Ok(frequency) => {
                    serial::write_str("X11-OS: LAPIC timer calibrated Hz = ");
                    serial::write_usize(frequency.hz() as usize);
                    serial::write_str("\r\n");
                    match timer_device.set_periodic(timer::TimerInterval::new(10_000_000).unwrap()) {
                        Ok(()) => serial::write_str("X11-OS: LAPIC periodic timer enabled at 100 Hz\r\n"),
                        Err(_) => serial::write_str("X11-OS: LAPIC periodic timer enable failed\r\n"),
                    }
                }
                Err(_) => serial::write_str("X11-OS: LAPIC timer calibration failed\r\n"),
            },
            Err(_) => serial::write_str("X11-OS: LAPIC timer backend unavailable\r\n"),
        }
    }

    let mut frame_allocator = memory::EarlyFrameAllocator::new(&boot_info.memory_regions);
    if let Some(mapping) = physical_mapping {
        if let Some(frames) = frame_allocator.allocate_contiguous(heap::INITIAL_HEAP_FRAMES) {
            if let (Some(physical_range), Some(size)) = (frames.physical_range(), frames.byte_len()) {
                if let Some(virtual_range) = mapping.translate_range(physical_range) {
                    if let Some(region) = heap::HeapRegion::new(virtual_range.start(), heap::INITIAL_HEAP_SIZE) {
                        if size == heap::INITIAL_HEAP_SIZE && virtual_range.len() == size as u64 {
                            if heap::GLOBAL.init(region).is_ok() {
                                serial::write_str("X11-OS: heap initialized\r\n");
                                let mut probe = Vec::with_capacity(64);
                                for value in 0..64u64 { probe.push(value * 3); }
                                let valid = probe.len() == 64 && probe.iter().enumerate().all(|(i, v)| *v == i as u64 * 3);
                                drop(probe);
                                serial::write_str(if valid { "X11-OS: heap allocation verified\r\n" } else { "X11-OS: heap allocation verification failed\r\n" });
                            } else {
                                serial::write_str("X11-OS: heap initialization failed\r\n");
                            }
                        }
                    }
                }
            }
        }
    }

    let mut runtime = arch::x86_64::runtime::KernelRuntime::new();
    // SAFETY: runtime is boxed and remains alive for the entire kernel lifetime.
    unsafe { runtime.bind_cpu().expect("runtime must bind to bootstrap CPU"); }
    let _task_a = runtime.spawn(scheduler::Priority::DEFAULT, preemption_task_a).expect("task A must spawn");
    let _task_b = runtime.spawn(scheduler::Priority::DEFAULT, preemption_task_b).expect("task B must spawn");

    serial::write_str("X11-OS: kernel runtime bound to CPU0\r\n");
    serial::write_str("X11-OS: preemption proof tasks armed\r\n");
    serial::write_str("X11-OS: interrupts remain disabled until task B starts\r\n");

    // SAFETY: runtime owns all execution bindings and the bootstrap dispatch
    // targets a fresh task. Task B enables interrupts after A yields to it.
    unsafe { runtime.dispatch_once().expect("initial runtime dispatch must succeed"); }

    loop { core::hint::spin_loop(); }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial::write_str("X11-OS: KERNEL PANIC\r\n");
    loop { core::hint::spin_loop(); }
}