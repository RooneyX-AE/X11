//! x86_64 interrupt-entry register frame.
//!
//! This module is intentionally not installed into the IDT yet. It freezes the
//! assembly/Rust ABI first so preemption can be reviewed independently from
//! interrupt routing.

use core::arch::asm;

/// Register state captured by the timer entry stub before Rust executes.
///
/// The CPU-owned return frame follows these fields on the interrupted stack.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InterruptContext {
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
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

const GPR_BYTES: usize = 15 * core::mem::size_of::<u64>();
const SAME_CPL_FRAME_BYTES: usize = 3 * core::mem::size_of::<u64>();
const CROSS_CPL_FRAME_BYTES: usize = 5 * core::mem::size_of::<u64>();

const _: () = assert!(core::mem::size_of::<InterruptContext>() == 160);
const _: () = assert!(core::mem::align_of::<InterruptContext>() == 8);

impl InterruptContext {
    pub const fn same_cpl(rip: u64, cs: u64, rflags: u64) -> Self {
        Self {
            rax: 0,
            rcx: 0,
            rdx: 0,
            rbx: 0,
            rbp: 0,
            rsi: 0,
            rdi: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip,
            cs,
            rflags,
            rsp: 0,
            ss: 0,
        }
    }

    pub const fn is_kernel_return(self) -> bool {
        self.rip != 0 && self.cs & 3 == 0 && self.rflags & 2 != 0
    }

    pub const fn captured_bytes(same_cpl: bool) -> usize {
        GPR_BYTES + if same_cpl { SAME_CPL_FRAME_BYTES } else { CROSS_CPL_FRAME_BYTES }
    }
}

/// Rust-side hook used by the future timer entry path.
///
/// Keeping it as a no-op for now lets the assembly ABI compile and be tested
/// without enabling actual preemption before scheduler integration is ready.
#[inline(never)]
extern "C" fn timer_entry_rust(_context: *mut InterruptContext) {}

/// Naked timer-entry prototype. It saves all general-purpose registers,
/// invokes the Rust hook, restores the registers, and returns with IRETQ.
///
/// This symbol is not installed in the IDT yet.
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
    use super::InterruptContext;

    #[test]
    fn same_cpl_context_requires_kernel_code_segment() {
        let context = InterruptContext::same_cpl(0x1000, 0x10, 0x202);
        assert!(context.is_kernel_return());
    }

    #[test]
    fn user_code_segment_is_not_kernel_return() {
        let context = InterruptContext::same_cpl(0x1000, 0x1b, 0x202);
        assert!(!context.is_kernel_return());
    }

    #[test]
    fn captured_sizes_match_amd64_interrupt_frames() {
        assert_eq!(InterruptContext::captured_bytes(true), 144);
        assert_eq!(InterruptContext::captured_bytes(false), 160);
    }
}

#[allow(dead_code)]
fn _keep_asm_import_live() {
    let _ = asm as unsafe fn();
}
