# X11-OS

A Rust-first operating system project focused on a verifiable kernel foundation, explicit subsystem boundaries, and a long-term graphical desktop architecture.

## Status

This repository is at the foundation stage. The current goal is to establish a small, testable kernel workspace before adding hardware-specific functionality.

## Development principles

- Verify interfaces against specifications and real execution, not compilation alone.
- Keep platform-specific code isolated behind explicit boundaries.
- Prefer small subsystems with clear invariants and tests.
- Do not add desktop/UI complexity before the kernel and userspace contracts are stable.

## Planned layers

`boot -> kernel -> syscall ABI -> userspace -> drivers -> filesystem -> networking -> graphics -> desktop -> optional AI services`

See [`docs/BLUEPRINT.md`](docs/BLUEPRINT.md) for the development roadmap.
