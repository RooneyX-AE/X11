//! Bounded process registry with explicit lifecycle ownership.
//!
//! This registry owns process identity and state only. Scheduler policy,
//! address-space mapping, and architecture execution remain separate.

use super::{ProcessId, ProcessImage, ProcessState};

pub const MAX_PROCESSES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessManagerError {
    Full,
    InvalidProcess,
    InvalidTransition,
    GenerationExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Slot {
    generation: u32,
    image: Option<ProcessImage>,
    state: Option<ProcessState>,
}

impl Slot {
    const EMPTY: Self = Self { generation: 0, image: None, state: None };
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessManager {
    slots: [Slot; MAX_PROCESSES],
    next_hint: usize,
}

impl ProcessManager {
    pub const fn new() -> Self {
        Self { slots: [Slot::EMPTY; MAX_PROCESSES], next_hint: 0 }
    }

    pub fn register_ready(&mut self, image: ProcessImage) -> Result<ProcessId, ProcessManagerError> {
        for offset in 0..MAX_PROCESSES {
            let index = (self.next_hint + offset) % MAX_PROCESSES;
            if self.slots[index].image.is_some() { continue; }

            let generation = self.slots[index]
                .generation
                .checked_add(1)
                .ok_or(ProcessManagerError::GenerationExhausted)?;
            let id = ProcessId::new(index as u32, generation);
            self.slots[index] = Slot {
                generation,
                image: Some(image),
                state: Some(ProcessState::Ready),
            };
            self.next_hint = (index + 1) % MAX_PROCESSES;
            return Ok(id);
        }
        Err(ProcessManagerError::Full)
    }

    pub fn state(&self, id: ProcessId) -> Result<ProcessState, ProcessManagerError> {
        self.slot(id)?.state.ok_or(ProcessManagerError::InvalidProcess)
    }

    pub fn image(&self, id: ProcessId) -> Result<ProcessImage, ProcessManagerError> {
        self.slot(id)?.image.ok_or(ProcessManagerError::InvalidProcess)
    }

    pub fn start(&mut self, id: ProcessId) -> Result<(), ProcessManagerError> {
        let slot = self.slot_mut(id)?;
        if slot.state != Some(ProcessState::Ready) {
            return Err(ProcessManagerError::InvalidTransition);
        }
        slot.state = Some(ProcessState::Running);
        Ok(())
    }

    pub fn exit(&mut self, id: ProcessId) -> Result<(), ProcessManagerError> {
        let slot = self.slot_mut(id)?;
        if !matches!(slot.state, Some(ProcessState::Ready | ProcessState::Running)) {
            return Err(ProcessManagerError::InvalidTransition);
        }
        slot.state = Some(ProcessState::Exited);
        slot.image = None;
        Ok(())
    }

    pub fn contains(&self, id: ProcessId) -> bool {
        self.slot(id).is_ok()
    }

    fn slot(&self, id: ProcessId) -> Result<&Slot, ProcessManagerError> {
        let index = id.index() as usize;
        let slot = self.slots.get(index).ok_or(ProcessManagerError::InvalidProcess)?;
        if slot.generation != id.generation() || slot.image.is_none() {
            return Err(ProcessManagerError::InvalidProcess);
        }
        Ok(slot)
    }

    fn slot_mut(&mut self, id: ProcessId) -> Result<&mut Slot, ProcessManagerError> {
        let index = id.index() as usize;
        let slot = self.slots.get_mut(index).ok_or(ProcessManagerError::InvalidProcess)?;
        if slot.generation != id.generation() || slot.image.is_none() {
            return Err(ProcessManagerError::InvalidProcess);
        }
        Ok(slot)
    }
}

impl Default for ProcessManager {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::{ProcessManager, ProcessManagerError};
    use crate::process::{AddressSpaceId, AddressSpaceSpec, ElfImage, LoadPlan, ProcessImage, ProcessState, UserStackPlan};

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
    fn lifecycle_is_owned_by_manager() {
        let mut manager = ProcessManager::new();
        let id = manager.register_ready(image()).unwrap();
        assert_eq!(manager.state(id), Ok(ProcessState::Ready));
        manager.start(id).unwrap();
        assert_eq!(manager.state(id), Ok(ProcessState::Running));
        manager.exit(id).unwrap();
        assert!(!manager.contains(id));
    }

    #[test]
    fn stale_id_cannot_touch_replacement_slot() {
        let mut manager = ProcessManager::new();
        let old = manager.register_ready(image()).unwrap();
        manager.exit(old).unwrap();
        let new = manager.register_ready(image()).unwrap();
        assert_eq!(old.index(), new.index());
        assert_ne!(old.generation(), new.generation());
        assert_eq!(manager.state(old), Err(ProcessManagerError::InvalidProcess));
        assert_eq!(manager.state(new), Ok(ProcessState::Ready));
    }
}
