//! Address-space-aware validation for userspace return targets.
//!
//! This layer sits above CPU-frame validation and below the final CR3/iretq
//! boundary. It verifies that RIP/RSP are usable in the target address space
//! with the permissions required by a userspace instruction stream and stack.

use crate::memory::{is_valid_user_stack_pointer, user_stack_range, UserMemoryView, USER_SPACE_START, KERNEL_SPACE_START};

use super::interrupted_state::InterruptedState;
use super::user_return::UserReturnFrame;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserReturnTargetError {
    InvalidInstructionPointer,
    InstructionPageUnavailable,
    InstructionPageNotExecutable,
    InstructionPageNotUserAccessible,
    InvalidStackPointer,
    StackPageUnavailable,
    StackPageNotWritable,
    StackPageNotUserAccessible,
}

fn validate_instruction_pointer<M: UserMemoryView>(mapper: &M, rip: u64) -> Result<(), UserReturnTargetError> {
    if !(USER_SPACE_START..KERNEL_SPACE_START).contains(&rip) {
        return Err(UserReturnTargetError::InvalidInstructionPointer);
    }

    let access = mapper.page_access(rip);
    if !access.mapped {
        return Err(UserReturnTargetError::InstructionPageUnavailable);
    }
    if !access.user {
        return Err(UserReturnTargetError::InstructionPageNotUserAccessible);
    }
    if !access.executable {
        return Err(UserReturnTargetError::InstructionPageNotExecutable);
    }
    Ok(())
}

fn validate_stack_pointer<M: UserMemoryView>(mapper: &M, rsp: u64) -> Result<(), UserReturnTargetError> {
    if !is_valid_user_stack_pointer(rsp) {
        return Err(UserReturnTargetError::InvalidStackPointer);
    }

    let stack = user_stack_range().ok_or(UserReturnTargetError::InvalidStackPointer)?;
    let probe_address = if rsp == stack.end() { rsp.checked_sub(1).ok_or(UserReturnTargetError::InvalidStackPointer)? } else { rsp };
    let access = mapper.page_access(probe_address);
    if !access.mapped {
        return Err(UserReturnTargetError::StackPageUnavailable);
    }
    if !access.user {
        return Err(UserReturnTargetError::StackPageNotUserAccessible);
    }
    if !access.writable {
        return Err(UserReturnTargetError::StackPageNotWritable);
    }
    Ok(())
}

pub fn validate_user_return_frame<M: UserMemoryView>(mapper: &M, frame: UserReturnFrame) -> Result<(), UserReturnTargetError> {
    validate_instruction_pointer(mapper, frame.rip)?;
    validate_stack_pointer(mapper, frame.rsp)
}

pub fn validate_user_resume<M: UserMemoryView>(mapper: &M, state: InterruptedState) -> Result<(), UserReturnTargetError> {
    if !state.is_user_valid() {
        return Err(UserReturnTargetError::InvalidInstructionPointer);
    }
    validate_instruction_pointer(mapper, state.return_state().rip())?;
    validate_stack_pointer(
        mapper,
        state.return_state().rsp().ok_or(UserReturnTargetError::InvalidStackPointer)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{PageAccess, VirtRange};

    struct FakeMapper {
        instruction: PageAccess,
        stack: PageAccess,
    }

    impl UserMemoryView for FakeMapper {
        fn translate(&self, virtual_address: u64) -> Option<u64> {
            if !(USER_SPACE_START..KERNEL_SPACE_START).contains(&virtual_address) { None } else { Some(virtual_address) }
        }

        fn page_access(&self, virtual_address: u64) -> PageAccess {
            let stack = user_stack_range().unwrap();
            if stack.start() <= virtual_address && virtual_address <= stack.end() {
                self.stack
            } else {
                self.instruction
            }
        }

        fn address_space(&self) -> VirtRange { VirtRange::new(USER_SPACE_START, KERNEL_SPACE_START).unwrap() }
    }

    fn valid_mapper() -> FakeMapper {
        FakeMapper { instruction: PageAccess { mapped: true, user: true, readable: true, writable: false, executable: true }, stack: PageAccess { mapped: true, user: true, readable: true, writable: true, executable: false } }
    }

    #[test]
    fn accepts_executable_user_code_and_writable_stack() {
        let mapper = valid_mapper();
        let stack = user_stack_range().unwrap();
        let frame = UserReturnFrame { rip: USER_SPACE_START + 0x1000, cs: 0x1b, rflags: 0x202, rsp: stack.end(), ss: 0x23 };
        assert!(validate_user_return_frame(&mapper, frame).is_ok());
    }

    #[test]
    fn rejects_kernel_rip() {
        let mapper = valid_mapper();
        let stack = user_stack_range().unwrap();
        let frame = UserReturnFrame { rip: KERNEL_SPACE_START, cs: 0x1b, rflags: 0x202, rsp: stack.end(), ss: 0x23 };
        assert_eq!(validate_user_return_frame(&mapper, frame), Err(UserReturnTargetError::InvalidInstructionPointer));
    }

    #[test]
    fn rejects_nx_instruction_page() {
        let mut mapper = valid_mapper();
        mapper.instruction.executable = false;
        let stack = user_stack_range().unwrap();
        let frame = UserReturnFrame { rip: USER_SPACE_START + 0x1000, cs: 0x1b, rflags: 0x202, rsp: stack.end(), ss: 0x23 };
        assert_eq!(validate_user_return_frame(&mapper, frame), Err(UserReturnTargetError::InstructionPageNotExecutable));
    }

    #[test]
    fn rejects_read_only_stack_page() {
        let mut mapper = valid_mapper();
        mapper.stack.writable = false;
        let stack = user_stack_range().unwrap();
        let frame = UserReturnFrame { rip: USER_SPACE_START + 0x1000, cs: 0x1b, rflags: 0x202, rsp: stack.end(), ss: 0x23 };
        assert_eq!(validate_user_return_frame(&mapper, frame), Err(UserReturnTargetError::StackPageNotWritable));
    }
}
