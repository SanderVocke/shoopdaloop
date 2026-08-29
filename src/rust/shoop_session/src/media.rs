use crate::archive::{
    audio_from_bytes, audio_to_bytes, check_standalone_version, encode_standalone_manifest,
    inspect_standalone_archive, payload_hash, read_standalone_entry, SessionError,
    SESSION_MANIFEST_PATH, STANDALONE_AUDIO_FORMAT, STANDALONE_MIDI_FORMAT,
};
use crate::document::{
    ExactMidi, ExactMidiEvent, FormatVersion, LoopAudio, LoopAudioChannel, DOCUMENT_VERSION,
};
use midly::num::u28;
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Cursor;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExactMidiManifest {
    format: String,
    format_version: FormatVersion,
    document_version: u16,
    midi: ExactMidi,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LoopAudioManifest {
    format: String,
    format_version: FormatVersion,
    document_version: u16,
    sample_rate: u32,
    channels: Vec<LoopAudioRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LoopAudioRecord {
    label: String,
    role: String,
    path: String,
    frames: u64,
    uncompressed_bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StandardMidiExport {
    pub bytes: Vec<u8>,
    pub max_quantization_error_frames: f64,
}

pub fn encode_exact_midi(midi: &ExactMidi) -> Result<Vec<u8>, SessionError> {
    validate_exact_midi(midi)?;
    let manifest = ExactMidiManifest {
        format: STANDALONE_MIDI_FORMAT.to_owned(),
        format_version: FormatVersion::default(),
        document_version: DOCUMENT_VERSION,
        midi: midi.clone(),
    };
    encode_standalone_manifest(&manifest, BTreeMap::new())
}

pub fn decode_exact_midi(bytes: &[u8]) -> Result<ExactMidi, SessionError> {
    let mut archive = inspect_standalone_archive(bytes)?;
    let manifest_bytes =
        read_standalone_entry(&mut archive, SESSION_MANIFEST_PATH, 256 * 1024 * 1024)?;
    let manifest: ExactMidiManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| SessionError::Manifest(error.to_string()))?;
    check_standalone_version(
        &manifest.format,
        STANDALONE_MIDI_FORMAT,
        manifest.format_version,
        manifest.document_version,
    )?;
    validate_exact_midi(&manifest.midi)?;
    Ok(manifest.midi)
}

pub fn encode_loop_audio(audio: &LoopAudio) -> Result<Vec<u8>, SessionError> {
    if audio.sample_rate == 0 {
        return Err(SessionError::Validation(
            "loop audio sample rate must be non-zero".to_owned(),
        ));
    }
    let mut payloads = BTreeMap::new();
    let mut records = Vec::with_capacity(audio.channels.len());
    for (index, channel) in audio.channels.iter().enumerate() {
        let path = format!("audio/{index:08}.f32le");
        let bytes = audio_to_bytes(&channel.samples);
        records.push(LoopAudioRecord {
            label: channel.label.clone(),
            role: channel.role.clone(),
            path: path.clone(),
            frames: channel.samples.len() as u64,
            uncompressed_bytes: bytes.len() as u64,
            sha256: payload_hash(&bytes),
        });
        payloads.insert(path, bytes);
    }
    let manifest = LoopAudioManifest {
        format: STANDALONE_AUDIO_FORMAT.to_owned(),
        format_version: FormatVersion::default(),
        document_version: DOCUMENT_VERSION,
        sample_rate: audio.sample_rate,
        channels: records,
    };
    encode_standalone_manifest(&manifest, payloads)
}

pub fn decode_loop_audio(bytes: &[u8]) -> Result<LoopAudio, SessionError> {
    let mut archive = inspect_standalone_archive(bytes)?;
    let manifest_bytes =
        read_standalone_entry(&mut archive, SESSION_MANIFEST_PATH, 256 * 1024 * 1024)?;
    let manifest: LoopAudioManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| SessionError::Manifest(error.to_string()))?;
    check_standalone_version(
        &manifest.format,
        STANDALONE_AUDIO_FORMAT,
        manifest.format_version,
        manifest.document_version,
    )?;
    if manifest.sample_rate == 0 {
        return Err(SessionError::Validation(
            "loop audio sample rate must be non-zero".to_owned(),
        ));
    }
    let mut channels = Vec::with_capacity(manifest.channels.len());
    for (index, record) in manifest.channels.into_iter().enumerate() {
        let bytes = read_standalone_entry(&mut archive, &record.path, record.uncompressed_bytes)?;
        if bytes.len() as u64 != record.uncompressed_bytes || payload_hash(&bytes) != record.sha256
        {
            return Err(SessionError::HashMismatch {
                id: format!("audio channel {index}"),
            });
        }
        let samples = audio_from_bytes(&format!("audio channel {index}"), &bytes)?;
        if samples.len() as u64 != record.frames {
            return Err(SessionError::InvalidMediaShape {
                id: format!("audio channel {index}"),
            });
        }
        channels.push(LoopAudioChannel {
            label: record.label,
            role: record.role,
            samples,
        });
    }
    Ok(LoopAudio {
        sample_rate: manifest.sample_rate,
        channels,
    })
}

pub fn encode_float_wav(audio: &LoopAudio) -> Result<Vec<u8>, SessionError> {
    if audio.sample_rate == 0 || audio.channels.is_empty() {
        return Err(SessionError::Validation(
            "WAV needs a sample rate and at least one channel".to_owned(),
        ));
    }
    let channels = u16::try_from(audio.channels.len()).map_err(|_| {
        SessionError::Validation("WAV cannot represent this channel count".to_owned())
    })?;
    let frames = audio.channels[0].samples.len();
    if audio
        .channels
        .iter()
        .any(|channel| channel.samples.len() != frames)
    {
        return Err(SessionError::Validation(
            "WAV channels have inconsistent lengths".to_owned(),
        ));
    }
    let mut bytes = Vec::new();
    {
        let cursor = Cursor::new(&mut bytes);
        let spec = hound::WavSpec {
            channels,
            sample_rate: audio.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::new(cursor, spec)
            .map_err(|error| SessionError::Archive(error.to_string()))?;
        for frame in 0..frames {
            for channel in &audio.channels {
                writer
                    .write_sample(channel.samples[frame])
                    .map_err(|error| SessionError::Archive(error.to_string()))?;
            }
        }
        writer
            .finalize()
            .map_err(|error| SessionError::Archive(error.to_string()))?;
    }
    Ok(bytes)
}

pub fn decode_wav(bytes: &[u8]) -> Result<LoopAudio, SessionError> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|error| SessionError::UnsupportedFormat.then_archive(error.to_string()))?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err(SessionError::Validation("invalid WAV shape".to_owned()));
    }
    let n_channels = spec.channels as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| sample.map_err(|error| SessionError::Archive(error.to_string())))
            .collect::<Result<_, _>>()?,
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => {
            let scale = (1_u32 << spec.bits_per_sample.saturating_sub(1)) as f32;
            reader
                .samples::<i16>()
                .map(|sample| {
                    sample
                        .map(|sample| sample as f32 / scale)
                        .map_err(|error| SessionError::Archive(error.to_string()))
                })
                .collect::<Result<_, _>>()?
        }
        hound::SampleFormat::Int => {
            let scale = (1_u64 << spec.bits_per_sample.saturating_sub(1)) as f64;
            reader
                .samples::<i32>()
                .map(|sample| {
                    sample
                        .map(|sample| (sample as f64 / scale) as f32)
                        .map_err(|error| SessionError::Archive(error.to_string()))
                })
                .collect::<Result<_, _>>()?
        }
    };
    if !interleaved.len().is_multiple_of(n_channels) {
        return Err(SessionError::Validation("ragged WAV data".to_owned()));
    }
    let frames = interleaved.len() / n_channels;
    let mut channels = (0..n_channels)
        .map(|index| LoopAudioChannel {
            label: format!("Channel {}", index + 1),
            role: "direct".to_owned(),
            samples: Vec::with_capacity(frames),
        })
        .collect::<Vec<_>>();
    for frame in interleaved.chunks_exact(n_channels) {
        for (channel, sample) in channels.iter_mut().zip(frame) {
            channel.samples.push(*sample);
        }
    }
    Ok(LoopAudio {
        sample_rate: spec.sample_rate,
        channels,
    })
}

pub fn encode_standard_midi(midi: &ExactMidi) -> Result<StandardMidiExport, SessionError> {
    validate_exact_midi(midi)?;
    const TICKS_PER_SECOND: f64 = 30.0 * 255.0;
    let arena = midly::Arena::new();
    let mut all = Vec::new();
    for (order, data) in midi.start_state.iter().enumerate() {
        all.push((0_u64, order as u32, data));
    }
    for event in &midi.events {
        all.push((event.frame, event.order, &event.data));
    }
    all.sort_by_key(|(frame, order, _)| (*frame, *order));
    let mut track = midly::Track::new();
    let mut previous_tick = 0_u64;
    let mut max_error = 0.0_f64;
    for (frame, _, data) in all {
        let event = midly::live::LiveEvent::parse(data)
            .map_err(|error| SessionError::Validation(format!("invalid MIDI bytes: {error}")))?;
        let exact_tick = frame as f64 * TICKS_PER_SECOND / midi.sample_rate as f64;
        let tick = exact_tick.round() as u64;
        max_error = max_error
            .max((tick as f64 - exact_tick).abs() * midi.sample_rate as f64 / TICKS_PER_SECOND);
        let delta = tick.saturating_sub(previous_tick);
        if delta > u28::max_value().as_int() as u64 {
            return Err(SessionError::Validation(
                "standard MIDI event gap exceeds the selected timebase".to_owned(),
            ));
        }
        track.push(TrackEvent {
            delta: u28::new(delta as u32),
            kind: event.as_track_event(&arena),
        });
        previous_tick = tick;
    }
    let end_tick =
        (midi.length_frames as f64 * TICKS_PER_SECOND / midi.sample_rate as f64).round() as u64;
    let end_delta = end_tick.saturating_sub(previous_tick);
    if end_delta > u28::max_value().as_int() as u64 {
        return Err(SessionError::Validation(
            "standard MIDI duration exceeds the selected timebase".to_owned(),
        ));
    }
    track.push(TrackEvent {
        delta: u28::new(end_delta as u32),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    let smf = Smf {
        header: Header {
            format: Format::SingleTrack,
            timing: Timing::Timecode(midly::Fps::Fps30, 255),
        },
        tracks: vec![track],
    };
    let mut bytes = Vec::new();
    smf.write_std(&mut bytes)
        .map_err(|error| SessionError::Archive(error.to_string()))?;
    Ok(StandardMidiExport {
        bytes,
        max_quantization_error_frames: max_error,
    })
}

pub fn decode_standard_midi(bytes: &[u8], sample_rate: u32) -> Result<ExactMidi, SessionError> {
    if sample_rate == 0 {
        return Err(SessionError::Validation(
            "target MIDI sample rate must be non-zero".to_owned(),
        ));
    }
    let smf = Smf::parse(bytes)
        .map_err(|error| SessionError::UnsupportedFormat.then_archive(error.to_string()))?;
    let mut raw = Vec::<RawMidiEvent>::new();
    let mut tempos = Vec::<(u64, u32)>::new();
    let mut end_tick = 0_u64;
    for (track_index, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0_u64;
        for (event_index, event) in track.iter().enumerate() {
            tick = tick.saturating_add(event.delta.as_int() as u64);
            end_tick = end_tick.max(tick);
            if let TrackEventKind::Meta(MetaMessage::Tempo(tempo)) = event.kind {
                tempos.push((tick, tempo.as_int()));
            }
            if let Some(live) = event.kind.as_live_event() {
                let mut data = Vec::new();
                live.write_std(&mut data)
                    .map_err(|error| SessionError::Archive(error.to_string()))?;
                raw.push(RawMidiEvent {
                    tick,
                    track: track_index,
                    order: event_index,
                    data,
                });
            }
        }
    }
    raw.sort_by_key(|event| (event.tick, event.track, event.order));
    tempos.sort_by_key(|(tick, _)| *tick);
    tempos.dedup_by_key(|(tick, _)| *tick);
    let tick_to_seconds = |tick| tick_seconds(tick, smf.header.timing, &tempos);
    let mut events = Vec::with_capacity(raw.len());
    for (order, event) in raw.into_iter().enumerate() {
        let frame = (tick_to_seconds(event.tick) * sample_rate as f64).round() as u64;
        events.push(ExactMidiEvent {
            frame,
            order: order as u32,
            data: event.data,
        });
    }
    let mut length_frames = (tick_to_seconds(end_tick) * sample_rate as f64).ceil() as u64;
    if let Some(last) = events.last() {
        length_frames = length_frames.max(last.frame.saturating_add(1));
    }
    Ok(ExactMidi {
        sample_rate,
        length_frames,
        start_state: Vec::new(),
        events,
    })
}

struct RawMidiEvent {
    tick: u64,
    track: usize,
    order: usize,
    data: Vec<u8>,
}

fn tick_seconds(tick: u64, timing: Timing, tempos: &[(u64, u32)]) -> f64 {
    match timing {
        Timing::Timecode(fps, subframe) => tick as f64 / (fps.as_f32() as f64 * subframe as f64),
        Timing::Metrical(ppq) => {
            let ppq = ppq.as_int() as f64;
            let mut seconds = 0.0;
            let mut previous_tick = 0_u64;
            let mut tempo = 500_000_u32;
            for &(tempo_tick, next_tempo) in tempos {
                if tempo_tick > tick {
                    break;
                }
                seconds += (tempo_tick.saturating_sub(previous_tick)) as f64 * tempo as f64
                    / ppq
                    / 1_000_000.0;
                previous_tick = tempo_tick;
                tempo = next_tempo;
            }
            seconds + tick.saturating_sub(previous_tick) as f64 * tempo as f64 / ppq / 1_000_000.0
        }
    }
}

fn validate_exact_midi(midi: &ExactMidi) -> Result<(), SessionError> {
    if midi.sample_rate == 0 {
        return Err(SessionError::Validation(
            "MIDI sample rate must be non-zero".to_owned(),
        ));
    }
    let mut previous = None;
    for event in &midi.events {
        if midi.length_frames == 0 || event.frame >= midi.length_frames {
            return Err(SessionError::Validation(
                "MIDI event lies outside its duration".to_owned(),
            ));
        }
        if previous.is_some_and(|key| key > (event.frame, event.order)) {
            return Err(SessionError::Validation(
                "MIDI events are not ordered".to_owned(),
            ));
        }
        previous = Some((event.frame, event.order));
    }
    Ok(())
}

trait UnsupportedContext {
    fn then_archive(self, detail: String) -> SessionError;
}

impl UnsupportedContext for SessionError {
    fn then_archive(self, detail: String) -> SessionError {
        match self {
            SessionError::UnsupportedFormat => {
                SessionError::Archive(format!("unsupported or malformed media: {detail}"))
            }
            other => other,
        }
    }
}

#[allow(dead_code)]
fn _midi_message_is_supported(message: MidiMessage) -> MidiMessage {
    message
}
