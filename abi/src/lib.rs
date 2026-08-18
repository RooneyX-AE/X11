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

#[cfg(test)]
mod tests {
    use super::{Syscall, ABI_MAJOR, ABI_MINOR};

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
}
