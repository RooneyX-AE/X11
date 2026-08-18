//! Interrupt-return context model for x86_64 preemption.
//!
//! This is intentionally separate from the cooperative `Context`. An
//! interrupted task carries volatile registers plus the architectural return
//! frame and therefore needs a distinct ABI before an `iretq`-based switch can
//! be implemented safely.

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InterruptContext {
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl InterruptContext {
    const RFLAGS_RESERVED_ONE: u64 = 1 << 1;

    pub const fn is_valid(self) -> bool {
        self.rip != 0 && self.rflags & Self::RFLAGS_RESERVED_ONE != 0
    }
}

const _: () = assert!(core::mem::size_of::<InterruptContext>() == 112);

#[cfg(test)]
mod tests {
    use super::InterruptContext;

    #[test]
    fn interrupted_context_requires_valid_return_frame() {
        let invalid = InterruptContext { rip: 0x1000, ..InterruptContext::default() };
        assert!(!invalid.is_valid());

        let valid = InterruptContext {
            rip: 0x1000,
            rflags: 1 << 1,
            ..InterruptContext::default()
        };
        assert!(valid.is_valid());
    }
}
