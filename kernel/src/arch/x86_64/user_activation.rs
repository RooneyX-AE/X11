//! Final x86_64 userspace activation boundary.
//!
//! This is the only architecture primitive that combines address-space
//! activation with the userspace `iretq` entry. Higher layers must prepare a
//! `PreparedUserLaunch` first, so the CR3 root and return frame remain paired.

use x86_64::instructions::interrupts;

use super::address_space::activate;
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
/// return path, its user mappings must contain the validated RIP/RSP, the GDT
/// must be initialized with the expected ring-3 selectors, and no other CPU
/// context may concurrently mutate the address space.
///
/// This function never returns after `enter_user`.
pub unsafe fn activate_and_enter_user(prepared: PreparedUserLaunch) -> ! {
    // Interrupts must remain disabled between CR3 activation and `iretq` so a
    // timer or external IRQ cannot observe the half-switched execution state.
    interrupts::disable();

    // The return frame is a kernel-local value and `enter_user` consumes its
    // address only while the old kernel stack is still active.
    let frame = prepared.frame();

    // SAFETY: caller guarantees the prepared root is a valid level-4 table and
    // owns the target userspace mappings.
    unsafe { activate(prepared.root()) };

    // SAFETY: `UserReturnFrame` was validated when `PreparedUserLaunch` was
    // constructed, and the target CR3 is now active.
    unsafe { enter_user(&frame) }
}
