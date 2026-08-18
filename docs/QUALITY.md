# X11-OS Quality Gate

X11-OS treats quality as an engineering constraint, not a final cleanup pass.

## Required before merge

- `cargo fmt --check`
- `cargo check` for every affected target/package
- `cargo clippy` with warnings treated as errors for supported kernel targets where the toolchain permits it
- No unexplained compiler warnings
- New unsafe code has a local safety justification
- Public interfaces document ownership, lifetime, error behavior, and compatibility expectations
- New persistent formats specify versioning or an explicit statement that no persistent format is being introduced

## Runtime validation

Hardware-facing changes require execution in QEMU or equivalent emulator coverage. Device-specific code additionally requires real-hardware validation before being considered stable.

## Change-size discipline

Prefer small changes with one architectural purpose. Large refactors must be divided by subsystem or contract so regressions can be localized.

## Dependency discipline

Dependencies must have a clear reason to exist. Version upgrades are reviewed for API, MSRV/toolchain, generated-code, licensing, and runtime behavior changes. Transitive dependency growth is monitored in long-lived kernel code.

## Compatibility discipline

Never change a stable ABI, on-disk format, or IPC protocol merely to make an implementation easier. Introduce versioning or migration paths first.

## Security discipline

Never trust firmware data, device data, filesystem metadata, network packets, or userspace pointers. Validate at the boundary and convert into safer internal representations.

## Performance discipline

Performance changes require a measurable hypothesis, benchmark or trace evidence, and an explanation of the trade-off. Micro-optimizations without evidence do not belong in the kernel.

## Definition of done

A feature is done when it has a coherent design, clean implementation, deterministic tests, runtime validation where applicable, documentation, and an explicit compatibility story.
