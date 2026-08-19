# X11-OS Syscall ABI

## Register contract

On entry from userspace:

- `RAX`: syscall number
- `RDI`: argument 0
- `RSI`: argument 1
- `RDX`: argument 2

On return to userspace:

- `RAX >= 0`: syscall succeeded and contains the unsigned result value.
- `RAX < 0`: syscall failed and contains the negated error code represented as a signed `i64`.

The kernel must always overwrite `RAX` before `iretq`. A failed syscall must never leak the interrupted userspace `RAX` value as an accidental return value.

## Current syscall numbers

| Number | Syscall |
|---:|---|
| 0 | `Write` |
| 1 | `Exit` |
| 2 | `Yield` |

These numbers are defined by `x11-os-abi` and are the single source of truth.

## `Write`

`Write` uses:

- `RDI`: userspace buffer pointer
- `RSI`: buffer length in bytes
- `RDX`: reserved, must be zero

Return value on success is the number of bytes accepted by the sink.

The kernel validates the complete userspace range before copying. An invalid range or inaccessible page is an error, not undefined behavior.

## Error namespace

Error `0` is never valid as an error result. Kernel syscall errors will be assigned positive numeric codes and encoded in `RAX` as their negated signed value.

The first implementation intentionally keeps the error table architecture-independent so userspace and kernel can share it without importing architecture-specific code.
