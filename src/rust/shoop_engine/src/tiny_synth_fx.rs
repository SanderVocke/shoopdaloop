use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;
use tinyviolin::midi::MidiMessage;
use tinyviolin::{AudioProcessor, Preset};

use crate::midi_storage::MidiStorageElem;

pub const MIN_MASTER_GAIN_DB: f32 = -60.0;
pub const MAX_MASTER_GAIN_DB: f32 = 0.0;
pub const DEFAULT_MASTER_GAIN_DB: f32 = -6.0;
const GAIN_SMOOTH_SECONDS: f32 = 0.02;
const STATE_PREFIX: &str = "shoop-tiny-synth-fx:1:";
const MAX_PROCESSOR_STATE_BYTES: usize = 256 * 1024;
type HostedAudioProcessor = AudioProcessor<32, 1>;

#[derive(Clone, Debug, PartialEq)]
pub struct TinySynthFxEditorState {
    pub selected_preset_id: Option<String>,
    pub master_gain_db: f32,
    pub reverb_enabled: bool,
    pub reverb_amount: f32,
    pub distortion_enabled: bool,
    pub distortion_drive: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TinySynthFxStateError {
    InvalidEnvelope,
    InvalidGain,
    InvalidProcessorState,
    StateTooLarge,
}

impl std::fmt::Display for TinySynthFxStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEnvelope => "invalid Tiny Synth/FX state envelope",
            Self::InvalidGain => "invalid Tiny Synth/FX master gain",
            Self::InvalidProcessorState => "invalid tinyviolin processor state",
            Self::StateTooLarge => "Tiny Synth/FX state exceeds its resource limit",
        })
    }
}

impl std::error::Error for TinySynthFxStateError {}

pub struct TinySynthFxControlState {
    audio: Box<HostedAudioProcessor>,
    master_gain_db: f32,
}

impl TinySynthFxControlState {
    pub fn new(sample_rate: f32) -> Result<Self, tinyviolin::ProcessError> {
        let mut audio = Box::new(HostedAudioProcessor::new(sample_rate, 1)?);
        if let Some(preset) = audio.available_presets().first().copied() {
            audio.select_preset(preset);
        }
        Ok(Self {
            audio,
            master_gain_db: DEFAULT_MASTER_GAIN_DB,
        })
    }

    pub fn from_encoded(sample_rate: f32, encoded: &str) -> Result<Self, TinySynthFxStateError> {
        let (master_gain_db, state) = decode_state(encoded)?;
        let mut result =
            Self::new(sample_rate).map_err(|_| TinySynthFxStateError::InvalidProcessorState)?;
        result
            .audio
            .load_state(&state)
            .map_err(|_| TinySynthFxStateError::InvalidProcessorState)?;
        result.master_gain_db = master_gain_db;
        Ok(result)
    }

    pub fn encode(&self) -> String {
        encode_state(self.master_gain_db, &self.audio.serialize_state())
    }

    pub fn editor_state(&self) -> TinySynthFxEditorState {
        let settings = self.audio.effect_settings();
        TinySynthFxEditorState {
            selected_preset_id: self
                .audio
                .selected_preset()
                .map(|preset| preset.id().to_owned()),
            master_gain_db: self.master_gain_db,
            reverb_enabled: settings.reverb_enabled,
            reverb_amount: settings.reverb_amount,
            distortion_enabled: settings.distortion_enabled,
            distortion_drive: settings.distortion_drive,
        }
    }

    pub fn select_preset(&mut self, id: &str) -> Result<(), tinyviolin::midi::MidiError> {
        self.audio.select_preset_by_id(id)
    }

    pub fn set_master_gain_db(&mut self, gain_db: f32) -> Result<(), TinySynthFxStateError> {
        validate_gain(gain_db)?;
        self.master_gain_db = gain_db;
        Ok(())
    }

    pub fn set_reverb_enabled(&mut self, enabled: bool) {
        self.audio.set_reverb_enabled(enabled);
    }

    pub fn set_reverb_amount(&mut self, amount: f32) -> Result<(), tinyviolin::ProcessError> {
        self.audio.set_reverb_amount(amount)
    }

    pub fn set_distortion_enabled(&mut self, enabled: bool) {
        self.audio.set_distortion_enabled(enabled);
    }

    pub fn set_distortion_drive(&mut self, drive: f32) -> Result<(), tinyviolin::ProcessError> {
        self.audio.set_distortion_drive(drive)
    }

    pub fn prepare_processor(
        &self,
        sample_rate: f32,
        channel_count: usize,
        max_frames: usize,
    ) -> Result<TinySynthFxProcessor, TinySynthFxStateError> {
        TinySynthFxProcessor::from_state(
            sample_rate,
            channel_count,
            max_frames,
            &self.audio.serialize_state(),
            self.master_gain_db,
        )
    }
}

pub struct TinySynthFxProcessor {
    audio: Box<HostedAudioProcessor>,
    planes: AudioPlanes,
    logical_channel_count: usize,
    sample_rate: f32,
    current_gain: f32,
    target_gain: f32,
    gain_step: f32,
    gain_samples_remaining: u32,
}

impl std::fmt::Debug for TinySynthFxProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TinySynthFxProcessor")
            .field("logical_channel_count", &self.logical_channel_count)
            .field("max_frames", &self.planes.max_frames)
            .finish_non_exhaustive()
    }
}

impl TinySynthFxProcessor {
    fn from_state(
        sample_rate: f32,
        channel_count: usize,
        max_frames: usize,
        state: &[u8],
        master_gain_db: f32,
    ) -> Result<Self, TinySynthFxStateError> {
        validate_gain(master_gain_db)?;
        let processing_channels = channel_count.max(1);
        let max_frames = max_frames.max(1);
        let mut audio = Box::new(
            HostedAudioProcessor::new(sample_rate, processing_channels)
                .map_err(|_| TinySynthFxStateError::InvalidProcessorState)?,
        );
        audio
            .load_state(state)
            .map_err(|_| TinySynthFxStateError::InvalidProcessorState)?;
        let planes = AudioPlanes::new(processing_channels, max_frames);
        let gain = db_to_gain(master_gain_db);
        Ok(Self {
            audio,
            planes,
            logical_channel_count: channel_count,
            sample_rate,
            current_gain: gain,
            target_gain: gain,
            gain_step: 0.0,
            gain_samples_remaining: 0,
        })
    }

    pub fn logical_channel_count(&self) -> usize {
        self.logical_channel_count
    }

    pub fn max_frames(&self) -> usize {
        self.planes.max_frames
    }

    pub fn plane(&self, index: usize, frames: usize) -> Option<&[f32]> {
        self.planes
            .planes
            .get(index)
            .map(|plane| &plane[..frames.min(self.planes.max_frames)])
    }

    pub fn plane_mut(&mut self, index: usize, frames: usize) -> Option<&mut [f32]> {
        self.planes
            .planes
            .get_mut(index)
            .map(|plane| &mut plane[..frames.min(self.planes.max_frames)])
    }

    pub fn clear_silent_plane(&mut self, frames: usize) {
        if self.logical_channel_count == 0 {
            self.planes.planes[0][..frames.min(self.planes.max_frames)].fill(0.0);
        }
    }

    pub fn process(&mut self, frames: usize, events: &[MidiStorageElem]) {
        let frames = frames.min(self.planes.max_frames);
        let mut cursor = 0;
        for event in events {
            let offset = (event.time as usize).min(frames).max(cursor);
            let _ = self
                .audio
                .render_range(&mut self.planes.planes, cursor..offset);
            if let Ok(message) = MidiMessage::new(event.data()) {
                let _ = self.audio.dispatch_midi(message);
            }
            cursor = offset;
        }
        let _ = self
            .audio
            .render_range(&mut self.planes.planes, cursor..frames);
        for frame in 0..frames {
            let gain = self.next_gain();
            for plane in self
                .planes
                .planes
                .iter_mut()
                .take(self.logical_channel_count)
            {
                plane[frame] *= gain;
            }
        }
    }

    pub fn select_preset(&mut self, id: &str) {
        let _ = self.audio.select_preset_by_id(id);
    }

    pub fn set_master_gain_db(&mut self, gain_db: f32) {
        if validate_gain(gain_db).is_err() {
            return;
        }
        self.target_gain = db_to_gain(gain_db);
        self.gain_samples_remaining =
            (self.sample_rate * GAIN_SMOOTH_SECONDS).round().max(1.0) as u32;
        self.gain_step =
            (self.target_gain - self.current_gain) / self.gain_samples_remaining as f32;
    }

    pub fn set_reverb_enabled(&mut self, enabled: bool) {
        self.audio.set_reverb_enabled(enabled);
    }

    pub fn set_reverb_amount(&mut self, amount: f32) {
        let _ = self.audio.set_reverb_amount(amount);
    }

    pub fn set_distortion_enabled(&mut self, enabled: bool) {
        self.audio.set_distortion_enabled(enabled);
    }

    pub fn set_distortion_drive(&mut self, drive: f32) {
        let _ = self.audio.set_distortion_drive(drive);
    }

    pub fn panic(&mut self) {
        self.audio.panic();
    }

    fn next_gain(&mut self) -> f32 {
        if self.gain_samples_remaining > 0 {
            self.current_gain += self.gain_step;
            self.gain_samples_remaining -= 1;
            if self.gain_samples_remaining == 0 {
                self.current_gain = self.target_gain;
            }
        }
        self.current_gain
    }
}

struct AudioPlanes {
    planes: Vec<&'static mut [f32]>,
    _owners: Vec<Box<[f32]>>,
    max_frames: usize,
}

impl AudioPlanes {
    fn new(channel_count: usize, max_frames: usize) -> Self {
        let mut owners = (0..channel_count)
            .map(|_| vec![0.0_f32; max_frames].into_boxed_slice())
            .collect::<Vec<_>>();
        let planes = owners
            .iter_mut()
            .map(|owner| {
                let pointer: *mut [f32] = owner.as_mut();
                // SAFETY: every pointer addresses one distinct owned slice. The
                // boxes do not move their sample allocations, samples are accessed
                // only through these views, and views are dropped before owners.
                unsafe { &mut *pointer }
            })
            .collect();
        Self {
            planes,
            _owners: owners,
            max_frames,
        }
    }
}

impl Drop for AudioPlanes {
    fn drop(&mut self) {
        self.planes.clear();
    }
}

pub fn available_presets() -> impl Iterator<Item = (&'static str, &'static str)> {
    Preset::available()
        .iter()
        .copied()
        .map(|preset| (preset.id(), preset.name()))
}

fn encode_state(master_gain_db: f32, state: &[u8]) -> String {
    format!(
        "{STATE_PREFIX}{:08x}:{}",
        master_gain_db.to_bits(),
        STANDARD_NO_PAD.encode(state)
    )
}

fn decode_state(encoded: &str) -> Result<(f32, Vec<u8>), TinySynthFxStateError> {
    let rest = encoded
        .strip_prefix(STATE_PREFIX)
        .ok_or(TinySynthFxStateError::InvalidEnvelope)?;
    let (gain, payload) = rest
        .split_once(':')
        .ok_or(TinySynthFxStateError::InvalidEnvelope)?;
    if gain.len() != 8 {
        return Err(TinySynthFxStateError::InvalidEnvelope);
    }
    let gain_bits =
        u32::from_str_radix(gain, 16).map_err(|_| TinySynthFxStateError::InvalidEnvelope)?;
    let gain_db = f32::from_bits(gain_bits);
    validate_gain(gain_db)?;
    let max_encoded_bytes = MAX_PROCESSOR_STATE_BYTES.div_ceil(3) * 4;
    if payload.len() > max_encoded_bytes {
        return Err(TinySynthFxStateError::StateTooLarge);
    }
    let state = STANDARD_NO_PAD
        .decode(payload)
        .map_err(|_| TinySynthFxStateError::InvalidEnvelope)?;
    if state.len() > MAX_PROCESSOR_STATE_BYTES {
        return Err(TinySynthFxStateError::StateTooLarge);
    }
    Ok((gain_db, state))
}

fn validate_gain(gain_db: f32) -> Result<(), TinySynthFxStateError> {
    if gain_db.is_finite() && (MIN_MASTER_GAIN_DB..=MAX_MASTER_GAIN_DB).contains(&gain_db) {
        Ok(())
    } else {
        Err(TinySynthFxStateError::InvalidGain)
    }
}

fn db_to_gain(gain_db: f32) -> f32 {
    10.0_f32.powf(gain_db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_state_round_trips_library_state_and_gain() {
        let mut source = TinySynthFxControlState::new(48_000.0).unwrap();
        source.select_preset("pad").unwrap();
        source.set_master_gain_db(-12.5).unwrap();
        source.set_reverb_enabled(true);
        source.set_reverb_amount(0.4).unwrap();
        source.set_distortion_enabled(true);
        source.set_distortion_drive(8.0).unwrap();
        let encoded = source.encode();
        let restored = TinySynthFxControlState::from_encoded(44_100.0, &encoded).unwrap();
        assert_eq!(restored.editor_state(), source.editor_state());
        assert_eq!(restored.encode(), encoded);
    }

    #[test]
    fn malformed_or_out_of_range_state_is_rejected() {
        assert!(TinySynthFxControlState::from_encoded(48_000.0, "not-state").is_err());
        let invalid_gain = format!(
            "{STATE_PREFIX}{:08x}:{}",
            3.0_f32.to_bits(),
            STANDARD_NO_PAD.encode([1, 2, 3])
        );
        assert_eq!(
            TinySynthFxControlState::from_encoded(48_000.0, &invalid_gain)
                .err()
                .unwrap(),
            TinySynthFxStateError::InvalidGain
        );
        let oversized = format!(
            "{STATE_PREFIX}{:08x}:{}",
            DEFAULT_MASTER_GAIN_DB.to_bits(),
            "A".repeat(MAX_PROCESSOR_STATE_BYTES.div_ceil(3) * 4 + 1)
        );
        assert_eq!(
            TinySynthFxControlState::from_encoded(48_000.0, &oversized)
                .err()
                .unwrap(),
            TinySynthFxStateError::StateTooLarge
        );
    }

    #[test]
    fn matched_mono_stereo_and_seven_channel_audio_mix_the_same_timed_synth() {
        for channels in [1, 2, 7] {
            let control = TinySynthFxControlState::new(48_000.0).unwrap();
            let mut processor = control.prepare_processor(48_000.0, channels, 64).unwrap();
            for channel in 0..channels {
                processor
                    .plane_mut(channel, 64)
                    .unwrap()
                    .fill((channel + 1) as f32 * 0.01);
            }
            let events = [
                MidiStorageElem::new(16, &[0x90, 69, 127]).unwrap(),
                MidiStorageElem::new(48, &[0x80, 69, 0]).unwrap(),
            ];
            processor.process(64, &events);
            let host_gain = db_to_gain(DEFAULT_MASTER_GAIN_DB);
            for channel in 0..channels {
                let input = (channel + 1) as f32 * 0.01;
                assert!(
                    (processor.plane(channel, 64).unwrap()[8] - input * host_gain).abs() < 1.0e-6
                );
            }
            let reference_synth = processor.plane(0, 64).unwrap()[24] / host_gain - 0.01;
            assert!(reference_synth.abs() > 0.001);
            for channel in 1..channels {
                let input = (channel + 1) as f32 * 0.01;
                let synth = processor.plane(channel, 64).unwrap()[24] / host_gain - input;
                assert!((synth - reference_synth).abs() < 1.0e-5);
            }
        }
    }

    #[test]
    fn unsupported_or_malformed_midi_preserves_the_audio_quantum() {
        let control = TinySynthFxControlState::new(48_000.0).unwrap();
        let mut processor = control.prepare_processor(48_000.0, 1, 64).unwrap();
        processor.plane_mut(0, 64).unwrap().fill(0.2);
        let events = [
            MidiStorageElem::new(8, &[0xF8]).unwrap(),
            MidiStorageElem::new(24, &[0x90]).unwrap(),
            MidiStorageElem::new(40, &[0xB0, 7, 100]).unwrap(),
        ];
        processor.process(64, &events);
        let expected = 0.2 * db_to_gain(DEFAULT_MASTER_GAIN_DB);
        assert!(processor
            .plane(0, 64)
            .unwrap()
            .iter()
            .all(|sample| (*sample - expected).abs() < 1.0e-6));
    }

    #[test]
    fn all_notes_off_and_all_sound_off_reach_tinyviolin_at_sample_offsets() {
        let control = TinySynthFxControlState::new(48_000.0).unwrap();
        let note_on = MidiStorageElem::new(0, &[0x90, 69, 127]).unwrap();

        let mut sustained = control.prepare_processor(48_000.0, 1, 64).unwrap();
        sustained.process(64, std::slice::from_ref(&note_on));
        let sustained = sustained.plane(0, 64).unwrap().to_vec();
        assert!(sustained[8..32].iter().any(|sample| sample.abs() > 0.001));

        let mut released = control.prepare_processor(48_000.0, 1, 64).unwrap();
        released.process(
            64,
            &[
                note_on.clone(),
                MidiStorageElem::new(32, &[0xB0, 123, 0]).unwrap(),
            ],
        );
        assert!(released.plane(0, 64).unwrap()[33..]
            .iter()
            .zip(&sustained[33..])
            .any(|(released, sustained)| (*released - *sustained).abs() > 1.0e-5));

        let mut stopped = control.prepare_processor(48_000.0, 1, 64).unwrap();
        stopped.process(
            64,
            &[note_on, MidiStorageElem::new(32, &[0xB0, 120, 0]).unwrap()],
        );
        assert!(stopped.plane(0, 64).unwrap()[32..]
            .iter()
            .all(|sample| sample.abs() < 1.0e-7));
    }

    #[test]
    fn zero_audio_midi_and_effect_controls_are_stable() {
        let control = TinySynthFxControlState::new(48_000.0).unwrap();
        let mut silent = control.prepare_processor(48_000.0, 0, 64).unwrap();
        let events = [
            MidiStorageElem::new(0, &[0x90, 69, 127]).unwrap(),
            MidiStorageElem::new(16, &[0xB0, 123, 0]).unwrap(),
            MidiStorageElem::new(24, &[0xB0, 120, 0]).unwrap(),
            MidiStorageElem::new(32, &[0xF8]).unwrap(),
            MidiStorageElem::new(40, &[0x90]).unwrap(),
        ];
        silent.process(64, &events);
        assert_eq!(silent.logical_channel_count(), 0);
        assert!(silent
            .plane(0, 64)
            .unwrap()
            .iter()
            .all(|sample| sample.is_finite()));

        let mut processor = control.prepare_processor(48_000.0, 1, 64).unwrap();
        processor.plane_mut(0, 64).unwrap().fill(0.2);
        processor.set_distortion_enabled(true);
        processor.set_distortion_drive(8.0);
        processor.set_reverb_enabled(true);
        processor.set_reverb_amount(0.5);
        processor.process(64, &events[3..]);
        let processed = processor.plane(0, 64).unwrap();
        assert!(processed.iter().all(|sample| sample.is_finite()));
        assert!(processed
            .iter()
            .any(|sample| (*sample - 0.2 * db_to_gain(DEFAULT_MASTER_GAIN_DB)).abs() > 0.01));
        processor.panic();
        processor.process(64, &[]);
    }

    #[test]
    fn presets_are_runtime_advertised_with_unique_stable_ids() {
        let presets = available_presets().collect::<Vec<_>>();
        assert!(!presets.is_empty());
        for (index, (id, name)) in presets.iter().enumerate() {
            assert!(!id.is_empty());
            assert!(!name.is_empty());
            assert!(presets[..index].iter().all(|other| other.0 != *id));
        }
    }
}
