use super::{BuiltInFxParameter, DriveType, ParameterSmoother};
use fundsp::prelude32::{dcblock, lowpass, shape_fn, AudioUnit};

const MIN_TONE_HZ: f32 = 800.0;
const MAX_TONE_HZ: f32 = 20_000.0;
const TONE_Q: f32 = 0.707;
const MAX_WET_MAGNITUDE: f32 = 4.0;

pub(super) struct DriveProcessor {
    sample_rate: f32,
    shapers: Vec<[Box<dyn AudioUnit>; 4]>,
    tone_filters: Vec<Box<dyn AudioUnit>>,
    dc_blocks: Vec<Box<dyn AudioUnit>>,
    #[cfg(test)]
    type_process_calls: [u64; 4],
}

impl std::fmt::Debug for DriveProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DriveProcessor")
            .field("audio_channels", &self.shapers.len())
            .finish_non_exhaustive()
    }
}

impl DriveProcessor {
    pub(super) fn new(sample_rate: f32, audio_channels: usize) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let shapers = (0..audio_channels)
            .map(|_| std::array::from_fn(|index| prepare(make_shaper(index), sample_rate)))
            .collect();
        let tone_filters = (0..audio_channels)
            .map(|_| prepare(Box::new(lowpass()), sample_rate))
            .collect();
        let dc_blocks = (0..audio_channels)
            .map(|_| prepare(Box::new(dcblock()), sample_rate))
            .collect();
        Self {
            sample_rate,
            shapers,
            tone_filters,
            dc_blocks,
            #[cfg(test)]
            type_process_calls: [0; 4],
        }
    }

    pub(super) fn reset(&mut self) {
        for channel in 0..self.shapers.len() {
            for shaper in &mut self.shapers[channel] {
                shaper.reset();
            }
            self.tone_filters[channel].reset();
            self.dc_blocks[channel].reset();
        }
    }

    pub(super) fn process(
        &mut self,
        drive_type: DriveType,
        frames: usize,
        input: &[Vec<f32>],
        output: &mut [Vec<f32>],
        smoothers: &mut [ParameterSmoother; 23],
    ) {
        let type_index = drive_type_index(drive_type);
        #[cfg(test)]
        {
            self.type_process_calls[type_index] += 1;
        }
        let maximum_cutoff = MAX_TONE_HZ.min(self.sample_rate * 0.45).max(MIN_TONE_HZ);
        let mut shaped = [0.0];
        let mut toned = [0.0];
        let mut dc_blocked = [0.0];
        for frame in 0..frames {
            let drive_db = smoothers[BuiltInFxParameter::Drive.index()].next();
            let tone = smoothers[BuiltInFxParameter::DriveTone.index()].next();
            let mix = smoothers[BuiltInFxParameter::DriveMix.index()].next();
            let output_db = smoothers[BuiltInFxParameter::DriveOutput.index()].next();
            let drive_gain = db_amp(drive_db);
            let output_gain = db_amp(output_db);
            let cutoff = xerp(MIN_TONE_HZ, maximum_cutoff, tone);
            for channel in 0..input.len() {
                let dry = input[channel][frame];
                self.shapers[channel][type_index].tick(&[dry * drive_gain], &mut shaped);
                self.tone_filters[channel].tick(&[shaped[0], cutoff, TONE_Q], &mut toned);
                self.dc_blocks[channel].tick(&toned, &mut dc_blocked);
                let wet = dc_blocked[0].clamp(-MAX_WET_MAGNITUDE, MAX_WET_MAGNITUDE);
                let sample = (dry + (wet - dry) * mix) * output_gain;
                output[channel][frame] = if sample.is_finite() { sample } else { 0.0 };
            }
        }
    }

    #[cfg(test)]
    pub(super) fn type_process_calls(&self, drive_type: DriveType) -> u64 {
        self.type_process_calls[drive_type_index(drive_type)]
    }
}

fn drive_type_index(drive_type: DriveType) -> usize {
    match drive_type {
        DriveType::Saturation => 0,
        DriveType::Overdrive => 1,
        DriveType::Distortion => 2,
        DriveType::Fuzz => 3,
    }
}

fn prepare(mut unit: Box<dyn AudioUnit>, sample_rate: f32) -> Box<dyn AudioUnit> {
    unit.set_sample_rate(f64::from(sample_rate));
    unit.reset();
    unit.allocate();
    unit
}

fn make_shaper(index: usize) -> Box<dyn AudioUnit> {
    match index {
        0 => Box::new(shape_fn(|sample| sample.tanh())),
        1 => Box::new(shape_fn(|sample| {
            if sample >= 0.0 {
                1.0 - (-sample).exp()
            } else {
                -0.75 * (1.0 - (sample / 0.75).exp())
            }
        })),
        2 => Box::new(shape_fn(|sample| sample.clamp(-1.0, 1.0))),
        _ => Box::new(shape_fn(|sample| {
            if sample.abs() < 0.02 {
                0.0
            } else {
                (sample * 4.0).tanh()
            }
        })),
    }
}

fn db_amp(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn xerp(minimum: f32, maximum: f32, normalized: f32) -> f32 {
    minimum * (maximum / minimum).powf(normalized)
}
