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

    if !has_io_apic_for_gsi(topology, route.gsi) {
        return Err(IrqRoutingError::NoIoApicForGsi);
    }

    Ok(route)
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

fn has_io_apic_for_gsi(topology: &ApicTopology, gsi: u32) -> bool {
    topology.io_apics[..topology.io_apic_count]
        .iter()
        .flatten()
        .any(|ioapic| ioapic.global_system_interrupt_base <= gsi)
}

#[cfg(test)]
mod tests {
    use super::{IrqRoute, Polarity, TriggerMode};

    #[test]
    fn identity_route_preserves_isa_irq() {
        let route = IrqRoute::identity(1);
        assert_eq!(route.gsi, 1);
        assert_eq!(route.polarity, Polarity::ConformsToBus);
        assert_eq!(route.trigger, TriggerMode::ConformsToBus);
    }
}
