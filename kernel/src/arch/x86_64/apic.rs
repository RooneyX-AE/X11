//! x86_64 local APIC capability detection and mode selection.
//!
//! Hardware mutation remains intentionally small here. Routing, vector
//! allocation, timer programming, and IPI policy belong to higher layers.

use core::arch::asm;

use x86_64::registers::model_specific::{ApicBase, ApicBaseFlags, X2Apic};

use crate::interrupts::{EXTERNAL_VECTOR_BASE, VECTOR_MAX};

/// Local APIC operating mode supported by the kernel backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApicMode {
    XApic,
    X2Apic,
}

/// Capability snapshot collected before APIC initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApicCapabilities {
    pub apic: bool,
    pub x2apic: bool,
}

impl ApicCapabilities {
    pub fn detect() -> Self {
        let leaf1 = cpuid(1, 0);
        let leaf7 = cpuid(7, 0);
        let apic = leaf1.0 & (1 << 9) != 0;
        let x2apic = apic && leaf7.2 & (1 << 21) != 0;
        Self { apic, x2apic }
    }

    pub const fn preferred_mode(self) -> Option<ApicMode> {
        if self.x2apic {
            Some(ApicMode::X2Apic)
        } else if self.apic {
            Some(ApicMode::XApic)
        } else {
            None
        }
    }
}

/// Vector allocator reserved for external interrupts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VectorAllocator {
    next: u8,
}

impl VectorAllocator {
    pub const fn new() -> Self {
        Self { next: EXTERNAL_VECTOR_BASE }
    }

    pub fn allocate(&mut self) -> Option<u8> {
        if self.next >= VECTOR_MAX {
            return None;
        }
        let vector = self.next;
        self.next += 1;
        Some(vector)
    }
}

/// Read the local APIC base and preserve its existing enable flags.
pub fn apic_base() -> (u64, ApicBaseFlags) {
    let (frame, flags) = ApicBase::read();
    (frame.start_address().as_u64(), flags)
}

/// Enable the local APIC and, when available, select x2APIC mode.
///
/// # Safety
///
/// The caller must ensure this CPU is in a kernel context with interrupts
/// still disabled while the local APIC is configured.
pub unsafe fn enable_preferred_mode(capabilities: ApicCapabilities) -> Option<ApicMode> {
    let mode = capabilities.preferred_mode()?;
    let (frame, flags) = ApicBase::read();

    if flags.contains(ApicBaseFlags::ENABLE) == false {
        let mut new_flags = flags;
        new_flags.insert(ApicBaseFlags::ENABLE);
        if matches!(mode, ApicMode::X2Apic) {
            new_flags.insert(ApicBaseFlags::EXTENDED);
        }
        unsafe { ApicBase::write(frame, new_flags) };
    } else if matches!(mode, ApicMode::X2Apic) && !flags.contains(ApicBaseFlags::EXTENDED) {
        let mut new_flags = flags;
        new_flags.insert(ApicBaseFlags::EXTENDED);
        unsafe { ApicBase::write(frame, new_flags) };
    }

    Some(mode)
}

#[inline]
fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx);
    // SAFETY: CPUID is available on x86_64 CPUs and does not access memory.
    unsafe {
        asm!(
            "cpuid",
            inlateout("eax") leaf => eax,
            inlateout("ecx") subleaf => ecx,
            lateout("ebx") ebx,
            lateout("edx") edx,
            options(nomem, nostack, preserves_flags),
        );
    }
    (eax, ebx, ecx, edx)
}

#[cfg(test)]
mod tests {
    use super::{ApicCapabilities, ApicMode, VectorAllocator};
    use crate::interrupts::EXTERNAL_VECTOR_BASE;

    #[test]
    fn prefers_x2apic() {
        let capabilities = ApicCapabilities { apic: true, x2apic: true };
        assert_eq!(capabilities.preferred_mode(), Some(ApicMode::X2Apic));
    }

    #[test]
    fn falls_back_to_xapic() {
        let capabilities = ApicCapabilities { apic: true, x2apic: false };
        assert_eq!(capabilities.preferred_mode(), Some(ApicMode::XApic));
    }

    #[test]
    fn refuses_missing_apic() {
        let capabilities = ApicCapabilities { apic: false, x2apic: false };
        assert_eq!(capabilities.preferred_mode(), None);
    }

    #[test]
    fn vectors_start_after_exception_space() {
        let mut allocator = VectorAllocator::new();
        assert_eq!(allocator.allocate(), Some(EXTERNAL_VECTOR_BASE));
    }
}
