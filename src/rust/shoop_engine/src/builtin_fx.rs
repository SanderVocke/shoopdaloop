use fundsp::prelude32::{multipass, reverb_stereo, AudioUnit, BufferVec};
use thiserror::Error;

const STATE_FORMAT: &str = "shoop-builtin-fx";
const STATE_VERSION: &str = "1";
pub const AUDIO_CHANNELS: usize = 2;
pub const ROOM_SIZE_METERS: f32 = 10.0;
pub const REVERB_TIME_SECONDS: f32 = 2.5;
pub const DAMPING: f32 = 0.5;
pub const REVERB_GAIN: f32 = 0.2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltInFxState {
    pub reverb_enabled: bool,
}

impl Default for BuiltInFxState {
    fn default() -> Self {
        Self {
            reverb_enabled: true,
        }
    }
}

impl BuiltInFxState {
    pub fn encode(self) -> String {
        format!(
            "{STATE_FORMAT}:{STATE_VERSION}:{}",
            u8::from(self.reverb_enabled)
        )
    }

    pub fn decode(encoded: &str) -> Result<Self, BuiltInFxStateError> {
        let mut fields = encoded.split(':');
        if fields.next() != Some(STATE_FORMAT) {
            return Err(BuiltInFxStateError::InvalidEnvelope);
        }
        let version = fields.next().ok_or(BuiltInFxStateError::InvalidEnvelope)?;
        if version != STATE_VERSION {
            return Err(BuiltInFxStateError::UnsupportedVersion(version.to_owned()));
        }
        let reverb_enabled = match fields.next() {
            Some("0") => false,
            Some("1") => true,
            _ => return Err(BuiltInFxStateError::InvalidReverbEnabled),
        };
        if fields.next().is_some() {
            return Err(BuiltInFxStateError::InvalidEnvelope);
        }
        let state = Self { reverb_enabled };
        if state.encode() != encoded {
            return Err(BuiltInFxStateError::InvalidEnvelope);
        }
        Ok(state)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BuiltInFxStateError {
    #[error("invalid Built-in FX state envelope")]
    InvalidEnvelope,
    #[error("unsupported Built-in FX state version {0}")]
    UnsupportedVersion(String),
    #[error("invalid Built-in FX reverb enabled value")]
    InvalidReverbEnabled,
}

#[derive(Clone, Debug, Default)]
pub struct BuiltInFxControlState {
    state: BuiltInFxState,
}

impl BuiltInFxControlState {
    pub fn from_encoded(encoded: &str) -> Result<Self, BuiltInFxStateError> {
        Ok(Self {
            state: BuiltInFxState::decode(encoded)?,
        })
    }

    pub fn state(&self) -> BuiltInFxState {
        self.state
    }

    pub fn encode(&self) -> String {
        self.state.encode()
    }

    pub fn set_reverb_enabled(&mut self, enabled: bool) {
        self.state.reverb_enabled = enabled;
    }

    pub fn prepare_processor(&self, sample_rate: f32, max_frames: usize) -> BuiltInFxProcessor {
        BuiltInFxProcessor::new(sample_rate, max_frames, self.state)
    }
}

pub struct BuiltInFxProcessor {
    reverb: Box<dyn AudioUnit>,
    state: BuiltInFxState,
    inputs: [Vec<f32>; AUDIO_CHANNELS],
    outputs: [Vec<f32>; AUDIO_CHANNELS],
    fundsp_input: BufferVec,
    fundsp_output: BufferVec,
    #[cfg(test)]
    reverb_process_calls: u64,
}

impl std::fmt::Debug for BuiltInFxProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuiltInFxProcessor")
            .field("state", &self.state)
            .field("max_frames", &self.max_frames())
            .finish_non_exhaustive()
    }
}

impl BuiltInFxProcessor {
    pub fn new(sample_rate: f32, max_frames: usize, state: BuiltInFxState) -> Self {
        let graph = multipass()
            & REVERB_GAIN * reverb_stereo(ROOM_SIZE_METERS, REVERB_TIME_SECONDS, DAMPING);
        let mut reverb: Box<dyn AudioUnit> = Box::new(graph);
        reverb.set_sample_rate(f64::from(sample_rate.max(1.0)));
        reverb.reset();
        reverb.allocate();
        let max_frames = max_frames.max(1);
        Self {
            reverb,
            state,
            inputs: std::array::from_fn(|_| vec![0.0; max_frames]),
            outputs: std::array::from_fn(|_| vec![0.0; max_frames]),
            fundsp_input: BufferVec::new(AUDIO_CHANNELS),
            fundsp_output: BufferVec::new(AUDIO_CHANNELS),
            #[cfg(test)]
            reverb_process_calls: 0,
        }
    }

    pub fn state(&self) -> BuiltInFxState {
        self.state
    }

    pub fn max_frames(&self) -> usize {
        self.inputs[0].len()
    }

    pub fn input_mut(&mut self, channel: usize, frames: usize) -> Option<&mut [f32]> {
        let frames = frames.min(self.max_frames());
        self.inputs
            .get_mut(channel)
            .map(|input| &mut input[..frames])
    }

    pub fn output(&self, channel: usize, frames: usize) -> Option<&[f32]> {
        self.outputs
            .get(channel)
            .map(|output| &output[..frames.min(self.max_frames())])
    }

    pub fn set_reverb_enabled(&mut self, enabled: bool) {
        if self.state.reverb_enabled == enabled {
            return;
        }
        if !enabled {
            self.reverb.reset();
        }
        self.state.reverb_enabled = enabled;
    }

    pub fn reset(&mut self) {
        self.reverb.reset();
    }

    pub fn process(&mut self, frames: usize) {
        let frames = frames.min(self.max_frames());
        if !self.state.reverb_enabled {
            for channel in 0..AUDIO_CHANNELS {
                self.outputs[channel][..frames].copy_from_slice(&self.inputs[channel][..frames]);
            }
            return;
        }

        let _span =
            shoop_tracing::realtime_span_detail!("engine.rt.fx.builtin_fx_process", value = frames);
        let mut start = 0;
        while start < frames {
            let chunk = (frames - start).min(fundsp::MAX_BUFFER_SIZE);
            for channel in 0..AUDIO_CHANNELS {
                self.fundsp_input.channel_f32_mut(channel)[..chunk]
                    .copy_from_slice(&self.inputs[channel][start..start + chunk]);
            }
            self.reverb.process(
                chunk,
                &self.fundsp_input.buffer_ref(),
                &mut self.fundsp_output.buffer_mut(),
            );
            for channel in 0..AUDIO_CHANNELS {
                self.outputs[channel][start..start + chunk]
                    .copy_from_slice(&self.fundsp_output.channel_f32_mut(channel)[..chunk]);
            }
            start += chunk;
            #[cfg(test)]
            {
                self.reverb_process_calls += 1;
            }
        }
    }

    #[cfg(test)]
    fn reverb_process_calls(&self) -> u64 {
        self.reverb_process_calls
    }
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<BuiltInFxProcessor>();
};

#[cfg(test)]
mod tests {
    use super::*;

    fn processor(enabled: bool, max_frames: usize) -> BuiltInFxProcessor {
        BuiltInFxProcessor::new(
            48_000.0,
            max_frames,
            BuiltInFxState {
                reverb_enabled: enabled,
            },
        )
    }

    fn set_impulse(processor: &mut BuiltInFxProcessor, frames: usize) {
        for channel in 0..AUDIO_CHANNELS {
            let input = processor.input_mut(channel, frames).unwrap();
            input.fill(0.0);
            input[0] = 1.0;
        }
    }

    fn set_silence(processor: &mut BuiltInFxProcessor, frames: usize) {
        for channel in 0..AUDIO_CHANNELS {
            processor.input_mut(channel, frames).unwrap().fill(0.0);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn state_codec_is_canonical_and_strict() {
        assert_eq!(BuiltInFxState::default().encode(), "shoop-builtin-fx:1:1");
        assert_eq!(
            BuiltInFxState::decode("shoop-builtin-fx:1:0").unwrap(),
            BuiltInFxState {
                reverb_enabled: false
            }
        );
        for invalid in [
            "",
            "shoop-builtin-fx",
            "shoop-builtin-fx:1",
            "shoop-builtin-fx:1:false",
            "shoop-builtin-fx:1:01",
            "shoop-builtin-fx:1:1:extra",
            "other:1:1",
        ] {
            assert!(BuiltInFxState::decode(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            BuiltInFxState::decode("shoop-builtin-fx:2:1"),
            Err(BuiltInFxStateError::UnsupportedVersion("2".to_owned()))
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabled_reverb_is_exact_passthrough_and_skips_fundsp() {
        let frames = 257;
        let mut processor = processor(false, frames);
        for channel in 0..AUDIO_CHANNELS {
            let input = processor.input_mut(channel, frames).unwrap();
            for (index, sample) in input.iter_mut().enumerate() {
                *sample = (index as f32 + channel as f32) / frames as f32;
            }
        }
        processor.process(frames);
        for channel in 0..AUDIO_CHANNELS {
            assert_eq!(
                processor.output(channel, frames).unwrap(),
                processor.inputs[channel]
            );
        }
        assert_eq!(processor.reverb_process_calls(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn enabled_reverb_processes_large_blocks_and_produces_a_tail() {
        let frames = 257;
        let mut processor = processor(true, frames);
        set_impulse(&mut processor, frames);
        processor.process(frames);
        assert_eq!(processor.reverb_process_calls(), 5);
        assert!(processor.output(0, frames).unwrap()[0] > 0.9);

        let mut heard_tail = false;
        for _ in 0..400 {
            set_silence(&mut processor, frames);
            processor.process(frames);
            heard_tail |= processor
                .output(0, frames)
                .unwrap()
                .iter()
                .any(|sample| sample.abs() > 1.0e-6);
        }
        assert!(heard_tail);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabling_discards_the_existing_tail() {
        let frames = 128;
        let mut processor = processor(true, frames);
        set_impulse(&mut processor, frames);
        processor.process(frames);
        processor.set_reverb_enabled(false);
        set_silence(&mut processor, frames);
        processor.process(frames);
        processor.set_reverb_enabled(true);
        for _ in 0..400 {
            set_silence(&mut processor, frames);
            processor.process(frames);
            assert!(processor
                .output(0, frames)
                .unwrap()
                .iter()
                .all(|sample| *sample == 0.0));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn processor_uses_runtime_sample_rate_and_maximum_block_size() {
        for (sample_rate, frames) in [(44_100.0, 1), (48_000.0, 128), (96_000.0, 2_048)] {
            let mut processor =
                BuiltInFxProcessor::new(sample_rate, frames, BuiltInFxState::default());
            assert_eq!(processor.max_frames(), frames);
            set_impulse(&mut processor, frames);
            processor.process(frames);
            assert!(processor.output(0, frames).unwrap()[0] > 0.9);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn steady_state_processing_and_transitions_do_not_allocate() {
        let frames = 257;
        let mut processor = processor(true, frames);
        set_impulse(&mut processor, frames);
        assert_no_alloc::assert_no_alloc(|| processor.process(frames));
        assert_no_alloc::assert_no_alloc(|| processor.set_reverb_enabled(false));
        assert_no_alloc::assert_no_alloc(|| processor.process(frames));
        assert_no_alloc::assert_no_alloc(|| processor.set_reverb_enabled(true));
        assert_no_alloc::assert_no_alloc(|| processor.reset());
    }
}
