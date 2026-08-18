# X11-OS Architecture Contract

## Goals

X11-OS is designed as a long-lived Rust operating system. The architecture favors explicit boundaries, replaceable implementations, observable behavior, and compatibility over short-term convenience.

## Layering

```text
Firmware / UEFI
        |
     bootloader
        |
   kernel entry
        |
+---------------------------+
| Architecture layer        |
| CPU / GDT / IDT / APIC    |
+---------------------------+
| Kernel mechanisms         |
| memory / scheduler / IPC  |
+---------------------------+
| Kernel services           |
| process / VFS / drivers   |
+---------------------------+
| Syscall + userspace ABI   |
+---------------------------+
| Userspace services        |
| init / network / graphics |
+---------------------------+
| Applications / AI         |
+---------------------------+
```

## Dependency direction

Lower layers must not depend on higher-level policy. Architecture code may expose primitives to the kernel, but architecture modules must not know about filesystems, desktop policy, or AI services.

The kernel may depend on architecture-specific interfaces through an explicit architecture boundary. Architecture-independent code should not import x86_64 implementation details directly.

Memory policy and contracts live in `kernel/src/memory/`. Concrete hardware paging implementations live behind `kernel/src/arch/<architecture>/` and consume only the public memory contracts. The generic memory layer must remain portable enough that a future architecture can provide its own backend without moving page-allocation policy into the architecture module.

## API stability classes

### Internal

Private implementation details may change freely within a subsystem.

### Kernel contract

Interfaces shared by kernel subsystems require documentation, invariants, and focused tests. Changes require checking all consumers.

### External compatibility boundary

Boot contracts, system-call ABI, IPC/protocol formats, filesystem formats, and public userspace SDKs require explicit versioning and compatibility decisions.

## Unsafe-code policy

Unsafe Rust is permitted where the hardware or ABI requires it. Each unsafe block should have a local safety comment explaining the invariant being upheld. Unsafe code should remain close to the abstraction that owns the invariant.

Examples include port I/O, descriptor-table loading, page-table manipulation, context switching, and user-pointer access.

## Ownership and lifetime rules

Kernel APIs should make ownership visible in types where practical. Borrowed boot metadata must be copied or transformed before ownership is handed to longer-lived subsystems. Global mutable state requires explicit synchronization and a documented initialization order.

## Error policy

Subsystems must distinguish programmer invariants, hardware faults, invalid external input, and resource exhaustion. Silent fallback is prohibited at security or ABI boundaries.

## Observability

The kernel must have deterministic early-boot diagnostics before complex services exist. Serial logging is the initial backend. Future logging should preserve the same sink-independent event model so diagnostics can move to tracing buffers, framebuffer output, or remote transports without rewriting callers.

## Testing model

Every subsystem should have four validation layers where applicable:

1. Pure unit tests for policy and data structures.
2. Integration tests for subsystem contracts.
3. Emulator tests for hardware-facing behavior.
4. Real-hardware validation for device-specific assumptions.

Tests should assert invariants, not merely line coverage.

## Evolution policy

Do not freeze an abstraction just because it exists. Freeze it when an external consumer depends on its semantics. Prefer narrow interfaces that can support multiple implementations.

The objective is not to predict thirty years of requirements. The objective is to make change cheap without making old behavior ambiguous.
