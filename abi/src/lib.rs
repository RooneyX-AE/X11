#![no_std]

//! Stable data and syscall-number contract shared by kernel and userspace.
//! Keep this crate dependency-free so either side can evolve independently.

pub const ABI_MAJOR: u16 = 0;
pub const ABI_MINOR: u16 = 1;

#[repr(u64)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Syscall {
    Write = 0,
    Exit = 1,
    Yield = 2,
}

impl Syscall {
    pub const fn number(self) -> u64 {
        self as u64
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserSlice {
    pub ptr: u64,
    pub len: u64,
}

impl UserSlice {
    pub const fn empty() -> Self {
        Self { ptr: 0, len: 0 }
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

const _: () = assert!(core::mem::size_of::<UserSlice>() == 16);
const _: () = assert!(core::mem::align_of::<UserSlice>() == 8);

#[cfg(test)]
mod tests {
    use super::{Syscall, UserSlice, ABI_MAJOR, ABI_MINOR};

    #[test]
    fn syscall_numbers_are_stable() {
        assert_eq!(Syscall::Write.number(), 0);
        assert_eq!(Syscall::Exit.number(), 1);
        assert_eq!(Syscall::Yield.number(), 2);
    }

    #[test]
    fn abi_version_is_explicit() {
        assert_eq!(ABI_MAJOR, 0);
        assert_eq!(ABI_MINOR, 1);
    }

    #[test]
    fn user_slice_layout_is_stable() {
        assert_eq!(core::mem::size_of::<UserSlice>(), 16);
        assert_eq!(core::mem::align_of::<UserSlice>(), 8);
        assert!(UserSlice::empty().is_empty());
    }
}
