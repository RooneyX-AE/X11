//! Process lifecycle state.
//!
//! A process becomes runnable only after construction has produced a fully
//! validated `ProcessImage`. State transitions are explicit and one-way here.

use super::ProcessImage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Constructing,
    Ready,
    Running,
    Exited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadyProcess {
    image: ProcessImage,
    state: ProcessState,
}

impl ReadyProcess {
    pub const fn from_image(image: ProcessImage) -> Self {
        Self { image, state: ProcessState::Ready }
    }

    pub const fn image(self) -> ProcessImage { self.image }
    pub const fn state(self) -> ProcessState { self.state }

    pub fn start(self) -> Result<RunningProcess, ProcessTransitionError> {
        if self.state != ProcessState::Ready {
            return Err(ProcessTransitionError::InvalidTransition);
        }
        Ok(RunningProcess { image: self.image, state: ProcessState::Running })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunningProcess {
    image: ProcessImage,
    state: ProcessState,
}

impl RunningProcess {
    pub const fn image(self) -> ProcessImage { self.image }
    pub const fn state(self) -> ProcessState { self.state }

    pub fn exit(self) -> ExitedProcess {
        ExitedProcess { image: self.image, state: ProcessState::Exited }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitedProcess {
    image: ProcessImage,
    state: ProcessState,
}

impl ExitedProcess {
    pub const fn image(self) -> ProcessImage { self.image }
    pub const fn state(self) -> ProcessState { self.state }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessTransitionError {
    InvalidTransition,
}

#[cfg(test)]
mod tests {
    use super::{ProcessState, ReadyProcess};
    use crate::process::{AddressSpaceId, AddressSpaceSpec, ElfImage, LoadPlan, ProcessImage, UserStackPlan};

    fn image() -> ProcessImage {
        let mut bytes = [0u8; 120];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        let p = 64usize;
        bytes[p..p + 4].copy_from_slice(&1u32.to_le_bytes());
        bytes[p + 4..p + 8].copy_from_slice(&5u32.to_le_bytes());
        bytes[p + 16..p + 24].copy_from_slice(&0x401000u64.to_le_bytes());
        bytes[p + 32..p + 40].copy_from_slice(&16u64.to_le_bytes());
        bytes[p + 40..p + 48].copy_from_slice(&0x1000u64.to_le_bytes());
        let parsed = ElfImage::parse(&bytes).unwrap();
        let address_space = AddressSpaceSpec::new(AddressSpaceId::new(1).unwrap());
        let plan = LoadPlan::build(address_space, parsed).unwrap();
        ProcessImage::build(address_space, plan, UserStackPlan::build().unwrap()).unwrap()
    }

    #[test]
    fn lifecycle_is_explicit_and_one_way() {
        let ready = ReadyProcess::from_image(image());
        assert_eq!(ready.state(), ProcessState::Ready);
        let running = ready.start().unwrap();
        assert_eq!(running.state(), ProcessState::Running);
        let exited = running.exit();
        assert_eq!(exited.state(), ProcessState::Exited);
    }
}
