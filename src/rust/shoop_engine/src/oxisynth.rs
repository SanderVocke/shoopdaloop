use anyhow::{Context, Result};
use oxisynth::{MidiEvent, SoundFont, Synth, SynthDescriptor};
use std::io::Cursor;

use crate::midi_storage::MidiStorageElem;

pub const SOUNDFONT_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../third_party/timgm6mb/TimGM6mb.sf2"
));
pub const SOUNDFONT_SHA256: &str =
    "c5378b62028c920cb11e4803327983fee2f2cdff5dc89c708e39da417e51c854";
pub const POLYPHONY: u16 = 256;

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
        Ok(Self {
            synth: create_synth(sample_rate)?,
            left: vec![0.0; max_frames],
            right: vec![0.0; max_frames],
        })
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
}
