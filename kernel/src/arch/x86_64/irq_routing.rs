//! ACPI-aware interrupt routing policy.
//!
//! This layer turns an ISA IRQ into a GSI and preserves the MADT polarity and
//! trigger-mode override. It does not perform MMIO writes itself.

use super::acpi::{ApicTopology, InterruptSourceOverride};

/// Electrical polarity required by the I/O APIC redirection entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Polarity {
    ConformsToBus,
    ActiveHigh,
    ActiveLow,
}

/// Trigger mode required by the I/O APIC redirection entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriggerMode {
    ConformsToBus,
    Edge,
    Level,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrqRoute {
    pub isa_irq: u8,
    pub gsi: u32,
    pub polarity: Polarity,
    pub trigger: TriggerMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrqRoutingError {
    UnsupportedBus,
    InvalidOverrideFlags,
    NoIoApicForGsi,
}

impl IrqRoute {
    pub const fn identity(isa_irq: u8) -> Self {
        Self {
            isa_irq,
            gsi: isa_irq as u32,
            polarity: Polarity::ConformsToBus,
            trigger: TriggerMode::ConformsToBus,
        }
    }
}

/// Resolves an ISA IRQ using the platform's MADT source overrides.
pub fn resolve_isa_irq(
    topology: &ApicTopology,
    isa_irq: u8,
) -> Result<IrqRoute, IrqRoutingError> {
    let mut route = IrqRoute::identity(isa_irq);

    for entry in topology.source_overrides[..topology.source_override_count]
        .iter()
        .flatten()
    {
        if entry.bus != 0 || entry.source != isa_irq {
            continue;
        }

        route.gsi = entry.global_system_interrupt;
        let (polarity, trigger) = decode_flags(entry)?;
        route.polarity = polarity;
        route.trigger = trigger;
        break;
    }

    if io_apic_index_for_gsi(topology, route.gsi).is_none() {
        return Err(IrqRoutingError::NoIoApicForGsi);
    }

    Ok(route)
}

/// Returns the I/O APIC whose GSI base is the greatest base not exceeding the GSI.
///
/// The final hardware-range check is intentionally deferred to `IoApic`, because
/// the implemented redirection count is reported by that physical device's
/// `IOAPICVER` register rather than by MADT.
pub fn io_apic_index_for_gsi(topology: &ApicTopology, gsi: u32) -> Option<usize> {
    topology.io_apics[..topology.io_apic_count]
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| entry.map(|ioapic| (index, ioapic)))
        .filter(|(_, ioapic)| ioapic.global_system_interrupt_base <= gsi)
        .max_by_key(|(_, ioapic)| ioapic.global_system_interrupt_base)
        .map(|(index, _)| index)
}

fn decode_flags(entry: &InterruptSourceOverride) -> Result<(Polarity, TriggerMode), IrqRoutingError> {
    let polarity = match entry.flags & 0b11 {
        0b00 => Polarity::ConformsToBus,
        0b01 => Polarity::ActiveHigh,
        0b11 => Polarity::ActiveLow,
        _ => return Err(IrqRoutingError::InvalidOverrideFlags),
    };
    let trigger = match (entry.flags >> 2) & 0b11 {
        0b00 => TriggerMode::ConformsToBus,
        0b01 => TriggerMode::Edge,
        0b11 => TriggerMode::Level,
        _ => return Err(IrqRoutingError::InvalidOverrideFlags),
    };
    Ok((polarity, trigger))
}

#[cfg(test)]
mod tests {
    use super::{io_apic_index_for_gsi, IrqRoute, Polarity, TriggerMode};
    use crate::arch::x86_64::acpi::{ApicTopology, IoApicInfo};

    #[test]
    fn identity_route_preserves_isa_irq() {
        let route = IrqRoute::identity(1);
        assert_eq!(route.gsi, 1);
        assert_eq!(route.polarity, Polarity::ConformsToBus);
        assert_eq!(route.trigger, TriggerMode::ConformsToBus);
    }

    #[test]
    fn selects_nearest_lower_ioapic_gsi_base() {
        let mut topology = ApicTopology::empty_for_tests();
        topology.io_apics[0] = Some(IoApicInfo {
            id: 1,
            address: 0xfec0_0000,
            global_system_interrupt_base: 0,
        });
        topology.io_apics[1] = Some(IoApicInfo {
            id: 2,
            address: 0xfec0_1000,
            global_system_interrupt_base: 24,
        });
        topology.io_apic_count = 2;

        assert_eq!(io_apic_index_for_gsi(&topology, 30), Some(1));
        assert_eq!(io_apic_index_for_gsi(&topology, 10), Some(0));
    }
}
