//! Architecture-independent interrupt event contracts.
//!
//! Interrupt delivery is separated from policy so future APIC, timer, device,
//! and scheduler implementations can evolve without changing the event model.

pub const EXCEPTION_VECTOR_MAX: u8 = 31;
pub const EXTERNAL_VECTOR_BASE: u8 = 32;
pub const VECTOR_MAX: u8 = 255;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptSource {
    Exception(u8),
    Timer,
    External(u8),
    InterProcessor(u8),
    Spurious,
}

impl InterruptSource {
    pub const fn vector(self) -> u8 {
        match self {
            Self::Exception(vector) | Self::External(vector) | Self::InterProcessor(vector) => vector,
            Self::Timer => EXTERNAL_VECTOR_BASE,
            Self::Spurious => VECTOR_MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptEvent {
    source: InterruptSource,
    vector: u8,
}

impl InterruptEvent {
    pub const fn new(source: InterruptSource) -> Option<Self> {
        let vector = source.vector();
        match source {
            InterruptSource::Exception(_) if vector > EXCEPTION_VECTOR_MAX => None,
            InterruptSource::External(_) | InterruptSource::InterProcessor(_) if vector < EXTERNAL_VECTOR_BASE => None,
            _ => Some(Self { source, vector }),
        }
    }

    pub const fn source(self) -> InterruptSource {
        self.source
    }

    pub const fn vector(self) -> u8 {
        self.vector
    }
}

#[cfg(test)]
mod tests {
    use super::{InterruptEvent, InterruptSource, EXTERNAL_VECTOR_BASE, VECTOR_MAX};

    #[test]
    fn rejects_external_vector_below_exception_range() {
        assert!(InterruptEvent::new(InterruptSource::External(31)).is_none());
    }

    #[test]
    fn accepts_external_vector_at_boundary() {
        let event = InterruptEvent::new(InterruptSource::External(EXTERNAL_VECTOR_BASE)).unwrap();
        assert_eq!(event.vector(), EXTERNAL_VECTOR_BASE);
    }

    #[test]
    fn spurious_vector_is_maximum() {
        let event = InterruptEvent::new(InterruptSource::Spurious).unwrap();
        assert_eq!(event.vector(), VECTOR_MAX);
    }
}
