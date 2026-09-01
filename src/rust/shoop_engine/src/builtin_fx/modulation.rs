use super::{BuiltInFxParameter, ModulationType, ParameterSmoother};

const FLANGER_MIN_DELAY_MS: f32 = 0.5;
const FLANGER_MAX_VARIATION_MS: f32 = 5.0;
const PHASER_STAGES: usize = 4;
const PHASER_MIN_HZ: f32 = 300.0;
const PHASER_MAX_HZ: f32 = 3_000.0;

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

    fn process(&mut self, sample: f32, delay_samples: f32) -> f32 {
        let length = self.samples.len() as f32;
        let position = (self.write as f32 - delay_samples).rem_euclid(length);
        let lower = position.floor() as usize % self.samples.len();
        let upper = (lower + 1) % self.samples.len();
        let fraction = position - lower as f32;
        let delayed = self.samples[lower] + (self.samples[upper] - self.samples[lower]) * fraction;
        self.samples[self.write] = sample;
        self.write += 1;
        if self.write == self.samples.len() {
            self.write = 0;
        }
        delayed
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AllpassState {
    input: f32,
    output: f32,
}

#[derive(Debug)]
pub(super) struct ModulationProcessor {
    sample_rate: f32,
    phases: Vec<f32>,
    flanger_delays: Vec<DelayLine>,
    flanger_feedback: Vec<f32>,
    phaser_states: Vec<[AllpassState; PHASER_STAGES]>,
    phaser_feedback: Vec<f32>,
    #[cfg(test)]
    type_process_calls: [u64; 3],
}

impl ModulationProcessor {
    pub(super) fn new(sample_rate: f32, audio_channels: usize) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let delay_length = ((7.0 * 0.001 * sample_rate).ceil() as usize + 2).max(2);
        Self {
            sample_rate,
            phases: (0..audio_channels)
                .map(|channel| {
                    if audio_channels == 2 {
                        0.0
                    } else {
                        deterministic_phase(channel)
                    }
                })
                .collect(),
            flanger_delays: (0..audio_channels)
                .map(|_| DelayLine::new(delay_length))
                .collect(),
            flanger_feedback: vec![0.0; audio_channels],
            phaser_states: vec![[AllpassState::default(); PHASER_STAGES]; audio_channels],
            phaser_feedback: vec![0.0; audio_channels],
            #[cfg(test)]
            type_process_calls: [0; 3],
        }
    }

    pub(super) fn reset(&mut self) {
        let stereo = self.phases.len() == 2;
        for (channel, phase) in self.phases.iter_mut().enumerate() {
            *phase = if stereo {
                0.0
            } else {
                deterministic_phase(channel)
            };
        }
        for delay in &mut self.flanger_delays {
            delay.reset();
        }
        self.flanger_feedback.fill(0.0);
        self.phaser_states
            .fill([AllpassState::default(); PHASER_STAGES]);
        self.phaser_feedback.fill(0.0);
    }

    pub(super) fn process(
        &mut self,
        modulation_type: ModulationType,
        frames: usize,
        input: &[Vec<f32>],
        output: &mut [Vec<f32>],
        smoothers: &mut [ParameterSmoother; 23],
    ) {
        #[cfg(test)]
        {
            self.type_process_calls[modulation_type_index(modulation_type)] += 1;
        }
        let stereo = input.len() == 2;
        for frame in 0..frames {
            let rate = smoothers[BuiltInFxParameter::ModulationRate.index()].next();
            let depth = smoothers[BuiltInFxParameter::ModulationDepth.index()].next();
            let mix = smoothers[BuiltInFxParameter::ModulationMix.index()].next();
            let spread = smoothers[BuiltInFxParameter::ModulationSpread.index()].next();
            let feedback = if modulation_type == ModulationType::Tremolo {
                0.0
            } else {
                smoothers[BuiltInFxParameter::ModulationFeedback.index()].next()
            };
            let phase_increment = rate / self.sample_rate;
            for channel in 0..input.len() {
                let dry = input[channel][frame];
                let offset = if stereo && channel == 1 {
                    spread * 0.5
                } else {
                    0.0
                };
                let phase = (self.phases[channel] + offset).fract();
                let lfo = (phase * std::f32::consts::TAU).sin();
                let wet = match modulation_type {
                    ModulationType::Tremolo => {
                        let amplitude = 1.0 - depth * 0.5 + depth * 0.5 * lfo;
                        dry * amplitude
                    }
                    ModulationType::Flanger => {
                        let delay_ms = FLANGER_MIN_DELAY_MS
                            + FLANGER_MAX_VARIATION_MS * depth * (lfo * 0.5 + 0.5);
                        let delayed = self.flanger_delays[channel].process(
                            dry + self.flanger_feedback[channel] * feedback,
                            (delay_ms * 0.001 * self.sample_rate).max(1.0),
                        );
                        self.flanger_feedback[channel] = delayed;
                        (dry + delayed) * 0.5
                    }
                    ModulationType::Phaser => {
                        let sweep = depth * (lfo * 0.5 + 0.5);
                        let frequency = PHASER_MIN_HZ * (PHASER_MAX_HZ / PHASER_MIN_HZ).powf(sweep);
                        let tangent = (std::f32::consts::PI * frequency / self.sample_rate).tan();
                        let coefficient = (tangent - 1.0) / (tangent + 1.0);
                        let mut sample = dry + self.phaser_feedback[channel] * feedback;
                        for state in &mut self.phaser_states[channel] {
                            let next =
                                coefficient * sample + state.input - coefficient * state.output;
                            state.input = sample;
                            state.output = next;
                            sample = next;
                        }
                        self.phaser_feedback[channel] = sample;
                        (dry + sample) * 0.5
                    }
                };
                let sample = dry + (wet - dry) * mix;
                output[channel][frame] = if sample.is_finite() { sample } else { 0.0 };
                self.phases[channel] = (self.phases[channel] + phase_increment).fract();
            }
        }
    }

    #[cfg(test)]
    pub(super) fn type_process_calls(&self, modulation_type: ModulationType) -> u64 {
        self.type_process_calls[modulation_type_index(modulation_type)]
    }
}

#[cfg(test)]
fn modulation_type_index(modulation_type: ModulationType) -> usize {
    match modulation_type {
        ModulationType::Tremolo => 0,
        ModulationType::Flanger => 1,
        ModulationType::Phaser => 2,
    }
}

fn deterministic_phase(channel: usize) -> f32 {
    ((channel as u32).wrapping_mul(2_654_435_761) & 0xffff) as f32 / 65_536.0
}
