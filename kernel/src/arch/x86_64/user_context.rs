//! x86_64 user-mode execution context contract.
//!
//! This is data only. Entry/return assembly remains separate so process
//! construction cannot accidentally perform a privilege transition itself.

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserContext {
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub cs: u16,
    pub ss: u16,
}

impl UserContext {
    pub const fn new(rip: u64, rsp: u64, cs: u16, ss: u16) -> Self {
        Self {
            rip,
            rsp,
            rflags: 0x202,
            cs,
            ss,
        }
    }

    pub const fn is_user_selectors(self, user_code: u16, user_data: u16) -> bool {
        self.cs == user_code && self.ss == user_data && (self.cs & 3) == 3 && (self.ss & 3) == 3
    }
}

const _: () = {
    assert!(core::mem::size_of::<UserContext>() == 32);
    assert!(core::mem::align_of::<UserContext>() == 8);
};

#[cfg(test)]
mod tests {
    use super::UserContext;

    #[test]
    fn builds_ring3_context_with_interrupts_enabled() {
        let context = UserContext::new(0x400000, 0x800000, 0x23, 0x1b);
        assert_eq!(context.rip, 0x400000);
        assert_eq!(context.rsp, 0x800000);
        assert_eq!(context.rflags & (1 << 9), 1 << 9);
        assert!(context.is_user_selectors(0x23, 0x1b));
    }

    #[test]
    fn rejects_kernel_selectors() {
        let context = UserContext::new(0x400000, 0x800000, 0x08, 0x10);
        assert!(!context.is_user_selectors(0x23, 0x1b));
    }
}
