# X11-OS x86_64 Context-Switch ABI

This document defines the first kernel-thread context-switch contract.

## Saved state

`arch::x86_64::context_switch::Context` is `#[repr(C)]` and occupies exactly 64 bytes:

| Offset | State |
|---:|---|
| 0x00 | `rsp` |
| 0x08 | `rbp` |
| 0x10 | `rbx` |
| 0x18 | `r12` |
| 0x20 | `r13` |
| 0x28 | `r14` |
| 0x30 | `r15` |
| 0x38 | continuation `rip` |

The context switch saves the SysV64 callee-saved general-purpose registers and the continuation address. It deliberately does not save CR3, segment state, FPU/SIMD state, debug registers, APIC state, or interrupt-enable state.

Those resources belong to higher-level scheduler and CPU-state contracts and must be made explicit before preemptive or userspace switching is enabled.

## Stack invariant

The kernel uses the Rust `x86_64-unknown-none` target. Its C ABI follows SysV64 without a red zone. A newly bootstrapped kernel thread enters its first function with `RSP % 16 == 8`, matching the normal SysV64 function-entry condition.

Kernel stacks are owned by the task that created them and must remain writable and live for the entire lifetime of the task. The initial context has no caller continuation. The switch trampoline pushes the configured entry `rip` and returns into it.

## Interrupt boundary

This context switch is initially intended for voluntary kernel-thread switching while the normal call/return stack is active. A timer interrupt arrives with a CPU-created interrupt frame, so preemptive switching must introduce a separate trap-frame/context model rather than pretending the interrupt frame is a normal SysV call frame.

## Evolution rule

Any change to `Context` requires updating:

1. The offset table above.
2. The naked assembly offsets.
3. Compile-time size/alignment assertions.
4. QEMU context-switch integration tests.

Userspace context, address-space switching, and FPU/SIMD state must be added as explicit versioned capabilities rather than silently extending this initial kernel-thread ABI.
