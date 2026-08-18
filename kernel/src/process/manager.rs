//! Bounded process registry with explicit lifecycle ownership.
//!
//! This registry owns process identity, lifecycle state, and the relationship
//! between a process and its execution binding. Scheduler policy, address-space
//! mapping, and architecture execution remain separate.

use super::{ProcessExecutionBinding, ProcessId, ProcessImage, ProcessState};
use crate::memory::AddressSpaceId;
use crate::scheduler::{ExecutionHandle, TaskId};

pub const MAX_PROCESSES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessManagerError {
    Full,
    InvalidProcess,
    InvalidTransition,
    GenerationExhausted,
    AlreadyBound,
    BindingMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Slot {
    generation: u32,
    image: Option<ProcessImage>,
    state: Option<ProcessState>,
    binding: Option<ProcessExecutionBinding>,
}

impl Slot {
    const EMPTY: Self = Self {
        generation: 0,
        image: None,
        state: None,
        binding: None,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnedProcess {
    id: ProcessId,
    binding: ProcessExecutionBinding,
}

impl SpawnedProcess {
    pub const fn id(self) -> ProcessId { self.id }
    pub const fn binding(self) -> ProcessExecutionBinding { self.binding }
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessManager {
    slots: [Slot; MAX_PROCESSES],
    next_hint: usize,
}

impl ProcessManager {
    pub const fn new() -> Self {
        Self {
            slots: [Slot::EMPTY; MAX_PROCESSES],
            next_hint: 0,
        }
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
                binding: None,
            };
            self.next_hint = (index + 1) % MAX_PROCESSES;
            return Ok(id);
        }
        Err(ProcessManagerError::Full)
    }

    /// Atomically registers a process and attaches its execution identity.
    /// If binding validation fails, the registration is rolled back.
    pub fn spawn_ready(
        &mut self,
        image: ProcessImage,
        task: TaskId,
        execution: ExecutionHandle,
        address_space: AddressSpaceId,
    ) -> Result<SpawnedProcess, ProcessManagerError> {
        let id = self.register_ready(image)?;
        match self.attach_execution(id, task, execution, address_space) {
            Ok(binding) => Ok(SpawnedProcess { id, binding }),
            Err(error) => {
                let _ = self.remove_registered(id);
                Err(error)
            }
        }
    }

    pub fn state(&self, id: ProcessId) -> Result<ProcessState, ProcessManagerError> {
        self.slot(id)?.state.ok_or(ProcessManagerError::InvalidProcess)
    }

    pub fn image(&self, id: ProcessId) -> Result<ProcessImage, ProcessManagerError> {
        self.slot(id)?.image.ok_or(ProcessManagerError::InvalidProcess)
    }

    pub fn binding(&self, id: ProcessId) -> Result<Option<ProcessExecutionBinding>, ProcessManagerError> {
        Ok(self.slot(id)?.binding)
    }

    pub fn attach_execution(
        &mut self,
        id: ProcessId,
        task: TaskId,
        execution: ExecutionHandle,
        address_space: AddressSpaceId,
    ) -> Result<ProcessExecutionBinding, ProcessManagerError> {
        let slot = self.slot_mut(id)?;
        if slot.binding.is_some() {
            return Err(ProcessManagerError::AlreadyBound);
        }
        if slot.image.ok_or(ProcessManagerError::InvalidProcess)?.address_space().id() != address_space {
            return Err(ProcessManagerError::BindingMismatch);
        }
        let binding = ProcessExecutionBinding::new(id, task, execution, address_space)
            .ok_or(ProcessManagerError::BindingMismatch)?;
        slot.binding = Some(binding);
        Ok(binding)
    }

    pub fn start(&mut self, id: ProcessId) -> Result<(), ProcessManagerError> {
        let slot = self.slot_mut(id)?;
        if slot.state != Some(ProcessState::Ready) {
            return Err(ProcessManagerError::InvalidTransition);
        }
        if slot.binding.is_none() {
            return Err(ProcessManagerError::BindingMismatch);
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
        slot.binding = None;
        Ok(())
    }

    pub fn contains(&self, id: ProcessId) -> bool {
        self.slot(id).is_ok()
    }

    fn remove_registered(&mut self, id: ProcessId) -> Result<(), ProcessManagerError> {
        let slot = self.slot_mut(id)?;
        slot.image = None;
        slot.state = None;
        slot.binding = None;
        Ok(())
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
    use crate::memory::AddressSpaceId;
    use crate::process::{AddressSpaceSpec, ElfImage, LoadPlan, ProcessImage, ProcessState, UserStackPlan};
    use crate::scheduler::{ExecutionHandle, TaskId};

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
        let address_space = AddressSpaceId::new(1).unwrap();
        let spec = AddressSpaceSpec::new(address_space);
        let plan = LoadPlan::build(spec, parsed).unwrap();
        ProcessImage::build(spec, plan, UserStackPlan::build().unwrap()).unwrap()
    }

    #[test]
    fn lifecycle_requires_execution_binding_before_running() {
        let mut manager = ProcessManager::new();
        let id = manager.register_ready(image()).unwrap();
        assert_eq!(manager.start(id), Err(ProcessManagerError::BindingMismatch));
    }

    #[test]
    fn matching_execution_binding_allows_running() {
        let mut manager = ProcessManager::new();
        let id = manager.register_ready(image()).unwrap();
        let task = TaskId::new(3, 1);
        let execution = ExecutionHandle::for_task(task);
        let as_id = AddressSpaceId::new(1).unwrap();
        assert!(manager.attach_execution(id, task, execution, as_id).is_ok());
        manager.start(id).unwrap();
        assert_eq!(manager.state(id), Ok(ProcessState::Running));
    }

    #[test]
    fn spawn_ready_rolls_back_on_binding_mismatch() {
        let mut manager = ProcessManager::new();
        let task = TaskId::new(3, 1);
        let wrong_as = AddressSpaceId::new(2).unwrap();
        assert_eq!(
            manager.spawn_ready(image(), task, ExecutionHandle::for_task(task), wrong_as),
            Err(ProcessManagerError::BindingMismatch)
        );
        let replacement = manager.register_ready(image()).unwrap();
        assert_eq!(replacement.index(), 0);
        assert_eq!(replacement.generation(), 2);
    }

    #[test]
    fn wrong_address_space_is_rejected() {
        let mut manager = ProcessManager::new();
        let id = manager.register_ready(image()).unwrap();
        let task = TaskId::new(3, 1);
        let wrong = AddressSpaceId::new(2).unwrap();
        assert_eq!(
            manager.attach_execution(id, task, ExecutionHandle::for_task(task), wrong),
            Err(ProcessManagerError::BindingMismatch)
        );
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
