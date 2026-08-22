use anyhow::{Context, Result};
use oxisynth::{MidiEvent, SoundFont, SoundFontId, Synth, SynthDescriptor};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use crate::midi_storage::MidiStorageElem;

pub const SOUNDFONT_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../third_party/timgm6mb/TimGM6mb.sf2"
));
pub const SOUNDFONT_SHA256: &str =
    "c5378b62028c920cb11e4803327983fee2f2cdff5dc89c708e39da417e51c854";
pub const POLYPHONY: u16 = 256;
pub const MIDI_CHANNELS: usize = 16;
pub const MIDI_CONTROLLERS: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OxiSynthPreset {
    pub bank: u32,
    pub program: u8,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OxiSynthControl {
    SelectProgram {
        channel: u8,
        bank: u32,
        program: u8,
    },
    Audition {
        channel: u8,
        key: u8,
        velocity: u8,
        pressed: bool,
    },
    Panic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OxiSynthChannelSnapshot {
    pub baseline_bank: u32,
    pub baseline_program: u8,
    pub bank: u32,
    pub program: u8,
    pub controllers: [u8; MIDI_CONTROLLERS],
    pub pitch_bend: u16,
    pub pitch_wheel_sensitivity: u8,
    pub channel_pressure: u8,
}

impl Default for OxiSynthChannelSnapshot {
    fn default() -> Self {
        Self {
            bank: 0,
            program: 0,
            baseline_bank: 0,
            baseline_program: 0,
            controllers: [0; MIDI_CONTROLLERS],
            pitch_bend: 8192,
            pitch_wheel_sensitivity: 2,
            channel_pressure: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OxiSynthSnapshot {
    pub revision: u64,
    pub midi_activity_revision: u64,
    pub channels: [OxiSynthChannelSnapshot; MIDI_CHANNELS],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OxiSynthProgramConfiguration {
    pub bank: u32,
    pub program: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OxiSynthConfiguration {
    pub version: u16,
    pub soundfont_sha256: String,
    pub channels: [OxiSynthProgramConfiguration; MIDI_CHANNELS],
}

impl Default for OxiSynthSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            midi_activity_revision: 0,
            channels: [OxiSynthChannelSnapshot::default(); MIDI_CHANNELS],
        }
    }
}

pub fn embedded_presets() -> Result<Vec<OxiSynthPreset>> {
    let mut bytes = Cursor::new(SOUNDFONT_BYTES);
    let font = soundfont::SoundFont2::load(&mut bytes).context("inspect embedded SoundFont")?;
    let mut presets = font
        .presets
        .into_iter()
        .filter_map(|preset| {
            let program = u8::try_from(preset.header.preset).ok()?;
            Some(OxiSynthPreset {
                bank: u32::from(preset.header.bank),
                program,
                name: preset.header.name,
            })
        })
        .collect::<Vec<_>>();
    presets.sort_by_key(|preset| (preset.bank, preset.program, preset.name.clone()));
    Ok(presets)
}

pub fn create_synth(sample_rate: f32) -> Result<Synth> {
    let mut bytes = Cursor::new(SOUNDFONT_BYTES);
    let font = SoundFont::load(&mut bytes).context("parse embedded TimGM6mb SoundFont")?;
    let mut synth = Synth::new(SynthDescriptor {
        sample_rate,
        polyphony: POLYPHONY,
        midi_channels: 16,
        audio_channels: 1,
        audio_groups: 1,
        ..SynthDescriptor::default()
    })
    .context("configure OxiSynth")?;
    synth.add_font(font, true);
    Ok(synth)
}

pub struct OxiSynthProcessor {
    synth: Synth,
    sound_font_id: SoundFontId,
    snapshot: OxiSynthSnapshot,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl std::fmt::Debug for OxiSynthProcessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxiSynthProcessor")
            .field("max_frames", &self.left.len())
            .finish_non_exhaustive()
    }
}

impl OxiSynthProcessor {
    pub fn new(sample_rate: f32, max_frames: usize) -> Result<Self> {
        let max_frames = max_frames.max(1);
        let synth = create_synth(sample_rate)?;
        let sound_font_id = synth
            .program(0)?
            .0
            .context("embedded SoundFont did not initialize a program")?;
        let mut result = Self {
            synth,
            sound_font_id,
            snapshot: OxiSynthSnapshot::default(),
            left: vec![0.0; max_frames],
            right: vec![0.0; max_frames],
        };
        result.refresh_all_channels();
        for channel in &mut result.snapshot.channels {
            channel.baseline_bank = channel.bank;
            channel.baseline_program = channel.program;
        }
        Ok(result)
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
        if self.synth.send_event(MidiEvent::SystemReset).is_ok() {
            self.refresh_all_channels();
        }
    }

    pub fn snapshot(&self) -> OxiSynthSnapshot {
        self.snapshot
    }

    pub fn select_program(&mut self, channel: u8, bank: u32, program: u8) -> Result<()> {
        self.synth
            .select_program(channel, self.sound_font_id, bank, program)
            .context("select OxiSynth program")?;
        self.refresh_channel(channel);
        let state = &mut self.snapshot.channels[channel as usize];
        state.baseline_bank = bank;
        state.baseline_program = program;
        Ok(())
    }

    pub fn configuration(&self) -> OxiSynthConfiguration {
        OxiSynthConfiguration {
            version: 1,
            soundfont_sha256: SOUNDFONT_SHA256.to_owned(),
            channels: std::array::from_fn(|channel| OxiSynthProgramConfiguration {
                bank: self.snapshot.channels[channel].baseline_bank,
                program: self.snapshot.channels[channel].baseline_program,
            }),
        }
    }

    pub fn encode_configuration(&self) -> Result<String> {
        serde_json::to_string(&self.configuration()).context("encode OxiSynth configuration")
    }

    pub fn restore_configuration(&mut self, encoded: &str) -> Result<()> {
        let configuration: OxiSynthConfiguration =
            serde_json::from_str(encoded).context("decode OxiSynth configuration")?;
        if configuration.version != 1 || configuration.soundfont_sha256 != SOUNDFONT_SHA256 {
            anyhow::bail!("unsupported OxiSynth configuration");
        }
        for (channel, program) in configuration.channels.into_iter().enumerate() {
            self.select_program(channel as u8, program.bank, program.program)?;
        }
        Ok(())
    }

    pub fn panic(&mut self) {
        for channel in 0..MIDI_CHANNELS as u8 {
            let _ = self.synth.send_event(MidiEvent::AllSoundOff { channel });
        }
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
    }

    pub fn audition(&mut self, channel: u8, key: u8, velocity: u8, pressed: bool) -> Result<()> {
        let event = if pressed {
            MidiEvent::NoteOn {
                channel,
                key,
                vel: velocity,
            }
        } else {
            MidiEvent::NoteOff { channel, key }
        };
        self.synth.send_event(event).context("audition OxiSynth")
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
                if self.synth.send_event(event).is_ok() {
                    self.note_midi_event(event);
                }
            }
            cursor = offset;
        }
        self.render(cursor, frames);
    }

    pub fn process_midi_controls_only(&mut self, events: &[MidiStorageElem]) {
        for event in events {
            let Some(event) = translate_midi(event.data()) else {
                continue;
            };
            if matches!(
                event,
                MidiEvent::ControlChange { .. }
                    | MidiEvent::ProgramChange { .. }
                    | MidiEvent::PitchBend { .. }
                    | MidiEvent::ChannelPressure { .. }
                    | MidiEvent::SystemReset
            ) && self.synth.send_event(event).is_ok()
            {
                self.note_midi_event(event);
            }
        }
    }

    fn render(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        self.synth
            .write((&mut self.left[start..end], &mut self.right[start..end]));
    }

    fn note_midi_event(&mut self, event: MidiEvent) {
        self.snapshot.midi_activity_revision = self.snapshot.midi_activity_revision.wrapping_add(1);
        match event {
            MidiEvent::SystemReset => {
                for channel in &mut self.snapshot.channels {
                    channel.channel_pressure = 0;
                }
                self.refresh_all_channels();
            }
            MidiEvent::ControlChange {
                channel,
                ctrl,
                value,
            } => {
                if let Some(state) = self.snapshot.channels.get_mut(channel as usize) {
                    state.controllers[ctrl as usize] = value;
                    if matches!(ctrl, 0 | 32) {
                        if let Ok((_, bank, program)) = self.synth.program(channel) {
                            state.bank = bank;
                            state.program = program as u8;
                        }
                    }
                    self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
                }
            }
            MidiEvent::PitchBend { channel, value } => {
                if let Some(state) = self.snapshot.channels.get_mut(channel as usize) {
                    state.pitch_bend = value;
                    self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
                }
            }
            MidiEvent::ProgramChange {
                channel,
                program_id,
            } => {
                if let Some(state) = self.snapshot.channels.get_mut(channel as usize) {
                    state.program = program_id;
                    if let Ok((_, bank, program)) = self.synth.program(channel) {
                        state.bank = bank;
                        state.program = program as u8;
                    }
                    self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
                }
            }
            MidiEvent::ChannelPressure { channel, value } => {
                if let Some(state) = self.snapshot.channels.get_mut(channel as usize) {
                    state.channel_pressure = value;
                    self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
                }
            }
            _ => {}
        }
    }

    fn refresh_all_channels(&mut self) {
        for channel in 0..MIDI_CHANNELS as u8 {
            self.refresh_channel(channel);
        }
    }

    fn refresh_channel(&mut self, channel: u8) {
        let Some(state) = self.snapshot.channels.get_mut(channel as usize) else {
            return;
        };
        if let Ok((_, bank, program)) = self.synth.program(channel) {
            state.bank = bank;
            state.program = program as u8;
        }
        for (controller, destination) in state.controllers.iter_mut().enumerate() {
            if let Ok(value) = self.synth.cc(channel, controller as u16) {
                *destination = value;
            }
        }
        if let Ok(value) = self.synth.pitch_bend(channel) {
            state.pitch_bend = value;
        }
        if let Ok(value) = self.synth.pitch_wheel_sensitivity(channel) {
            state.pitch_wheel_sensitivity = value;
        }
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
    }
}

pub fn translate_midi(data: &[u8]) -> Option<MidiEvent> {
    let status = *data.first()?;
    if status == 0xff && data.len() == 1 {
        return Some(MidiEvent::SystemReset);
    }
    let channel = status & 0x0f;
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
            120 => Some(MidiEvent::AllSoundOff { channel }),
            123 => Some(MidiEvent::AllNotesOff { channel }),
            _ => Some(MidiEvent::ControlChange {
                channel,
                ctrl: data[1],
                value: data[2],
            }),
        },
        0xc0 if data.len() == 2 && data[1] <= 127 => Some(MidiEvent::ProgramChange {
            channel,
            program_id: data[1],
        }),
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

        let mut synth = create_synth(48_000.0).unwrap();
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
    fn midi_translation_is_strict_and_complete() {
        assert!(matches!(
            translate_midi(&[0x90, 60, 0]),
            Some(MidiEvent::NoteOff { .. })
        ));
        assert!(matches!(
            translate_midi(&[0xa2, 60, 20]),
            Some(MidiEvent::PolyphonicKeyPressure { channel: 2, .. })
        ));
        assert!(matches!(
            translate_midi(&[0xb0, 120, 0]),
            Some(MidiEvent::AllSoundOff { .. })
        ));
        assert!(matches!(
            translate_midi(&[0xb0, 123, 0]),
            Some(MidiEvent::AllNotesOff { .. })
        ));
        assert!(matches!(
            translate_midi(&[0xc0, 12]),
            Some(MidiEvent::ProgramChange { .. })
        ));
        assert!(matches!(
            translate_midi(&[0xd0, 12]),
            Some(MidiEvent::ChannelPressure { .. })
        ));
        assert!(matches!(
            translate_midi(&[0xe0, 0, 64]),
            Some(MidiEvent::PitchBend { value: 8192, .. })
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
        let mut processor = OxiSynthProcessor::new(48_000.0, 256).unwrap();
        let note = MidiStorageElem::new(128, &[0x90, 60, 100]).unwrap();
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
        let note_off = MidiStorageElem::new(0, &[0x80, 60, 0]).unwrap();
        assert_no_alloc::assert_no_alloc(|| processor.process(256, &[note_off, note]));
        processor.reset();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn sustained_polyphony_remains_bounded_and_allocation_free() {
        let mut processor = OxiSynthProcessor::new(48_000.0, 128).unwrap();
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

    #[shoop_wasm_test_support::shoop_test]
    fn embedded_preset_catalog_and_direct_selection_are_consistent() {
        let presets = embedded_presets().unwrap();
        assert!(!presets.is_empty());
        assert!(presets
            .windows(2)
            .all(|pair| (pair[0].bank, pair[0].program) <= (pair[1].bank, pair[1].program)));

        let preset = presets.iter().find(|preset| preset.bank < 128).unwrap();
        let mut processor = OxiSynthProcessor::new(48_000.0, 64).unwrap();
        processor
            .select_program(3, preset.bank, preset.program)
            .unwrap();
        let channel = processor.snapshot().channels[3];
        assert_eq!(
            (channel.bank, channel.program),
            (preset.bank, preset.program)
        );
        assert!(processor.select_program(16, 0, 0).is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn external_midi_updates_the_authoritative_snapshot() {
        let mut processor = OxiSynthProcessor::new(48_000.0, 64).unwrap();
        let before = processor.snapshot();
        let events = [
            MidiStorageElem::new(0, &[0xb2, 7, 91]).unwrap(),
            MidiStorageElem::new(0, &[0xc2, 12]).unwrap(),
            MidiStorageElem::new(0, &[0xe2, 1, 65]).unwrap(),
            MidiStorageElem::new(0, &[0xd2, 44]).unwrap(),
        ];
        processor.process(64, &events);
        let after = processor.snapshot();
        assert!(after.revision > before.revision);
        assert_eq!(
            after.midi_activity_revision,
            before.midi_activity_revision + 4
        );
        assert_eq!(after.channels[2].controllers[7], 91);
        assert_eq!(after.channels[2].program, 12);
        assert_eq!(after.channels[2].pitch_bend, 65 << 7 | 1);
        assert_eq!(after.channels[2].channel_pressure, 44);

        processor.process(64, &[MidiStorageElem::new(0, &[0xff]).unwrap()]);
        assert_eq!(processor.snapshot().channels[2].channel_pressure, 0);
    }
}
