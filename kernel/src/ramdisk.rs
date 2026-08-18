//! Safe boundary for the bootloader-provided ramdisk mapping.

use bootloader_api::{info::Optional, BootInfo};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RamdiskError {
    Missing,
    LengthOverflow,
}

pub fn bytes(boot_info: &'static BootInfo) -> Result<&'static [u8], RamdiskError> {
    let address = match boot_info.ramdisk_addr {
        Optional::Some(value) => value,
        Optional::None => return Err(RamdiskError::Missing),
    };

    let length = usize::try_from(boot_info.ramdisk_len).map_err(|_| RamdiskError::LengthOverflow)?;
    let start = address as *const u8;

    // SAFETY: bootloader_api maps the ramdisk into the kernel address space and
    // reports the virtual base and byte length through BootInfo. The caller
    // keeps BootInfo and its bootloader mappings alive for the kernel lifetime.
    Ok(unsafe { core::slice::from_raw_parts(start, length) })
}

#[cfg(test)]
mod tests {
    use super::RamdiskError;

    #[test]
    fn error_contract_is_distinct() {
        assert_ne!(RamdiskError::Missing, RamdiskError::LengthOverflow);
    }
}
