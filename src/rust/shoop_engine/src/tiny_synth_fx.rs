use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;
use tinyviolin::midi::MidiMessage;
use tinyviolin::{AudioProcessor, Preset};

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use crate::midi_storage::MidiStorageElem;

pub const MIN_MASTER_GAIN_DB: f32 = -60.0;
pub const MAX_MASTER_GAIN_DB: f32 = 0.0;
pub const DEFAULT_MASTER_GAIN_DB: f32 = -6.0;
pub const MIN_EQ_GAIN_DB: f32 = -12.0;
pub const MAX_EQ_GAIN_DB: f32 = 12.0;
const GAIN_SMOOTH_SECONDS: f32 = 0.02;
const STATE_PREFIX: &str = "shoop-tiny-synth-fx:1:";
const MAX_PROCESSOR_STATE_BYTES: usize = 256 * 1024;
type HostedAudioProcessor = AudioProcessor<32, 1>;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum TinySynthFxParameter {
    MasterGain,
    ReverbAmount,
    DistortionDrive,
    CompressorAmount,
    EqLow,
    EqMid,
    EqHigh,
}

impl TinySynthFxParameter {
    pub const ALL: [Self; 7] = [
        Self::MasterGain,
        Self::ReverbAmount,
        Self::DistortionDrive,
        Self::CompressorAmount,
        Self::EqLow,
        Self::EqMid,
        Self::EqHigh,
    ];

    fn index(self) -> usize {
        self as usize
    }

    pub fn value_from_cc(self, value: u8) -> f32 {
        let normalized = value as f32 / 127.0;
        let (minimum, maximum) = match self {
            Self::MasterGain => (MIN_MASTER_GAIN_DB, MAX_MASTER_GAIN_DB),
            Self::ReverbAmount | Self::CompressorAmount => (0.0, 1.0),
            Self::DistortionDrive => (1.0, 20.0),
            Self::EqLow | Self::EqMid | Self::EqHigh => (MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB),
        };
        minimum + normalized * (maximum - minimum)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TinySynthFxMidiCcAssignment {
    pub parameter: TinySynthFxParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TinySynthFxMidiCcAssignments {
    sources: [Option<(u8, u8)>; 7],
}

impl TinySynthFxMidiCcAssignments {
    pub fn assign(&mut self, assignment: TinySynthFxMidiCcAssignment) -> bool {
        if assignment.channel > 15 || assignment.controller > 127 {
            return false;
        }
        for source in &mut self.sources {
            if *source == Some((assignment.channel, assignment.controller)) {
                *source = None;
            }
        }
        self.sources[assignment.parameter.index()] =
            Some((assignment.channel, assignment.controller));
        true
    }

    pub fn remove(&mut self, parameter: TinySynthFxParameter) {
        self.sources[parameter.index()] = None;
    }

    pub fn clear(&mut self) {
        self.sources.fill(None);
    }

    pub fn iter(&self) -> impl Iterator<Item = TinySynthFxMidiCcAssignment> + '_ {
        TinySynthFxParameter::ALL
            .into_iter()
            .filter_map(|parameter| {
                self.sources[parameter.index()].map(|(channel, controller)| {
                    TinySynthFxMidiCcAssignment {
                        parameter,
                        channel,
                        controller,
                    }
                })
            })
    }

    fn matching_parameter(&self, channel: u8, controller: u8) -> Option<TinySynthFxParameter> {
        TinySynthFxParameter::ALL
            .into_iter()
            .find(|parameter| self.sources[parameter.index()] == Some((channel, controller)))
    }
}

#[derive(Debug)]
struct TinySynthFxRuntimeState {
    values: [AtomicU32; 7],
    revision: AtomicU64,
}

impl TinySynthFxRuntimeState {
    fn new(values: [f32; 7]) -> Self {
        Self {
            values: values.map(|value| AtomicU32::new(value.to_bits())),
            revision: AtomicU64::new(1),
        }
    }

    fn publish(&self, parameter: TinySynthFxParameter, value: f32) -> u64 {
        self.values[parameter.index()].store(value.to_bits(), Ordering::Relaxed);
        self.revision.fetch_add(1, Ordering::Release) + 1
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn values(&self) -> [f32; 7] {
        std::array::from_fn(|index| f32::from_bits(self.values[index].load(Ordering::Relaxed)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TinySynthFxEditorState {
    pub selected_preset_id: Option<String>,
    pub master_gain_db: f32,
    pub reverb_enabled: bool,
    pub reverb_amount: f32,
    pub distortion_enabled: bool,
    pub distortion_drive: f32,
    pub compressor_enabled: bool,
    pub compressor_amount: f32,
    pub eq_enabled: bool,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub midi_cc_assignments: Vec<TinySynthFxMidiCcAssignment>,
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
    midi_cc_assignments: TinySynthFxMidiCcAssignments,
    runtime_state: Arc<TinySynthFxRuntimeState>,
    synchronized_revision: u64,
}

impl TinySynthFxControlState {
    pub fn new(sample_rate: f32) -> Result<Self, tinyviolin::ProcessError> {
        let mut audio = Box::new(HostedAudioProcessor::new(sample_rate, 1)?);
        if let Some(preset) = audio.available_presets().first().copied() {
            audio.select_preset(preset);
        }
        let runtime_state = Arc::new(TinySynthFxRuntimeState::new(control_values(
            DEFAULT_MASTER_GAIN_DB,
            &audio,
        )));
        Ok(Self {
            audio,
            master_gain_db: DEFAULT_MASTER_GAIN_DB,
            midi_cc_assignments: TinySynthFxMidiCcAssignments::default(),
            runtime_state,
            synchronized_revision: 1,
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
        result.publish_all_runtime_values();
        Ok(result)
    }

    pub fn encode(&mut self) -> String {
        self.synchronize_runtime_values();
        encode_state(self.master_gain_db, &self.audio.serialize_state())
    }

    pub fn editor_state(&mut self) -> TinySynthFxEditorState {
        self.synchronize_runtime_values();
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
            compressor_enabled: settings.compressor_enabled,
            compressor_amount: settings.compressor_amount,
            eq_enabled: settings.eq_enabled,
            eq_low_db: settings.eq_low_db,
            eq_mid_db: settings.eq_mid_db,
            eq_high_db: settings.eq_high_db,
            midi_cc_assignments: self.midi_cc_assignments.iter().collect(),
        }
    }

    pub fn select_preset(&mut self, id: &str) -> Result<(), tinyviolin::midi::MidiError> {
        self.audio.select_preset_by_id(id)?;
        self.publish_all_runtime_values();
        Ok(())
    }

    pub fn set_master_gain_db(&mut self, gain_db: f32) -> Result<(), TinySynthFxStateError> {
        validate_gain(gain_db)?;
        self.master_gain_db = gain_db;
        self.synchronized_revision = self
            .runtime_state
            .publish(TinySynthFxParameter::MasterGain, gain_db);
        Ok(())
    }

    pub fn set_reverb_enabled(&mut self, enabled: bool) {
        self.audio.set_reverb_enabled(enabled);
    }

    pub fn set_reverb_amount(&mut self, amount: f32) -> Result<(), tinyviolin::ProcessError> {
        self.audio.set_reverb_amount(amount)?;
        self.synchronized_revision = self
            .runtime_state
            .publish(TinySynthFxParameter::ReverbAmount, amount);
        Ok(())
    }

    pub fn set_distortion_enabled(&mut self, enabled: bool) {
        self.audio.set_distortion_enabled(enabled);
    }

    pub fn set_distortion_drive(&mut self, drive: f32) -> Result<(), tinyviolin::ProcessError> {
        self.audio.set_distortion_drive(drive)?;
        self.synchronized_revision = self
            .runtime_state
            .publish(TinySynthFxParameter::DistortionDrive, drive);
        Ok(())
    }

    pub fn set_compressor_enabled(&mut self, enabled: bool) {
        self.audio.set_compressor_enabled(enabled);
    }

    pub fn set_compressor_amount(&mut self, amount: f32) -> Result<(), tinyviolin::ProcessError> {
        self.audio.set_compressor_amount(amount)?;
        self.synchronized_revision = self
            .runtime_state
            .publish(TinySynthFxParameter::CompressorAmount, amount);
        Ok(())
    }

    pub fn set_eq_enabled(&mut self, enabled: bool) {
        self.audio.set_eq_enabled(enabled);
    }

    pub fn set_eq_low_db(&mut self, gain_db: f32) -> Result<(), tinyviolin::ProcessError> {
        self.audio.set_eq_low_db(gain_db)?;
        self.synchronized_revision = self
            .runtime_state
            .publish(TinySynthFxParameter::EqLow, gain_db);
        Ok(())
    }

    pub fn set_eq_mid_db(&mut self, gain_db: f32) -> Result<(), tinyviolin::ProcessError> {
        self.audio.set_eq_mid_db(gain_db)?;
        self.synchronized_revision = self
            .runtime_state
            .publish(TinySynthFxParameter::EqMid, gain_db);
        Ok(())
    }

    pub fn set_eq_high_db(&mut self, gain_db: f32) -> Result<(), tinyviolin::ProcessError> {
        self.audio.set_eq_high_db(gain_db)?;
        self.synchronized_revision = self
            .runtime_state
            .publish(TinySynthFxParameter::EqHigh, gain_db);
        Ok(())
    }

    pub fn assign_midi_cc(&mut self, assignment: TinySynthFxMidiCcAssignment) -> bool {
        self.midi_cc_assignments.assign(assignment)
    }

    pub fn remove_midi_cc(&mut self, parameter: TinySynthFxParameter) {
        self.midi_cc_assignments.remove(parameter);
    }

    pub fn clear_midi_cc_assignments(&mut self) {
        self.midi_cc_assignments.clear();
    }

    pub fn midi_cc_assignments(&self) -> TinySynthFxMidiCcAssignments {
        self.midi_cc_assignments
    }

    pub fn set_midi_cc_assignments(&mut self, assignments: TinySynthFxMidiCcAssignments) {
        self.midi_cc_assignments = assignments;
    }

    fn publish_all_runtime_values(&mut self) {
        let values = control_values(self.master_gain_db, &self.audio);
        for (parameter, value) in TinySynthFxParameter::ALL.into_iter().zip(values) {
            self.synchronized_revision = self.runtime_state.publish(parameter, value);
        }
    }

    fn synchronize_runtime_values(&mut self) {
        let revision = self.runtime_state.revision();
        if revision == self.synchronized_revision {
            return;
        }
        let values = self.runtime_state.values();
        self.master_gain_db = values[TinySynthFxParameter::MasterGain.index()];
        let _ = self
            .audio
            .set_reverb_amount(values[TinySynthFxParameter::ReverbAmount.index()]);
        let _ = self
            .audio
            .set_distortion_drive(values[TinySynthFxParameter::DistortionDrive.index()]);
        let _ = self
            .audio
            .set_compressor_amount(values[TinySynthFxParameter::CompressorAmount.index()]);
        let _ = self
            .audio
            .set_eq_low_db(values[TinySynthFxParameter::EqLow.index()]);
        let _ = self
            .audio
            .set_eq_mid_db(values[TinySynthFxParameter::EqMid.index()]);
        let _ = self
            .audio
            .set_eq_high_db(values[TinySynthFxParameter::EqHigh.index()]);
        self.synchronized_revision = revision;
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
            self.midi_cc_assignments,
            Arc::clone(&self.runtime_state),
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
    midi_cc_assignments: TinySynthFxMidiCcAssignments,
    runtime_state: Arc<TinySynthFxRuntimeState>,
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
        midi_cc_assignments: TinySynthFxMidiCcAssignments,
        runtime_state: Arc<TinySynthFxRuntimeState>,
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
            midi_cc_assignments,
            runtime_state,
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
        let _span =
            shoop_tracing::realtime_span_detail!("engine.rt.fx.tiny_synth_process", value = frames);
        let frames = frames.min(self.planes.max_frames);
        let mut cursor = 0;
        for event in events {
            let offset = (event.time as usize).min(frames).max(cursor);
            let _ = self
                .audio
                .render_range(&mut self.planes.planes, cursor..offset);
            self.apply_gain(cursor, offset);
            self.apply_midi_cc(event.data());
            if let Ok(message) = MidiMessage::new(event.data()) {
                let _ = self.audio.dispatch_midi(message);
            }
            cursor = offset;
        }
        let _ = self
            .audio
            .render_range(&mut self.planes.planes, cursor..frames);
        self.apply_gain(cursor, frames);
    }

    fn apply_gain(&mut self, start: usize, end: usize) {
        for frame in start..end {
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

    pub fn process_midi_controls_only(&mut self, events: &[MidiStorageElem]) {
        for event in events {
            self.apply_midi_cc(event.data());
        }
    }

    pub fn assign_midi_cc(&mut self, assignment: TinySynthFxMidiCcAssignment) -> bool {
        self.midi_cc_assignments.assign(assignment)
    }

    pub fn remove_midi_cc(&mut self, parameter: TinySynthFxParameter) {
        self.midi_cc_assignments.remove(parameter);
    }

    pub fn clear_midi_cc_assignments(&mut self) {
        self.midi_cc_assignments.clear();
    }

    fn apply_midi_cc(&mut self, data: &[u8]) {
        if data.len() != 3 || data[0] & 0xf0 != 0xb0 || data[1] > 127 || data[2] > 127 {
            return;
        }
        let Some(parameter) = self
            .midi_cc_assignments
            .matching_parameter(data[0] & 0x0f, data[1])
        else {
            return;
        };
        let value = parameter.value_from_cc(data[2]);
        match parameter {
            TinySynthFxParameter::MasterGain => self.set_master_gain_db(value),
            TinySynthFxParameter::ReverbAmount => self.set_reverb_amount(value),
            TinySynthFxParameter::DistortionDrive => self.set_distortion_drive(value),
            TinySynthFxParameter::CompressorAmount => self.set_compressor_amount(value),
            TinySynthFxParameter::EqLow => self.set_eq_low_db(value),
            TinySynthFxParameter::EqMid => self.set_eq_mid_db(value),
            TinySynthFxParameter::EqHigh => self.set_eq_high_db(value),
        }
        self.runtime_state.publish(parameter, value);
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

    pub fn set_compressor_enabled(&mut self, enabled: bool) {
        self.audio.set_compressor_enabled(enabled);
    }

    pub fn set_compressor_amount(&mut self, amount: f32) {
        let _ = self.audio.set_compressor_amount(amount);
    }

    pub fn set_eq_enabled(&mut self, enabled: bool) {
        self.audio.set_eq_enabled(enabled);
    }

    pub fn set_eq_low_db(&mut self, gain_db: f32) {
        let _ = self.audio.set_eq_low_db(gain_db);
    }

    pub fn set_eq_mid_db(&mut self, gain_db: f32) {
        let _ = self.audio.set_eq_mid_db(gain_db);
    }

    pub fn set_eq_high_db(&mut self, gain_db: f32) {
        let _ = self.audio.set_eq_high_db(gain_db);
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

fn control_values(master_gain_db: f32, audio: &HostedAudioProcessor) -> [f32; 7] {
    let settings = audio.effect_settings();
    [
        master_gain_db,
        settings.reverb_amount,
        settings.distortion_drive,
        settings.compressor_amount,
        settings.eq_low_db,
        settings.eq_mid_db,
        settings.eq_high_db,
    ]
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
    use assert2::check;

    #[test]
    fn control_state_round_trips_library_state_and_gain() {
        let mut source = TinySynthFxControlState::new(48_000.0).unwrap();
        source.select_preset("pad").unwrap();
        source.set_master_gain_db(-12.5).unwrap();
        source.set_reverb_enabled(true);
        source.set_reverb_amount(0.4).unwrap();
        source.set_distortion_enabled(true);
        source.set_distortion_drive(8.0).unwrap();
        source.set_compressor_enabled(true);
        source.set_compressor_amount(0.6).unwrap();
        source.set_eq_enabled(true);
        source.set_eq_low_db(3.0).unwrap();
        source.set_eq_mid_db(-2.0).unwrap();
        source.set_eq_high_db(1.5).unwrap();
        let encoded = source.encode();
        let mut restored = TinySynthFxControlState::from_encoded(44_100.0, &encoded).unwrap();
        assert_eq!(restored.editor_state(), source.editor_state());
        assert_eq!(restored.encode(), encoded);
        for (channels, max_frames) in [(0, 17), (1, 64), (7, 257)] {
            let processor = restored
                .prepare_processor(44_100.0, channels, max_frames)
                .unwrap();
            assert_eq!(processor.logical_channel_count(), channels);
            assert_eq!(processor.max_frames(), max_frames);
        }
    }

    #[test]
    fn midi_cc_assignments_replace_source_and_target_conflicts() {
        let mut assignments = TinySynthFxMidiCcAssignments::default();
        check!(assignments.assign(TinySynthFxMidiCcAssignment {
            parameter: TinySynthFxParameter::MasterGain,
            channel: 2,
            controller: 7,
        }));
        check!(assignments.assign(TinySynthFxMidiCcAssignment {
            parameter: TinySynthFxParameter::ReverbAmount,
            channel: 2,
            controller: 7,
        }));
        check!(
            assignments.iter().collect::<Vec<_>>()
                == [TinySynthFxMidiCcAssignment {
                    parameter: TinySynthFxParameter::ReverbAmount,
                    channel: 2,
                    controller: 7,
                }]
        );
        check!(assignments.assign(TinySynthFxMidiCcAssignment {
            parameter: TinySynthFxParameter::ReverbAmount,
            channel: 3,
            controller: 8,
        }));
        check!(
            assignments.iter().collect::<Vec<_>>()
                == [TinySynthFxMidiCcAssignment {
                    parameter: TinySynthFxParameter::ReverbAmount,
                    channel: 3,
                    controller: 8,
                }]
        );
        check!(!assignments.assign(TinySynthFxMidiCcAssignment {
            parameter: TinySynthFxParameter::EqLow,
            channel: 16,
            controller: 1,
        }));
    }

    #[test]
    fn midi_cc_controls_every_continuous_parameter_and_updates_control_state() {
        for parameter in TinySynthFxParameter::ALL {
            for cc_value in [0, 63, 127] {
                let mut control = TinySynthFxControlState::new(48_000.0).unwrap();
                check!(control.assign_midi_cc(TinySynthFxMidiCcAssignment {
                    parameter,
                    channel: 5,
                    controller: 17,
                }));
                let mut processor = control.prepare_processor(48_000.0, 1, 4).unwrap();
                let event = MidiStorageElem::new(2, &[0xb5, 17, cc_value]).unwrap();
                processor.process(4, &[event]);
                let editor = control.editor_state();
                let actual = match parameter {
                    TinySynthFxParameter::MasterGain => editor.master_gain_db,
                    TinySynthFxParameter::ReverbAmount => editor.reverb_amount,
                    TinySynthFxParameter::DistortionDrive => editor.distortion_drive,
                    TinySynthFxParameter::CompressorAmount => editor.compressor_amount,
                    TinySynthFxParameter::EqLow => editor.eq_low_db,
                    TinySynthFxParameter::EqMid => editor.eq_mid_db,
                    TinySynthFxParameter::EqHigh => editor.eq_high_db,
                };
                check!((actual - parameter.value_from_cc(cc_value)).abs() < 1.0e-6);
            }
        }
    }

    #[test]
    fn mapped_master_gain_starts_at_the_cc_sample_offset() {
        let mut control = TinySynthFxControlState::new(48_000.0).unwrap();
        control.assign_midi_cc(TinySynthFxMidiCcAssignment {
            parameter: TinySynthFxParameter::MasterGain,
            channel: 0,
            controller: 7,
        });
        let mut processor = control.prepare_processor(48_000.0, 1, 8).unwrap();
        processor.plane_mut(0, 8).unwrap().fill(1.0);
        processor.process(8, &[MidiStorageElem::new(4, &[0xb0, 7, 0]).unwrap()]);
        let output = processor.plane(0, 8).unwrap();
        let initial_gain = db_to_gain(DEFAULT_MASTER_GAIN_DB);
        check!(output[..4]
            .iter()
            .all(|sample| (*sample - initial_gain).abs() < 1.0e-6));
        check!(output[4] < initial_gain);
    }

    #[test]
    fn midi_cc_mapping_requires_an_exact_channel_and_controller_match() {
        let mut control = TinySynthFxControlState::new(48_000.0).unwrap();
        control.assign_midi_cc(TinySynthFxMidiCcAssignment {
            parameter: TinySynthFxParameter::ReverbAmount,
            channel: 3,
            controller: 12,
        });
        let initial = control.editor_state().reverb_amount;
        let mut processor = control.prepare_processor(48_000.0, 1, 4).unwrap();
        for data in [[0xb2, 12, 127], [0xb3, 13, 127], [0x93, 12, 127]] {
            processor.process_midi_controls_only(&[MidiStorageElem::new(0, &data).unwrap()]);
        }
        check!(control.editor_state().reverb_amount == initial);
        processor.process_midi_controls_only(&[MidiStorageElem::new(0, &[0xb3, 12, 127]).unwrap()]);
        let editor = control.editor_state();
        check!(editor.reverb_amount == 1.0);
        check!(editor.selected_preset_id.is_none());
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

        let mut panicked = control.prepare_processor(48_000.0, 1, 64).unwrap();
        panicked.process(64, &[MidiStorageElem::new(0, &[0x90, 69, 127]).unwrap()]);
        assert!(panicked
            .plane(0, 64)
            .unwrap()
            .iter()
            .any(|sample| sample.abs() > 0.001));
        panicked.panic();
        panicked.plane_mut(0, 64).unwrap().fill(0.0);
        panicked.process(64, &[]);
        assert!(panicked
            .plane(0, 64)
            .unwrap()
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
        processor.set_compressor_enabled(true);
        processor.set_compressor_amount(0.6);
        processor.set_eq_enabled(true);
        processor.set_eq_low_db(3.0);
        processor.set_eq_mid_db(-2.0);
        processor.set_eq_high_db(1.5);
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
    fn pitch_bend_and_modulation_wheel_reach_tinyviolin() {
        let control = TinySynthFxControlState::new(48_000.0).unwrap();
        let note_on = MidiStorageElem::new(0, &[0x90, 69, 127]).unwrap();

        let mut centered = control.prepare_processor(48_000.0, 1, 256).unwrap();
        centered.process(256, std::slice::from_ref(&note_on));
        let centered = centered.plane(0, 256).unwrap().to_vec();

        let mut bent = control.prepare_processor(48_000.0, 1, 256).unwrap();
        bent.process(
            256,
            &[
                MidiStorageElem::new(0, &[0xE0, 0x7F, 0x7F]).unwrap(),
                note_on.clone(),
            ],
        );
        assert!(bent
            .plane(0, 256)
            .unwrap()
            .iter()
            .zip(&centered)
            .any(|(bent, centered)| (*bent - *centered).abs() > 1.0e-4));

        let mut modulated = control.prepare_processor(48_000.0, 1, 256).unwrap();
        modulated.process(
            256,
            &[MidiStorageElem::new(0, &[0xB0, 1, 127]).unwrap(), note_on],
        );
        assert!(modulated
            .plane(0, 256)
            .unwrap()
            .iter()
            .zip(&centered)
            .any(|(modulated, centered)| (*modulated - *centered).abs() > 1.0e-4));
    }

    #[test]
    fn master_gain_changes_are_smoothed_to_the_exact_target() {
        let control = TinySynthFxControlState::new(48_000.0).unwrap();
        let mut processor = control.prepare_processor(48_000.0, 1, 128).unwrap();
        processor.set_master_gain_db(-60.0);
        processor.plane_mut(0, 128).unwrap().fill(1.0);
        processor.process(128, &[]);
        let first_block = processor.plane(0, 128).unwrap();
        let target = db_to_gain(-60.0);
        assert!(first_block[0] < db_to_gain(DEFAULT_MASTER_GAIN_DB));
        assert!(first_block[0] > target);
        assert!(first_block.windows(2).all(|pair| pair[1] < pair[0]));
        assert!(first_block[127] > target);

        for _ in 0..8 {
            processor.plane_mut(0, 128).unwrap().fill(1.0);
            processor.process(128, &[]);
        }
        assert!(processor
            .plane(0, 128)
            .unwrap()
            .iter()
            .all(|sample| (*sample - target).abs() < 1.0e-6));
    }

    #[test]
    fn sustained_variable_block_processing_remains_finite_and_active() {
        let control = TinySynthFxControlState::new(48_000.0).unwrap();
        let mut processor = control.prepare_processor(48_000.0, 2, 128).unwrap();
        let block_sizes = [1, 7, 31, 64, 127, 128, 3, 96];
        let mut observed_signal = false;
        for iteration in 0..2_000 {
            let frames = block_sizes[iteration % block_sizes.len()];
            processor.plane_mut(0, frames).unwrap().fill(0.01);
            processor.plane_mut(1, frames).unwrap().fill(-0.02);
            let event = match iteration {
                0 => Some(MidiStorageElem::new(0, &[0x90, 69, 127]).unwrap()),
                1_000 => Some(MidiStorageElem::new(0, &[0x80, 69, 0]).unwrap()),
                _ => None,
            };
            processor.process(frames, event.as_slice());
            for channel in 0..2 {
                let output = processor.plane(channel, frames).unwrap();
                assert!(output.iter().all(|sample| sample.is_finite()));
                observed_signal |= output.iter().any(|sample| sample.abs() > 0.001);
            }
        }
        assert!(observed_signal);
    }

    #[test]
    fn presets_are_runtime_advertised_with_unique_stable_ids() {
        let presets = available_presets().collect::<Vec<_>>();
        assert!(presets.len() >= 12);
        assert!(presets.iter().any(|preset| preset.0 == "pluck"));
        for (index, (id, name)) in presets.iter().enumerate() {
            assert!(!id.is_empty());
            assert!(!name.is_empty());
            assert!(presets[..index].iter().all(|other| other.0 != *id));
        }
    }
}
