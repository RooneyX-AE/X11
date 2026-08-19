//! x86_64 process/runtime ownership bridge.
//!
//! This adapter is the only layer allowed to coordinate the generic process
//! registry with the x86_64 execution runtime. Syscall code does not reach into
//! either owner directly.

use crate::process::{ProcessId, ProcessManager, ProcessManagerError, ProcessState};
use crate::scheduler::TaskId;

use super::runtime::{KernelRuntime, RuntimeError};

#[derive(Debug)]
pub struct ProcessRuntimeOwner<'a> {
    processes: &'a mut ProcessManager,
    runtime: &'a mut KernelRuntime,
    current_process: Option<ProcessId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessRuntimeError {
    NoCurrentProcess,
    Process(ProcessManagerError),
    Runtime(RuntimeError),
    TaskBindingMismatch,
    ProcessNotRunning,
}

impl From<ProcessManagerError> for ProcessRuntimeError {
    fn from(error: ProcessManagerError) -> Self { Self::Process(error) }
}

impl From<RuntimeError> for ProcessRuntimeError {
    fn from(error: RuntimeError) -> Self { Self::Runtime(error) }
}

impl<'a> ProcessRuntimeOwner<'a> {
    pub fn new(processes: &'a mut ProcessManager, runtime: &'a mut KernelRuntime) -> Self {
        Self { processes, runtime, current_process: None }
    }

    pub fn bind_current(&mut self, process: ProcessId) -> Result<(), ProcessRuntimeError> {
        let binding = self.processes.binding(process)?.ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        let current_task = self.runtime.manager().scheduler.current().ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        if binding.task() != current_task {
            return Err(ProcessRuntimeError::TaskBindingMismatch);
        }
        if self.processes.state(process)? != ProcessState::Running {
            return Err(ProcessRuntimeError::ProcessNotRunning);
        }
        self.current_process = Some(process);
        Ok(())
    }

    pub fn current_process(&self) -> Option<ProcessId> { self.current_process }

    pub fn current_task(&self) -> Option<TaskId> {
        self.current_process.and_then(|process| self.processes.binding(process).ok().flatten().map(|binding| binding.task()))
    }

    /// Marks the current process exited and removes the matching runtime task.
    ///
    /// All cross-owner invariants are checked before mutating either owner, so
    /// a binding mismatch cannot partially destroy process/runtime state.
    pub fn exit_current(&mut self, _code: u64) -> Result<(), ProcessRuntimeError> {
        let process = self.current_process.ok_or(ProcessRuntimeError::NoCurrentProcess)?;
        let binding = self.processes.binding(process)?.ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        let task = self.runtime.manager().scheduler.current().ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        if binding.task() != task {
            return Err(ProcessRuntimeError::TaskBindingMismatch);
        }
        if self.processes.state(process)? != ProcessState::Running {
            return Err(ProcessRuntimeError::ProcessNotRunning);
        }

        self.processes.exit(process)?;
        if !self.runtime.manager_mut().exit_current() {
            return Err(ProcessRuntimeError::TaskBindingMismatch);
        }
        self.current_process = None;
        Ok(())
    }

    /// Requests a deferred reschedule. The interrupt-return boundary owns the
    /// actual context transfer and therefore remains the only place that may
    /// consume the pending request.
    pub fn request_yield(&mut self) -> Result<(), ProcessRuntimeError> {
        let process = self.current_process.ok_or(ProcessRuntimeError::NoCurrentProcess)?;
        let binding = self.processes.binding(process)?.ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        let current_task = self.runtime.manager().scheduler.current().ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        if binding.task() != current_task {
            return Err(ProcessRuntimeError::TaskBindingMismatch);
        }
        if self.processes.state(process)? != ProcessState::Running {
            return Err(ProcessRuntimeError::ProcessNotRunning);
        }
        self.runtime.request_reschedule();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcessRuntimeError, ProcessRuntimeOwner};
    use crate::memory::AddressSpaceId;
    use crate::process::{AddressSpaceSpec, ElfImage, LoadPlan, ProcessImage, ProcessManager, ProcessState, UserStackPlan};
    use crate::scheduler::{ExecutionHandle, Priority, TaskId};

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

    extern "C" fn never_returns() -> ! { loop {} }

    #[test]
    fn bind_current_rejects_task_mismatch() {
        let mut processes = ProcessManager::new();
        let process = processes.register_ready(image()).unwrap();
        let task = TaskId::new(3, 1);
        let mut runtime = crate::arch::x86_64::runtime::KernelRuntime::new();
        let _runtime_task = runtime.spawn(Priority::DEFAULT, never_returns).unwrap();
        let _execution = ExecutionHandle::for_task(task);
        assert_eq!(
            ProcessRuntimeOwner::new(&mut processes, &mut runtime).bind_current(process),
            Err(ProcessRuntimeError::TaskBindingMismatch)
        );
    }

    #[test]
    fn request_yield_requires_bound_running_process() {
        let mut processes = ProcessManager::new();
        let process = processes.register_ready(image()).unwrap();
        let mut runtime = crate::arch::x86_64::runtime::KernelRuntime::new();
        let mut owner = ProcessRuntimeOwner::new(&mut processes, &mut runtime);
        assert_eq!(owner.request_yield(), Err(ProcessRuntimeError::NoCurrentProcess));
        assert_eq!(processes.state(process), Ok(ProcessState::Ready));
    }
}
