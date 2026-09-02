use super::{BuiltInFxParameter, ParameterSmoother};

const VOICES: usize = 5;
const BASE_DELAYS_MS: [f32; VOICES] = [10.0, 14.0, 18.0, 22.0, 26.0];
const MAX_VARIATION_MS: f32 = 5.0;

#[derive(Debug)]
struct DelayLine {
    samples: Vec<f32>,
    write: usize,
}

impl DelayLine {
    fn new(length: usize) -> Self {
        Self {
            samples: vec![0.0; length.max(2)],
            write: 0,
        }
    }

    fn reset(&mut self) {
        self.samples.fill(0.0);
        self.write = 0;
    }

    fn write(&mut self, sample: f32) {
        self.samples[self.write] = sample;
    }

    fn read(&self, delay_samples: f32) -> f32 {
        let length = self.samples.len() as f32;
        let position = (self.write as f32 - delay_samples).rem_euclid(length);
        let lower = position.floor() as usize % self.samples.len();
        let upper = (lower + 1) % self.samples.len();
        let fraction = position - lower as f32;
        self.samples[lower] + (self.samples[upper] - self.samples[lower]) * fraction
    }

    fn advance(&mut self) {
        self.write += 1;
        if self.write == self.samples.len() {
            self.write = 0;
        }
    }
}

#[derive(Debug)]
pub(super) struct ChorusProcessor {
    sample_rate: f32,
    delay_lines: Vec<[DelayLine; VOICES]>,
    phases: Vec<f32>,
}

impl ChorusProcessor {
    pub(super) fn new(sample_rate: f32, audio_channels: usize) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let maximum_delay = ((31.0 * 0.001 * sample_rate).ceil() as usize + 2).max(2);
        Self {
            sample_rate,
            delay_lines: (0..audio_channels)
                .map(|_| std::array::from_fn(|_| DelayLine::new(maximum_delay)))
                .collect(),
            phases: (0..audio_channels)
                .map(|channel| {
                    if audio_channels == 2 {
                        0.0
                    } else {
                        deterministic_phase(channel)
                    }
                })
                .collect(),
        }
    }

    pub(super) fn reset(&mut self) {
        for channel in &mut self.delay_lines {
            for voice in channel {
                voice.reset();
            }
        }
        let stereo = self.phases.len() == 2;
        for (channel, phase) in self.phases.iter_mut().enumerate() {
            *phase = if stereo {
                0.0
            } else {
                deterministic_phase(channel)
            };
        }
    }

    pub(super) fn process(
        &mut self,
        frames: usize,
        input: &[Vec<f32>],
        output: &mut [Vec<f32>],
        smoothers: &mut [ParameterSmoother; 23],
    ) {
        let stereo = input.len() == 2;
        for frame in 0..frames {
            let rate = smoothers[BuiltInFxParameter::ChorusRate.index()].next();
            let depth = smoothers[BuiltInFxParameter::ChorusDepth.index()].next();
            let mix = smoothers[BuiltInFxParameter::ChorusMix.index()].next();
            let width = smoothers[BuiltInFxParameter::ChorusWidth.index()].next();
            let phase_increment = rate / self.sample_rate;
            for channel in 0..input.len() {
                let dry = input[channel][frame];
                let stereo_offset = if stereo && channel == 1 {
                    width * 0.25
                } else {
                    0.0
                };
                let mut wet = 0.0;
                for voice in 0..VOICES {
                    let voice_phase =
                        (self.phases[channel] + stereo_offset + voice as f32 / VOICES as f32)
                            .fract();
                    let modulation = (voice_phase * std::f32::consts::TAU).sin();
                    let delay_ms = BASE_DELAYS_MS[voice] + MAX_VARIATION_MS * depth * modulation;
                    let delay_samples = (delay_ms * 0.001 * self.sample_rate).max(1.0);
                    self.delay_lines[channel][voice].write(dry);
                    wet += self.delay_lines[channel][voice].read(delay_samples);
                    self.delay_lines[channel][voice].advance();
                }
                wet /= VOICES as f32;
                output[channel][frame] = dry + (wet - dry) * mix;
                self.phases[channel] = (self.phases[channel] + phase_increment).fract();
            }
        }
    }
}

fn deterministic_phase(channel: usize) -> f32 {
    ((channel as u32).wrapping_mul(2_654_435_761) & 0xffff) as f32 / 65_536.0
}
