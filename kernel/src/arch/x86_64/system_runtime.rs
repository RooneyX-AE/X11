//! Unified single-CPU owner for process, scheduler, and userspace execution state.

use alloc::boxed::Box;

use crate::process::{ProcessId, ProcessImage, ProcessManager, ProcessState};
use crate::scheduler::{Priority, TaskId, TaskState};
use crate::syscall::{ProcessSyscallControl, SyscallError};

use super::interrupted_state::InterruptedState;
use super::kernel_task::KernelTaskError;
use super::runtime::{KernelRuntime, RuntimeError};
use super::user_execution::{UserExecutionBinding, UserExecutionRegistry, UserExecutionRegistryError};
use super::user_launch::PreparedUserLaunch;
use super::user_successor::{select_userspace_successor, UserSuccessorError};
use super::user_return_transfer::UserReturnTransfer;

extern "C" fn userspace_kernel_stub() -> ! { loop {} }

#[derive(Debug)]
pub struct SystemRuntime {
    processes: ProcessManager,
    runtime: Box<KernelRuntime>,
    userspace: UserExecutionRegistry,
    current_process: Option<ProcessId>,
    pending_exit: Option<(ProcessId, TaskId, u64)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemRuntimeError {
    Runtime(RuntimeError),
    UserBinding(UserExecutionRegistryError),
    Process(crate::process::ProcessManagerError),
    AddressSpaceMismatch,
    NoCurrentProcess,
    TaskBindingMismatch,
    SchedulerTaskNotReady,
    Successor(UserSuccessorError),
    KernelStackUnavailable,
}

impl From<crate::process::ProcessManagerError> for SystemRuntimeError {
    fn from(error: crate::process::ProcessManagerError) -> Self { Self::Process(error) }
}

impl SystemRuntime {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            processes: ProcessManager::new(),
            runtime: KernelRuntime::new(),
            userspace: UserExecutionRegistry::new(),
            current_process: None,
            pending_exit: None,
        })
    }

    pub const fn processes(&self) -> &ProcessManager { &self.processes }
    pub const fn processes_mut(&mut self) -> &mut ProcessManager { &mut self.processes }
    pub const fn runtime(&self) -> &KernelRuntime { &self.runtime }
    pub const fn runtime_mut(&mut self) -> &mut KernelRuntime { &mut self.runtime }
    pub const fn userspace(&self) -> &UserExecutionRegistry { &self.userspace }
    pub const fn pending_exit(&self) -> Option<(ProcessId, TaskId, u64)> { self.pending_exit }

    pub unsafe fn bind_cpu(&mut self) -> Result<(), SystemRuntimeError> {
        unsafe { self.runtime.bind_cpu().map_err(SystemRuntimeError::Runtime)?; }
        unsafe {
            crate::arch::x86_64::cpu_local::local()
                .bind_system_runtime(self as *mut Self as *mut ())
                .map_err(|_| SystemRuntimeError::TaskBindingMismatch)?;
        }
        Ok(())
    }

    pub fn spawn_user_ready(&mut self, image: ProcessImage, launch: PreparedUserLaunch, priority: Priority) -> Result<(ProcessId, TaskId), SystemRuntimeError> {
        let address_space = image.address_space().id();
        if launch.address_space() != address_space { return Err(SystemRuntimeError::AddressSpaceMismatch); }

        let (task, execution) = {
            let scheduler = &mut self.runtime.manager_mut().scheduler;
            let task = scheduler.create_user_task(priority);
            let execution = match scheduler.attach_execution(task) {
                Ok(handle) => handle,
                Err(error) => { let _ = scheduler.destroy_created(task); return Err(SystemRuntimeError::Runtime(RuntimeError::Task(error.into()))); }
            };
            (task, execution)
        };

        if let Err(error) = self.runtime.manager_mut().executions.insert(execution, userspace_kernel_stub) {
            let _ = self.runtime.manager_mut().scheduler.destroy_created(task);
            return Err(SystemRuntimeError::Runtime(RuntimeError::Task(KernelTaskError::Registry(error))));
        }

        let spawned = match self.processes.spawn_ready(image, task, execution, address_space) {
            Ok(spawned) => spawned,
            Err(error) => { let _ = self.runtime.manager_mut().executions.remove(execution); let _ = self.runtime.manager_mut().scheduler.destroy_created(task); return Err(SystemRuntimeError::Process(error)); }
        };

        let binding = match UserExecutionBinding::new(spawned.id(), task, address_space, launch) {
            Ok(binding) => binding,
            Err(_) => { let _ = self.runtime.manager_mut().executions.remove(execution); let _ = self.processes.abort_ready(spawned.id()); let _ = self.runtime.manager_mut().scheduler.destroy_created(task); return Err(SystemRuntimeError::AddressSpaceMismatch); }
        };

        if let Err(error) = self.userspace.insert(binding) {
            let _ = self.runtime.manager_mut().executions.remove(execution); let _ = self.processes.abort_ready(spawned.id()); let _ = self.runtime.manager_mut().scheduler.destroy_created(task); return Err(SystemRuntimeError::UserBinding(error));
        }
        if !self.runtime.manager_mut().scheduler.make_ready(task) {
            let _ = self.userspace.remove(task); let _ = self.runtime.manager_mut().executions.remove(execution); let _ = self.processes.abort_ready(spawned.id()); let _ = self.runtime.manager_mut().scheduler.destroy_created(task); return Err(SystemRuntimeError::TaskBindingMismatch);
        }
        Ok((spawned.id(), task))
    }

    pub unsafe fn start_user(&mut self, process: ProcessId) -> Result<(), SystemRuntimeError> {
        let binding = self.processes.binding(process).map_err(SystemRuntimeError::Process)?.ok_or(SystemRuntimeError::TaskBindingMismatch)?;
        let task = binding.task();
        let user_binding = self.userspace.get(task).ok_or(SystemRuntimeError::TaskBindingMismatch)?;
        if user_binding.process() != process || user_binding.address_space() != binding.address_space() { return Err(SystemRuntimeError::TaskBindingMismatch); }
        if self.processes.state(process).map_err(SystemRuntimeError::Process)? != ProcessState::Ready { return Err(SystemRuntimeError::Process(crate::process::ProcessManagerError::InvalidTransition)); }
        let stack_top = self.runtime.manager().executions.kernel_stack_top(task).ok_or(SystemRuntimeError::KernelStackUnavailable)?;
        {
            let scheduler = &mut self.runtime.manager_mut().scheduler;
            if scheduler.state(task) != Some(TaskState::Ready) || scheduler.next_ready() != Some(task) { return Err(SystemRuntimeError::SchedulerTaskNotReady); }
        }
        self.pending_exit = None;
        self.processes.start(process).map_err(SystemRuntimeError::Process)?;
        let decision = self.runtime.manager_mut().scheduler.schedule_next();
        if decision.next != Some(task) || self.runtime.manager().scheduler.current() != Some(task) { return Err(SystemRuntimeError::TaskBindingMismatch); }
        unsafe { crate::arch::x86_64::gdt::set_kernel_stack_top(stack_top); }
        self.current_process = Some(process);
        unsafe { crate::arch::x86_64::cpu_local::local().set_current_task(Some(task)); }
        let launch = user_binding.launch();
        unsafe { crate::arch::x86_64::user_activation::activate_and_enter_user(launch, stack_top) };
    }

    pub fn current_process(&self) -> Option<ProcessId> { self.current_process }
    pub fn current_user_binding(&self) -> Option<UserExecutionBinding> { self.runtime.manager().scheduler.current().and_then(|task| self.userspace.get(task)) }
    pub fn take_pending_exit(&mut self) -> Option<(ProcessId, TaskId, u64)> { self.pending_exit.take() }

    pub fn commit_pending_exit(&mut self) -> Result<UserReturnTransfer, SystemRuntimeError> {
        let (process, task, _code) = self.pending_exit.ok_or(SystemRuntimeError::NoCurrentProcess)?;
        if self.current_process != Some(process) || self.runtime.manager().scheduler.current() != Some(task) { return Err(SystemRuntimeError::TaskBindingMismatch); }
        let successor = select_userspace_successor(self, Some(task)).map_err(SystemRuntimeError::Successor)?;
        let current_binding = self.processes.binding(process).map_err(SystemRuntimeError::Process)?.ok_or(SystemRuntimeError::TaskBindingMismatch)?;
        if current_binding.task() != task || self.processes.state(process).map_err(SystemRuntimeError::Process)? != ProcessState::Running { return Err(SystemRuntimeError::TaskBindingMismatch); }
        let current_execution = self.runtime.manager().scheduler.execution(task).ok_or(SystemRuntimeError::TaskBindingMismatch)?;
        let successor_stack_top = self.runtime.manager().executions.kernel_stack_top(successor.task()).ok_or(SystemRuntimeError::KernelStackUnavailable)?;
        let transfer = UserReturnTransfer::new(successor.binding(), successor_stack_top).validate(Some(task)).map_err(|_| SystemRuntimeError::TaskBindingMismatch)?;
        self.processes.start(successor.process()).map_err(SystemRuntimeError::Process)?;
        self.processes.exit(process).map_err(SystemRuntimeError::Process)?;
        self.runtime.manager_mut().scheduler.terminate_current_to(successor.task()).map_err(|_| SystemRuntimeError::TaskBindingMismatch)?;
        let _ = self.userspace.remove(task).map_err(SystemRuntimeError::UserBinding)?;
        let _ = self.runtime.manager_mut().executions.remove(current_execution);
        if transfer.resume().is_some() { self.userspace.clear_resume(successor.task()).expect("validated successor resume binding disappeared"); }
        unsafe { crate::arch::x86_64::gdt::set_kernel_stack_top(successor_stack_top); }
        self.current_process = Some(successor.process());
        self.pending_exit = None;
        unsafe { crate::arch::x86_64::cpu_local::local().set_current_task(Some(successor.task())); }
        Ok(transfer)
    }

    pub fn commit_userspace_yield(&mut self) -> Result<Option<UserReturnTransfer>, SystemRuntimeError> { self.commit_userspace_yield_with_snapshot(None) }

    pub fn commit_userspace_yield_with_snapshot(&mut self, interrupted: Option<(TaskId, InterruptedState)>) -> Result<Option<UserReturnTransfer>, SystemRuntimeError> {
        let current_process = self.current_process.ok_or(SystemRuntimeError::NoCurrentProcess)?;
        let current_task = self.runtime.manager().scheduler.current().ok_or(SystemRuntimeError::NoCurrentProcess)?;
        let current_binding = self.processes.binding(current_process).map_err(SystemRuntimeError::Process)?.ok_or(SystemRuntimeError::TaskBindingMismatch)?;
        if current_binding.task() != current_task || self.processes.state(current_process).map_err(SystemRuntimeError::Process)? != ProcessState::Running { return Err(SystemRuntimeError::TaskBindingMismatch); }
        let successor = match select_userspace_successor(self, Some(current_task)) { Ok(successor) => successor, Err(UserSuccessorError::NoReadyTask) => return Ok(None), Err(error) => return Err(SystemRuntimeError::Successor(error)) };
        let successor_stack_top = self.runtime.manager().executions.kernel_stack_top(successor.task()).ok_or(SystemRuntimeError::KernelStackUnavailable)?;
        let transfer = UserReturnTransfer::new(successor.binding(), successor_stack_top).validate(Some(current_task)).map_err(|_| SystemRuntimeError::TaskBindingMismatch)?;
        if let Some((captured_task, state)) = interrupted {
            if captured_task != current_task || !state.is_user_valid() { return Err(SystemRuntimeError::TaskBindingMismatch); }
            self.userspace.install_resume(current_task, state).map_err(SystemRuntimeError::UserBinding)?;
        }
        self.processes.yield_to(current_process, successor.process()).map_err(SystemRuntimeError::Process)?;
        let decision = self.runtime.manager_mut().scheduler.schedule_next();
        if decision.next != Some(successor.task()) || self.runtime.manager().scheduler.current() != Some(successor.task()) { return Err(SystemRuntimeError::TaskBindingMismatch); }
        if transfer.resume().is_some() { self.userspace.clear_resume(successor.task()).expect("validated successor resume binding disappeared"); }
        unsafe { crate::arch::x86_64::gdt::set_kernel_stack_top(successor_stack_top); }
        self.current_process = Some(successor.process());
        unsafe { crate::arch::x86_64::cpu_local::local().set_current_task(Some(successor.task())); }
        Ok(Some(transfer))
    }

    fn current_binding_checked(&self) -> Result<UserExecutionBinding, SyscallError> {
        if self.pending_exit.is_some() { return Err(SyscallError::InvalidArguments); }
        let process = self.current_process.ok_or(SyscallError::InvalidArguments)?;
        let binding = self.processes.binding(process).map_err(|_| SyscallError::InvalidArguments)?.ok_or(SyscallError::InvalidArguments)?;
        let task = self.runtime.manager().scheduler.current().ok_or(SyscallError::InvalidArguments)?;
        if binding.task() != task || self.processes.state(process).map_err(|_| SyscallError::InvalidArguments)? != ProcessState::Running { return Err(SyscallError::InvalidArguments); }
        let user = self.userspace.get(task).ok_or(SyscallError::InvalidArguments)?;
        if user.process() != process || user.address_space() != binding.address_space() { return Err(SyscallError::InvalidArguments); }
        Ok(user)
    }
}

impl ProcessSyscallControl for SystemRuntime {
    fn exit(&mut self, code: u64) -> Result<(), SyscallError> { let user = self.current_binding_checked()?; self.pending_exit = Some((user.process(), user.task(), code)); self.runtime.request_reschedule(); Ok(()) }
    fn yield_now(&mut self) -> Result<(), SyscallError> { let _ = self.current_binding_checked()?; self.runtime.request_reschedule(); Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::SystemRuntime;
    use crate::arch::x86_64::address_space::AddressSpaceRoot;
    use crate::arch::x86_64::user_launch::prepare_launch;
    use crate::memory::user_stack_range;
    use crate::process::{AddressSpaceId, AddressSpaceSpec, ElfImage, LoadPlan, ProcessImage, ProcessState, UserLaunchPlan, UserStackPlan};
    use crate::scheduler::{Priority, TaskState};

    fn image(id: AddressSpaceId) -> ProcessImage {
        let mut bytes = [0u8; 120]; bytes[0..4].copy_from_slice(b"\x7fELF"); bytes[4]=2; bytes[5]=1; bytes[16..18].copy_from_slice(&2u16.to_le_bytes()); bytes[18..20].copy_from_slice(&62u16.to_le_bytes()); bytes[24..32].copy_from_slice(&0x401000u64.to_le_bytes()); bytes[32..40].copy_from_slice(&64u64.to_le_bytes()); bytes[54..56].copy_from_slice(&56u16.to_le_bytes()); bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        let p=64usize; bytes[p..p+4].copy_from_slice(&1u32.to_le_bytes()); bytes[p+4..p+8].copy_from_slice(&5u32.to_le_bytes()); bytes[p+16..p+24].copy_from_slice(&0x401000u64.to_le_bytes()); bytes[p+32..p+40].copy_from_slice(&16u64.to_le_bytes()); bytes[p+40..p+48].copy_from_slice(&0x1000u64.to_le_bytes());
        let parsed=ElfImage::parse(&bytes).unwrap(); let spec=AddressSpaceSpec::new(id); let plan=LoadPlan::build(spec,parsed).unwrap(); ProcessImage::build(spec,plan,UserStackPlan::build().unwrap()).unwrap()
    }

    fn launch(id: AddressSpaceId) -> crate::arch::x86_64::user_launch::PreparedUserLaunch {
        let root=AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap(); let stack=user_stack_range().unwrap(); prepare_launch(root,UserLaunchPlan { address_space:id, entry:crate::memory::USER_SPACE_START+0x1000, stack_pointer:stack.end() }).unwrap()
    }

    #[test]
    fn user_ready_transaction_stops_before_running() {
        let mut system=SystemRuntime::new(); let id=AddressSpaceId::new(7).unwrap(); let (process,task)=system.spawn_user_ready(image(id),launch(id),Priority::DEFAULT).unwrap();
        assert_eq!(system.processes().state(process),Ok(ProcessState::Ready)); assert_eq!(system.runtime().manager().scheduler.state(task),Some(TaskState::Ready)); assert!(system.runtime().manager().executions.kernel_stack_top(task).is_some()); assert_eq!(system.current_process(),None); assert_eq!(system.current_user_binding(),None); assert_eq!(system.pending_exit(),None);
    }
}
