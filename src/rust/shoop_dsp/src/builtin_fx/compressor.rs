use super::{BuiltInFxParameter, ParameterSmoother};

const SOFT_KNEE_DB: f32 = 6.0;
const SILENCE_DB_FLOOR: f32 = 1.0e-12;

#[derive(Debug)]
pub(super) struct CompressorProcessor {
    sample_rate: f32,
    envelopes: Vec<f32>,
}

impl CompressorProcessor {
    pub(super) fn new(sample_rate: f32, audio_channels: usize) -> Self {
        let detector_count = if audio_channels == 2 {
            1
        } else {
            audio_channels
        };
        Self {
            sample_rate: sample_rate.max(1.0),
            envelopes: vec![0.0; detector_count],
        }
    }

    pub(super) fn reset(&mut self) {
        self.envelopes.fill(0.0);
    }

    pub(super) fn process(
        &mut self,
        frames: usize,
        input: &[Vec<f32>],
        output: &mut [Vec<f32>],
        smoothers: &mut [ParameterSmoother; 23],
    ) {
        let stereo_linked = input.len() == 2;
        for frame in 0..frames {
            let threshold = smoothers[BuiltInFxParameter::CompressorThreshold.index()].next();
            let ratio = smoothers[BuiltInFxParameter::CompressorRatio.index()].next();
            let attack_ms = smoothers[BuiltInFxParameter::CompressorAttack.index()].next();
            let release_ms = smoothers[BuiltInFxParameter::CompressorRelease.index()].next();
            let makeup_db = smoothers[BuiltInFxParameter::CompressorMakeup.index()].next();
            let attack = coefficient(self.sample_rate, attack_ms);
            let release = coefficient(self.sample_rate, release_ms);

            if stereo_linked {
                let detector = input[0][frame].abs().max(input[1][frame].abs());
                let envelope = follow(&mut self.envelopes[0], detector, attack, release);
                let gain = gain_amplitude(envelope, threshold, ratio, makeup_db);
                output[0][frame] = input[0][frame] * gain;
                output[1][frame] = input[1][frame] * gain;
            } else {
                for channel in 0..input.len() {
                    let detector = input[channel][frame].abs();
                    let envelope = follow(&mut self.envelopes[channel], detector, attack, release);
                    let gain = gain_amplitude(envelope, threshold, ratio, makeup_db);
                    output[channel][frame] = input[channel][frame] * gain;
                }
            }
        }
    }
}

fn coefficient(sample_rate: f32, milliseconds: f32) -> f32 {
    (-1.0 / (sample_rate * milliseconds.max(0.001) * 0.001)).exp()
}

fn follow(envelope: &mut f32, detector: f32, attack: f32, release: f32) -> f32 {
    let coefficient = if detector > *envelope {
        attack
    } else {
        release
    };
    *envelope = coefficient * *envelope + (1.0 - coefficient) * detector;
    *envelope
}

fn gain_amplitude(envelope: f32, threshold: f32, ratio: f32, makeup_db: f32) -> f32 {
    let level_db = 20.0 * envelope.max(SILENCE_DB_FLOOR).log10();
    let over_db = level_db - threshold;
    let slope = 1.0 - ratio.max(1.0).recip();
    let half_knee = SOFT_KNEE_DB * 0.5;
    let reduction_db = if over_db <= -half_knee {
        0.0
    } else if over_db >= half_knee {
        over_db * slope
    } else {
        let knee_position = over_db + half_knee;
        slope * knee_position * knee_position / (2.0 * SOFT_KNEE_DB)
    };
    10.0_f32.powf((makeup_db - reduction_db) / 20.0)
}
