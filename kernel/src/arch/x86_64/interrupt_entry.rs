//! x86_64 interrupt-entry register frame.
//!
//! The timer entry stub is deliberately separate from the Rust
//! `extern "x86-interrupt"` path. It owns the exact assembly/Rust ABI, saves all
//! general-purpose registers, exposes the CPU-owned return frame, and returns
//! with `iretq` without changing scheduler state.

/// General-purpose registers saved by the timer entry stub.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SavedRegisters {
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
}

/// CPU-owned interrupt return frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptReturnFrame {
    raw: *mut u64,
}

unsafe impl Send for InterruptReturnFrame {}
unsafe impl Sync for InterruptReturnFrame {}

impl InterruptReturnFrame {
    pub const unsafe fn from_raw(raw: *mut u64) -> Self {
        Self { raw }
    }

    pub unsafe fn rip(self) -> u64 {
        unsafe { *self.raw.add(0) }
    }

    pub unsafe fn cs(self) -> u64 {
        unsafe { *self.raw.add(1) }
    }

    pub unsafe fn rflags(self) -> u64 {
        unsafe { *self.raw.add(2) }
    }

    pub unsafe fn rsp(self) -> Option<u64> {
        if unsafe { self.cs() } & 3 == 0 {
            return None;
        }
        Some(unsafe { *self.raw.add(3) })
    }

    pub unsafe fn ss(self) -> Option<u64> {
        if unsafe { self.cs() } & 3 == 0 {
            return None;
        }
        Some(unsafe { *self.raw.add(4) })
    }

    pub unsafe fn is_kernel_return(self) -> bool {
        unsafe { self.rip() != 0 && self.cs() & 3 == 0 && self.rflags() & 2 != 0 }
    }
}

const GPR_BYTES: usize = core::mem::size_of::<SavedRegisters>();
const SAME_CPL_FRAME_BYTES: usize = 3 * core::mem::size_of::<u64>();
const CROSS_CPL_FRAME_BYTES: usize = 5 * core::mem::size_of::<u64>();

const _: () = assert!(GPR_BYTES == 120);
const _: () = assert!(core::mem::align_of::<SavedRegisters>() == 8);

/// Rust-side timer entry hook.
///
/// The current implementation only acknowledges the timer and records the
/// deferred event. Scheduler policy is intentionally not executed here.
#[inline(never)]
extern "C" fn timer_entry_rust(
    _registers: *mut SavedRegisters,
    return_frame: *mut u64,
) {
    let frame = unsafe { InterruptReturnFrame::from_raw(return_frame) };
    // The current raw timer path is a kernel-only path. If this invariant ever
    // changes, the entry ABI must explicitly support the user-return frame.
    if unsafe { frame.is_kernel_return() } {
        crate::arch::x86_64::idt::record_timer_interrupt();
    }
}

/// Raw timer-entry ABI.
///
/// # Safety
/// The symbol is valid only as an x86_64 IDT interrupt entry. The caller must
/// install it with `Entry::set_handler_addr()` only after verifying this exact
/// ABI.
#[unsafe(naked)]
pub unsafe extern "C" fn timer_entry() {
    core::arch::naked_asm!(
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rdi",
        "push rsi",
        "push rbp",
        "push rbx",
        "push rdx",
        "push rcx",
        "push rax",
        "mov rdi, rsp",
        "lea rsi, [rsp + 120]",
        "call {rust_hook}",
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rbx",
        "pop rbp",
        "pop rsi",
        "pop rdi",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "iretq",
        rust_hook = sym timer_entry_rust,
    );
}

#[cfg(test)]
mod tests {
    use super::{InterruptReturnFrame, SavedRegisters, CROSS_CPL_FRAME_BYTES, GPR_BYTES, SAME_CPL_FRAME_BYTES};

    #[test]
    fn saved_register_layout_is_exact() {
        assert_eq!(core::mem::size_of::<SavedRegisters>(), 120);
        assert_eq!(GPR_BYTES, 120);
    }

    #[test]
    fn return_frame_sizes_match_same_and_cross_cpl() {
        assert_eq!(SAME_CPL_FRAME_BYTES, 24);
        assert_eq!(CROSS_CPL_FRAME_BYTES, 40);
    }

    #[test]
    fn kernel_return_frame_does_not_read_rsp_or_ss() {
        let mut raw = [0u64; 5];
        raw[0] = 0x1000;
        raw[1] = 0x10;
        raw[2] = 0x202;
        raw[3] = 0xDEAD;
        raw[4] = 0xBEEF;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        assert!(unsafe { frame.is_kernel_return() });
        assert_eq!(unsafe { frame.rsp() }, None);
        assert_eq!(unsafe { frame.ss() }, None);
    }

    #[test]
    fn user_return_frame_reads_rsp_and_ss() {
        let mut raw = [0u64; 5];
        raw[0] = 0x1000;
        raw[1] = 0x1B;
        raw[2] = 0x202;
        raw[3] = 0x8000;
        raw[4] = 0x23;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        assert!(!unsafe { frame.is_kernel_return() });
        assert_eq!(unsafe { frame.rsp() }, Some(0x8000));
        assert_eq!(unsafe { frame.ss() }, Some(0x23));
    }
}
