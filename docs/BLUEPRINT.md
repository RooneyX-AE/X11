# X11-OS development blueprint

## Milestone 0: Foundation

- Establish a reproducible Rust workspace.
- Define repository boundaries and platform assumptions.
- Add CI for formatting, linting, and compilation.
- Keep kernel entry code minimal until the boot contract is selected.

## Milestone 1: Boot and CPU

- Select UEFI/boot protocol and target architecture.
- Enter the kernel with a verified boot-to-kernel contract.
- Establish CPU initialization, exceptions, interrupts, and serial logging.

## Milestone 2: Memory

- Physical frame allocator.
- Page-table and virtual-memory management.
- Kernel heap with documented invariants.
- Allocation and mapping tests where practical.

## Milestone 3: Execution

- Timer source.
- Context switching.
- Threads and processes.
- Scheduler with deterministic tests.

## Milestone 4: ABI and userspace

- Stable syscall ABI.
- User/kernel boundary.
- ELF loading.
- `init` and a minimal userspace runtime.

## Milestone 5: Devices and storage

- PCI and essential device discovery.
- Input and timer drivers.
- Block device abstraction.
- VFS plus an initial filesystem.

## Milestone 6: Networking

- Ethernet abstraction.
- IPv4/IPv6 foundations.
- UDP/TCP and socket API.

## Milestone 7: Graphics

- Framebuffer/display abstraction.
- Window/compositor protocol.
- Input routing and rendering pipeline.

## Milestone 8: Desktop

- Window manager/compositor.
- Shell and basic applications.
- Stable graphical userspace APIs.

## Milestone 9: Hardening

- Capability and permission model.
- Fault isolation.
- Fuzzing and negative tests for parsers and ABI boundaries.
- Reproducible builds and release artifacts.

## Milestone 10: Optional AI services

AI belongs in userspace or isolated services. The kernel must remain functional when AI services are unavailable.

## Definition of done

A subsystem is not considered complete merely because it compiles. It needs documented invariants, targeted tests, integration coverage where applicable, and execution validation in an emulator or real hardware before being promoted to stable status.
