# Execution Architecture Audit

Status: design gate before preemption/SMP

## Verified contracts

- `x86_64-unknown-none` uses System V ABI without a red zone.
- The generic scheduler owns task identity, lifecycle state, and queue policy.
- Task control blocks use stable `Box` allocation so slot-table growth cannot relocate a live task object.
- x86_64 owns register context, kernel-stack representation, and context-switch assembly.
- Voluntary context switching and interrupt/preemptive switching are distinct contracts.
- The scheduler exposes an architecture-independent `ExecutionBinding` contract.
- The x86_64 execution adapter owns its kernel stack and voluntary `Context` behind that contract.
- The bootstrap context-switch smoke test now owns its context, stack, continuation state, and test flag on the caller's stack; it has no static `UnsafeCell` state.

## Required invariants

1. A running task's kernel stack must remain at a stable address for its entire execution lifetime.
2. A task execution object must not be moved or dropped while its stack is active.
3. A voluntary `Context` must not be reused as an interrupt-frame representation.
4. CR3/address-space state, FPU/SIMD state, debug state, and interrupt state are not part of the current voluntary context ABI.
5. Any architecture-specific execution binding must stay behind an adapter boundary so the generic scheduler remains architecture-independent.
6. Preemption must preserve and restore the CPU-created interrupt frame separately from the voluntary call-return continuation.
7. SMP work must eliminate global mutable context state and replace it with CPU-local ownership.

## Current blockers

- The context-switch smoke test needs runtime/QEMU evidence before its assembly path can be treated as verified.
- The x86_64 execution binding is not yet owned by the generic task table, so task lifecycle and execution-lifetime coupling still need an explicit ownership boundary.
- APIC EOI configuration still uses bootstrap-global state; it must become CPU-local before SMP.
- The timer handler currently counts interrupts but does not yet perform scheduler preemption.

## Next implementation gate

1. Obtain CI/QEMU evidence for the context-switch smoke path.
2. Define how a task owns its architecture-specific `ExecutionBinding` without making the generic task type architecture-dependent.
3. Define the interrupt/preemption frame contract separately from voluntary `Context`.
4. Route the timer interrupt through a CPU-local scheduler state without switching tasks yet.
5. Only after those contracts are validated, implement timer-driven preemption.
6. Only after preemption is proven should SMP bring-up begin.
