use anyhow::{Context, Result};
use oxisynth::{SoundFont, Synth, SynthDescriptor};
use std::io::Cursor;

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

#[cfg(test)]
mod tests {
    use super::*;
    use oxisynth::MidiEvent;
    use sha2::{Digest, Sha256};

    #[test]
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
}
