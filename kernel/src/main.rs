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
mod serial;
mod timer;

use alloc::vec::Vec;
use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use memory::{FrameAllocator, PageTableMapper};

use arch::x86_64::page_table::X86PageTableMapper;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

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
            if let Ok(now) = timer::Clocksource::now(&clock) {
                serial::write_str("X11-OS: TSC timebase initialized at ns = ");
                serial::write_usize(now.as_nanos() as usize);
                serial::write_str("\r\n");
            }
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
    match physical_mapping {
        Some(mapping) => {
            serial::write_str("X11-OS: physical memory mapping enabled at 0x");
            serial::write_hex(mapping.offset());
            serial::write_str("\r\n");
        }
        None => {
            serial::write_str("X11-OS: physical memory mapping unavailable\r\n");
        }
    }

    let mut apic_ready = false;
    let mut lapic_timer = None;

    if let (Some(mapping), Some(rsdp_address)) = (
        physical_mapping,
        Option::<u64>::from(boot_info.rsdp_addr),
    ) {
        // SAFETY: `rsdp_address` is supplied by the bootloader and `mapping`
        // is the bootloader's validated direct physical-memory mapping.
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
                serial::write_str("X11-OS: interrupt source overrides = ");
                serial::write_usize(topology.source_override_count);
                serial::write_str("\r\n");

                if topology.io_apic_count > 0 {
                    arch::x86_64::pic::mask_all();
                    serial::write_str("X11-OS: legacy 8259 masked\r\n");

                    if let Some(mode) = apic.preferred_mode() {
                        // SAFETY: CPU interrupt delivery is not enabled yet,
                        // and the capability snapshot came directly from CPUID.
                        if unsafe { arch::x86_64::apic::enable_preferred_mode(apic) }
                            == Some(mode)
                        {
                            match unsafe {
                                arch::x86_64::local_apic::initialize(mode, Some(mapping))
                            } {
                                Ok(()) => {
                                    serial::write_str("X11-OS: local APIC mode = ");
                                    serial::write_str(match mode {
                                        arch::x86_64::apic::ApicMode::XApic => "xAPIC\r\n",
                                        arch::x86_64::apic::ApicMode::X2Apic => "x2APIC\r\n",
                                    });
                                    apic_ready = true;
                                }
                                Err(_) => {
                                    serial::write_str("X11-OS: Local APIC EOI initialization failed\r\n");
                                }
                            }
                        } else {
                            serial::write_str("X11-OS: local APIC mode enable failed\r\n");
                        }
                    } else {
                        serial::write_str("X11-OS: no usable local APIC mode\r\n");
                    }
                } else {
                    serial::write_str("X11-OS: no I/O APIC present, IRQ routing remains disabled\r\n");
                }
            }
            Err(_) => {
                serial::write_str("X11-OS: ACPI MADT discovery failed\r\n");
            }
        }
    } else {
        serial::write_str("X11-OS: ACPI RSDP unavailable\r\n");
    }

    if apic_ready {
        if let (Some(mapping), Some(tsc)) = (physical_mapping, tsc_clock.as_ref()) {
            // SAFETY: The APIC mode was enabled above and the direct map is the
            // bootloader-provided physical-memory mapping.
            match unsafe {
                arch::x86_64::lapic_timer::LapicTimer::new(
                    apic.preferred_mode().expect("APIC mode was marked ready"),
                    Some(mapping),
                )
            } {
                Ok(mut timer_device) => match timer_device.calibrate(tsc) {
                    Ok(frequency) => {
                        serial::write_str("X11-OS: LAPIC timer calibrated Hz = ");
                        serial::write_usize(frequency.hz() as usize);
                        serial::write_str("\r\n");
                        lapic_timer = Some(timer_device);
                    }
                    Err(_) => serial::write_str("X11-OS: LAPIC timer calibration failed\r\n"),
                },
                Err(_) => serial::write_str("X11-OS: LAPIC timer backend unavailable\r\n"),
            }
        } else {
            serial::write_str("X11-OS: LAPIC timer calibration skipped\r\n");
        }
        serial::write_str("X11-OS: APIC platform initialization ready\r\n");
    }

    let mut frame_allocator = memory::EarlyFrameAllocator::new(&boot_info.memory_regions);
    let first_frame = frame_allocator.allocate_frame();
    match first_frame {
        Some(frame) => {
            serial::write_str("X11-OS: first free frame = 0x");
            serial::write_hex(frame.start_address());
            serial::write_str("\r\n");
        }
        None => {
            serial::write_str("X11-OS: no usable physical frame available\r\n");
        }
    }

    if let (Some(mapping), Some(frame)) = (physical_mapping, first_frame) {
        let page_start = memory::KERNEL_SPACE_START + 0x20_0000;
        let page = memory::Page4K::from_start_address(page_start)
            .expect("kernel mapping test page must be aligned");

        // SAFETY: The bootloader established the complete physical-memory
        // direct mapping, and this mapper is initialized exactly once here.
        let mut mapper = unsafe {
            X86PageTableMapper::new(
                mapping.offset(),
                &mut frame_allocator,
                memory::KERNEL_ADDRESS_SPACE,
            )
        };

        if mapper.translate(page.start_address()).is_none() {
            match mapper.map_page(page, frame.start_address()) {
                Ok(flush) => {
                    flush.flush();
                    if mapper.translate(page.start_address()) == Some(frame.start_address()) {
                        serial::write_str("X11-OS: page mapping verified\r\n");
                    } else {
                        serial::write_str("X11-OS: page mapping verification failed\r\n");
                    }

                    match mapper.unmap_page(page) {
                        Ok((unmapped_frame, flush)) => {
                            flush.flush();
                            if unmapped_frame == frame.start_address()
                                && mapper.translate(page.start_address()).is_none()
                            {
                                serial::write_str("X11-OS: page unmapping verified\r\n");
                            } else {
                                serial::write_str("X11-OS: page unmapping verification failed\r\n");
                            }
                        }
                        Err(_) => serial::write_str("X11-OS: page unmapping failed\r\n"),
                    }
                }
                Err(_) => serial::write_str("X11-OS: page mapping failed\r\n"),
            }
        } else {
            serial::write_str("X11-OS: page mapping test skipped, page already mapped\r\n");
        }
    } else {
        serial::write_str("X11-OS: page-table integration test skipped\r\n");
    }

    if let Some(mapping) = physical_mapping {
        if let Some(frames) = frame_allocator.allocate_contiguous(heap::INITIAL_HEAP_FRAMES) {
            if let Some(physical_range) = frames.physical_range() {
                if let Some(virtual_range) = mapping.translate_range(physical_range) {
                    if let (Some(size), Some(region)) = (
                        frames.byte_len(),
                        heap::HeapRegion::new(virtual_range.start(), heap::INITIAL_HEAP_SIZE),
                    ) {
                        if size == heap::INITIAL_HEAP_SIZE && virtual_range.len() == size as u64 {
                            match heap::GLOBAL.init(region) {
                                Ok(()) => {
                                    serial::write_str("X11-OS: heap initialized\r\n");

                                    let before = heap::GLOBAL.stats();
                                    let mut probe = Vec::with_capacity(64);
                                    for value in 0..64u64 {
                                        probe.push(value * 3);
                                    }

                                    let valid = probe.len() == 64
                                        && probe.iter().enumerate().all(|(index, value)| {
                                            *value == index as u64 * 3
                                        });
                                    serial::write_str(if valid {
                                        "X11-OS: heap allocation verified\r\n"
                                    } else {
                                        "X11-OS: heap allocation verification failed\r\n"
                                    });

                                    drop(probe);
                                    let after = heap::GLOBAL.stats();
                                    let reclaimed = after.used() <= before.used();
                                    serial::write_str(if reclaimed {
                                        "X11-OS: heap deallocation verified\r\n"
                                    } else {
                                        "X11-OS: heap deallocation verification failed\r\n"
                                    });

                                    serial::write_str("X11-OS: heap used bytes = ");
                                    serial::write_usize(after.used());
                                    serial::write_str("\r\n");
                                }
                                Err(_) => {
                                    serial::write_str("X11-OS: heap initialization failed\r\n");
                                }
                            }
                        } else {
                            serial::write_str("X11-OS: heap region size mismatch\r\n");
                        }
                    } else {
                        serial::write_str("X11-OS: invalid heap region\r\n");
                    }
                } else {
                    serial::write_str("X11-OS: heap direct-map translation overflow\r\n");
                }
            } else {
                serial::write_str("X11-OS: heap physical range invalid\r\n");
            }
        } else {
            serial::write_str("X11-OS: insufficient contiguous frames for heap\r\n");
        }
    }

    if let Some(mut timer_device) = lapic_timer {
        if let Some(interval) = timer::TimerInterval::new(10_000_000) {
            match timer::TimerDevice::set_periodic(&mut timer_device, interval) {
                Ok(()) => {
                    let start = arch::x86_64::idt::timer_ticks();
                    serial::write_str("X11-OS: LAPIC periodic timer enabled at 100 Hz\r\n");

                    x86_64::instructions::interrupts::enable();
                    while arch::x86_64::idt::timer_ticks() < start.saturating_add(3) {
                        x86_64::instructions::hlt();
                    }
                    x86_64::instructions::interrupts::disable();

                    let ticks = arch::x86_64::idt::timer_ticks();
                    match timer::TimerDevice::disable(&mut timer_device) {
                        Ok(()) => {
                            serial::write_str("X11-OS: LAPIC timer interrupt verified, ticks = ");
                            serial::write_usize(ticks as usize);
                            serial::write_str("\r\n");
                        }
                        Err(_) => serial::write_str("X11-OS: LAPIC timer disable failed\r\n"),
                    }
                }
                Err(_) => serial::write_str("X11-OS: LAPIC periodic timer setup failed\r\n"),
            }
        }
    }

    serial::write_str("X11-OS: entering idle state\r\n");

    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial::write_str("X11-OS: KERNEL PANIC\r\n");

    loop {
        core::hint::spin_loop();
    }
}
