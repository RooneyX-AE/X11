//! Final x86_64 userspace activation boundary.
//!
//! This is the only architecture primitive that combines address-space
//! activation with the userspace `iretq` entry. Higher layers must prepare a
//! `PreparedUserLaunch` first, so the CR3 root, optional PCID, and return frame
//! remain paired.

use x86_64::instructions::interrupts;
use x86_64::registers::control::{Cr4, Cr4Flags};

use super::address_space::{activate, activate_with_pcid};
use super::user_entry::enter_user;
use super::user_launch::PreparedUserLaunch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserActivationError {
    FrameAddressOverflow,
}

/// Activates the prepared address space and transitions to CPL3.
///
/// # Safety
/// The prepared root must contain valid supervisor mappings required by the
/// return path, its user mappings must contain a RIP/RSP already validated
/// against that root, the GDT must be initialized with the expected ring-3
/// selectors, and no other CPU context may concurrently mutate the address
/// space. If a PCID is present, CR4.PCIDE must be enabled and the PCID must be
/// unique for this address space on the current CPU.
///
/// This function never returns after `enter_user`.
pub unsafe fn activate_and_enter_user(prepared: PreparedUserLaunch) -> ! {
    // Interrupts must remain disabled between CR3 activation and `iretq` so a
    // timer or external IRQ cannot observe the half-switched execution state.
    interrupts::disable();

    // The return frame is a kernel-local value and `enter_user` consumes its
    // address only while the old kernel stack is still active.
    let frame = prepared.frame();

    if let Some(pcid) = prepared.pcid() {
        if Cr4::read().contains(Cr4Flags::PCID) {
            // SAFETY: the PCID was assigned by the runtime from the dedicated
            // lifecycle allocator and is not recycled without invalidation.
            unsafe { activate_with_pcid(prepared.root(), pcid) };
        } else {
            // CPU supports PCID but it was not enabled during early boot. Keep
            // the architectural fallback rather than invoking a PCID CR3 write.
            unsafe { activate(prepared.root()) };
        }
    } else {
        // SAFETY: caller guarantees the prepared root is a valid level-4 table
        // and owns the target userspace mappings.
        unsafe { activate(prepared.root()) };
    }

    // SAFETY: the runtime has validated the frame against the target root and
    // the target CR3 is now active.
    unsafe { enter_user(&frame) }
}
