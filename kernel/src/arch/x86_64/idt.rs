//! CPU exception and early interrupt handling.
//!
//! Exception handlers stay intentionally small. The raw timer entry records a
//! hardware event, acknowledges the local APIC, and may hand control directly
//! to the scheduler return boundary when preemption is enabled.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use spin::Once;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode, PrivilegeLevel};
use x86_64::VirtAddr;

use crate::interrupts::{InterruptEvent, InterruptSource, TIMER_VECTOR};

use super::gdt::DOUBLE_FAULT_IST_INDEX;
use super::interrupt_entry;

pub const USER_TRAP_VECTOR: u8 = 0x80;

static IDT: Once<InterruptDescriptorTable> = Once::new();
static LAST_PAGE_FAULT_ADDRESS: AtomicUsize = AtomicUsize::new(0);
static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);
static TIMER_PENDING: AtomicBool = AtomicBool::new(false);
static USER_TRAP_COUNT: AtomicU64 = AtomicU64::new(0);
static USER_SYSCALL_OK_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let idt = IDT.call_once(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            idt[USER_TRAP_VECTOR as usize]
                .set_handler_addr(VirtAddr::new(
                    interrupt_entry::syscall_entry as *const () as usize as u64,
                ))
                .set_privilege_level(PrivilegeLevel::Ring3);
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(DOUBLE_FAULT_IST_INDEX);
            idt[TIMER_VECTOR].set_handler_addr(VirtAddr::new(
                interrupt_entry::timer_entry as *const () as usize as u64,
            ));
        }
        idt
    });

    idt.load();
}

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    crate::serial::write_str("[x11-os] breakpoint exception\n");
}

pub fn record_user_trap(dispatch_succeeded: bool) {
    USER_TRAP_COUNT.fetch_add(1, Ordering::AcqRel);
    if dispatch_succeeded {
        USER_SYSCALL_OK_COUNT.fetch_add(1, Ordering::AcqRel);
    }
    crate::serial::write_str("X11-OS: userspace int 0x80 trap reached\r\n");
}

extern "x86-interrupt" fn page_fault_handler(
    _stack_frame: InterruptStackFrame,
    _error_code: PageFaultErrorCode,
) {
    let address = Cr2::read_raw();
    LAST_PAGE_FAULT_ADDRESS.store(address as usize, Ordering::Release);
    crate::serial::write_str("[x11-os] page fault\n");
    loop { core::hint::spin_loop(); }
}

extern "x86-interrupt" fn double_fault_handler(_stack_frame: InterruptStackFrame, _error_code: u64) -> ! {
    crate::serial::write_str("[x11-os] double fault\n");
    loop { core::hint::spin_loop(); }
}

/// Shared timer bookkeeping for the raw timer entry.
///
/// This is the sole timer-event accounting path. It owns no scheduler policy;
/// the interrupt-return boundary decides whether to keep the current task or
/// transfer control to a different execution context.
pub fn record_timer_interrupt() {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    TIMER_PENDING.store(true, Ordering::Release);
    if let Some(event) = InterruptEvent::new(InterruptSource::Timer) {
        crate::arch::x86_64::local_apic::end_of_interrupt(event);
    }
}

pub fn timer_ticks() -> u64 { TIMER_TICKS.load(Ordering::Acquire) }
pub fn take_timer_pending() -> bool { TIMER_PENDING.swap(false, Ordering::AcqRel) }
pub fn user_trap_count() -> u64 { USER_TRAP_COUNT.load(Ordering::Acquire) }
pub fn user_syscall_ok_count() -> u64 { USER_SYSCALL_OK_COUNT.load(Ordering::Acquire) }
