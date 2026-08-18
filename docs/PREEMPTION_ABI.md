# X11-OS x86_64 Preemption ABI

## Scope

The first preemption implementation is limited to kernel-to-kernel timer interrupts on one CPU. It must not be used for privilege-changing interrupts or userspace until a separate user-return contract is defined.

## CPU-created interrupt frame

For a timer interrupt taken while executing at CPL0, the CPU creates a return frame containing `RIP`, `CS`, and `RFLAGS`. `RSP` and `SS` are not present because no privilege transition occurred.

For a later userspace-to-kernel transition, the CPU additionally saves the interrupted `RSP` and `SS`. That state is represented separately and must not be inferred from the kernel timer frame.

## Preemption state

The preemption path must preserve two different categories of state:

1. CPU-created interrupt return state, owned by the interrupt/trap layer.
2. Scheduler execution state, owned by the task execution binding.

The existing voluntary `Context` remains a call/return continuation ABI. It must not be reinterpreted as the CPU interrupt frame.

## First implementation constraints

- Single CPU only.
- Kernel tasks only.
- Timer vector only.
- No CR3 switch.
- No userspace return.
- No FPU/SIMD lazy state handling.
- No debug-register switching.
- Interrupts must remain correctly masked/disabled across the scheduling decision and EOI sequence.
- A task's kernel stack and execution binding must remain alive and at a stable address while selected.

## Required validation

Before enabling preemption in the normal boot path, QEMU must demonstrate:

1. Timer IRQ enters the handler.
2. Current task state is preserved.
3. Scheduler selects a different ready task.
4. The selected task resumes at its expected continuation.
5. The interrupted task can later resume without stack corruption.
6. APIC EOI is issued exactly once for the delivered timer interrupt.

Userspace and SMP are separate milestones and must not be inferred from this kernel-only proof.
