//! Validated IA-32e IRET return-frame representation.
//!
//! The frame is kept as architecture data until a dedicated assembly return
//! path materializes it on the active stack. This prevents scheduler code from
//! constructing raw stack words ad hoc.

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelIretFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IretFrameError {
    InvalidRip,
    InvalidCodeSelector,
    InvalidRflags,
}

impl KernelIretFrame {
    pub fn new(rip: u64, cs: u16, rflags: u64) -> Result<Self, IretFrameError> {
        if rip == 0 {
            return Err(IretFrameError::InvalidRip);
        }
        if cs as u64 & 3 != 0 {
            return Err(IretFrameError::InvalidCodeSelector);
        }
        // x86 RFLAGS bit 1 is reserved and reads as 1.
        if rflags & 2 == 0 {
            return Err(IretFrameError::InvalidRflags);
        }
        Ok(Self {
            rip,
            cs: cs as u64,
            rflags,
        })
    }

    pub const fn words(self) -> [u64; 3] {
        [self.rip, self.cs, self.rflags]
    }
}

#[cfg(test)]
mod tests {
    use super::{IretFrameError, KernelIretFrame};

    #[test]
    fn kernel_frame_has_architectural_word_order() {
        let frame = KernelIretFrame::new(0x1000, 0x8, 0x202).unwrap();
        assert_eq!(frame.words(), [0x1000, 0x8, 0x202]);
    }

    #[test]
    fn frame_rejects_null_rip() {
        assert_eq!(
            KernelIretFrame::new(0, 0x8, 0x202),
            Err(IretFrameError::InvalidRip)
        );
    }

    #[test]
    fn frame_rejects_user_code_selector() {
        assert_eq!(
            KernelIretFrame::new(0x1000, 0x1B, 0x202),
            Err(IretFrameError::InvalidCodeSelector)
        );
    }

    #[test]
    fn frame_requires_reserved_rflags_bit() {
        assert_eq!(
            KernelIretFrame::new(0x1000, 0x8, 0x200),
            Err(IretFrameError::InvalidRflags)
        );
    }
}
