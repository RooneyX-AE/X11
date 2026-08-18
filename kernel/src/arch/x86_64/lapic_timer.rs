//! Local APIC timer backend for xAPIC and x2APIC.
//!
//! The timer frequency is calibrated empirically against the invariant TSC so
//! the backend does not assume a particular processor bus or crystal clock.
//! Calibration is performed while the timer interrupt remains masked.

use crate::interrupts::TIMER_VECTOR;
use crate::memory::PhysicalMemoryMapping;
use crate::timer::{TimerDevice, TimerInterval};
use x86_64::registers::model_specific::Msr;

use super::apic::ApicMode;
use super::tsc::TscClocksource;

const LVT_TIMER_OFFSET: u64 = 0x320;
const TIMER_INIT_COUNT_OFFSET: u64 = 0x380;
const TIMER_CURRENT_COUNT_OFFSET: u64 = 0x390;
const TIMER_DIVIDE_OFFSET: u64 = 0x3E0;

const X2APIC_LVT_TIMER: u32 = 0x832;
const X2APIC_INIT_COUNT: u32 = 0x838;
const X2APIC_CURRENT_COUNT: u32 = 0x839;
const X2APIC_DIVIDE_CONFIG: u32 = 0x83E;

const LVT_MASKED: u32 = 1 << 16;
const LVT_PERIODIC: u32 = 1 << 17;
const MAX_INITIAL_COUNT: u32 = u32::MAX;
const DIVIDE_BY_16_ENCODING: u32 = 0b0011;
const CALIBRATION_TSC_NANOS: u64 = 10_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LapicTimerError {
    UnsupportedMode,
    InvalidMapping,
    CalibrationFailed,
    FrequencyOverflow,
    NotCalibrated,
    IntervalTooLarge,
}

/// Local APIC timer frequency in timer ticks per second after the selected divisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LapicTimerFrequency {
    hz: u64,
}

impl LapicTimerFrequency {
    pub const fn hz(self) -> u64 {
        self.hz
    }
}

/// Local APIC timer state for the current CPU.
pub struct LapicTimer {
    mode: ApicMode,
    mmio_base: Option<u64>,
    frequency: Option<LapicTimerFrequency>,
    lvt_timer: Option<Msr>,
    init_count: Option<Msr>,
    current_count: Option<Msr>,
    divide_config: Option<Msr>,
}

impl LapicTimer {
    /// Creates a masked timer backend without enabling timer interrupts.
    ///
    /// # Safety
    ///
    /// For xAPIC mode, `mapping` must cover the Local APIC MMIO page reported
    /// by `IA32_APIC_BASE`. The caller must keep the APIC mode stable while the
    /// returned object is alive.
    pub unsafe fn new(
        mode: ApicMode,
        mapping: Option<PhysicalMemoryMapping>,
    ) -> Result<Self, LapicTimerError> {
        match mode {
            ApicMode::XApic => {
                let Some(mapping) = mapping else {
                    return Err(LapicTimerError::UnsupportedMode);
                };
                let (frame, _) = x86_64::registers::model_specific::ApicBase::read();
                let base = frame.start_address().as_u64();
                let mapped = mapping
                    .translate(base)
                    .ok_or(LapicTimerError::InvalidMapping)?;
                Ok(Self {
                    mode,
                    mmio_base: Some(mapped),
                    frequency: None,
                    lvt_timer: None,
                    init_count: None,
                    current_count: None,
                    divide_config: None,
                })
            }
            ApicMode::X2Apic => Ok(Self {
                mode,
                mmio_base: None,
                frequency: None,
                lvt_timer: Some(Msr::new(X2APIC_LVT_TIMER)),
                init_count: Some(Msr::new(X2APIC_INIT_COUNT)),
                current_count: Some(Msr::new(X2APIC_CURRENT_COUNT)),
                divide_config: Some(Msr::new(X2APIC_DIVIDE_CONFIG)),
            }),
        }
    }

    pub const fn frequency(&self) -> Option<LapicTimerFrequency> {
        self.frequency
    }

    /// Calibrates the LAPIC timer against an invariant TSC.
    ///
    /// The timer interrupt stays masked during calibration.
    pub fn calibrate(
        &mut self,
        tsc: &TscClocksource,
    ) -> Result<LapicTimerFrequency, LapicTimerError> {
        unsafe {
            self.write_divide(DIVIDE_BY_16_ENCODING)?;
            self.write_lvt((TIMER_VECTOR as u32) | LVT_MASKED)?;
            self.write_initial(MAX_INITIAL_COUNT)?;
        }

        let tsc_hz = tsc.frequency().hz();
        let target_ticks = tsc_hz
            .checked_mul(CALIBRATION_TSC_NANOS)
            .and_then(|value| value.checked_div(1_000_000_000))
            .ok_or(LapicTimerError::FrequencyOverflow)?;
        if target_ticks == 0 {
            return Err(LapicTimerError::CalibrationFailed);
        }

        let start = TscClocksource::read_ticks();
        loop {
            let current = TscClocksource::read_ticks();
            if current.wrapping_sub(start) >= target_ticks {
                break;
            }
            core::hint::spin_loop();
        }

        let current_count = unsafe { self.read_current()? };
        let elapsed_count = MAX_INITIAL_COUNT.saturating_sub(current_count) as u64;
        if elapsed_count == 0 {
            return Err(LapicTimerError::CalibrationFailed);
        }

        let frequency_hz = elapsed_count
            .checked_mul(1_000_000_000)
            .and_then(|value| value.checked_div(CALIBRATION_TSC_NANOS))
            .ok_or(LapicTimerError::FrequencyOverflow)?;
        if frequency_hz == 0 {
            return Err(LapicTimerError::CalibrationFailed);
        }

        let frequency = LapicTimerFrequency { hz: frequency_hz };
        self.frequency = Some(frequency);
        unsafe {
            self.write_initial(0)?;
            self.write_lvt((TIMER_VECTOR as u32) | LVT_MASKED)?;
        }
        Ok(frequency)
    }

    unsafe fn write_lvt(&self, value: u32) -> Result<(), LapicTimerError> {
        self.write_register(LVT_TIMER_OFFSET, self.lvt_timer, value)
    }

    unsafe fn write_initial(&self, value: u32) -> Result<(), LapicTimerError> {
        self.write_register(TIMER_INIT_COUNT_OFFSET, self.init_count, value)
    }

    unsafe fn write_divide(&self, value: u32) -> Result<(), LapicTimerError> {
        self.write_register(TIMER_DIVIDE_OFFSET, self.divide_config, value)
    }

    unsafe fn read_current(&self) -> Result<u32, LapicTimerError> {
        self.read_register(TIMER_CURRENT_COUNT_OFFSET, self.current_count)
    }

    unsafe fn write_register(
        &self,
        offset: u64,
        msr: Option<Msr>,
        value: u32,
    ) -> Result<(), LapicTimerError> {
        match self.mode {
            ApicMode::XApic => {
                let base = self.mmio_base.ok_or(LapicTimerError::InvalidMapping)?;
                let address = base
                    .checked_add(offset)
                    .ok_or(LapicTimerError::InvalidMapping)?
                    as *mut u32;
                // SAFETY: `base` was derived from IA32_APIC_BASE through the
                // bootloader physical direct map and `offset` is an APIC timer
                // register defined by the architecture.
                unsafe { core::ptr::write_volatile(address, value) };
                Ok(())
            }
            ApicMode::X2Apic => {
                let msr = msr.ok_or(LapicTimerError::UnsupportedMode)?;
                // SAFETY: Register number is the architectural x2APIC timer MSR.
                unsafe { msr.write(value as u64) };
                Ok(())
            }
        }
    }

    unsafe fn read_register(
        &self,
        offset: u64,
        msr: Option<Msr>,
    ) -> Result<u32, LapicTimerError> {
        match self.mode {
            ApicMode::XApic => {
                let base = self.mmio_base.ok_or(LapicTimerError::InvalidMapping)?;
                let address = base
                    .checked_add(offset)
                    .ok_or(LapicTimerError::InvalidMapping)?
                    as *const u32;
                // SAFETY: Same MMIO invariant as `write_register`.
                Ok(unsafe { core::ptr::read_volatile(address) })
            }
            ApicMode::X2Apic => {
                let msr = msr.ok_or(LapicTimerError::UnsupportedMode)?;
                // SAFETY: Register number is the architectural x2APIC timer MSR.
                Ok(unsafe { msr.read() as u32 })
            }
        }
    }
}

impl TimerDevice for LapicTimer {
    type Error = LapicTimerError;

    fn set_periodic(&mut self, interval: TimerInterval) -> Result<(), Self::Error> {
        let frequency = self
            .frequency
            .ok_or(LapicTimerError::NotCalibrated)?
            .hz;
        let initial = frequency
            .checked_mul(interval.as_nanos())
            .and_then(|value| value.checked_div(1_000_000_000))
            .ok_or(LapicTimerError::IntervalTooLarge)?;
        let initial = u32::try_from(initial).map_err(|_| LapicTimerError::IntervalTooLarge)?;
        if initial == 0 {
            return Err(LapicTimerError::IntervalTooLarge);
        }

        unsafe {
            self.write_lvt((TIMER_VECTOR as u32) | LVT_PERIODIC)?;
            self.write_initial(initial)?;
        }
        Ok(())
    }

    fn disable(&mut self) -> Result<(), Self::Error> {
        unsafe {
            self.write_initial(0)?;
            self.write_lvt((TIMER_VECTOR as u32) | LVT_MASKED)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DIVIDE_BY_16_ENCODING;

    #[test]
    fn divide_encoding_is_sixteen() {
        assert_eq!(DIVIDE_BY_16_ENCODING, 0b0011);
    }
}
