use anyhow::{Context, Result};
use oxisynth::{GeneratorType, MidiEvent, SoundFont, SoundFontId, Synth, SynthDescriptor};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;

use crate::midi_storage::MidiStorageElem;

pub const SOUNDFONT_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../third_party/timgm6mb/TimGM6mb.sf2"
));
pub const SOUNDFONT_SHA256: &str =
    "c5378b62028c920cb11e4803327983fee2f2cdff5dc89c708e39da417e51c854";
pub const SOUNDFONT_ID: &str = "timgm6mb";
pub const POLYPHONY: u16 = 256;
pub const DEFAULT_PRESET: OxiSynthPresetId = OxiSynthPresetId {
    bank: 0,
    program: 0,
};
const STATE_FORMAT: &str = "shoop-oxisynth";
const STATE_VERSION: &str = "2";
pub const MIN_SEND: f32 = 0.0;
pub const MAX_SEND: f32 = 1.0;
const MAX_SEND_GENERATOR_UNITS: f32 = 200.0;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OxiSynthPresetId {
    pub bank: u16,
    pub program: u8,
}

impl OxiSynthPresetId {
    pub fn from_stable_id(value: &str) -> Result<Self, OxiSynthStateError> {
        let mut fields = value.split(':');
        let bank = fields
            .next()
            .ok_or(OxiSynthStateError::InvalidPresetId)?
            .parse::<u16>()
            .map_err(|_| OxiSynthStateError::InvalidPresetId)?;
        let program = fields
            .next()
            .ok_or(OxiSynthStateError::InvalidPresetId)?
            .parse::<u8>()
            .map_err(|_| OxiSynthStateError::InvalidPresetId)?;
        if fields.next().is_some() || program > 127 {
            return Err(OxiSynthStateError::InvalidPresetId);
        }
        let id = Self { bank, program };
        validate_preset(id)?;
        if id.stable_id() != value {
            return Err(OxiSynthStateError::InvalidPresetId);
        }
        Ok(id)
    }

    pub fn stable_id(self) -> String {
        format!("{}:{}", self.bank, self.program)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OxiSynthPresetDescriptor {
    pub id: OxiSynthPresetId,
    pub name: &'static str,
}

const PRESETS: &[OxiSynthPresetDescriptor] =
    include!(concat!(env!("OUT_DIR"), "/oxisynth_presets.rs"));

pub fn available_presets() -> &'static [OxiSynthPresetDescriptor] {
    PRESETS
}

pub fn preset_descriptor(id: OxiSynthPresetId) -> Option<&'static OxiSynthPresetDescriptor> {
    PRESETS
        .binary_search_by_key(&id, |preset| preset.id)
        .ok()
        .map(|index| &PRESETS[index])
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum OxiSynthParameter {
    ReverbSend,
    ChorusSend,
}

impl OxiSynthParameter {
    pub const ALL: [Self; 2] = [Self::ReverbSend, Self::ChorusSend];

    fn index(self) -> usize {
        self as usize
    }

    pub fn value_from_cc(value: u8) -> f32 {
        value as f32 / 127.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OxiSynthMidiCcAssignment {
    pub parameter: OxiSynthParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OxiSynthMidiCcAssignments {
    sources: [Option<(u8, u8)>; 2],
}

impl OxiSynthMidiCcAssignments {
    pub fn assign(&mut self, assignment: OxiSynthMidiCcAssignment) -> bool {
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

    pub fn remove(&mut self, parameter: OxiSynthParameter) {
        self.sources[parameter.index()] = None;
    }

    pub fn clear(&mut self) {
        self.sources.fill(None);
    }

    pub fn iter(&self) -> impl Iterator<Item = OxiSynthMidiCcAssignment> + '_ {
        OxiSynthParameter::ALL.into_iter().filter_map(|parameter| {
            self.sources[parameter.index()].map(|(channel, controller)| OxiSynthMidiCcAssignment {
                parameter,
                channel,
                controller,
            })
        })
    }

    fn matching_parameter(&self, channel: u8, controller: u8) -> Option<OxiSynthParameter> {
        OxiSynthParameter::ALL
            .into_iter()
            .find(|parameter| self.sources[parameter.index()] == Some((channel, controller)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OxiSynthState {
    pub preset: OxiSynthPresetId,
    pub reverb_send: f32,
    pub chorus_send: f32,
}

impl Default for OxiSynthState {
    fn default() -> Self {
        Self {
            preset: DEFAULT_PRESET,
            reverb_send: 0.0,
            chorus_send: 0.0,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum OxiSynthStateError {
    #[error("invalid OxiSynth preset ID")]
    InvalidPresetId,
    #[error("invalid OxiSynth state envelope")]
    InvalidEnvelope,
    #[error("unsupported OxiSynth state version {0}")]
    UnsupportedVersion(String),
    #[error("unknown OxiSynth SoundFont {0}")]
    UnknownSoundFont(String),
    #[error("invalid OxiSynth preset bank")]
    InvalidBank,
    #[error("invalid OxiSynth preset program")]
    InvalidProgram,
    #[error("unknown OxiSynth preset {bank}:{program}")]
    UnknownPreset { bank: u16, program: u8 },
    #[error("invalid OxiSynth send value")]
    InvalidSend,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OxiSynthEditorState {
    pub selected_preset: OxiSynthPresetId,
    pub reverb_send: f32,
    pub chorus_send: f32,
    pub midi_cc_assignments: Vec<OxiSynthMidiCcAssignment>,
}

#[derive(Debug)]
struct OxiSynthRuntimeState {
    values: [AtomicU32; 2],
    revision: AtomicU64,
}

impl OxiSynthRuntimeState {
    fn new(state: OxiSynthState) -> Self {
        Self {
            values: [
                AtomicU32::new(state.reverb_send.to_bits()),
                AtomicU32::new(state.chorus_send.to_bits()),
            ],
            revision: AtomicU64::new(1),
        }
    }

    fn publish(&self, parameter: OxiSynthParameter, value: f32) -> u64 {
        self.values[parameter.index()].store(value.to_bits(), Ordering::Relaxed);
        self.revision.fetch_add(1, Ordering::Release) + 1
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn value(&self, parameter: OxiSynthParameter) -> f32 {
        f32::from_bits(self.values[parameter.index()].load(Ordering::Relaxed))
    }
}

#[derive(Debug)]
pub struct OxiSynthControlState {
    state: OxiSynthState,
    midi_cc_assignments: OxiSynthMidiCcAssignments,
    runtime_state: Arc<OxiSynthRuntimeState>,
}

impl Clone for OxiSynthControlState {
    fn clone(&self) -> Self {
        let state = self.current_state();
        Self {
            state,
            midi_cc_assignments: self.midi_cc_assignments,
            runtime_state: Arc::new(OxiSynthRuntimeState::new(state)),
        }
    }
}

impl Default for OxiSynthControlState {
    fn default() -> Self {
        let state = OxiSynthState::default();
        Self {
            state,
            midi_cc_assignments: OxiSynthMidiCcAssignments::default(),
            runtime_state: Arc::new(OxiSynthRuntimeState::new(state)),
        }
    }
}

impl OxiSynthControlState {
    pub fn from_state(state: OxiSynthState) -> Result<Self, OxiSynthStateError> {
        validate_state(state)?;
        Ok(Self {
            state,
            midi_cc_assignments: OxiSynthMidiCcAssignments::default(),
            runtime_state: Arc::new(OxiSynthRuntimeState::new(state)),
        })
    }

    pub fn from_encoded(encoded: &str) -> Result<Self, OxiSynthStateError> {
        let mut fields = encoded.split(':');
        if fields.next() != Some(STATE_FORMAT) {
            return Err(OxiSynthStateError::InvalidEnvelope);
        }
        let version = fields.next().ok_or(OxiSynthStateError::InvalidEnvelope)?;
        if version != STATE_VERSION {
            return Err(OxiSynthStateError::UnsupportedVersion(version.to_owned()));
        }
        let soundfont = fields.next().ok_or(OxiSynthStateError::InvalidEnvelope)?;
        if soundfont != SOUNDFONT_ID {
            return Err(OxiSynthStateError::UnknownSoundFont(soundfont.to_owned()));
        }
        let bank = fields
            .next()
            .ok_or(OxiSynthStateError::InvalidEnvelope)?
            .parse::<u16>()
            .map_err(|_| OxiSynthStateError::InvalidBank)?;
        let program = fields
            .next()
            .ok_or(OxiSynthStateError::InvalidEnvelope)?
            .parse::<u8>()
            .map_err(|_| OxiSynthStateError::InvalidProgram)?;
        let reverb_send = decode_send(fields.next())?;
        let chorus_send = decode_send(fields.next())?;
        if fields.next().is_some() {
            return Err(OxiSynthStateError::InvalidEnvelope);
        }
        if program > 127 {
            return Err(OxiSynthStateError::InvalidProgram);
        }
        let control = Self::from_state(OxiSynthState {
            preset: OxiSynthPresetId { bank, program },
            reverb_send,
            chorus_send,
        })?;
        if control.encode() != encoded {
            return Err(OxiSynthStateError::InvalidEnvelope);
        }
        Ok(control)
    }

    pub fn state(&self) -> OxiSynthState {
        self.current_state()
    }

    pub fn selected_preset(&self) -> OxiSynthPresetId {
        self.state.preset
    }

    pub fn editor_state(&self) -> OxiSynthEditorState {
        let state = self.current_state();
        OxiSynthEditorState {
            selected_preset: state.preset,
            reverb_send: state.reverb_send,
            chorus_send: state.chorus_send,
            midi_cc_assignments: self.midi_cc_assignments.iter().collect(),
        }
    }

    pub fn select_preset(&mut self, preset: OxiSynthPresetId) -> Result<(), OxiSynthStateError> {
        validate_preset(preset)?;
        self.state.preset = preset;
        Ok(())
    }

    pub fn set_send(
        &mut self,
        parameter: OxiSynthParameter,
        value: f32,
    ) -> Result<(), OxiSynthStateError> {
        validate_send(value)?;
        match parameter {
            OxiSynthParameter::ReverbSend => self.state.reverb_send = value,
            OxiSynthParameter::ChorusSend => self.state.chorus_send = value,
        }
        self.runtime_state.publish(parameter, value);
        Ok(())
    }

    pub fn assign_midi_cc(&mut self, assignment: OxiSynthMidiCcAssignment) -> bool {
        self.midi_cc_assignments.assign(assignment)
    }

    pub fn remove_midi_cc(&mut self, parameter: OxiSynthParameter) {
        self.midi_cc_assignments.remove(parameter);
    }

    pub fn clear_midi_cc_assignments(&mut self) {
        self.midi_cc_assignments.clear();
    }

    pub fn midi_cc_assignments(&self) -> OxiSynthMidiCcAssignments {
        self.midi_cc_assignments
    }

    pub fn set_midi_cc_assignments(&mut self, assignments: OxiSynthMidiCcAssignments) {
        self.midi_cc_assignments = assignments;
    }

    pub fn encode(&self) -> String {
        let state = self.current_state();
        format!(
            "{STATE_FORMAT}:{STATE_VERSION}:{SOUNDFONT_ID}:{}:{}:{:08x}:{:08x}",
            state.preset.bank,
            state.preset.program,
            state.reverb_send.to_bits(),
            state.chorus_send.to_bits()
        )
    }

    pub fn prepare_processor(
        &self,
        sample_rate: f32,
        max_frames: usize,
    ) -> Result<OxiSynthProcessor> {
        OxiSynthProcessor::new_with_runtime(
            sample_rate,
            max_frames,
            self.current_state(),
            self.midi_cc_assignments,
            Arc::clone(&self.runtime_state),
        )
    }

    fn current_state(&self) -> OxiSynthState {
        OxiSynthState {
            preset: self.state.preset,
            reverb_send: self.runtime_state.value(OxiSynthParameter::ReverbSend),
            chorus_send: self.runtime_state.value(OxiSynthParameter::ChorusSend),
        }
    }
}

fn decode_send(value: Option<&str>) -> Result<f32, OxiSynthStateError> {
    let value = value.ok_or(OxiSynthStateError::InvalidEnvelope)?;
    if value.len() != 8 {
        return Err(OxiSynthStateError::InvalidEnvelope);
    }
    let bits = u32::from_str_radix(value, 16).map_err(|_| OxiSynthStateError::InvalidEnvelope)?;
    let value = f32::from_bits(bits);
    validate_send(value)?;
    Ok(value)
}

fn validate_state(state: OxiSynthState) -> Result<(), OxiSynthStateError> {
    validate_preset(state.preset)?;
    validate_send(state.reverb_send)?;
    validate_send(state.chorus_send)
}

fn validate_send(value: f32) -> Result<(), OxiSynthStateError> {
    if value.is_finite() && (MIN_SEND..=MAX_SEND).contains(&value) {
        Ok(())
    } else {
        Err(OxiSynthStateError::InvalidSend)
    }
}

fn validate_preset(preset: OxiSynthPresetId) -> Result<(), OxiSynthStateError> {
    if preset.program > 127 {
        return Err(OxiSynthStateError::InvalidProgram);
    }
    if preset_descriptor(preset).is_none() {
        return Err(OxiSynthStateError::UnknownPreset {
            bank: preset.bank,
            program: preset.program,
        });
    }
    Ok(())
}

fn create_synth(sample_rate: f32, state: OxiSynthState) -> Result<(Synth, SoundFontId)> {
    validate_state(state).context("validate OxiSynth state")?;
    let mut bytes = Cursor::new(SOUNDFONT_BYTES);
    let font = SoundFont::load(&mut bytes).context("parse embedded TimGM6mb SoundFont")?;
    let mut synth = Synth::new(SynthDescriptor {
        sample_rate,
        polyphony: POLYPHONY,
        midi_channels: 16,
        reverb_active: true,
        chorus_active: true,
        drums_channel_active: false,
        audio_channels: 1,
        audio_groups: 1,
        ..SynthDescriptor::default()
    })
    .context("configure OxiSynth")?;
    let soundfont_id = synth.add_font(font, false);
    synth
        .select_program(
            0,
            soundfont_id,
            u32::from(state.preset.bank),
            state.preset.program,
        )
        .context("select OxiSynth preset")?;
    apply_send(&mut synth, OxiSynthParameter::ReverbSend, state.reverb_send)
        .context("set OxiSynth reverb send")?;
    apply_send(&mut synth, OxiSynthParameter::ChorusSend, state.chorus_send)
        .context("set OxiSynth chorus send")?;
    Ok((synth, soundfont_id))
}

fn apply_send(
    synth: &mut Synth,
    parameter: OxiSynthParameter,
    value: f32,
) -> Result<(), oxisynth::OxiError> {
    let generator = match parameter {
        OxiSynthParameter::ReverbSend => GeneratorType::ReverbSend,
        OxiSynthParameter::ChorusSend => GeneratorType::ChorusSend,
    };
    synth.set_gen(0, generator, value * MAX_SEND_GENERATOR_UNITS)
}

pub struct OxiSynthProcessor {
    synth: Synth,
    soundfont_id: SoundFontId,
    selected_preset: OxiSynthPresetId,
    reverb_send: f32,
    chorus_send: f32,
    midi_cc_assignments: OxiSynthMidiCcAssignments,
    runtime_state: Arc<OxiSynthRuntimeState>,
    synchronized_revision: u64,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl std::fmt::Debug for OxiSynthProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxiSynthProcessor")
            .field("selected_preset", &self.selected_preset)
            .field("max_frames", &self.left.len())
            .finish_non_exhaustive()
    }
}

impl OxiSynthProcessor {
    pub fn new(sample_rate: f32, max_frames: usize, state: OxiSynthState) -> Result<Self> {
        let runtime_state = Arc::new(OxiSynthRuntimeState::new(state));
        Self::new_with_runtime(
            sample_rate,
            max_frames,
            state,
            OxiSynthMidiCcAssignments::default(),
            runtime_state,
        )
    }

    fn new_with_runtime(
        sample_rate: f32,
        max_frames: usize,
        state: OxiSynthState,
        midi_cc_assignments: OxiSynthMidiCcAssignments,
        runtime_state: Arc<OxiSynthRuntimeState>,
    ) -> Result<Self> {
        let max_frames = max_frames.max(1);
        let (synth, soundfont_id) = create_synth(sample_rate, state)?;
        Ok(Self {
            synth,
            soundfont_id,
            selected_preset: state.preset,
            reverb_send: state.reverb_send,
            chorus_send: state.chorus_send,
            midi_cc_assignments,
            runtime_state,
            synchronized_revision: 1,
            left: vec![0.0; max_frames],
            right: vec![0.0; max_frames],
        })
    }

    pub fn selected_preset(&self) -> OxiSynthPresetId {
        self.selected_preset
    }

    pub fn select_preset(&mut self, preset: OxiSynthPresetId) -> Result<()> {
        validate_preset(preset).context("validate OxiSynth preset")?;
        self.stop_voices();
        self.synth
            .select_program(0, self.soundfont_id, u32::from(preset.bank), preset.program)
            .context("select OxiSynth preset")?;
        self.selected_preset = preset;
        self.reapply_sends();
        Ok(())
    }

    pub fn set_send(
        &mut self,
        parameter: OxiSynthParameter,
        value: f32,
    ) -> Result<(), OxiSynthStateError> {
        validate_send(value)?;
        self.apply_send_value(parameter, value);
        self.synchronized_revision = self.runtime_state.publish(parameter, value);
        Ok(())
    }

    pub fn assign_midi_cc(&mut self, assignment: OxiSynthMidiCcAssignment) -> bool {
        self.midi_cc_assignments.assign(assignment)
    }

    pub fn remove_midi_cc(&mut self, parameter: OxiSynthParameter) {
        self.midi_cc_assignments.remove(parameter);
    }

    pub fn clear_midi_cc_assignments(&mut self) {
        self.midi_cc_assignments.clear();
    }

    pub fn max_frames(&self) -> usize {
        self.left.len()
    }

    pub fn output(&self, channel: usize, frames: usize) -> Option<&[f32]> {
        let frames = frames.min(self.max_frames());
        match channel {
            0 => Some(&self.left[..frames]),
            1 => Some(&self.right[..frames]),
            _ => None,
        }
    }

    pub fn clear(&mut self, frames: usize) {
        let frames = frames.min(self.max_frames());
        self.left[..frames].fill(0.0);
        self.right[..frames].fill(0.0);
    }

    pub fn reset(&mut self) {
        self.stop_voices();
        let _ = self.synth.select_program(
            0,
            self.soundfont_id,
            u32::from(self.selected_preset.bank),
            self.selected_preset.program,
        );
        self.reapply_sends();
    }

    pub fn panic(&mut self) {
        self.stop_voices();
    }

    pub fn process(&mut self, frames: usize, events: &[MidiStorageElem]) {
        let _span =
            shoop_tracing::realtime_span_detail!("engine.rt.fx.oxisynth_process", value = frames);
        self.synchronize_runtime_values();
        let frames = frames.min(self.max_frames());
        let mut cursor = 0;
        for event in events {
            let offset = (event.time as usize).min(frames).max(cursor);
            self.render(cursor, offset);
            self.apply_midi_cc(event.data());
            if let Some(event) = translate_midi(event.data()) {
                match event {
                    MidiEvent::SystemReset => self.reset(),
                    event => {
                        let _ = self.synth.send_event(event);
                    }
                }
            }
            cursor = offset;
        }
        self.render(cursor, frames);
    }

    pub fn process_midi_controls_only(&mut self, events: &[MidiStorageElem]) {
        self.synchronize_runtime_values();
        for event in events {
            self.apply_midi_cc(event.data());
        }
    }

    fn stop_voices(&mut self) {
        let _ = self.synth.send_event(MidiEvent::AllSoundOff { channel: 0 });
    }

    fn reapply_sends(&mut self) {
        let _ = apply_send(
            &mut self.synth,
            OxiSynthParameter::ReverbSend,
            self.reverb_send,
        );
        let _ = apply_send(
            &mut self.synth,
            OxiSynthParameter::ChorusSend,
            self.chorus_send,
        );
    }

    fn apply_send_value(&mut self, parameter: OxiSynthParameter, value: f32) {
        let _ = apply_send(&mut self.synth, parameter, value);
        match parameter {
            OxiSynthParameter::ReverbSend => self.reverb_send = value,
            OxiSynthParameter::ChorusSend => self.chorus_send = value,
        }
    }

    fn synchronize_runtime_values(&mut self) {
        let revision = self.runtime_state.revision();
        if revision == self.synchronized_revision {
            return;
        }
        for parameter in OxiSynthParameter::ALL {
            let value = self.runtime_state.value(parameter);
            self.apply_send_value(parameter, value);
        }
        self.synchronized_revision = revision;
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
        let value = OxiSynthParameter::value_from_cc(data[2]);
        self.apply_send_value(parameter, value);
        self.synchronized_revision = self.runtime_state.publish(parameter, value);
    }

    fn render(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        self.synth
            .write((&mut self.left[start..end], &mut self.right[start..end]));
    }
}

fn translate_midi(data: &[u8]) -> Option<MidiEvent> {
    let status = *data.first()?;
    if status == 0xff && data.len() == 1 {
        return Some(MidiEvent::SystemReset);
    }
    let channel = 0;
    match status & 0xf0 {
        0x80 if data.len() == 3 && data[1] <= 127 && data[2] <= 127 => Some(MidiEvent::NoteOff {
            channel,
            key: data[1],
        }),
        0x90 if data.len() == 3 && data[1] <= 127 && data[2] <= 127 => {
            if data[2] == 0 {
                Some(MidiEvent::NoteOff {
                    channel,
                    key: data[1],
                })
            } else {
                Some(MidiEvent::NoteOn {
                    channel,
                    key: data[1],
                    vel: data[2],
                })
            }
        }
        0xa0 if data.len() == 3 && data[1] <= 127 && data[2] <= 127 => {
            Some(MidiEvent::PolyphonicKeyPressure {
                channel,
                key: data[1],
                value: data[2],
            })
        }
        0xb0 if data.len() == 3 && data[1] <= 127 && data[2] <= 127 => match data[1] {
            1 | 11 | 64 => Some(MidiEvent::ControlChange {
                channel,
                ctrl: data[1],
                value: data[2],
            }),
            _ => None,
        },
        0xc0 if data.len() == 2 && data[1] <= 127 => None,
        0xd0 if data.len() == 2 && data[1] <= 127 => Some(MidiEvent::ChannelPressure {
            channel,
            value: data[1],
        }),
        0xe0 if data.len() == 3 && data[1] <= 127 && data[2] <= 127 => Some(MidiEvent::PitchBend {
            channel,
            value: u16::from(data[1]) | (u16::from(data[2]) << 7),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxisynth::MidiEvent;
    use sha2::{Digest, Sha256};

    #[shoop_wasm_test_support::shoop_test]
    fn embedded_soundfont_has_expected_digest_and_renders_stereo() {
        let digest = Sha256::digest(SOUNDFONT_BYTES);
        let digest = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(digest, SOUNDFONT_SHA256);

        let (mut synth, _) = create_synth(48_000.0, OxiSynthState::default()).unwrap();
        synth
            .send_event(MidiEvent::NoteOn {
                channel: 0,
                key: 60,
                vel: 100,
            })
            .unwrap();
        let mut left = [0.0; 2048];
        let mut right = [0.0; 2048];
        synth.write((&mut left[..], &mut right[..]));
        assert!(left.iter().any(|sample| sample.abs() > f32::EPSILON));
        assert!(right.iter().any(|sample| sample.abs() > f32::EPSILON));

        assert_no_alloc::assert_no_alloc(|| synth.write((&mut left[..], &mut right[..])));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn embedded_soundfont_defaults_to_bank_zero_program_zero() {
        let (synth, _) = create_synth(48_000.0, OxiSynthState::default()).unwrap();
        assert_eq!(synth.channel_count(), 16);
        let (_, bank, program) = synth.program(0).unwrap();
        assert_eq!((bank, program), (0, 0));
        let preset = synth.channel_preset(0).unwrap();
        assert_eq!(preset.name(), "Piano 1");
        assert_eq!((preset.banknum(), preset.num()), (0, 0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dependency_rejects_a_single_internal_midi_channel() {
        assert!(Synth::new(SynthDescriptor {
            midi_channels: 1,
            ..SynthDescriptor::default()
        })
        .is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn generated_catalog_is_complete_sorted_and_selectable() {
        assert_eq!(available_presets().len(), 136);
        assert_eq!(
            available_presets()
                .iter()
                .map(|preset| preset.id.bank)
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([0, 128])
        );
        assert!(available_presets()
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id));
        assert_eq!(preset_descriptor(DEFAULT_PRESET).unwrap().name, "Piano 1");

        let mut processor =
            OxiSynthProcessor::new(48_000.0, 128, OxiSynthState::default()).unwrap();
        for preset in available_presets() {
            processor.select_preset(preset.id).unwrap();
            assert_eq!(processor.selected_preset(), preset.id);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn state_codec_is_canonical_and_strict() {
        let mut control = OxiSynthControlState::default();
        assert_eq!(
            control.encode(),
            "shoop-oxisynth:2:timgm6mb:0:0:00000000:00000000"
        );
        let preset = OxiSynthPresetId {
            bank: 128,
            program: 0,
        };
        control.select_preset(preset).unwrap();
        control
            .set_send(OxiSynthParameter::ReverbSend, 0.5)
            .unwrap();
        control
            .set_send(OxiSynthParameter::ChorusSend, 1.0)
            .unwrap();
        let encoded = control.encode();
        let decoded = OxiSynthControlState::from_encoded(&encoded).unwrap();
        assert_eq!(decoded.selected_preset(), preset);
        assert_eq!(decoded.state().reverb_send, 0.5);
        assert_eq!(decoded.state().chorus_send, 1.0);

        assert!(matches!(
            OxiSynthControlState::from_encoded("shoop-oxisynth:1:timgm6mb:0:0:00000000:00000000"),
            Err(OxiSynthStateError::UnsupportedVersion(_))
        ));
        assert!(matches!(
            OxiSynthControlState::from_encoded("shoop-oxisynth:2:other:0:0:00000000:00000000"),
            Err(OxiSynthStateError::UnknownSoundFont(_))
        ));
        assert!(matches!(
            OxiSynthControlState::from_encoded("shoop-oxisynth:2:timgm6mb:0:128:00000000:00000000"),
            Err(OxiSynthStateError::InvalidProgram)
        ));
        assert!(matches!(
            OxiSynthControlState::from_encoded("shoop-oxisynth:2:timgm6mb:1:0:00000000:00000000"),
            Err(OxiSynthStateError::UnknownPreset { .. })
        ));
        for malformed in [
            "",
            "not-state",
            "shoop-oxisynth:2:timgm6mb:0",
            "shoop-oxisynth:2:timgm6mb:0:0:00000000",
            "shoop-oxisynth:2:timgm6mb:0:0:00000000:00000000:extra",
            "shoop-oxisynth:2:timgm6mb:00:0:00000000:00000000",
            "shoop-oxisynth:2:timgm6mb:0:0:7fc00000:00000000",
            "shoop-oxisynth:2:timgm6mb:0:0:3f800001:00000000",
        ] {
            assert!(OxiSynthControlState::from_encoded(malformed).is_err());
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_translation_is_strict_single_channel_and_blocks_preset_changes() {
        for source_channel in 0..16 {
            assert!(matches!(
                translate_midi(&[0x90 | source_channel, 60, 100]),
                Some(MidiEvent::NoteOn { channel: 0, .. })
            ));
            assert!(matches!(
                translate_midi(&[0xa0 | source_channel, 60, 20]),
                Some(MidiEvent::PolyphonicKeyPressure { channel: 0, .. })
            ));
            assert!(matches!(
                translate_midi(&[0xd0 | source_channel, 12]),
                Some(MidiEvent::ChannelPressure { channel: 0, .. })
            ));
            assert!(matches!(
                translate_midi(&[0xe0 | source_channel, 0, 64]),
                Some(MidiEvent::PitchBend {
                    channel: 0,
                    value: 8192
                })
            ));
            assert!(translate_midi(&[0xc0 | source_channel, 12]).is_none());
            assert!(translate_midi(&[0xb0 | source_channel, 0, 12]).is_none());
            assert!(translate_midi(&[0xb0 | source_channel, 32, 12]).is_none());
        }
        assert!(matches!(
            translate_midi(&[0x90, 60, 0]),
            Some(MidiEvent::NoteOff { channel: 0, .. })
        ));
        for controller in [1, 11, 64] {
            assert!(matches!(
                translate_midi(&[0xb4, controller, 20]),
                Some(MidiEvent::ControlChange {
                    channel: 0,
                    ctrl,
                    value: 20,
                }) if ctrl == controller
            ));
        }
        for controller in [0, 7, 10, 32, 91, 93, 120, 123] {
            assert!(translate_midi(&[0xb4, controller, 20]).is_none());
        }
        assert!(matches!(
            translate_midi(&[0xff]),
            Some(MidiEvent::SystemReset)
        ));
        for malformed in [
            &[][..],
            &[0x90, 60][..],
            &[0x90, 60, 128][..],
            &[0xf8][..],
            &[0xf0, 1, 0xf7][..],
            &[0xff, 0][..],
        ] {
            assert!(
                translate_midi(malformed).is_none(),
                "accepted {malformed:?}"
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn direct_send_controls_and_midi_learn_publish_without_forwarding_cc() {
        let mut control = OxiSynthControlState::default();
        assert!(control.assign_midi_cc(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ReverbSend,
            channel: 15,
            controller: 91,
        }));
        assert!(control.assign_midi_cc(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ChorusSend,
            channel: 2,
            controller: 74,
        }));
        let mut processor = control.prepare_processor(48_000.0, 128).unwrap();

        control
            .set_send(OxiSynthParameter::ChorusSend, 0.5)
            .unwrap();
        processor.process(128, &[]);
        assert_eq!(
            processor.synth.gen(0, GeneratorType::ChorusSend).unwrap(),
            100.0
        );

        let learned = MidiStorageElem::new(0, &[0xbf, 91, 127]).unwrap();
        assert_no_alloc::assert_no_alloc(|| processor.process(128, &[learned]));
        assert_eq!(
            processor.synth.gen(0, GeneratorType::ReverbSend).unwrap(),
            200.0
        );
        assert_eq!(processor.synth.cc(0, 91).unwrap(), 0);
        let editor = control.editor_state();
        assert_eq!(editor.reverb_send, 1.0);
        assert_eq!(editor.chorus_send, 0.5);
        assert_eq!(editor.midi_cc_assignments.len(), 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_assignments_are_unique_and_validated() {
        let mut assignments = OxiSynthMidiCcAssignments::default();
        assert!(assignments.assign(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ReverbSend,
            channel: 1,
            controller: 20,
        }));
        assert!(assignments.assign(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ChorusSend,
            channel: 1,
            controller: 20,
        }));
        assert_eq!(assignments.iter().count(), 1);
        assert!(!assignments.assign(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ReverbSend,
            channel: 16,
            controller: 20,
        }));
        assert!(!assignments.assign(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ReverbSend,
            channel: 1,
            controller: 128,
        }));
        assignments.remove(OxiSynthParameter::ChorusSend);
        assert_eq!(assignments.iter().count(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn only_modulation_expression_and_sustain_cc_reach_oxisynth() {
        let mut processor =
            OxiSynthProcessor::new(48_000.0, 128, OxiSynthState::default()).unwrap();
        let events = [
            MidiStorageElem::new(0, &[0xb7, 1, 12]).unwrap(),
            MidiStorageElem::new(0, &[0xb7, 11, 34]).unwrap(),
            MidiStorageElem::new(0, &[0xb7, 64, 127]).unwrap(),
            MidiStorageElem::new(0, &[0xb7, 7, 1]).unwrap(),
            MidiStorageElem::new(0, &[0xb7, 10, 2]).unwrap(),
            MidiStorageElem::new(0, &[0xb7, 91, 3]).unwrap(),
            MidiStorageElem::new(0, &[0xb7, 93, 4]).unwrap(),
        ];
        processor.process(128, &events);
        assert_eq!(processor.synth.cc(0, 1).unwrap(), 12);
        assert_eq!(processor.synth.cc(0, 11).unwrap(), 34);
        assert_eq!(processor.synth.cc(0, 64).unwrap(), 127);
        assert_eq!(processor.synth.cc(0, 7).unwrap(), 100);
        assert_eq!(processor.synth.cc(0, 10).unwrap(), 64);
        assert_eq!(processor.synth.cc(0, 91).unwrap(), 0);
        assert_eq!(processor.synth.cc(0, 93).unwrap(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn processor_preserves_event_offsets_and_allocates_nothing_realtime() {
        let mut processor =
            OxiSynthProcessor::new(48_000.0, 256, OxiSynthState::default()).unwrap();
        let note = MidiStorageElem::new(128, &[0x9f, 60, 100]).unwrap();
        processor.process(256, &[note]);
        let pre_event_peak = processor
            .output(0, 128)
            .unwrap()
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            pre_event_peak <= 1.0e-4,
            "pre-event peak was {pre_event_peak}"
        );
        assert!(processor.output(0, 256).unwrap()[128..]
            .iter()
            .any(|sample| sample.abs() > f32::EPSILON));
        let note_off = MidiStorageElem::new(0, &[0x8f, 60, 0]).unwrap();
        assert_no_alloc::assert_no_alloc(|| processor.process(256, &[note_off, note]));
        let filtered = [
            MidiStorageElem::new(0, &[0xbf, 0, 1]).unwrap(),
            MidiStorageElem::new(0, &[0xbf, 32, 2]).unwrap(),
            MidiStorageElem::new(0, &[0xcf, 3]).unwrap(),
        ];
        assert_no_alloc::assert_no_alloc(|| processor.process(256, &filtered));
        let selected = processor.selected_preset();
        assert_no_alloc::assert_no_alloc(|| processor.reset());
        assert_no_alloc::assert_no_alloc(|| processor.panic());
        assert_eq!(processor.selected_preset(), selected);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn preset_switch_changes_rendering_and_stops_previous_voices() {
        let mut processor =
            OxiSynthProcessor::new(48_000.0, 2048, OxiSynthState::default()).unwrap();
        let note = MidiStorageElem::new(0, &[0x90, 60, 100]).unwrap();
        processor.process(2048, &[note]);
        for _ in 0..8 {
            processor.process(2048, &[]);
        }
        let piano = processor.output(0, 2048).unwrap().to_vec();

        let violin = OxiSynthPresetId {
            bank: 0,
            program: 40,
        };
        processor.select_preset(violin).unwrap();
        for _ in 0..128 {
            assert_no_alloc::assert_no_alloc(|| processor.process(2048, &[]));
        }
        let tail_peak = processor
            .output(0, 2048)
            .unwrap()
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            tail_peak < 1.0e-4,
            "preset-switch tail peak was {tail_peak}"
        );

        assert_no_alloc::assert_no_alloc(|| processor.process(2048, &[note]));
        for _ in 0..8 {
            assert_no_alloc::assert_no_alloc(|| processor.process(2048, &[]));
        }
        let violin = processor.output(0, 2048).unwrap();
        let difference = piano
            .iter()
            .zip(violin)
            .fold(0.0_f32, |peak, (left, right)| {
                peak.max((left - right).abs())
            });
        assert!(
            difference > 1.0e-6,
            "preset render difference was {difference}"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn selected_preset_remains_loaded_after_switch_and_reset() {
        let mut processor =
            OxiSynthProcessor::new(48_000.0, 128, OxiSynthState::default()).unwrap();
        let violin = OxiSynthPresetId {
            bank: 0,
            program: 40,
        };

        processor.select_preset(violin).unwrap();
        assert_eq!(
            processor
                .synth
                .program(0)
                .map(|(_, bank, program)| (bank, program))
                .unwrap(),
            (0, 40)
        );

        processor.reset();
        assert_eq!(
            processor
                .synth
                .program(0)
                .map(|(_, bank, program)| (bank, program))
                .unwrap(),
            (0, 40)
        );

        let system_reset = MidiStorageElem::new(0, &[0xff]).unwrap();
        processor.process(128, &[system_reset]);
        assert_eq!(
            processor
                .synth
                .program(0)
                .map(|(_, bank, program)| (bank, program))
                .unwrap(),
            (0, 40)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn panic_and_preset_changes_preserve_effect_tails() {
        let note = MidiStorageElem::new(0, &[0x90, 60, 127]).unwrap();
        for change in [
            None,
            Some(OxiSynthPresetId {
                bank: 0,
                program: 40,
            }),
        ] {
            let mut processor =
                OxiSynthProcessor::new(48_000.0, 4096, OxiSynthState::default()).unwrap();
            processor.process(4096, &[note]);
            if let Some(preset) = change {
                processor.select_preset(preset).unwrap();
            } else {
                processor.panic();
            }
            processor.process(4096, &[]);
            let peak = processor
                .output(0, 4096)
                .unwrap()
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            assert!(peak > 1.0e-6, "effect tail was cleared: {peak}");
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn sustain_holds_and_releases_a_note() {
        let mut sustained =
            OxiSynthProcessor::new(48_000.0, 2048, OxiSynthState::default()).unwrap();
        let mut released =
            OxiSynthProcessor::new(48_000.0, 2048, OxiSynthState::default()).unwrap();
        let note_on = MidiStorageElem::new(0, &[0x90, 60, 127]).unwrap();
        sustained.process(2048, &[note_on]);
        released.process(2048, &[note_on]);
        let sustain_on = MidiStorageElem::new(0, &[0xb0, 64, 127]).unwrap();
        let note_off = MidiStorageElem::new(0, &[0x80, 60, 0]).unwrap();
        sustained.process(2048, &[sustain_on, note_off]);
        released.process(2048, &[note_off]);
        for _ in 0..8 {
            sustained.process(2048, &[]);
            released.process(2048, &[]);
        }
        let sustained_peak = sustained
            .output(0, 2048)
            .unwrap()
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        let released_peak = released
            .output(0, 2048)
            .unwrap()
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            sustained_peak > released_peak * 2.0,
            "sustain did not retain the note: {sustained_peak} vs {released_peak}"
        );
        let sustain_off = MidiStorageElem::new(0, &[0xb0, 64, 0]).unwrap();
        sustained.process(2048, &[sustain_off]);
        for _ in 0..128 {
            sustained.process(2048, &[]);
        }
        let final_peak = sustained
            .output(0, 2048)
            .unwrap()
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(final_peak < sustained_peak * 0.01);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn preset_selection_rejects_unknown_presets_without_mutating_state() {
        let mut processor =
            OxiSynthProcessor::new(48_000.0, 128, OxiSynthState::default()).unwrap();
        let unknown = OxiSynthPresetId {
            bank: 1,
            program: 0,
        };
        assert!(processor.select_preset(unknown).is_err());
        assert_eq!(processor.selected_preset(), DEFAULT_PRESET);
        processor.panic();
        assert_eq!(processor.selected_preset(), DEFAULT_PRESET);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn sustained_polyphony_remains_bounded_and_allocation_free() {
        let mut processor =
            OxiSynthProcessor::new(48_000.0, 128, OxiSynthState::default()).unwrap();
        let events = (0..POLYPHONY)
            .map(|index| {
                MidiStorageElem::new(
                    0,
                    &[
                        0x90 | (index % 16) as u8,
                        24 + ((index / 16) % 96) as u8,
                        100,
                    ],
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        processor.process(128, &events);
        assert_no_alloc::assert_no_alloc(|| {
            for _ in 0..64 {
                processor.process(128, &[]);
            }
        });
        assert!(processor
            .output(0, 128)
            .unwrap()
            .iter()
            .all(|sample| sample.is_finite()));
        processor.reset();
    }
}
