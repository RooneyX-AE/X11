//! Single-CPU kernel runtime owner.
//!
//! The runtime owns the scheduler, execution registry, and boot continuation.
//! Tasks never access these components through global mutable state. The runtime
//! is boxed so its address remains stable while a spawned task is executing.

use alloc::boxed::Box;

use crate::scheduler::{Priority, TaskId};

use super::context_switch::Context;
use super::kernel_task::{KernelTaskError, KernelTaskManager};
use super::yield::{self, YieldError};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RuntimeError {
    Task(KernelTaskError),
    NoRunnableTask,
    MissingExecutionPair,
    Yield(YieldError),
}

impl From<YieldError> for RuntimeError {
    fn from(error: YieldError) -> Self {
        Self::Yield(error)
    }
}

#[derive(Debug)]
pub struct KernelRuntime {
    manager: KernelTaskManager,
    boot_context: Context,
}

impl KernelRuntime {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            manager: KernelTaskManager::new(),
            boot_context: Context::empty(),
        })
    }

    pub const fn manager(&self) -> &KernelTaskManager {
        &self.manager
    }

    pub const fn manager_mut(&mut self) -> &mut KernelTaskManager {
        &mut self.manager
    }

    pub const fn boot_context(&self) -> &Context {
        &self.boot_context
    }

    pub fn boot_context_mut(&mut self) -> &mut Context {
        &mut self.boot_context
    }

    pub fn address(&self) -> u64 {
        self as *const Self as usize as u64
    }

    pub fn spawn(
        &mut self,
        priority: Priority,
        entry: extern "C" fn() -> !,
    ) -> Result<TaskId, RuntimeError> {
        self.manager.spawn(priority, entry).map_err(RuntimeError::Task)
    }

    /// Select and validate one runnable task without performing a CPU switch.
    pub fn prepare_run(&mut self) -> Result<TaskId, RuntimeError> {
        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        decision.next.ok_or(RuntimeError::NoRunnableTask)
    }

    /// Select the next runnable task and perform the architecture-specific
    /// voluntary context switch. The first call activates a task from the boot
    /// continuation; later calls switch between two live task contexts.
    ///
    /// # Safety
    /// The task entry functions must obey the kernel-task ABI and return only
    /// through the scheduler/runtime's explicit lifecycle paths. Interrupt
    /// state must be controlled by the caller during the switch boundary.
    pub unsafe fn dispatch_once(&mut self) -> Result<(), RuntimeError> {
        let decision = self.manager.prepare_dispatch().map_err(RuntimeError::Task)?;
        let next = decision.next.ok_or(RuntimeError::NoRunnableTask)?;

        match decision.previous {
            None => {
                let next_context = self
                    .manager
                    .executions
                    .get(next)
                    .ok_or(RuntimeError::MissingExecutionPair)?
                    .context();

                // SAFETY: `boot_context` is the live continuation owned by this
                // runtime and `next_context` belongs to a validated execution
                // binding whose stack lifetime is owned by the registry.
                unsafe {
                    yield::activate_first(
                        &mut self.boot_context as *mut Context,
                        next_context as *const Context,
                    )?;
                }
            }
            Some(previous) => {
                let (current, next_context) = self
                    .manager
                    .executions
                    .context_pair_mut(previous, next)
                    .ok_or(RuntimeError::MissingExecutionPair)?;

                // SAFETY: `context_pair_mut` guarantees distinct execution
                // objects and the registry owns their backing stacks.
                unsafe {
                    yield::switch(current as *mut Context, next_context as *const Context)?;
                }
            }
        }

        Ok(())
    }

    pub fn execution_ready(&self, task_id: TaskId) -> bool {
        self.manager.is_executable(task_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{KernelRuntime, RuntimeError};

    #[test]
    fn boxed_runtime_has_stable_address() {
        let runtime = KernelRuntime::new();
        let before = runtime.address();
        let moved = runtime;
        assert_eq!(before, moved.address());
    }

    #[test]
    fn dispatch_requires_a_runnable_task() {
        let mut runtime = KernelRuntime::new();
        let result = unsafe { runtime.dispatch_once() };
        assert_eq!(result, Err(RuntimeError::NoRunnableTask));
    }
}