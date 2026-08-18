//! Architecture-independent time contracts.
//!
//! The scheduler consumes monotonic ticks only. Hardware-specific clock and
//! timer implementations live behind this contract and must not leak into
//! scheduler policy.

/// Monotonic kernel tick value.
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

    /// Samples the current monotonic tick.
    fn now(&mut self) -> Result<Tick, Self::Error>;
}

/// Minimal timer programming contract.
pub trait TimerSource {
    type Error;

    /// Programs the next timer event at or after `deadline`.
    fn arm(&mut self, deadline: Tick) -> Result<(), Self::Error>;

    /// Disarms a previously programmed event.
    fn disarm(&mut self) -> Result<(), Self::Error>;
}

/// Validates that a clock source never moves backwards.
#[derive(Debug)]
pub struct MonotonicClock<C> {
    source: C,
    last: Tick,
}

impl<C> MonotonicClock<C> {
    pub const fn new(source: C) -> Self {
        Self { source, last: Tick::ZERO }
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

    fn now(&mut self) -> Result<Tick, Self::Error> {
        let tick = self.source.now().map_err(MonotonicClockError::Source)?;
        if tick < self.last {
            return Err(MonotonicClockError::ClockWentBackwards);
        }
        self.last = tick;
        Ok(tick)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonotonicClockError<E> {
    Source(E),
    ClockWentBackwards,
}

#[cfg(test)]
mod tests {
    use super::{ClockSource, MonotonicClock, MonotonicClockError, Tick};

    struct SequenceClock {
        values: &'static [u64],
        index: usize,
    }

    impl ClockSource for SequenceClock {
        type Error = core::convert::Infallible;

        fn now(&mut self) -> Result<Tick, Self::Error> {
            let value = self.values[self.index];
            self.index += 1;
            Ok(Tick::new(value))
        }
    }

    #[test]
    fn tick_addition_saturates() {
        let tick = Tick::new(u64::MAX - 1).saturating_add(10);
        assert_eq!(tick, Tick::new(u64::MAX));
    }

    #[test]
    fn monotonic_clock_updates_last() {
        let mut clock = MonotonicClock::new(SequenceClock { values: &[42, 43], index: 0 });
        assert_eq!(clock.now().unwrap(), Tick::new(42));
        assert_eq!(clock.last(), Tick::new(42));
        assert_eq!(clock.now().unwrap(), Tick::new(43));
        assert_eq!(clock.last(), Tick::new(43));
    }

    #[test]
    fn backwards_clock_is_rejected() {
        let mut clock = MonotonicClock::new(SequenceClock { values: &[10, 9], index: 0 });
        assert_eq!(clock.now().unwrap(), Tick::new(10));
        assert_eq!(clock.now(), Err(MonotonicClockError::ClockWentBackwards));
        assert_eq!(clock.last(), Tick::new(10));
    }
}
