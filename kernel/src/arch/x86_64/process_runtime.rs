//! x86_64 process/runtime ownership bridge.
//!
//! This adapter is the only layer allowed to coordinate the generic process
//! registry with the x86_64 execution runtime. Syscall code does not reach into
//! either owner directly.

use crate::process::{AddressSpaceId, ProcessId, ProcessManager, ProcessManagerError, ProcessState};
use crate::scheduler::{ExecutionHandle, Priority, Scheduler, SchedulerError, TaskId};
use crate::syscall::{ProcessSyscallControl, SyscallError};

use super::runtime::{KernelRuntime, RuntimeError};
use super::user_execution::UserExecutionBinding;
use super::user_launch::PreparedUserLaunch;

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

impl From<ProcessManagerError> for ProcessRuntimeError { fn from(error: ProcessManagerError) -> Self { Self::Process(error) } }
impl From<RuntimeError> for ProcessRuntimeError { fn from(error: RuntimeError) -> Self { Self::Runtime(error) } }

impl ProcessRuntimeOwner<'_> {
    fn syscall_error(error: ProcessRuntimeError) -> SyscallError {
        match error {
            ProcessRuntimeError::NoCurrentProcess | ProcessRuntimeError::TaskBindingMismatch | ProcessRuntimeError::ProcessNotRunning | ProcessRuntimeError::Process(_) | ProcessRuntimeError::Runtime(_) => SyscallError::InvalidArguments,
        }
    }
}

impl ProcessSyscallControl for ProcessRuntimeOwner<'_> {
    fn exit(&mut self, code: u64) -> Result<(), SyscallError> { self.exit_current(code).map_err(Self::syscall_error) }
    fn yield_now(&mut self) -> Result<(), SyscallError> { self.request_yield().map_err(Self::syscall_error) }
}

/// Creates a process, creates its scheduler identity, binds the user iretq
/// launch state, and only then exposes the task to the ready queue.
///
/// The caller owns the returned UserExecutionBinding and may install it in a
/// long-lived userspace execution registry. No CPU switch occurs here.
pub fn spawn_user_ready(
    processes: &mut ProcessManager,
    scheduler: &mut Scheduler,
    image: crate::process::ProcessImage,
    launch: PreparedUserLaunch,
    priority: Priority,
) -> Result<(ProcessId, TaskId, UserExecutionBinding), UserSpawnError> {
    let address_space = image.address_space().id();
    if launch.address_space() != address_space {
        return Err(UserSpawnError::AddressSpaceMismatch);
    }

    let task = scheduler.create_task(priority);
    let execution = match scheduler.attach_execution(task) {
        Ok(handle) => handle,
        Err(error) => {
            let _ = scheduler.destroy_created(task);
            return Err(UserSpawnError::Scheduler(error));
        }
    };

    let spawned = match processes.spawn_ready(image, task, execution, address_space) {
        Ok(spawned) => spawned,
        Err(error) => {
            let _ = scheduler.destroy_created(task);
            return Err(UserSpawnError::Process(error));
        }
    };

    let binding = match UserExecutionBinding::new(spawned.id(), task, address_space, launch) {
        Ok(binding) => binding,
        Err(_) => {
            let _ = processes.abort_ready(spawned.id());
            let _ = scheduler.destroy_created(task);
            return Err(UserSpawnError::AddressSpaceMismatch);
        }
    };

    if !scheduler.make_ready(task) {
        let _ = processes.abort_ready(spawned.id());
        let _ = scheduler.destroy_created(task);
        return Err(UserSpawnError::ReadyQueueRejected);
    }

    Ok((spawned.id(), task, binding))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSpawnError { AddressSpaceMismatch, Process(ProcessManagerError), Scheduler(SchedulerError), ReadyQueueRejected }

impl<'a> ProcessRuntimeOwner<'a> {
    pub fn new(processes: &'a mut ProcessManager, runtime: &'a mut KernelRuntime) -> Self { Self { processes, runtime, current_process: None } }

    pub fn bind_current(&mut self, process: ProcessId) -> Result<(), ProcessRuntimeError> {
        let binding = self.processes.binding(process)?.ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        let current_task = self.runtime.manager().scheduler.current().ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        if binding.task() != current_task { return Err(ProcessRuntimeError::TaskBindingMismatch); }
        if self.processes.state(process)? != ProcessState::Running { return Err(ProcessRuntimeError::ProcessNotRunning); }
        self.current_process = Some(process); Ok(())
    }

    pub fn current_process(&self) -> Option<ProcessId> { self.current_process }
    pub fn current_task(&self) -> Option<TaskId> { self.current_process.and_then(|process| self.processes.binding(process).ok().flatten().map(|binding| binding.task())) }

    pub fn exit_current(&mut self, _code: u64) -> Result<(), ProcessRuntimeError> {
        let process = self.current_process.ok_or(ProcessRuntimeError::NoCurrentProcess)?;
        let binding = self.processes.binding(process)?.ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        let task = self.runtime.manager().scheduler.current().ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        if binding.task() != task { return Err(ProcessRuntimeError::TaskBindingMismatch); }
        if self.processes.state(process)? != ProcessState::Running { return Err(ProcessRuntimeError::ProcessNotRunning); }
        self.processes.exit(process)?;
        if !self.runtime.manager_mut().exit_current() { return Err(ProcessRuntimeError::TaskBindingMismatch); }
        self.current_process = None; Ok(())
    }

    pub fn request_yield(&mut self) -> Result<(), ProcessRuntimeError> {
        let process = self.current_process.ok_or(ProcessRuntimeError::NoCurrentProcess)?;
        let binding = self.processes.binding(process)?.ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        let current_task = self.runtime.manager().scheduler.current().ok_or(ProcessRuntimeError::TaskBindingMismatch)?;
        if binding.task() != current_task { return Err(ProcessRuntimeError::TaskBindingMismatch); }
        if self.processes.state(process)? != ProcessState::Running { return Err(ProcessRuntimeError::ProcessNotRunning); }
        self.runtime.request_reschedule(); Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{spawn_user_ready, ProcessRuntimeError, ProcessRuntimeOwner, UserSpawnError};
    use crate::arch::x86_64::address_space::AddressSpaceRoot;
    use crate::arch::x86_64::user_launch::prepare_launch;
    use crate::memory::{USER_SPACE_START, user_stack_range};
    use crate::process::{AddressSpaceId, AddressSpaceSpec, ElfImage, LoadPlan, ProcessImage, ProcessManager, ProcessState, UserLaunchPlan, UserStackPlan};
    use crate::scheduler::{Priority, Scheduler, TaskId};
    use crate::syscall::{ProcessSyscallControl, SyscallError};

    fn image(address_space: AddressSpaceId) -> ProcessImage {
        let mut bytes = [0u8; 120]; bytes[0..4].copy_from_slice(b"\x7fELF"); bytes[4]=2; bytes[5]=1; bytes[16..18].copy_from_slice(&2u16.to_le_bytes()); bytes[18..20].copy_from_slice(&62u16.to_le_bytes()); bytes[24..32].copy_from_slice(&0x401000u64.to_le_bytes()); bytes[32..40].copy_from_slice(&64u64.to_le_bytes()); bytes[54..56].copy_from_slice(&56u16.to_le_bytes()); bytes[56..58].copy_from_slice(&1u16.to_le_bytes()); let p=64usize; bytes[p..p+4].copy_from_slice(&1u32.to_le_bytes()); bytes[p+4..p+8].copy_from_slice(&5u32.to_le_bytes()); bytes[p+16..p+24].copy_from_slice(&0x401000u64.to_le_bytes()); bytes[p+32..p+40].copy_from_slice(&16u64.to_le_bytes()); bytes[p+40..p+48].copy_from_slice(&0x1000u64.to_le_bytes()); let parsed=ElfImage::parse(&bytes).unwrap(); let spec=AddressSpaceSpec::new(address_space); let plan=LoadPlan::build(spec,parsed).unwrap(); ProcessImage::build(spec,plan,UserStackPlan::build().unwrap()).unwrap()
    }

    fn launch(address_space: AddressSpaceId) -> PreparedUserLaunch {
        let root = AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap(); let stack=user_stack_range().unwrap(); let plan=UserLaunchPlan { address_space, entry: USER_SPACE_START+0x1000, stack_pointer: stack.end() }; prepare_launch(root,plan).unwrap()
    }

    extern "C" fn never_returns() -> ! { loop {} }

    #[test]
    fn spawn_user_ready_is_transactional_and_ready_only_at_end() {
        let address_space=AddressSpaceId::new(7).unwrap(); let mut processes=ProcessManager::new(); let mut scheduler=Scheduler::new();
        let image=image(address_space); let (process,task,binding)=spawn_user_ready(&mut processes,&mut scheduler,image,launch(address_space),Priority::DEFAULT).unwrap();
        assert_eq!(processes.state(process),Ok(ProcessState::Ready)); assert_eq!(scheduler.state(task),Some(crate::scheduler::TaskState::Ready)); assert_eq!(binding.task(),task); assert_eq!(binding.address_space(),address_space);
    }

    #[test]
    fn spawn_user_ready_rejects_address_space_mismatch_without_ready_task() {
        let image_space=AddressSpaceId::new(7).unwrap(); let launch_space=AddressSpaceId::new(8).unwrap(); let mut processes=ProcessManager::new(); let mut scheduler=Scheduler::new();
        assert_eq!(spawn_user_ready(&mut processes,&mut scheduler,image(image_space),launch(launch_space),Priority::DEFAULT),Err(UserSpawnError::AddressSpaceMismatch)); assert_eq!(scheduler.task_count(),0);
    }

    #[test]
    fn bind_current_requires_matching_running_task() {
        let mut processes=ProcessManager::new(); let process=processes.register_ready(image(AddressSpaceId::new(1).unwrap())).unwrap(); let mut runtime=crate::arch::x86_64::runtime::KernelRuntime::new(); let mut owner=ProcessRuntimeOwner::new(&mut processes,&mut runtime); assert_eq!(owner.bind_current(process),Err(ProcessRuntimeError::TaskBindingMismatch));
    }

    #[test]
    fn syscall_control_maps_unbound_yield() { let mut processes=ProcessManager::new(); let _=processes.register_ready(image(AddressSpaceId::new(1).unwrap())).unwrap(); let mut runtime=crate::arch::x86_64::runtime::KernelRuntime::new(); let mut owner=ProcessRuntimeOwner::new(&mut processes,&mut runtime); assert_eq!(owner.yield_now(),Err(SyscallError::InvalidArguments)); }

    #[allow(dead_code)] extern "C" fn _kernel_entry() -> ! { never_returns() }
    #[allow(dead_code)] const _: fn(TaskId) -> TaskId = |id| id;
}
