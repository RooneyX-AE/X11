//! x86_64 interrupt-frame contracts for preemptive scheduling.
//!
//! The CPU creates the architectural return frame on interrupt entry. This
//! module models the distinction between a kernel-to-kernel interrupt frame
//! and a later privilege-transition frame. It deliberately does not perform
//! context switching or mutate the live CPU frame yet.

/// CPU-saved frame for an interrupt taken while already running at CPL0.
///
/// The x86_64 interrupt ABI saves `RIP`, `CS`, and `RFLAGS` for this case.
/// The saved `RSP`/`SS` pair is only added when the interrupt crosses privilege
/// levels, so it must not be assumed to exist for the kernel timer path.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelInterruptFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
}

impl KernelInterruptFrame {
    pub const fn new(rip: u64, cs: u64, rflags: u64) -> Self {
        Self { rip, cs, rflags }
    }

    pub const fn interrupt_enabled(self) -> bool {
        self.rflags & (1 << 9) != 0
    }
}

/// Additional frame state present when an interrupt transfers privilege
/// levels, for example a future userspace (CPL3) -> kernel (CPL0) interrupt.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivilegeTransitionFrame {
    pub rsp: u64,
    pub ss: u64,
}

/// Combined return state for a privilege-changing interrupt.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserInterruptFrame {
    pub base: KernelInterruptFrame,
    pub transition: PrivilegeTransitionFrame,
}

#[cfg(test)]
mod tests {
    use super::{KernelInterruptFrame, PrivilegeTransitionFrame, UserInterruptFrame};

    #[test]
    fn kernel_frame_has_cpu_saved_shape() {
        assert_eq!(core::mem::size_of::<KernelInterruptFrame>(), 24);
        assert_eq!(core::mem::align_of::<KernelInterruptFrame>(), 8);
    }

    #[test]
    fn privilege_transition_state_is_explicit() {
        assert_eq!(core::mem::size_of::<PrivilegeTransitionFrame>(), 16);
        assert_eq!(core::mem::size_of::<UserInterruptFrame>(), 40);
    }

    #[test]
    fn interrupt_flag_is_read_without_mutating_frame() {
        let enabled = KernelInterruptFrame::new(0, 0, 1 << 9);
        let disabled = KernelInterruptFrame::new(0, 0, 0);
        assert!(enabled.interrupt_enabled());
        assert!(!disabled.interrupt_enabled());
    }
}
