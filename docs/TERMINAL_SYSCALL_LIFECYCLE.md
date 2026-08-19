# Terminal Syscall Lifecycle

This document defines the invariants for userspace `Yield` and `Exit` while the x86_64 return path is being completed.

## Yield

A userspace `Yield` must:

1. Validate the current `ProcessId`, `TaskId`, and `AddressSpaceId` binding.
2. Produce `SyscallReturnAction::Reschedule`.
3. Leave the current userspace frame intact until the architectural transfer is ready.
4. Select only a `TaskKind::Userspace` successor in `TaskState::Ready`.
5. Require the successor `ProcessExecutionBinding` and `UserExecutionBinding` to agree.
6. Transition process and scheduler state before executing `CR3 + iretq`.
7. Treat the single-runnable-userspace-task case as a no-op reschedule.

## Exit

A userspace `Exit(code)` must:

1. Validate the current process/task/address-space identity.
2. Record the exit request without destroying the current task inside the syscall handler.
3. Require a validated userspace successor before destructive scheduler mutation.
4. Remove the current process/task from ownership before transferring to the successor.
5. Never `iretq` into the terminated task's old userspace frame.
6. Preserve the exit code for diagnostics/accounting even when the transfer path is terminal.

## Forbidden shortcuts

The return path must not reinterpret a userspace `iretq` frame as a kernel context, destroy a task before its successor is validated, or rely on a global singleton to identify the current process.
