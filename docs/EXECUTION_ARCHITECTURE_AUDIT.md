# Execution Architecture Audit

Status: design gate before preemption/SMP

## Verified contracts

- `x86_64-unknown-none` uses System V ABI without a red zone.
- The generic scheduler owns task identity, lifecycle state, and queue policy.
- x86_64 owns register context, kernel-stack representation, and context-switch assembly.
- Voluntary context switching and interrupt/preemptive switching are distinct contracts.

## Required invariants

1. A running task's kernel stack must remain at a stable address for its entire execution lifetime.
2. A task execution object must not be moved or dropped while its stack is active.
3. A voluntary `Context` must not be reused as an interrupt-frame representation.
4. CR3/address-space state, FPU/SIMD state, debug state, and interrupt state are not part of the current voluntary context ABI.
5. Any architecture-specific execution binding must stay behind an adapter boundary so the generic scheduler remains architecture-independent.
6. Preemption must preserve and restore the CPU-created interrupt frame separately from the voluntary call-return continuation.
7. SMP work must eliminate global mutable context state and replace it with CPU-local ownership.

## Current blockers

- The bootstrap context-switch smoke test currently uses static `UnsafeCell` state. This is acceptable only for a bounded single-CPU diagnostic but is not a valid long-term SMP pattern and must be removed before SMP.
- `TaskExecutionState` currently owns its kernel stack through `Vec<u8>`. When execution state becomes part of a running task, the containing allocation must provide stable storage and must not move during execution.
- A future task table must therefore use stable allocation semantics rather than a movable `Vec<T>` containing self-referential stack/context bindings.
- The scheduler must expose a generic execution-binding interface rather than importing x86_64 context types.

## Next implementation gate

1. Replace global smoke-test state with stack-local state.
2. Introduce a stable task storage/arena boundary.
3. Add an architecture-independent execution binding trait.
4. Bind x86_64 `Context + kernel stack` to that trait.
5. Only then implement timer-driven preemption.
6. Only after preemption is proven should SMP/CPU-local scheduler state begin.
