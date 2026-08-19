#![no_std]

//! Stable data and syscall-number contract shared by kernel and userspace.
//! Keep this crate dependency-free so either side can evolve independently.

pub const ABI_MAJOR: u16 = 0;
pub const ABI_MINOR: u16 = 2;

#[repr(u64)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Syscall {
    Write = 0,
    Exit = 1,
    Yield = 2,
}

impl Syscall {
    pub const fn number(self) -> u64 { self as u64 }

    pub const fn from_number(number: u64) -> Option<Self> {
        match number {
            0 => Some(Self::Write),
            1 => Some(Self::Exit),
            2 => Some(Self::Yield),
            _ => None,
        }
    }
}

#[repr(u64)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallError {
    UnknownSyscall = 1,
    NotImplemented = 2,
    InvalidArguments = 3,
    InvalidUserRange = 4,
    InvalidUserMemory = 5,
    WriteFailed = 6,
}

impl SyscallError {
    pub const fn return_value(self) -> u64 {
        0u64.wrapping_sub(self as u64)
    }

    pub const fn from_return_value(value: u64) -> Option<Self> {
        match 0u64.wrapping_sub(value) {
            1 => Some(Self::UnknownSyscall),
            2 => Some(Self::NotImplemented),
            3 => Some(Self::InvalidArguments),
            4 => Some(Self::InvalidUserRange),
            5 => Some(Self::InvalidUserMemory),
            6 => Some(Self::WriteFailed),
            _ => None,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserSlice {
    pub ptr: u64,
    pub len: u64,
}

impl UserSlice {
    pub const fn empty() -> Self { Self { ptr: 0, len: 0 } }
    pub const fn is_empty(self) -> bool { self.len == 0 }
}

const _: () = assert!(core::mem::size_of::<UserSlice>() == 16);
const _: () = assert!(core::mem::align_of::<UserSlice>() == 8);

#[cfg(test)]
mod tests {
    use super::{Syscall, SyscallError, UserSlice, ABI_MAJOR, ABI_MINOR};

    #[test]
    fn syscall_numbers_are_stable() {
        assert_eq!(Syscall::Write.number(), 0);
        assert_eq!(Syscall::Exit.number(), 1);
        assert_eq!(Syscall::Yield.number(), 2);
    }

    #[test]
    fn syscall_numbers_decode_from_abi_contract() {
        assert_eq!(Syscall::from_number(0), Some(Syscall::Write));
        assert_eq!(Syscall::from_number(1), Some(Syscall::Exit));
        assert_eq!(Syscall::from_number(2), Some(Syscall::Yield));
        assert_eq!(Syscall::from_number(u64::MAX), None);
    }

    #[test]
    fn error_returns_are_negative_values() {
        for error in [
            SyscallError::UnknownSyscall,
            SyscallError::NotImplemented,
            SyscallError::InvalidArguments,
            SyscallError::InvalidUserRange,
            SyscallError::InvalidUserMemory,
            SyscallError::WriteFailed,
        ] {
            let value = error.return_value();
            assert!(value > u64::MAX - 16);
            assert_eq!(SyscallError::from_return_value(value), Some(error));
        }
    }

    #[test]
    fn success_is_not_an_error() {
        assert_eq!(SyscallError::from_return_value(0), None);
    }

    #[test]
    fn abi_version_is_explicit() {
        assert_eq!(ABI_MAJOR, 0);
        assert_eq!(ABI_MINOR, 2);
    }

    #[test]
    fn user_slice_layout_is_stable() {
        assert_eq!(core::mem::size_of::<UserSlice>(), 16);
        assert_eq!(core::mem::align_of::<UserSlice>(), 8);
        assert!(UserSlice::empty().is_empty());
    }
}
