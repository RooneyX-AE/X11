//! Architecture-independent timekeeping and timer contracts.
//!
//! A clocksource answers "what time is it?" while a timer device answers
//! "when should the kernel receive the next periodic event?". Keeping those
//! contracts separate lets TSC, HPET, and ACPI PM timer coexist with LAPIC or
//! other interrupt-capable timer devices without coupling the scheduler to a
//! specific hardware implementation.

/// Kernel time unit: nanoseconds since the selected clocksource was initialized.
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
        if nanos == 0 {
            None
        } else {
            Some(Self { nanos })
        }
    }

    pub const fn as_nanos(self) -> u64 {
        self.nanos
    }
}

/// Reads monotonic kernel time from a hardware-backed clocksource.
pub trait Clocksource {
    type Error;

    fn now(&self) -> Result<MonotonicTime, Self::Error>;
}

/// Programs periodic timer interrupts independently of the clocksource.
pub trait TimerDevice {
    type Error;

    fn set_periodic(&mut self, interval: TimerInterval) -> Result<(), Self::Error>;
    fn disable(&mut self) -> Result<(), Self::Error>;
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
