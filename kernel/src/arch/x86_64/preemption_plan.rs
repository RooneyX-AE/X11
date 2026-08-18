//! Preemption return-path selection.
//!
//! The scheduler policy chooses a task. This layer chooses the CPU return
//! mechanism required by that task's execution state.

use super::interrupted_state::KernelPreemptState;
use crate::scheduler::TaskId;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PreemptionPlan {
    /// The selected task has never been interrupted and therefore cannot be
    /// entered through an interrupted-state `iretq` path yet.
    Bootstrap { task_id: TaskId },
    /// The selected task owns a complete kernel interrupted-state snapshot.
    IretKernel { task_id: TaskId, state: KernelPreemptState },
}

impl PreemptionPlan {
    pub const fn task_id(self) -> TaskId {
        match self {
            Self::Bootstrap { task_id } | Self::IretKernel { task_id, .. } => task_id,
        }
    }

    pub const fn is_kernel_iret(self) -> bool {
        matches!(self, Self::IretKernel { .. })
    }
}
