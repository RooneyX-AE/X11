//! x86_64 TSC clocksource.
//!
//! The TSC is used only when the CPU exposes an invariant counter and a
//! trustworthy frequency can be derived. CPUID.15H is preferred because it
//! provides the TSC/crystal ratio; CPUID.16H nominal frequency is a weaker
//! fallback and is never used to claim invariant behavior by itself.

use core::arch::x86_64::{__cpuid, __cpuid_count, _rdtsc};

use crate::timer::{Clocksource, MonotonicTime};

const INVARIANT_TSC_BIT: u32 = 1 << 8;
const CPUID_EXTENDED_MAX: u32 = 0x8000_0000;
const CPUID_TSC_RATIO: u32 = 0x15;
const CPUID_PROCESSOR_FREQ: u32 = 0x16;
const CPUID_EXTENDED_FEATURES: u32 = 0x8000_0007;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TscError {
    Unsupported,
    FrequencyUnavailable,
    FrequencyOverflow,
    TimeOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TscFrequency {
    hz: u64,
}

impl TscFrequency {
    pub const fn hz(self) -> u64 {
        self.hz
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TscClocksource {
    start_ticks: u64,
    frequency: TscFrequency,
}

impl TscClocksource {
    /// Detects an invariant TSC and derives its frequency.
    pub fn try_new() -> Result<Self, TscError> {
        if !invariant_tsc_supported() {
            return Err(TscError::Unsupported);
        }

        let frequency = detect_frequency()?;
        let start_ticks = unsafe {
            // SAFETY: RDTSC is a non-privileged architectural instruction on
            // x86_64 and does not access memory.
            _rdtsc()
        };

        Ok(Self {
            start_ticks,
            frequency,
        })
    }

    pub const fn frequency(self) -> TscFrequency {
        self.frequency
    }
}

impl Clocksource for TscClocksource {
    type Error = TscError;

    fn now(&self) -> Result<MonotonicTime, Self::Error> {
        let ticks = unsafe {
            // SAFETY: See `try_new`; the CPU supports x86_64 RDTSC.
            _rdtsc()
        };
        let elapsed = ticks
            .checked_sub(self.start_ticks)
            .ok_or(TscError::TimeOverflow)?;
        ticks_to_nanos(elapsed, self.frequency.hz)
            .map(MonotonicTime::from_nanos)
            .ok_or(TscError::TimeOverflow)
    }
}

fn invariant_tsc_supported() -> bool {
    let max_extended = unsafe {
        // SAFETY: CPUID is architectural on x86_64.
        __cpuid(CPUID_EXTENDED_MAX).eax
    };
    if max_extended < CPUID_EXTENDED_FEATURES {
        return false;
    }

    let features = unsafe {
        // SAFETY: CPUID leaf is available after the max-leaf check above.
        __cpuid(CPUID_EXTENDED_FEATURES)
    };
    features.edx & INVARIANT_TSC_BIT != 0
}

fn detect_frequency() -> Result<TscFrequency, TscError> {
    let max_basic = unsafe {
        // SAFETY: CPUID is architectural on x86_64.
        __cpuid(0).eax
    };

    if max_basic >= CPUID_TSC_RATIO {
        let ratio = unsafe {
            // SAFETY: CPUID.15H is available after the max-basic-leaf check.
            __cpuid_count(CPUID_TSC_RATIO, 0)
        };
        let denominator = ratio.eax;
        let numerator = ratio.ebx;
        let crystal_hz = ratio.ecx as u64;

        if denominator != 0 && numerator != 0 && crystal_hz != 0 {
            let hz = crystal_hz
                .checked_mul(numerator as u64)
                .and_then(|value| value.checked_div(denominator as u64));
            if let Some(hz) = hz {
                if hz != 0 {
                    return Ok(TscFrequency { hz });
                }
            } else {
                return Err(TscError::FrequencyOverflow);
            }
        }
    }

    if max_basic >= CPUID_PROCESSOR_FREQ {
        let frequency = unsafe {
            // SAFETY: CPUID.16H is available after the max-basic-leaf check.
            __cpuid_count(CPUID_PROCESSOR_FREQ, 0)
        };
        let mhz = frequency.eax;
        if mhz != 0 {
            return Ok(TscFrequency {
                hz: (mhz as u64)
                    .checked_mul(1_000_000)
                    .ok_or(TscError::FrequencyOverflow)?,
            });
        }
    }

    Err(TscError::FrequencyUnavailable)
}

fn ticks_to_nanos(ticks: u64, frequency_hz: u64) -> Option<u64> {
    if frequency_hz == 0 {
        return None;
    }

    let seconds = ticks / frequency_hz;
    let remainder = ticks % frequency_hz;
    let whole_nanos = seconds.checked_mul(1_000_000_000)?;
    let fractional_nanos =
        ((remainder as u128) * 1_000_000_000u128 / frequency_hz as u128) as u64;
    whole_nanos.checked_add(fractional_nanos)
}

#[cfg(test)]
mod tests {
    use super::ticks_to_nanos;

    #[test]
    fn converts_exact_second() {
        assert_eq!(
            ticks_to_nanos(3_000_000_000, 3_000_000_000),
            Some(1_000_000_000)
        );
    }

    #[test]
    fn converts_fractional_second() {
        assert_eq!(ticks_to_nanos(1_500_000_000, 3_000_000_000), Some(500_000_000));
    }

    #[test]
    fn rejects_zero_frequency() {
        assert_eq!(ticks_to_nanos(1, 0), None);
    }
}
