//! Architecture-independent timekeeping and timer contracts.
//!
//! Hardware timer implementations must report monotonic time and deliver
//! periodic scheduling events through this narrow interface. The scheduler
//! must not depend on APIC, HPET, or another hardware timer directly.

/// Kernel time unit: nanoseconds since the kernel timebase was initialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MonotonicTime(u64);

impl MonotonicTime {
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }
}

/// Requested periodic timer interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerInterval {
    nanos: u64,
}

impl TimerInterval {
    pub const fn new(nanos: u64) -> Option<Self> {
        if nanos == 0 { None } else { Some(Self { nanos }) }
    }

    pub const fn as_nanos(self) -> u64 {
        self.nanos
    }
}

/// Minimal interface expected from a kernel timer backend.
pub trait Timer {
    type Error;

    fn now(&self) -> MonotonicTime;
    fn set_periodic(&mut self, interval: TimerInterval) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{MonotonicTime, TimerInterval};

    #[test]
    fn monotonic_time_preserves_nanoseconds() {
        let time = MonotonicTime::from_nanos(123_456);
        assert_eq!(time.as_nanos(), 123_456);
    }

    #[test]
    fn zero_interval_is_rejected() {
        assert!(TimerInterval::new(0).is_none());
    }

    #[test]
    fn interval_round_trips() {
        let interval = TimerInterval::new(1_000_000).unwrap();
        assert_eq!(interval.as_nanos(), 1_000_000);
    }
}
