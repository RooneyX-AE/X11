use std::{env, fs, path::PathBuf};

fn main() {
    let kernel = env::var_os("CARGO_BIN_FILE_X11_OS_KERNEL_x11-os-kernel")
        .expect("kernel binary artifact was not produced");
    let kernel = PathBuf::from(kernel);

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is missing"));
    let image = out_dir.join("x11-os-uefi.img");

    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&image)
        .expect("failed to create UEFI boot image");

    println!("cargo:rustc-env=X11_OS_UEFI_IMAGE={}", image.display());
}
