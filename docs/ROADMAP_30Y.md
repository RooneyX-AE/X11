# X11-OS 30-Year Roadmap

X11-OS is a long-lived operating-system project. The roadmap is capability-oriented rather than date-oriented: milestones are completed when their contracts, invariants, tests, and runtime validation are mature.

## Architectural rules

1. Kernel mechanisms must be separated from policy.
2. Public interfaces require explicit ownership, lifetime, error, and compatibility rules.
3. Hardware-specific code stays behind narrow platform traits or modules.
4. Unsafe code is isolated, documented, and reviewed against the hardware contract it implements.
5. Every persistent on-disk or user/kernel format receives a versioning strategy before adoption.
6. Features are introduced behind replaceable boundaries when practical.
7. The kernel must remain usable without networking, graphics, storage extras, or AI services.
8. A green compiler result is never sufficient evidence of correctness.
9. Performance work follows measurement, not intuition.
10. Removing an abstraction requires proving that its compatibility cost is understood.

## Phase A: Bootstrap and CPU foundation

- Reproducible Rust toolchain and workspace.
- UEFI boot image generation.
- BootInfo ownership boundary.
- Serial diagnostics and panic path.
- GDT, IDT, exception handlers, and CPU feature discovery.
- Clear separation between architecture-independent kernel code and x86_64 code.

Exit criteria: boot reaches the kernel reliably in QEMU, diagnostics are deterministic, CPU faults are surfaced through the exception path, and CI verifies formatting, linting, compilation, and boot-image creation.

## Phase B: Memory and address spaces

- Physical frame allocator with explicit memory-region policy.
- Page-table abstraction.
- Kernel virtual-address layout specification.
- Mapping/unmapping APIs with ownership rules.
- Kernel heap and allocation-failure policy.
- Guard pages and debug checks where affordable.

Exit criteria: allocator invariants are tested independently of the scheduler and the virtual-memory layout is documented as a compatibility contract.

## Phase C: Execution and interrupts

- Local APIC and interrupt routing.
- Monotonic timer source.
- CPU-local state.
- Context switching.
- Threads and process primitives.
- Scheduler policy behind a stable scheduling interface.
- SMP bring-up and shutdown semantics.

Exit criteria: deterministic scheduler tests plus multi-core QEMU validation.

## Phase D: Stable ABI and userspace

- System-call number registry and ABI version policy.
- User/kernel pointer validation.
- Process address-space creation.
- ELF loader.
- Initial userspace process.
- Signals or equivalent asynchronous notification mechanism.
- IPC primitives.

Exit criteria: a documented ABI can support independent userspace development and has compatibility tests.

## Phase E: Hardware and storage

- PCI/ACPI discovery.
- Interrupt controllers and DMA abstractions.
- Block-device interface.
- VFS.
- Initial filesystem.
- Driver model with lifecycle and ownership rules.

Exit criteria: drivers can be added or replaced without rewriting process or filesystem policy.

## Phase F: Networking

- Link-layer abstraction.
- IPv4 and IPv6.
- UDP and TCP.
- Socket ABI.
- Network configuration service.
- Packet parser fuzzing.

Exit criteria: protocol parsing is fuzz-tested and userspace networking remains independent from kernel policy where feasible.

## Phase G: Graphics and desktop

- Display and framebuffer abstraction.
- GPU capability discovery.
- Input device model.
- Compositor protocol.
- Window-management policy in userspace.
- Accessibility and input routing contracts.

Exit criteria: graphical components communicate through documented protocols and can evolve independently.

## Phase H: Security and reliability

- Capability/permission model.
- Privilege separation.
- Sandboxing.
- Fault isolation.
- Secure boot integration where practical.
- Fuzzing of parsers and ABI surfaces.
- Crash diagnostics and reproducible bug reports.

Exit criteria: security boundaries are explicit, tested, and documented.

## Phase I: Tooling and ecosystem

- Native build and package tooling.
- Debugger integration.
- Trace and profiling infrastructure.
- Stable developer SDKs.
- Compatibility libraries where useful.
- Release engineering and reproducible artifacts.

## Phase J: Optional AI services

AI is an isolated userspace/service layer. It can inspect and automate the OS through explicit privileged interfaces, but kernel correctness must never depend on model availability.

## Long-term compatibility policy

Compatibility is preserved at deliberate boundaries: boot contract, syscall ABI, IPC/protocol formats, filesystem metadata, and userspace SDKs. Internal implementations may evolve freely when those boundaries remain valid.

A milestone is not complete because a demo works. It is complete when the design can survive the next subsystem without requiring a rewrite of an earlier contract.
