//! Minimal ACPI MADT discovery owned by the x86_64 platform layer.
//!
//! The bootloader supplies the physical RSDP address. This module validates
//! the RSDP, locates the XSDT, finds the MADT and extracts the interrupt
//! topology required by the APIC layer. All parsed values are copied out of
//! firmware tables so no borrowed firmware references escape this module.

use crate::memory::PhysicalMemoryMapping;

const RSDP_SIGNATURE: [u8; 8] = *b"RSD PTR ";
const XSDT_SIGNATURE: [u8; 4] = *b"XSDT";
const MADT_SIGNATURE: [u8; 4] = *b"APIC";
const MADT_TYPE_LOCAL_APIC: u8 = 0;
const MADT_TYPE_IO_APIC: u8 = 1;
const MADT_TYPE_INTERRUPT_SOURCE_OVERRIDE: u8 = 2;
const MADT_TYPE_LOCAL_X2APIC: u8 = 9;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalApicInfo {
    pub processor_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

impl LocalApicInfo {
    pub const fn enabled(self) -> bool {
        self.flags & 1 != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalX2ApicInfo {
    pub uid: u32,
    pub x2apic_id: u32,
    pub flags: u32,
}

impl LocalX2ApicInfo {
    pub const fn enabled(self) -> bool {
        self.flags & 1 != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IoApicInfo {
    pub id: u8,
    pub address: u32,
    pub global_system_interrupt_base: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptSourceOverride {
    pub bus: u8,
    pub source: u8,
    pub global_system_interrupt: u32,
    pub flags: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpiError {
    MissingRsdp,
    InvalidRsdpSignature,
    InvalidRsdpChecksum,
    InvalidExtendedChecksum,
    InvalidTableAddress,
    InvalidTableHeader,
    InvalidTableChecksum,
    XsdtNotFound,
    MadtNotFound,
    TruncatedMadtEntry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApicTopology {
    pub local_apic_address: u64,
    pub local_apics: [Option<LocalApicInfo>; 256],
    pub local_x2apics: [Option<LocalX2ApicInfo>; 256],
    pub io_apics: [Option<IoApicInfo>; 32],
    pub source_overrides: [Option<InterruptSourceOverride>; 24],
    pub local_apic_count: usize,
    pub local_x2apic_count: usize,
    pub io_apic_count: usize,
    pub source_override_count: usize,
}

impl ApicTopology {
    const fn empty() -> Self {
        Self {
            local_apic_address: 0,
            local_apics: [None; 256],
            local_x2apics: [None; 256],
            io_apics: [None; 32],
            source_overrides: [None; 24],
            local_apic_count: 0,
            local_x2apic_count: 0,
            io_apic_count: 0,
            source_override_count: 0,
        }
    }

    #[cfg(test)]
    pub(crate) const fn empty_for_tests() -> Self {
        Self::empty()
    }
}

/// Parses ACPI APIC topology through the bootloader's physical-memory map.
///
/// # Safety
///
/// `rsdp_physical_address` must be the physical RSDP address supplied by the
/// bootloader and `mapping` must reference a valid complete physical-memory
/// mapping. The parser copies all data before returning.
pub unsafe fn discover(
    rsdp_physical_address: u64,
    mapping: PhysicalMemoryMapping,
) -> Result<ApicTopology, AcpiError> {
    if rsdp_physical_address == 0 {
        return Err(AcpiError::MissingRsdp);
    }

    let rsdp = unsafe { read_bytes(mapping, rsdp_physical_address, 36)? };
    if rsdp[0..8] != RSDP_SIGNATURE {
        return Err(AcpiError::InvalidRsdpSignature);
    }
    if checksum(&rsdp[..20]) != 0 {
        return Err(AcpiError::InvalidRsdpChecksum);
    }

    let revision = rsdp[15];
    let xsdt_address = if revision == 0 {
        return Err(AcpiError::XsdtNotFound);
    } else {
        u64::from_le_bytes(rsdp[24..32].try_into().unwrap())
    };

    if checksum(&rsdp) != 0 {
        return Err(AcpiError::InvalidExtendedChecksum);
    }

    let xsdt_header = unsafe { read_bytes(mapping, xsdt_address, 36)? };
    if xsdt_header[0..4] != XSDT_SIGNATURE {
        return Err(AcpiError::InvalidTableHeader);
    }
    let xsdt_length = u32::from_le_bytes(xsdt_header[4..8].try_into().unwrap()) as usize;
    if xsdt_length < 36 {
        return Err(AcpiError::InvalidTableHeader);
    }
    let xsdt = unsafe { read_bytes(mapping, xsdt_address, xsdt_length)? };
    if checksum(&xsdt) != 0 {
        return Err(AcpiError::InvalidTableChecksum);
    }

    let entry_count = (xsdt_length - 36) / 8;
    for index in 0..entry_count {
        let offset = 36 + index * 8;
        let table_address = u64::from_le_bytes(xsdt[offset..offset + 8].try_into().unwrap());
        if table_address == 0 {
            continue;
        }

        let header = unsafe { read_bytes(mapping, table_address, 36)? };
        if header[0..4] != MADT_SIGNATURE {
            continue;
        }
        let madt_length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        if madt_length < 44 {
            return Err(AcpiError::InvalidTableHeader);
        }
        let madt = unsafe { read_bytes(mapping, table_address, madt_length)? };
        if checksum(&madt) != 0 {
            return Err(AcpiError::InvalidTableChecksum);
        }
        return parse_madt(&madt);
    }

    Err(AcpiError::MadtNotFound)
}

fn parse_madt(table: &[u8]) -> Result<ApicTopology, AcpiError> {
    let mut topology = ApicTopology::empty();
    topology.local_apic_address = u32::from_le_bytes(table[36..40].try_into().unwrap()) as u64;
    let mut offset = 44;

    while offset < table.len() {
        if table.len() - offset < 2 {
            return Err(AcpiError::TruncatedMadtEntry);
        }
        let entry_type = table[offset];
        let entry_length = table[offset + 1] as usize;
        if entry_length < 2 || offset + entry_length > table.len() {
            return Err(AcpiError::TruncatedMadtEntry);
        }

        match entry_type {
            MADT_TYPE_LOCAL_APIC if entry_length >= 8 => {
                let entry = &table[offset..offset + entry_length];
                if topology.local_apic_count < topology.local_apics.len() {
                    topology.local_apics[topology.local_apic_count] = Some(LocalApicInfo {
                        processor_id: entry[2],
                        apic_id: entry[3],
                        flags: u32::from_le_bytes(entry[4..8].try_into().unwrap()),
                    });
                    topology.local_apic_count += 1;
                }
            }
            MADT_TYPE_IO_APIC if entry_length >= 12 => {
                let entry = &table[offset..offset + entry_length];
                if topology.io_apic_count < topology.io_apics.len() {
                    topology.io_apics[topology.io_apic_count] = Some(IoApicInfo {
                        id: entry[2],
                        address: u32::from_le_bytes(entry[4..8].try_into().unwrap()),
                        global_system_interrupt_base:
                            u32::from_le_bytes(entry[8..12].try_into().unwrap()),
                    });
                    topology.io_apic_count += 1;
                }
            }
            MADT_TYPE_INTERRUPT_SOURCE_OVERRIDE if entry_length >= 10 => {
                let entry = &table[offset..offset + entry_length];
                if topology.source_override_count < topology.source_overrides.len() {
                    topology.source_overrides[topology.source_override_count] =
                        Some(InterruptSourceOverride {
                            bus: entry[2],
                            source: entry[3],
                            global_system_interrupt:
                                u32::from_le_bytes(entry[4..8].try_into().unwrap()),
                            flags: u16::from_le_bytes(entry[8..10].try_into().unwrap()),
                        });
                    topology.source_override_count += 1;
                }
            }
            MADT_TYPE_LOCAL_X2APIC if entry_length >= 16 => {
                let entry = &table[offset..offset + entry_length];
                if topology.local_x2apic_count < topology.local_x2apics.len() {
                    topology.local_x2apics[topology.local_x2apic_count] = Some(LocalX2ApicInfo {
                        x2apic_id: u32::from_le_bytes(entry[4..8].try_into().unwrap()),
                        flags: u32::from_le_bytes(entry[8..12].try_into().unwrap()),
                        uid: u32::from_le_bytes(entry[12..16].try_into().unwrap()),
                    });
                    topology.local_x2apic_count += 1;
                }
            }
            _ => {}
        }

        offset += entry_length;
    }

    Ok(topology)
}

unsafe fn read_bytes(
    mapping: PhysicalMemoryMapping,
    physical_address: u64,
    length: usize,
) -> Result<&'static [u8], AcpiError> {
    let virtual_address = mapping
        .translate(physical_address)
        .ok_or(AcpiError::InvalidTableAddress)?;
    let end = virtual_address
        .checked_add(length as u64)
        .ok_or(AcpiError::InvalidTableAddress)?;
    if end <= virtual_address {
        return Err(AcpiError::InvalidTableAddress);
    }

    let pointer = virtual_address as *const u8;
    // SAFETY: The caller guarantees that the bootloader's direct map covers the
    // supplied physical address. The caller also validates ACPI table lengths
    // before dereferencing the returned slice.
    Ok(unsafe { core::slice::from_raw_parts(pointer, length) })
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

#[cfg(test)]
mod tests {
    use super::{checksum, InterruptSourceOverride};

    #[test]
    fn checksum_wraps_at_u8() {
        assert_eq!(checksum(&[255, 1]), 0);
    }

    #[test]
    fn source_override_is_copyable() {
        let value = InterruptSourceOverride {
            bus: 0,
            source: 0,
            global_system_interrupt: 2,
            flags: 0,
        };
        assert_eq!(value.global_system_interrupt, 2);
    }
}
