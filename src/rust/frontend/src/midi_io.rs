use anyhow;
use backend_bindings::MidiEvent;
use common::logging::macros::*;
use midly::num::u28;
use std::path::Path;
shoop_log_unit!("Frontend.MidiIO");

pub fn save_to_standard_midi<'a>(
    filename: &Path,
    events: impl Iterator<Item = &'a MidiEvent>,
    _total_length: usize,
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
    let raw_data = std::fs::read(filename)?;
    let smf = midly::parse(&raw_data)?;
    let mut prev_timestamp: f64 = 0.0;
    let mut rval: Vec<MidiEvent> = Vec::default();
    let mut ticks_per_beat: Option<i32> = None;
    let mut samples_per_tick: Option<f64> = None;

    match smf.0.timing {
        midly::Timing::Metrical(u15) => {
            ticks_per_beat = Some(u15.as_int() as i32);
        }
        midly::Timing::Timecode(fps, subframe) => {
            let ticks_per_second = fps.as_int() as f64 * subframe as f64;
            samples_per_tick = Some((target_sample_rate as f64) / ticks_per_second);
        }
    };

    for (idx, track) in smf.1.enumerate() {
        if idx > 0 {
            warn!("MIDI file to load more than one track. Skipping all but the first.");
            break;
        }
        match track {
            Ok(track) => {
                for event in track {
                    match event {
                        Ok(event) => {
                            if let midly::TrackEventKind::Meta(meta) = event.kind {
                                if let midly::MetaMessage::Tempo(microseconds_per_beat) = meta {
                                    if let Some(ticks_per_beat) = ticks_per_beat {
                                        let microseconds_per_tick = microseconds_per_beat.as_int()
                                            as f64
                                            / ticks_per_beat as f64;
                                        samples_per_tick = Some(
                                            target_sample_rate as f64 * microseconds_per_tick
                                                / 1000000.0,
                                        );
                                    }
                                }
                            }
                            if samples_per_tick.is_none() {
                                return Err(anyhow::anyhow!("No tempo information"));
                            }
                            let delta = event.delta.as_int() as f64 * samples_per_tick.unwrap();
                            prev_timestamp += delta;
                            match event.kind.as_live_event() {
                                Some(event) => {
                                    let mut buf: Vec<u8> = Vec::default();
                                    event.write(&mut buf).map_err(|e| {
                                        anyhow::anyhow!("Could not write event: {e}")
                                    })?;
                                    let our_event = MidiEvent {
                                        data: buf,
                                        time: prev_timestamp as i32,
                                    };
                                    rval.push(our_event);
                                }
                                None => {
                                    debug!("skip msg (not live)");
                                }
                            }
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!("Failed to parse event: {e}"));
                        }
                    }
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to parse track: {e}"));
            }
        }
    }

    Ok(rval)
}
