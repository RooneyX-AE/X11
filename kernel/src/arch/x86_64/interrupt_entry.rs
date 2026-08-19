//! x86_64 interrupt-entry register frames.

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SavedRegisters {
    pub rax: u64, pub rcx: u64, pub rdx: u64, pub rbx: u64, pub rbp: u64, pub rsi: u64, pub rdi: u64,
    pub r8: u64, pub r9: u64, pub r10: u64, pub r11: u64, pub r12: u64, pub r13: u64, pub r14: u64, pub r15: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptReturnFrame { raw: *mut u64 }
impl InterruptReturnFrame {
    pub const unsafe fn from_raw(raw: *mut u64) -> Self { Self { raw } }
    pub unsafe fn rip(self) -> u64 { unsafe { *self.raw.add(0) } }
    pub unsafe fn cs(self) -> u64 { unsafe { *self.raw.add(1) } }
    pub unsafe fn rflags(self) -> u64 { unsafe { *self.raw.add(2) } }
    pub unsafe fn rsp(self) -> Option<u64> { if unsafe { self.cs() } & 3 == 0 { None } else { Some(unsafe { *self.raw.add(3) }) } }
    pub unsafe fn ss(self) -> Option<u64> { if unsafe { self.cs() } & 3 == 0 { None } else { Some(unsafe { *self.raw.add(4) }) } }
    pub fn resume_rsp(self) -> Option<u64> {
        let frame = self.raw as usize;
        let bytes = if unsafe { self.cs() } & 3 == 0 { SAME_CPL_FRAME_BYTES } else { CROSS_CPL_FRAME_BYTES };
        frame.checked_add(bytes).map(|value| value as u64)
    }
    pub unsafe fn is_kernel_return(self) -> bool { unsafe { self.rip() != 0 && self.cs() & 3 == 0 && self.rflags() & 2 != 0 } }
}

const GPR_BYTES: usize = core::mem::size_of::<SavedRegisters>();
const SAME_CPL_FRAME_BYTES: usize = 3 * core::mem::size_of::<u64>();
const CROSS_CPL_FRAME_BYTES: usize = 5 * core::mem::size_of::<u64>();
const _: () = assert!(GPR_BYTES == 120);
const _: () = assert!(core::mem::align_of::<SavedRegisters>() == 8);

fn syscall_result_abi_value(result: crate::syscall::SyscallResult) -> u64 {
    match result {
        Ok(value) => value,
        Err(error) => error.abi_return_value(),
    }
}

struct SerialWriteSink;
impl crate::syscall::WriteSink for SerialWriteSink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), crate::syscall::SyscallError> {
        crate::serial::write_bytes(bytes);
        Ok(())
    }
}

#[inline(never)]
extern "C" fn syscall_entry_rust(registers: *mut SavedRegisters, return_frame: *mut u64) {
    let frame = unsafe { InterruptReturnFrame::from_raw(return_frame) };
    if unsafe { frame.cs() } & 3 != 3 { return; }

    let registers = unsafe { &mut *registers };
    let request = crate::syscall::SyscallRequest::new(registers.rax, registers.rdi, registers.rsi, registers.rdx);
    let result = match crate::memory::PhysicalMemoryMapping::global() {
        Some(mapping) => {
            let view = unsafe { crate::arch::x86_64::user_memory::X86ActiveUserMemory::current(mapping.offset()) };
            let backend = crate::arch::x86_64::user_copy::X86UserCopyBackend::new(&view, mapping);
            let mut sink = SerialWriteSink;
            crate::syscall::dispatch_with_memory(request, &view, &backend, &mut sink)
        }
        None => Err(crate::syscall::SyscallError::NotImplemented),
    };

    let success = result.is_ok();
    registers.rax = syscall_result_abi_value(result);
    crate::arch::x86_64::idt::record_user_trap(success);
}

#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        "push r15", "push r14", "push r13", "push r12", "push r11", "push r10", "push r9", "push r8",
        "push rdi", "push rsi", "push rbp", "push rbx", "push rdx", "push rcx", "push rax",
        "mov rax, rsp", "and rsp, -16", "sub rsp, 16", "mov [rsp], rax",
        "mov rdi, rax", "lea rsi, [rax + 120]", "call {rust_hook}",
        "mov rax, [rsp]", "mov rsp, rax",
        "pop rax", "pop rcx", "pop rdx", "pop rbx", "pop rbp", "pop rsi", "pop rdi",
        "pop r8", "pop r9", "pop r10", "pop r11", "pop r12", "pop r13", "pop r14", "pop r15",
        "iretq",
        rust_hook = sym syscall_entry_rust,
    );
}

#[inline(never)]
extern "C" fn timer_entry_rust(registers: *mut SavedRegisters, return_frame: *mut u64) {
    let frame = unsafe { InterruptReturnFrame::from_raw(return_frame) };
    let Some(resume_rsp) = frame.resume_rsp() else { return; };
    let capture_result = unsafe { crate::arch::x86_64::cpu_local::local().capture_interrupted(registers, frame, resume_rsp) };
    crate::arch::x86_64::idt::record_timer_interrupt();
    if capture_result.is_err() { return; }
    let Some(runtime_ptr) = crate::arch::x86_64::cpu_local::local().runtime_ptr() else { return; };
    let outcome = unsafe { (&mut *(runtime_ptr as *mut crate::arch::x86_64::runtime::KernelRuntime)).handle_timer_preemption() };
    match outcome {
        Ok(crate::arch::x86_64::runtime::InterruptPreemption::ReturnToContext(context)) => unsafe { crate::arch::x86_64::preempt_return::return_to_context(&context) },
        Ok(crate::arch::x86_64::runtime::InterruptPreemption::ReturnToKernel(state)) => {
            crate::PREEMPT_IRET_RETURNED.store(true, core::sync::atomic::Ordering::Release);
            unsafe { crate::arch::x86_64::preempt_return::return_to_kernel(&state) }
        }
        Ok(crate::arch::x86_64::runtime::InterruptPreemption::ResumeCurrent) | Err(_) => {}
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn timer_entry() {
    core::arch::naked_asm!(
        "push r15", "push r14", "push r13", "push r12", "push r11", "push r10", "push r9", "push r8",
        "push rdi", "push rsi", "push rbp", "push rbx", "push rdx", "push rcx", "push rax",
        "mov rax, rsp", "and rsp, -16", "sub rsp, 16", "mov [rsp], rax",
        "mov rdi, rax", "lea rsi, [rax + 120]", "call {rust_hook}",
        "mov rax, [rsp]", "mov rsp, rax",
        "pop rax", "pop rcx", "pop rdx", "pop rbx", "pop rbp", "pop rsi", "pop rdi",
        "pop r8", "pop r9", "pop r10", "pop r11", "pop r12", "pop r13", "pop r14", "pop r15",
        "iretq", rust_hook = sym timer_entry_rust,
    );
}

#[cfg(test)]
mod tests {
    use super::{syscall_result_abi_value, InterruptReturnFrame, SavedRegisters, CROSS_CPL_FRAME_BYTES, GPR_BYTES, SAME_CPL_FRAME_BYTES};
    use crate::arch::x86_64::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};

    #[test] fn saved_register_layout_is_exact() { assert_eq!(core::mem::size_of::<SavedRegisters>(), 120); assert_eq!(GPR_BYTES, 120); }
    #[test] fn return_frame_sizes_match_same_and_cross_cpl() { assert_eq!(SAME_CPL_FRAME_BYTES, 24); assert_eq!(CROSS_CPL_FRAME_BYTES, 40); }

    #[test]
    fn syscall_success_returns_value_in_rax_contract() {
        assert_eq!(syscall_result_abi_value(Ok(300)), 300);
    }

    #[test]
    fn syscall_failure_returns_negative_abi_value() {
        let error = crate::syscall::SyscallError::WriteFailed;
        assert_eq!(syscall_result_abi_value(Err(error)), error.abi_return_value());
        assert!(syscall_result_abi_value(Err(error)) > u64::MAX - 16);
    }

    #[test]
    fn user_return_frame_reads_rsp_and_ss() {
        let mut raw = [0u64; 5]; raw[0] = 0x1000; raw[1] = USER_CODE_SELECTOR as u64; raw[2] = 0x202; raw[3] = 0x8000; raw[4] = USER_DATA_SELECTOR as u64;
        let frame = unsafe { InterruptReturnFrame::from_raw(raw.as_mut_ptr()) };
        assert_eq!(unsafe { frame.rsp() }, Some(0x8000));
        assert_eq!(unsafe { frame.ss() }, Some(USER_DATA_SELECTOR as u64));
        assert_eq!(frame.resume_rsp(), Some(0x8000));
    }
}
