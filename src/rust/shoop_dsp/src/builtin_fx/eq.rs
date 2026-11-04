use super::{BuiltInFxParameter, ParameterSmoother};
use fundsp::prelude32::{bell, highshelf, lowshelf, AudioUnit};

pub(super) const LOW_FREQUENCY_HZ: f32 = 120.0;
pub(super) const LOW_Q: f32 = 0.707;
pub(super) const MID_FREQUENCY_HZ: f32 = 1_000.0;
pub(super) const MID_Q: f32 = 0.8;
pub(super) const HIGH_FREQUENCY_HZ: f32 = 8_000.0;
pub(super) const HIGH_Q: f32 = 0.707;

pub(super) struct EqProcessor {
    low: Vec<Box<dyn AudioUnit>>,
    mid: Vec<Box<dyn AudioUnit>>,
    high: Vec<Box<dyn AudioUnit>>,
}

impl std::fmt::Debug for EqProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EqProcessor")
            .field("audio_channels", &self.low.len())
            .finish_non_exhaustive()
    }
}

impl EqProcessor {
    pub(super) fn new(sample_rate: f32, audio_channels: usize) -> Self {
        let sample_rate = sample_rate.max(1.0);
        Self {
            low: (0..audio_channels)
                .map(|_| prepare(Box::new(lowshelf()), sample_rate))
                .collect(),
            mid: (0..audio_channels)
                .map(|_| prepare(Box::new(bell()), sample_rate))
                .collect(),
            high: (0..audio_channels)
                .map(|_| prepare(Box::new(highshelf()), sample_rate))
                .collect(),
        }
    }

    pub(super) fn reset(&mut self) {
        for channel in 0..self.low.len() {
            self.low[channel].reset();
            self.mid[channel].reset();
            self.high[channel].reset();
        }
    }

    pub(super) fn process(
        &mut self,
        frames: usize,
        input: &[Vec<f32>],
        output: &mut [Vec<f32>],
        smoothers: &mut [ParameterSmoother; 23],
    ) {
        let mut low_output = [0.0];
        let mut mid_output = [0.0];
        let mut high_output = [0.0];
        for frame in 0..frames {
            let low_gain = db_amp(smoothers[BuiltInFxParameter::EqLow.index()].next());
            let mid_gain = db_amp(smoothers[BuiltInFxParameter::EqMid.index()].next());
            let high_gain = db_amp(smoothers[BuiltInFxParameter::EqHigh.index()].next());
            for channel in 0..input.len() {
                self.low[channel].tick(
                    &[input[channel][frame], LOW_FREQUENCY_HZ, LOW_Q, low_gain],
                    &mut low_output,
                );
                self.mid[channel].tick(
                    &[low_output[0], MID_FREQUENCY_HZ, MID_Q, mid_gain],
                    &mut mid_output,
                );
                self.high[channel].tick(
                    &[mid_output[0], HIGH_FREQUENCY_HZ, HIGH_Q, high_gain],
                    &mut high_output,
                );
                output[channel][frame] = high_output[0];
            }
        }
    }
}

fn prepare(mut unit: Box<dyn AudioUnit>, sample_rate: f32) -> Box<dyn AudioUnit> {
    unit.set_sample_rate(f64::from(sample_rate));
    unit.reset();
    unit.allocate();
    unit
}

fn db_amp(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
