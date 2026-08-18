#![no_std]

/// Kernel entrypoint placeholder.
///
/// Boot integration will be added only after the platform/boot contract is
/// explicitly chosen and verified.
#[no_mangle]
pub extern "C" fn x11_kernel_entry() -> ! {
    loop {
        core::hint::spin_loop();
    }
}
