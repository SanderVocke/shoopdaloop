//! DSP measurements from audio callbacks, FPGA quanta, and driver reports.
#![no_std]
#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DspMeasurement {
    Callback {
        elapsed_ns: u64,
        frames: u32,
        sample_rate: u32,
        paced: bool,
    },
    FpgaQuantum {
        cycles: u64,
        clock_hz: u32,
        frames: u32,
        sample_rate: u32,
    },
    JackServer {
        percent: f32,
    },
}

impl DspMeasurement {
    pub fn label(self) -> &'static str {
        match self {
            Self::Callback { paced: true, .. } => "Shoop callback time",
            Self::Callback { paced: false, .. } => "Unpaced Shoop callback time",
            Self::FpgaQuantum { .. } => "FPGA processing time",
            Self::JackServer { .. } => "JACK server load",
        }
    }

    pub fn microseconds(self) -> Option<f64> {
        match self {
            Self::Callback { elapsed_ns, .. } => Some(elapsed_ns as f64 / 1_000.),
            Self::FpgaQuantum {
                cycles, clock_hz, ..
            } if clock_hz > 0 => Some(cycles as f64 * 1_000_000. / f64::from(clock_hz)),
            _ => None,
        }
    }

    pub fn audio_period_us(self) -> Option<f64> {
        let (frames, sample_rate) = match self {
            Self::Callback {
                frames,
                sample_rate,
                ..
            }
            | Self::FpgaQuantum {
                frames,
                sample_rate,
                ..
            } => (frames, sample_rate),
            Self::JackServer { .. } => return None,
        };
        (frames > 0 && sample_rate > 0)
            .then(|| f64::from(frames) * 1_000_000. / f64::from(sample_rate))
    }

    pub fn percent(self) -> Option<f64> {
        if let Self::JackServer { percent } = self {
            return (percent.is_finite() && percent >= 0.).then(|| f64::from(percent));
        }
        Some(100. * self.microseconds()? / self.audio_period_us()?)
    }

    pub fn record_peak(self, peak: &mut Option<Self>) {
        let replaces = match (self.percent(), peak.and_then(Self::percent)) {
            (Some(percent), previous) => previous.is_none_or(|previous| percent >= previous),
            (None, Some(_)) => false,
            (None, None) => self.microseconds().is_some_and(|time| {
                peak.and_then(Self::microseconds)
                    .is_none_or(|previous| time >= previous)
            }),
        };
        if replaces {
            *peak = Some(self);
        }
    }
}

/// Transfers DSP peaks from a render owner to its telemetry reader. The render
/// side can keep writing while the reader drains the other bank.
#[derive(Debug, Default)]
pub struct Peak {
    state: AtomicU32,
    banks: [Bank; 2],
}

impl Peak {
    const WRITING: u32 = 2;

    pub fn record(&self, sample: DspMeasurement) {
        let state = self.state.fetch_or(Self::WRITING, Ordering::AcqRel);
        if state & Self::WRITING != 0 {
            return;
        }
        let bank = &self.banks[state as usize];
        let previous = bank.get();
        let mut peak = previous;
        sample.record_peak(&mut peak);
        if let Some(peak) = peak.filter(|_| peak != previous) {
            bank.set(peak);
        }
        self.state.fetch_and(!Self::WRITING, Ordering::Release);
    }

    /// Called by one telemetry reader. If a write is in progress, the peak
    /// remains in its bank for the next read.
    pub fn take(&self) -> Option<DspMeasurement> {
        let state = self.state.load(Ordering::Acquire);
        if state & Self::WRITING != 0 {
            return None;
        }
        self.state
            .compare_exchange(state, state ^ 1, Ordering::AcqRel, Ordering::Relaxed)
            .ok()?;
        let bank = &self.banks[state as usize];
        let sample = bank.get();
        bank.kind.store(0, Ordering::Relaxed);
        sample
    }
}

#[derive(Debug, Default)]
struct Bank {
    kind: AtomicU32,
    work: AtomicU64,
    audio: AtomicU64,
    clock_hz: AtomicU32,
}

impl Bank {
    fn get(&self) -> Option<DspMeasurement> {
        let kind = self.kind.load(Ordering::Relaxed);
        if kind == 0 {
            return None;
        }
        let work = self.work.load(Ordering::Relaxed);
        let audio = self.audio.load(Ordering::Relaxed);
        let frames = audio as u32;
        let sample_rate = (audio >> 32) as u32;
        Some(match kind {
            1 | 2 => DspMeasurement::Callback {
                elapsed_ns: work,
                frames,
                sample_rate,
                paced: kind == 1,
            },
            3 => DspMeasurement::FpgaQuantum {
                cycles: work,
                clock_hz: self.clock_hz.load(Ordering::Relaxed),
                frames,
                sample_rate,
            },
            4 => DspMeasurement::JackServer {
                percent: f32::from_bits(work as u32),
            },
            _ => return None,
        })
    }

    fn set(&self, sample: DspMeasurement) {
        let (kind, work, frames, sample_rate, clock_hz) = match sample {
            DspMeasurement::Callback {
                elapsed_ns,
                frames,
                sample_rate,
                paced,
            } => (
                if paced { 1 } else { 2 },
                elapsed_ns,
                frames,
                sample_rate,
                0,
            ),
            DspMeasurement::FpgaQuantum {
                cycles,
                clock_hz,
                frames,
                sample_rate,
            } => (3, cycles, frames, sample_rate, clock_hz),
            DspMeasurement::JackServer { percent } => (4, u64::from(percent.to_bits()), 0, 0, 0),
        };
        self.work.store(work, Ordering::Relaxed);
        self.audio.store(
            u64::from(frames) | (u64::from(sample_rate) << 32),
            Ordering::Relaxed,
        );
        self.clock_hz.store(clock_hz, Ordering::Relaxed);
        self.kind.store(kind, Ordering::Relaxed);
    }
}
