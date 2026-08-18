//! Architecture-independent time contracts.
//!
//! The scheduler consumes monotonic ticks only. Hardware-specific clock and
//! timer implementations live behind this contract and must not leak into
//! scheduler policy.

/// Monotonic kernel tick value.
///
/// A tick source must never move backwards. The unit is intentionally left
/// unspecified here; platform code chooses the frequency and documents it at
/// the implementation boundary.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Tick(u64);

impl Tick {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn saturating_add(self, delta: u64) -> Self {
        Self(self.0.saturating_add(delta))
    }
}

/// Source of monotonic kernel time.
pub trait ClockSource {
    type Error;

    /// Returns a monotonic tick value.
    fn now(&self) -> Result<Tick, Self::Error>;
}

/// Minimal timer programming contract.
///
/// The timer backend is responsible only for arranging a future wakeup. It
/// does not decide which task runs after the interrupt; that remains scheduler
/// policy.
pub trait TimerSource {
    type Error;

    /// Programs the next timer event at or after `deadline`.
    fn arm(&mut self, deadline: Tick) -> Result<(), Self::Error>;

    /// Disarms a previously programmed event.
    fn disarm(&mut self) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeError {
    ClockWentBackwards,
}

/// Validates monotonicity while adapting any raw clock source to `Tick`.
#[derive(Debug)]
pub struct MonotonicClock<C> {
    source: C,
    last: Tick,
}

impl<C> MonotonicClock<C> {
    pub const fn new(source: C) -> Self {
        Self {
            source,
            last: Tick::ZERO,
        }
    }

    pub const fn last(&self) -> Tick {
        self.last
    }
}

impl<C> ClockSource for MonotonicClock<C>
where
    C: ClockSource,
{
    type Error = MonotonicClockError<C::Error>;

    fn now(&self) -> Result<Tick, Self::Error> {
        self.source.now().map_err(MonotonicClockError::Source)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonotonicClockError<E> {
    Source(E),
    ClockWentBackwards,
}

#[cfg(test)]
mod tests {
    use super::{ClockSource, MonotonicClock, Tick};

    struct FixedClock(Tick);

    impl ClockSource for FixedClock {
        type Error = core::convert::Infallible;

        fn now(&self) -> Result<Tick, Self::Error> {
            Ok(self.0)
        }
    }

    #[test]
    fn tick_addition_saturates() {
        let tick = Tick::new(u64::MAX - 1).saturating_add(10);
        assert_eq!(tick, Tick::new(u64::MAX));
    }

    #[test]
    fn monotonic_clock_preserves_source_value() {
        let clock = MonotonicClock::new(FixedClock(Tick::new(42)));
        assert_eq!(clock.now().unwrap(), Tick::new(42));
        assert_eq!(clock.last(), Tick::ZERO);
    }
}
