//! Architecture-specific kernel primitives.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(not(target_arch = "x86_64"))]
compile_error!("X11-OS currently supports only x86_64");
