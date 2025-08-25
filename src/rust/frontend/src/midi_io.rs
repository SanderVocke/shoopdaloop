use anyhow;
use backend_bindings::MidiEvent;
use midly::num::u28;
use std::path::Path;

pub fn save_to_standard_midi<'a>(
    filename: &Path,
    events: impl Iterator<Item = &'a MidiEvent>,
    total_length: usize,
    samplerate: usize,
) -> Result<(), anyhow::Error> {
    let arena = midly::Arena::new();
    let header = midly::Header {
        format: midly::Format::SingleTrack,
        timing: midly::Timing::Timecode(midly::Fps::Fps30, 128),
    };
    let ticks_per_second = 30 * 128;
    let mut track: midly::Track = midly::Track::default();
    let mut prev_time: u64 = 0;
    for event in events {
        let live_event = midly::live::LiveEvent::parse(&event.data)?;
        let time_seconds: f64 = event.time as f64 / samplerate as f64;
        let time_ticks = (time_seconds * ticks_per_second as f64) as u64;
        let track_event = midly::TrackEvent {
            delta: u28::from_int_lossy((time_ticks - prev_time) as u32),
            kind: live_event.as_track_event(&arena),
        };
        prev_time = time_ticks;
        track.push(track_event);
    }

    let smf = midly::Smf {
        header: header,
        tracks: vec![track],
    };

    smf.save(filename).map_err(|e| anyhow::anyhow!(e))
}

pub fn load_standard_midi<'a>(
    filename: &Path,
    target_sample_rate: usize,
) -> Result<Vec<MidiEvent>, anyhow::Error> {
    todo!();
}
