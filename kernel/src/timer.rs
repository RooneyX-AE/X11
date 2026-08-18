//! Architecture-independent timekeeping and timer contracts.
//!
//! A clocksource answers "what time is it?" while a timer device answers
//! "when should the kernel receive the next event?". Keeping those contracts
//! separate lets TSC, HPET, ACPI PM timer, and LAPIC timer implementations
//! coexist without coupling scheduler policy to hardware.

/// Kernel time unit: nanoseconds since the selected clocksource was initialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MonotonicTime(u64);

impl MonotonicTime {
    pub const ZERO: Self = Self(0);

    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    pub const fn saturating_add(self, delta_nanos: u64) -> Self {
        Self(self.0.saturating_add(delta_nanos))
    }
}

/// Requested periodic timer interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerInterval(u64);

impl TimerInterval {
    pub const fn new(nanos: u64) -> Option<Self> {
        if nanos == 0 {
            None
        } else {
            Some(Self(nanos))
        }
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }
}

/// Absolute one-shot timer deadline in kernel monotonic time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TimerDeadline(MonotonicTime);

impl TimerDeadline {
    pub const fn new(time: MonotonicTime) -> Self {
        Self(time)
    }

    pub const fn time(self) -> MonotonicTime {
        self.0
    }
}

/// Reads monotonic kernel time from a hardware-backed clocksource.
pub trait Clocksource {
    type Error;

    fn now(&self) -> Result<MonotonicTime, Self::Error>;
}

/// Programs timer interrupts independently of scheduler policy.
pub trait TimerDevice {
    type Error;

    fn set_periodic(&mut self, interval: TimerInterval) -> Result<(), Self::Error>;

    fn set_deadline(&mut self, deadline: TimerDeadline) -> Result<(), Self::Error>;

    fn disable(&mut self) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{MonotonicTime, TimerDeadline, TimerInterval};

    #[test]
    fn monotonic_time_preserves_nanoseconds() {
        let time = MonotonicTime::from_nanos(123_456);
        assert_eq!(time.as_nanos(), 123_456);
    }

    #[test]
    fn time_addition_saturates() {
        let time = MonotonicTime::from_nanos(u64::MAX - 1).saturating_add(10);
        assert_eq!(time, MonotonicTime::from_nanos(u64::MAX));
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

    #[test]
    fn deadline_preserves_absolute_time() {
        let time = MonotonicTime::from_nanos(9_000);
        assert_eq!(TimerDeadline::new(time).time(), time);
    }
}
