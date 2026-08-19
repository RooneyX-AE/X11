# Execution Architecture Audit

Status: preemption design implemented; runtime verification gate remains open

## Verified contracts

- `x86_64-unknown-none` uses the System V ABI without a red zone.
- The generic scheduler owns task identity, lifecycle state, and queue policy.
- Task control blocks use stable `Box` allocation so slot-table growth cannot relocate a live task object.
- x86_64 owns register context, kernel-stack representation, and context-switch assembly.
- Voluntary context switching and interrupt/preemptive switching are distinct contracts.
- The scheduler exposes an architecture-independent `ExecutionBinding` contract.
- The x86_64 execution adapter owns its kernel stack, activation metadata, voluntary `Context`, and optional interrupted snapshot behind that contract.
- Interrupted CPU state is copied out of transient interrupt-stack memory before the IRQ return boundary consumes it.
- The interrupted-state packet has an explicit `#[repr(C)]` ABI and compile-time offset/size assertions because naked return assembly depends on its layout.
- `preemption_plan()` is non-destructive; interrupted state is consumed only after the scheduler commits the target task.
- Timer IRQ delivery is deferred only for timer bookkeeping; an actual timer preemption decision is made at the interrupt-return boundary.
- CPU-local runtime ownership exists for the single-CPU bootstrap path and is intentionally isolated from generic scheduler policy.

## Required invariants

1. A running task's kernel stack must remain at a stable address for its entire execution lifetime.
2. A task execution object must not be moved or dropped while its stack is active.
3. A voluntary `Context` must not be reused as an interrupt-frame representation.
4. CR3/address-space state, FPU/SIMD state, debug state, and interrupt state are not part of the current voluntary context ABI.
5. Any architecture-specific execution binding must stay behind an adapter boundary so the generic scheduler remains architecture-independent.
6. Preemption must preserve and restore the CPU-created interrupt frame separately from the voluntary call-return continuation.
7. `ReturnToContext` may target a task with a valid initialized cooperative context; kernel `iretq` return requires a complete validated interrupted snapshot.
8. A scheduler tick must never self-preempt the current task when no other runnable task exists.
9. SMP work must eliminate bootstrap-global context ownership and replace it with explicit per-CPU state and synchronization.

## Current blockers

- No GitHub Actions workflow run is currently associated with the latest `main` commit, so timer-driven A→B→A→B runtime evidence is still unverified.
- The repository currently has no `.github/workflows` path available through the repository contents API; the documented QEMU/OVMF gate therefore needs to be restored or reintroduced before CI evidence can exist.
- APIC EOI state is still a bootstrap-global implementation and must become per-CPU before SMP.
- User-mode preemption is intentionally not supported by the current kernel `iretq` path. It requires a separate privilege-transition frame, address-space/CR3 contract, syscall/interrupt policy, and security review.
- FPU/SIMD, debug-register, XSAVE, and other extended CPU state are not part of the current preemption packet and must be introduced explicitly before broader thread/process support.

## Current validation layers

1. Pure unit tests cover scheduler state transitions, wait/sleep queues, context layout, interrupt-frame interpretation, execution ownership, and preemption planning.
2. Integration code exercises the runtime, execution registry, timer service, and CPU-local ownership boundaries.
3. A QEMU/OVMF workflow is a required validation layer, but current GitHub state provides no workflow run evidence for the latest `main` commit.
4. Real-hardware validation is deferred until the x86_64 single-CPU contract is stable.

## Next implementation gate

1. Restore or add the QEMU/OVMF GitHub Actions workflow with deterministic timer-preemption markers.
2. Obtain a successful CI/QEMU run for the current single-CPU preemption path.
3. Fix any compiler, boot, interrupt-frame, stack, or scheduler errors revealed by the runner.
4. Freeze the validated single-CPU execution/preemption ABI in documentation and regression tests.
5. Convert APIC state and timer ownership to explicit per-CPU structures.
6. Add a CPU-local scheduler instance and synchronization rules for SMP bring-up.
7. Only after SMP and CPU-state ownership are explicit, begin process/address-space and userspace ABI work.
