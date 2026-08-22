use anyhow::{Context, Result};
use oxisynth::{MidiEvent, SoundFont, SoundFontId, Synth, SynthDescriptor};
use std::io::Cursor;
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
const STATE_VERSION: &str = "1";

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OxiSynthState {
    pub preset: OxiSynthPresetId,
}

impl Default for OxiSynthState {
    fn default() -> Self {
        Self {
            preset: DEFAULT_PRESET,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OxiSynthEditorState {
    pub selected_preset: OxiSynthPresetId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OxiSynthControlState {
    state: OxiSynthState,
}

impl Default for OxiSynthControlState {
    fn default() -> Self {
        Self {
            state: OxiSynthState::default(),
        }
    }
}

impl OxiSynthControlState {
    pub fn from_state(state: OxiSynthState) -> Result<Self, OxiSynthStateError> {
        validate_preset(state.preset)?;
        Ok(Self { state })
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
        if fields.next().is_some() {
            return Err(OxiSynthStateError::InvalidEnvelope);
        }
        if program > 127 {
            return Err(OxiSynthStateError::InvalidProgram);
        }
        let control = Self::from_state(OxiSynthState {
            preset: OxiSynthPresetId { bank, program },
        })?;
        if control.encode() != encoded {
            return Err(OxiSynthStateError::InvalidEnvelope);
        }
        Ok(control)
    }

    pub fn state(&self) -> OxiSynthState {
        self.state
    }

    pub fn selected_preset(&self) -> OxiSynthPresetId {
        self.state.preset
    }

    pub fn editor_state(&self) -> OxiSynthEditorState {
        OxiSynthEditorState {
            selected_preset: self.state.preset,
        }
    }

    pub fn select_preset(&mut self, preset: OxiSynthPresetId) -> Result<(), OxiSynthStateError> {
        validate_preset(preset)?;
        self.state.preset = preset;
        Ok(())
    }

    pub fn encode(&self) -> String {
        format!(
            "{STATE_FORMAT}:{STATE_VERSION}:{SOUNDFONT_ID}:{}:{}",
            self.state.preset.bank, self.state.preset.program
        )
    }

    pub fn prepare_processor(
        &self,
        sample_rate: f32,
        max_frames: usize,
    ) -> Result<OxiSynthProcessor> {
        OxiSynthProcessor::new(sample_rate, max_frames, self.state)
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

fn create_synth(sample_rate: f32, preset: OxiSynthPresetId) -> Result<(Synth, SoundFontId)> {
    validate_preset(preset).context("validate OxiSynth preset")?;
    let mut bytes = Cursor::new(SOUNDFONT_BYTES);
    let font = SoundFont::load(&mut bytes).context("parse embedded TimGM6mb SoundFont")?;
    let mut synth = Synth::new(SynthDescriptor {
        sample_rate,
        polyphony: POLYPHONY,
        midi_channels: 16,
        drums_channel_active: false,
        audio_channels: 1,
        audio_groups: 1,
        ..SynthDescriptor::default()
    })
    .context("configure OxiSynth")?;
    let soundfont_id = synth.add_font(font, false);
    synth
        .select_program(0, soundfont_id, u32::from(preset.bank), preset.program)
        .context("select OxiSynth preset")?;
    Ok((synth, soundfont_id))
}

pub struct OxiSynthProcessor {
    synth: Synth,
    soundfont_id: SoundFontId,
    selected_preset: OxiSynthPresetId,
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
        let max_frames = max_frames.max(1);
        let (synth, soundfont_id) = create_synth(sample_rate, state.preset)?;
        Ok(Self {
            synth,
            soundfont_id,
            selected_preset: state.preset,
            left: vec![0.0; max_frames],
            right: vec![0.0; max_frames],
        })
    }

    pub fn selected_preset(&self) -> OxiSynthPresetId {
        self.selected_preset
    }

    pub fn select_preset(&mut self, preset: OxiSynthPresetId) -> Result<()> {
        validate_preset(preset).context("validate OxiSynth preset")?;
        self.synth
            .select_program(0, self.soundfont_id, u32::from(preset.bank), preset.program)
            .context("select OxiSynth preset")?;
        self.reset();
        self.selected_preset = preset;
        Ok(())
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
        let _ = self.synth.send_event(MidiEvent::SystemReset);
    }

    pub fn panic(&mut self) {
        self.reset();
    }

    pub fn process(&mut self, frames: usize, events: &[MidiStorageElem]) {
        let _span =
            shoop_tracing::realtime_span_detail!("engine.rt.fx.oxisynth_process", value = frames);
        let frames = frames.min(self.max_frames());
        let mut cursor = 0;
        for event in events {
            let offset = (event.time as usize).min(frames).max(cursor);
            self.render(cursor, offset);
            if let Some(event) = translate_midi(event.data()) {
                let _ = self.synth.send_event(event);
            }
            cursor = offset;
        }
        self.render(cursor, frames);
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
            0 | 32 => None,
            120 => Some(MidiEvent::AllSoundOff { channel }),
            123 => Some(MidiEvent::AllNotesOff { channel }),
            _ => Some(MidiEvent::ControlChange {
                channel,
                ctrl: data[1],
                value: data[2],
            }),
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

        let (mut synth, _) = create_synth(48_000.0, DEFAULT_PRESET).unwrap();
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
        let (synth, _) = create_synth(48_000.0, DEFAULT_PRESET).unwrap();
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
        assert_eq!(control.encode(), "shoop-oxisynth:1:timgm6mb:0:0");
        let preset = OxiSynthPresetId {
            bank: 128,
            program: 0,
        };
        control.select_preset(preset).unwrap();
        let encoded = control.encode();
        assert_eq!(
            OxiSynthControlState::from_encoded(&encoded)
                .unwrap()
                .selected_preset(),
            preset
        );

        assert!(matches!(
            OxiSynthControlState::from_encoded("shoop-oxisynth:2:timgm6mb:0:0"),
            Err(OxiSynthStateError::UnsupportedVersion(_))
        ));
        assert!(matches!(
            OxiSynthControlState::from_encoded("shoop-oxisynth:1:other:0:0"),
            Err(OxiSynthStateError::UnknownSoundFont(_))
        ));
        assert!(matches!(
            OxiSynthControlState::from_encoded("shoop-oxisynth:1:timgm6mb:0:128"),
            Err(OxiSynthStateError::InvalidProgram)
        ));
        assert!(matches!(
            OxiSynthControlState::from_encoded("shoop-oxisynth:1:timgm6mb:1:0"),
            Err(OxiSynthStateError::UnknownPreset { .. })
        ));
        for malformed in [
            "",
            "not-state",
            "shoop-oxisynth:1:timgm6mb:0",
            "shoop-oxisynth:1:timgm6mb:0:0:extra",
            "shoop-oxisynth:1:timgm6mb:00:0",
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
        assert!(matches!(
            translate_midi(&[0xb4, 1, 20]),
            Some(MidiEvent::ControlChange { channel: 0, .. })
        ));
        assert!(matches!(
            translate_midi(&[0xb0, 120, 0]),
            Some(MidiEvent::AllSoundOff { channel: 0 })
        ));
        assert!(matches!(
            translate_midi(&[0xb0, 123, 0]),
            Some(MidiEvent::AllNotesOff { channel: 0 })
        ));
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
