use crate::{
    decode_wav, resample_loop_audio, ExactMidi, ExactMidiEvent, LoopAudio, LoopAudioChannel,
};
use std::collections::BTreeMap;
use thiserror::Error;

pub const MAX_CLICK_TRACK_CLICKS: u32 = 4_096;
pub const MAX_CLICK_TRACK_FRAMES: u32 = 10_000_000;
pub const MAX_CLICK_TRACK_MIDI_EVENTS: usize = MAX_CLICK_TRACK_CLICKS as usize * 2;

const CLICK_SOUNDS: [(&str, &[u8]); 4] = [
    (
        "click_high",
        include_bytes!("../../../../resources/clicks/click_high.wav"),
    ),
    (
        "click_low",
        include_bytes!("../../../../resources/clicks/click_low.wav"),
    ),
    (
        "shaker_primary",
        include_bytes!("../../../../resources/clicks/shaker_primary.wav"),
    ),
    (
        "shaker_secondary",
        include_bytes!("../../../../resources/clicks/shaker_secondary.wav"),
    ),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClickTrackTimingSpec {
    pub bpm: f64,
    pub click_count: u32,
    pub odd_click_delay_percent: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioClickTrackSpec {
    pub timing: ClickTrackTimingSpec,
    pub primary_sound: String,
    pub secondary_sound: Option<String>,
    pub secondary_clicks_per_primary: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MidiClickTrackSpec {
    pub timing: ClickTrackTimingSpec,
    pub note: u8,
    pub channel: u8,
    pub velocity: u8,
    pub note_length_seconds: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClickTrackTiming {
    pub output_frames: u32,
    pub click_start_frames: Vec<u32>,
}

#[derive(Debug, Error, PartialEq)]
pub enum ClickTrackError {
    #[error("sample rate must be non-zero")]
    InvalidSampleRate,
    #[error("clicks per minute must be finite and greater than zero")]
    InvalidBpm,
    #[error("click count must be in 1..={MAX_CLICK_TRACK_CLICKS}")]
    InvalidClickCount,
    #[error("odd-click delay must be between 0 and 100 percent")]
    InvalidOddClickDelay,
    #[error("generated click track has no frames")]
    EmptyOutput,
    #[error("generated click track exceeds the {MAX_CLICK_TRACK_FRAMES}-frame limit")]
    OutputTooLong,
    #[error("unknown click sound: {0}")]
    UnknownSound(String),
    #[error("could not decode click sound {sound}: {message}")]
    InvalidSound { sound: String, message: String },
    #[error("secondary clicks per primary exceeds the click-count limit")]
    InvalidSecondaryCount,
    #[error("MIDI channel must be in 0..=15")]
    InvalidMidiChannel,
    #[error("MIDI note length must be finite and between 0 and 10 seconds")]
    InvalidMidiNoteLength,
}

pub fn click_sound_ids() -> impl ExactSizeIterator<Item = &'static str> {
    CLICK_SOUNDS.iter().map(|(id, _)| *id)
}

pub fn generate_click_track_timing(
    spec: ClickTrackTimingSpec,
    sample_rate: u32,
) -> Result<ClickTrackTiming, ClickTrackError> {
    if sample_rate == 0 {
        return Err(ClickTrackError::InvalidSampleRate);
    }
    if !spec.bpm.is_finite() || spec.bpm <= 0.0 {
        return Err(ClickTrackError::InvalidBpm);
    }
    if spec.click_count == 0 || spec.click_count > MAX_CLICK_TRACK_CLICKS {
        return Err(ClickTrackError::InvalidClickCount);
    }
    if !spec.odd_click_delay_percent.is_finite()
        || !(0.0..=100.0).contains(&spec.odd_click_delay_percent)
    {
        return Err(ClickTrackError::InvalidOddClickDelay);
    }

    let frames_per_click = 60.0 * sample_rate as f64 / spec.bpm;
    let output_frames_f64 = frames_per_click * spec.click_count as f64;
    if !output_frames_f64.is_finite() || output_frames_f64 > MAX_CLICK_TRACK_FRAMES as f64 {
        return Err(ClickTrackError::OutputTooLong);
    }
    let output_frames = output_frames_f64.floor() as u32;
    if output_frames == 0 {
        return Err(ClickTrackError::EmptyOutput);
    }

    let odd_delay = (frames_per_click * spec.odd_click_delay_percent / 100.0).floor();
    let click_start_frames = (0..spec.click_count)
        .map(|index| {
            let base = (index as f64 * frames_per_click).floor();
            let delayed = if index % 2 == 1 {
                base + odd_delay
            } else {
                base
            };
            delayed.min(u32::MAX as f64) as u32
        })
        .collect();
    Ok(ClickTrackTiming {
        output_frames,
        click_start_frames,
    })
}

pub fn generate_audio_click_track(
    spec: &AudioClickTrackSpec,
    sample_rate: u32,
) -> Result<LoopAudio, ClickTrackError> {
    if spec.secondary_clicks_per_primary >= MAX_CLICK_TRACK_CLICKS {
        return Err(ClickTrackError::InvalidSecondaryCount);
    }
    let timing = generate_click_track_timing(spec.timing, sample_rate)?;
    let mut pattern = vec![spec.primary_sound.as_str()];
    if let Some(secondary) = spec.secondary_sound.as_deref() {
        pattern.extend(std::iter::repeat_n(
            secondary,
            spec.secondary_clicks_per_primary as usize,
        ));
    }

    let mut decoded = BTreeMap::<&str, Vec<f32>>::new();
    for sound in &pattern {
        if decoded.contains_key(sound) {
            continue;
        }
        decoded.insert(sound, decode_click_sound(sound, sample_rate)?);
    }

    let mut samples = vec![0.0_f32; timing.output_frames as usize];
    for (index, start) in timing.click_start_frames.iter().enumerate() {
        let sound = pattern[index % pattern.len()];
        let waveform = &decoded[sound];
        for (offset, sample) in waveform.iter().enumerate() {
            let Some(target) = (*start as usize).checked_add(offset) else {
                break;
            };
            let Some(output) = samples.get_mut(target) else {
                break;
            };
            *output += sample;
        }
    }

    Ok(LoopAudio {
        sample_rate,
        channels: vec![LoopAudioChannel {
            label: "click track".to_owned(),
            role: "generated".to_owned(),
            samples,
        }],
    })
}

pub fn generate_midi_click_track(
    spec: MidiClickTrackSpec,
    sample_rate: u32,
) -> Result<ExactMidi, ClickTrackError> {
    if spec.channel > 15 {
        return Err(ClickTrackError::InvalidMidiChannel);
    }
    if !spec.note_length_seconds.is_finite() || !(0.0..=10.0).contains(&spec.note_length_seconds) {
        return Err(ClickTrackError::InvalidMidiNoteLength);
    }
    let timing = generate_click_track_timing(spec.timing, sample_rate)?;
    let note_length_frames = (spec.note_length_seconds * sample_rate as f64).floor() as u64;
    let mut events = Vec::with_capacity(
        timing
            .click_start_frames
            .len()
            .saturating_mul(2)
            .min(MAX_CLICK_TRACK_MIDI_EVENTS),
    );
    for start in timing.click_start_frames {
        if start >= timing.output_frames {
            continue;
        }
        let start = u64::from(start);
        let end = start
            .saturating_add(note_length_frames)
            .min(u64::from(timing.output_frames - 1));
        events.push(ExactMidiEvent {
            frame: start,
            order: events.len() as u32,
            data: vec![0x90 | spec.channel, spec.note, spec.velocity],
        });
        events.push(ExactMidiEvent {
            frame: end,
            order: events.len() as u32,
            data: vec![0x80 | spec.channel, spec.note, spec.velocity],
        });
    }
    events.sort_by_key(|event| (event.frame, event.order));
    for (order, event) in events.iter_mut().enumerate() {
        event.order = order as u32;
    }
    Ok(ExactMidi {
        sample_rate,
        length_frames: u64::from(timing.output_frames),
        start_state: Vec::new(),
        events,
    })
}

fn decode_click_sound(id: &str, sample_rate: u32) -> Result<Vec<f32>, ClickTrackError> {
    let bytes = CLICK_SOUNDS
        .iter()
        .find_map(|(candidate, bytes)| (*candidate == id).then_some(*bytes))
        .ok_or_else(|| ClickTrackError::UnknownSound(id.to_owned()))?;
    let audio = decode_wav(bytes).map_err(|error| ClickTrackError::InvalidSound {
        sound: id.to_owned(),
        message: error.to_string(),
    })?;
    let audio = if audio.sample_rate == sample_rate {
        audio
    } else {
        resample_loop_audio(&audio, sample_rate).map_err(|error| ClickTrackError::InvalidSound {
            sound: id.to_owned(),
            message: error.to_string(),
        })?
    };
    audio
        .channels
        .into_iter()
        .next()
        .map(|channel| channel.samples)
        .ok_or_else(|| ClickTrackError::InvalidSound {
            sound: id.to_owned(),
            message: "sound contains no audio channels".to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(bpm: f64, clicks: u32, delay: f64) -> ClickTrackTimingSpec {
        ClickTrackTimingSpec {
            bpm,
            click_count: clicks,
            odd_click_delay_percent: delay,
        }
    }

    #[test]
    fn catalog_is_stable_sorted_and_all_assets_decode() {
        assert_eq!(
            click_sound_ids().collect::<Vec<_>>(),
            vec![
                "click_high",
                "click_low",
                "shaker_primary",
                "shaker_secondary"
            ]
        );
        for id in click_sound_ids() {
            let samples = decode_click_sound(id, 48_000).unwrap();
            assert!(!samples.is_empty());
            assert!(samples.iter().any(|sample| *sample != 0.0));
        }
    }

    #[test]
    fn timing_preserves_fractional_bpm_and_odd_delay_boundaries() {
        let straight = generate_click_track_timing(timing(100.5, 4, 0.0), 48_000).unwrap();
        assert_eq!(straight.output_frames, 114_626);
        assert_eq!(straight.click_start_frames, vec![0, 28_656, 57_313, 85_970]);

        let swung = generate_click_track_timing(timing(120.0, 4, 50.0), 48_000).unwrap();
        assert_eq!(swung.output_frames, 96_000);
        assert_eq!(swung.click_start_frames, vec![0, 36_000, 48_000, 84_000]);

        let full = generate_click_track_timing(timing(120.0, 4, 100.0), 48_000).unwrap();
        assert_eq!(full.click_start_frames, vec![0, 48_000, 48_000, 96_000]);
    }

    #[test]
    fn timing_rejects_invalid_and_unbounded_requests() {
        for spec in [
            timing(0.0, 4, 0.0),
            timing(f64::NAN, 4, 0.0),
            timing(120.0, 0, 0.0),
            timing(120.0, MAX_CLICK_TRACK_CLICKS + 1, 0.0),
            timing(120.0, 4, 101.0),
        ] {
            assert!(generate_click_track_timing(spec, 48_000).is_err());
        }
        assert_eq!(
            generate_click_track_timing(timing(1.0, 4_096, 0.0), 384_000),
            Err(ClickTrackError::OutputTooLong)
        );
        assert_eq!(
            generate_click_track_timing(timing(f64::MAX, 1, 0.0), 1),
            Err(ClickTrackError::EmptyOutput)
        );
    }

    #[test]
    fn audio_generation_cycles_pattern_resamples_and_truncates() {
        let spec = AudioClickTrackSpec {
            timing: timing(120.0, 4, 100.0),
            primary_sound: "click_high".to_owned(),
            secondary_sound: Some("shaker_secondary".to_owned()),
            secondary_clicks_per_primary: 1,
        };
        let first = generate_audio_click_track(&spec, 44_100).unwrap();
        let second = generate_audio_click_track(&spec, 44_100).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.sample_rate, 44_100);
        assert_eq!(first.channels[0].samples.len(), 88_200);
        assert!(first.channels[0]
            .samples
            .iter()
            .any(|sample| *sample != 0.0));
        assert_eq!(first.channels[0].role, "generated");

        let only_primary = generate_audio_click_track(
            &AudioClickTrackSpec {
                secondary_sound: None,
                secondary_clicks_per_primary: 3,
                ..spec
            },
            48_000,
        )
        .unwrap();
        assert_eq!(only_primary.channels[0].samples.len(), 96_000);
    }

    #[test]
    fn midi_generation_uses_visible_defaults_and_clamps_final_note_off() {
        let midi = generate_midi_click_track(
            MidiClickTrackSpec {
                timing: timing(600.0, 2, 0.0),
                note: 64,
                channel: 0,
                velocity: 127,
                note_length_seconds: 10.0,
            },
            48_000,
        )
        .unwrap();
        assert_eq!(midi.length_frames, 9_600);
        assert_eq!(midi.events.len(), 4);
        assert_eq!(midi.events[0].data, vec![0x90, 64, 127]);
        assert_eq!(midi.events[1].frame, 4_800);
        assert_eq!(midi.events[2].frame, 9_599);
        assert_eq!(midi.events[3].frame, 9_599);
        assert!(midi
            .events
            .iter()
            .all(|event| event.frame < midi.length_frames));
    }

    #[test]
    fn hundred_percent_delayed_click_at_end_is_omitted_safely() {
        let midi = generate_midi_click_track(
            MidiClickTrackSpec {
                timing: timing(120.0, 4, 100.0),
                note: 64,
                channel: 0,
                velocity: 127,
                note_length_seconds: 0.1,
            },
            48_000,
        )
        .unwrap();
        assert_eq!(midi.events.len(), 6);
        assert!(midi.events.iter().all(|event| event.frame < 96_000));
    }
}
