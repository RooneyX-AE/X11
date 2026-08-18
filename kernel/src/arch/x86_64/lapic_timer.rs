//! Local APIC timer backend for xAPIC and x2APIC.
//!
//! Periodic mode is calibrated empirically against the invariant TSC. One-shot
//! deadline mode uses the architectural IA32_TSC_DEADLINE MSR when CPUID
//! advertises support, avoiding assumptions about the LAPIC bus clock.

use crate::interrupts::TIMER_VECTOR;
use crate::memory::PhysicalMemoryMapping;
use crate::timer::{MonotonicTime, TimerDeadline, TimerDevice, TimerInterval};
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

const IA32_TSC_DEADLINE: u32 = 0x6E0;
const CPUID_TSC_DEADLINE: u32 = 0x1;
const TSC_DEADLINE_BIT: u32 = 1 << 24;

const LVT_MASKED: u32 = 1 << 16;
const LVT_PERIODIC: u32 = 1 << 17;
const MAX_INITIAL_COUNT: u32 = u32::MAX;
const DIVIDE_BY_16_ENCODING: u32 = 0b0011;
const CALIBRATION_TSC_NANOS: u64 = 10_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LapicTimerError {
    UnsupportedMode,
    UnsupportedTscDeadline,
    InvalidMapping,
    CalibrationFailed,
    FrequencyOverflow,
    NotCalibrated,
    IntervalTooLarge,
    DeadlineOverflow,
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
    tsc_deadline_supported: bool,
    tsc_start_ticks: Option<u64>,
    tsc_frequency_hz: Option<u64>,
    lvt_timer: Option<Msr>,
    init_count: Option<Msr>,
    current_count: Option<Msr>,
    divide_config: Option<Msr>,
    tsc_deadline: Msr,
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
        let tsc_deadline_supported = tsc_deadline_supported();
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
                    tsc_deadline_supported,
                    tsc_start_ticks: None,
                    tsc_frequency_hz: None,
                    lvt_timer: None,
                    init_count: None,
                    current_count: None,
                    divide_config: None,
                    tsc_deadline: Msr::new(IA32_TSC_DEADLINE),
                })
            }
            ApicMode::X2Apic => Ok(Self {
                mode,
                mmio_base: None,
                frequency: None,
                tsc_deadline_supported,
                tsc_start_ticks: None,
                tsc_frequency_hz: None,
                lvt_timer: Some(Msr::new(X2APIC_LVT_TIMER)),
                init_count: Some(Msr::new(X2APIC_INIT_COUNT)),
                current_count: Some(Msr::new(X2APIC_CURRENT_COUNT)),
                divide_config: Some(Msr::new(X2APIC_DIVIDE_CONFIG)),
                tsc_deadline: Msr::new(IA32_TSC_DEADLINE),
            }),
        }
    }

    pub const fn frequency(&self) -> Option<LapicTimerFrequency> {
        self.frequency
    }

    /// Calibrates periodic mode against an invariant TSC.
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
        self.tsc_start_ticks = Some(start);
        self.tsc_frequency_hz = Some(tsc_hz);
        unsafe {
            self.write_initial(0)?;
            self.write_lvt((TIMER_VECTOR as u32) | LVT_MASKED)?;
        }
        Ok(frequency)
    }

    unsafe fn write_lvt(&mut self, value: u32) -> Result<(), LapicTimerError> {
        let mode = self.mode;
        let mmio_base = self.mmio_base;
        let msr = self.lvt_timer.as_mut();
        unsafe { Self::write_register(mode, mmio_base, LVT_TIMER_OFFSET, msr, value) }
    }

    unsafe fn write_initial(&mut self, value: u32) -> Result<(), LapicTimerError> {
        let mode = self.mode;
        let mmio_base = self.mmio_base;
        let msr = self.init_count.as_mut();
        unsafe { Self::write_register(mode, mmio_base, TIMER_INIT_COUNT_OFFSET, msr, value) }
    }

    unsafe fn write_divide(&mut self, value: u32) -> Result<(), LapicTimerError> {
        let mode = self.mode;
        let mmio_base = self.mmio_base;
        let msr = self.divide_config.as_mut();
        unsafe { Self::write_register(mode, mmio_base, TIMER_DIVIDE_OFFSET, msr, value) }
    }

    unsafe fn read_current(&mut self) -> Result<u32, LapicTimerError> {
        let mode = self.mode;
        let mmio_base = self.mmio_base;
        let msr = self.current_count.as_mut();
        unsafe { Self::read_register(mode, mmio_base, TIMER_CURRENT_COUNT_OFFSET, msr) }
    }

    unsafe fn write_register(
        mode: ApicMode,
        mmio_base: Option<u64>,
        offset: u64,
        msr: Option<&mut Msr>,
        value: u32,
    ) -> Result<(), LapicTimerError> {
        match mode {
            ApicMode::XApic => {
                let base = mmio_base.ok_or(LapicTimerError::InvalidMapping)?;
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
        mode: ApicMode,
        mmio_base: Option<u64>,
        offset: u64,
        msr: Option<&mut Msr>,
    ) -> Result<u32, LapicTimerError> {
        match mode {
            ApicMode::XApic => {
                let base = mmio_base.ok_or(LapicTimerError::InvalidMapping)?;
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

    fn deadline_to_tsc(&self, deadline: MonotonicTime) -> Result<u64, LapicTimerError> {
        let start = self.tsc_start_ticks.ok_or(LapicTimerError::NotCalibrated)?;
        let frequency = self.tsc_frequency_hz.ok_or(LapicTimerError::NotCalibrated)?;
        let nanos = deadline.as_nanos();
        let delta = (nanos as u128)
            .checked_mul(frequency as u128)
            .and_then(|value| value.checked_div(1_000_000_000u128))
            .ok_or(LapicTimerError::DeadlineOverflow)?;
        let deadline = (start as u128)
            .checked_add(delta)
            .ok_or(LapicTimerError::DeadlineOverflow)?;
        u64::try_from(deadline).map_err(|_| LapicTimerError::DeadlineOverflow)
    }
}

fn tsc_deadline_supported() -> bool {
    let leaf = core::arch::x86_64::__cpuid(CPUID_TSC_DEADLINE);
    leaf.ecx & TSC_DEADLINE_BIT != 0
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

    fn set_deadline(&mut self, deadline: TimerDeadline) -> Result<(), Self::Error> {
        if !self.tsc_deadline_supported {
            return Err(LapicTimerError::UnsupportedTscDeadline);
        }
        let tsc_deadline = self.deadline_to_tsc(deadline.time())?;

        unsafe {
            self.write_initial(0)?;
            self.write_lvt(TIMER_VECTOR as u32)?;
            // SAFETY: IA32_TSC_DEADLINE is the architectural TSC target MSR
            // for the local APIC timer.
            self.tsc_deadline.write(tsc_deadline);
        }
        Ok(())
    }

    fn disable(&mut self) -> Result<(), Self::Error> {
        unsafe {
            self.tsc_deadline.write(0);
            self.write_initial(0)?;
            self.write_lvt((TIMER_VECTOR as u32) | LVT_MASKED)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{DIVIDE_BY_16_ENCODING, tsc_deadline_supported};

    #[test]
    fn divide_encoding_is_sixteen() {
        assert_eq!(DIVIDE_BY_16_ENCODING, 0b0011);
    }

    #[test]
    fn deadline_feature_is_a_runtime_cpu_property() {
        let _ = tsc_deadline_supported();
    }
}
