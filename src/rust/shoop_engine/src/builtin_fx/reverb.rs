use super::{BuiltInFxParameter, ParameterSmoother, ReverbType, AUDIO_CHANNELS};
use fundsp::prelude32::{
    highshelf, lowpole_hz, lowshelf, reverb2_stereo, reverb3_stereo, reverb_stereo, AudioUnit,
    BufferVec,
};

const ROOM_SIZE_METERS: f32 = 10.0;
const ROOM_TIME_SECONDS: f32 = 2.5;
const ROOM_DAMPING: f32 = 0.5;
const TONE_LOW_HZ: f32 = 500.0;
const TONE_HIGH_HZ: f32 = 5_000.0;
const TONE_Q: f32 = 0.707;
const MAX_TONE_TILT_DB: f32 = 6.0;

pub(super) struct ReverbProcessor {
    stereo_units: Option<[Box<dyn AudioUnit>; 3]>,
    mono_units: Vec<[Box<dyn AudioUnit>; 3]>,
    tone_low: Vec<Box<dyn AudioUnit>>,
    tone_high: Vec<Box<dyn AudioUnit>>,
    wet: Vec<Vec<f32>>,
    fundsp_input: BufferVec,
    fundsp_output: BufferVec,
    #[cfg(test)]
    type_process_calls: [u64; 3],
}

impl std::fmt::Debug for ReverbProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReverbProcessor")
            .field("audio_channels", &self.wet.len())
            .field("max_frames", &self.wet[0].len())
            .finish_non_exhaustive()
    }
}

impl ReverbProcessor {
    pub(super) fn new(sample_rate: f32, max_frames: usize, audio_channels: usize) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let make_units = || {
            std::array::from_fn(|index| {
                prepare(make_reverb(reverb_type_from_index(index)), sample_rate)
            })
        };
        let (stereo_units, mono_units) = if audio_channels == AUDIO_CHANNELS {
            (Some(make_units()), Vec::new())
        } else {
            (None, (0..audio_channels).map(|_| make_units()).collect())
        };
        Self {
            stereo_units,
            mono_units,
            tone_low: (0..audio_channels)
                .map(|_| prepare(Box::new(lowshelf()), sample_rate))
                .collect(),
            tone_high: (0..audio_channels)
                .map(|_| prepare(Box::new(highshelf()), sample_rate))
                .collect(),
            wet: (0..audio_channels)
                .map(|_| vec![0.0; max_frames.max(1)])
                .collect(),
            fundsp_input: BufferVec::new(AUDIO_CHANNELS),
            fundsp_output: BufferVec::new(AUDIO_CHANNELS),
            #[cfg(test)]
            type_process_calls: [0; 3],
        }
    }

    pub(super) fn reset(&mut self) {
        if let Some(units) = &mut self.stereo_units {
            for unit in units {
                unit.reset();
            }
        }
        for channel in &mut self.mono_units {
            for unit in channel {
                unit.reset();
            }
        }
        for channel in 0..self.tone_low.len() {
            self.tone_low[channel].reset();
            self.tone_high[channel].reset();
            self.wet[channel].fill(0.0);
        }
    }

    pub(super) fn process(
        &mut self,
        reverb_type: ReverbType,
        frames: usize,
        input: &[Vec<f32>],
        output: &mut [Vec<f32>],
        smoothers: &mut [ParameterSmoother; 23],
    ) {
        let type_index = reverb_type_index(reverb_type);
        if let Some(units) = &mut self.stereo_units {
            let unit = &mut units[type_index];
            let mut start = 0;
            while start < frames {
                let chunk = (frames - start).min(fundsp::MAX_BUFFER_SIZE);
                for channel in 0..AUDIO_CHANNELS {
                    self.fundsp_input.channel_f32_mut(channel)[..chunk]
                        .copy_from_slice(&input[channel][start..start + chunk]);
                }
                unit.process(
                    chunk,
                    &self.fundsp_input.buffer_ref(),
                    &mut self.fundsp_output.buffer_mut(),
                );
                for channel in 0..AUDIO_CHANNELS {
                    self.wet[channel][start..start + chunk]
                        .copy_from_slice(&self.fundsp_output.channel_f32_mut(channel)[..chunk]);
                }
                start += chunk;
                #[cfg(test)]
                {
                    self.type_process_calls[type_index] += 1;
                }
            }
        } else {
            for channel in 0..input.len() {
                let unit = &mut self.mono_units[channel][type_index];
                let mut start = 0;
                while start < frames {
                    let chunk = (frames - start).min(fundsp::MAX_BUFFER_SIZE);
                    for fundsp_channel in 0..AUDIO_CHANNELS {
                        self.fundsp_input.channel_f32_mut(fundsp_channel)[..chunk]
                            .copy_from_slice(&input[channel][start..start + chunk]);
                    }
                    unit.process(
                        chunk,
                        &self.fundsp_input.buffer_ref(),
                        &mut self.fundsp_output.buffer_mut(),
                    );
                    for index in 0..chunk {
                        let left = self.fundsp_output.channel_f32_mut(0)[index];
                        let right = self.fundsp_output.channel_f32_mut(1)[index];
                        self.wet[channel][start + index] = (left + right) * 0.5;
                    }
                    start += chunk;
                    #[cfg(test)]
                    {
                        self.type_process_calls[type_index] += 1;
                    }
                }
            }
        }

        let mut low_output = [0.0];
        let mut high_output = [0.0];
        for frame in 0..frames {
            let amount = smoothers[BuiltInFxParameter::ReverbAmount.index()].next();
            let tone = smoothers[BuiltInFxParameter::ReverbTone.index()].next();
            let tilt_db = (tone - 0.5) * 2.0 * MAX_TONE_TILT_DB;
            let low_gain = db_amp(-tilt_db);
            let high_gain = db_amp(tilt_db);
            for channel in 0..input.len() {
                self.tone_low[channel].tick(
                    &[self.wet[channel][frame], TONE_LOW_HZ, TONE_Q, low_gain],
                    &mut low_output,
                );
                self.tone_high[channel].tick(
                    &[low_output[0], TONE_HIGH_HZ, TONE_Q, high_gain],
                    &mut high_output,
                );
                output[channel][frame] = input[channel][frame] + amount * high_output[0];
            }
        }
    }

    #[cfg(test)]
    pub(super) fn type_process_calls(&self, reverb_type: ReverbType) -> u64 {
        self.type_process_calls[reverb_type_index(reverb_type)]
    }
}

fn prepare(mut unit: Box<dyn AudioUnit>, sample_rate: f32) -> Box<dyn AudioUnit> {
    unit.set_sample_rate(f64::from(sample_rate));
    unit.reset();
    unit.allocate();
    unit
}

fn make_reverb(reverb_type: ReverbType) -> Box<dyn AudioUnit> {
    match reverb_type {
        ReverbType::Room => Box::new(reverb_stereo(
            ROOM_SIZE_METERS,
            ROOM_TIME_SECONDS,
            ROOM_DAMPING,
        )),
        ReverbType::Hall => Box::new(reverb2_stereo(20.0, 4.0, 0.8, 0.4, lowpole_hz(8_000.0))),
        ReverbType::Plate => Box::new(reverb3_stereo(2.5, 0.8, lowpole_hz(10_000.0))),
    }
}

fn reverb_type_index(reverb_type: ReverbType) -> usize {
    match reverb_type {
        ReverbType::Room => 0,
        ReverbType::Hall => 1,
        ReverbType::Plate => 2,
    }
}

fn reverb_type_from_index(index: usize) -> ReverbType {
    match index {
        0 => ReverbType::Room,
        1 => ReverbType::Hall,
        _ => ReverbType::Plate,
    }
}

fn db_amp(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
