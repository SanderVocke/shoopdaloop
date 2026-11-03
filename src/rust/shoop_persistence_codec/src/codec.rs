use super::document;
use anyhow::{anyhow, ensure, Result};
use shoop_app_api::*;
use shoop_app_protocol::{persistence::*, persistence_image as image};
use shoop_session::*;
use std::{collections::BTreeSet, sync::Arc};

fn codec_limits() -> CodecLimits {
    CodecLimits {
        file_bytes: MAX_SESSION_FILE_BYTES,
        metadata_bytes: MAX_SESSION_METADATA_BYTES,
        expanded_bytes: MAX_SESSION_MEDIA_BYTES * 5 + MAX_SESSION_METADATA_BYTES,
        media_bytes: MAX_SESSION_MEDIA_BYTES,
        entries: MAX_SESSION_OBJECTS as usize + 1,
        midi_events: MAX_SESSION_MIDI_EVENTS,
        packet_bytes: 1024 * 1024,
        initial_packets: 16_384,
        audio_channels: 256,
    }
}

pub enum Decoded {
    Remote(crate::worker::Summary),
    Session(SessionBundle),
    Audio(LoopAudio),
    Midi(ExactMidi),
}
fn midi_bytes(m: &ExactMidi) -> u64 {
    m.events
        .iter()
        .map(|e| e.data.len() as u64 + 12)
        .sum::<u64>()
        + m.start_state
            .iter()
            .map(|p| p.len() as u64 + 4)
            .sum::<u64>()
}

impl Decoded {
    pub fn rate(&self) -> u32 {
        match self {
            Self::Remote(summary) => summary.rate,
            Self::Session(b) => b.document.sample_rate,
            Self::Audio(a) => a.sample_rate,
            Self::Midi(m) => m.sample_rate,
        }
    }
    pub fn audio_labels(&self) -> Option<Vec<String>> {
        match self {
            Self::Audio(audio) => Some(
                audio
                    .channels
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        if c.label.is_empty() {
                            format!("Audio {}", i + 1)
                        } else {
                            c.label.clone()
                        }
                    })
                    .collect(),
            ),
            Self::Remote(summary) => summary.audio_labels.clone(),
            _ => None,
        }
    }
    fn bytes(&self) -> Result<u64> {
        let (bytes, events) = match self {
            Self::Remote(_) => anyhow::bail!("decoded input belongs to the browser codec worker"),
            Self::Session(b) => {
                b.media
                    .values()
                    .fold((0, 0), |(bytes, events), payload| match payload {
                        MediaPayload::Audio(a) => (bytes + a.samples.len() as u64 * 4, events),
                        MediaPayload::Midi(m) => {
                            (bytes + midi_bytes(m), events + m.events.len() as u64)
                        }
                    })
            }
            Self::Audio(a) => (
                a.channels.iter().map(|c| c.samples.len() as u64 * 4).sum(),
                0,
            ),
            Self::Midi(m) => (midi_bytes(m), m.events.len() as u64),
        };
        ensure!(
            bytes <= MAX_SESSION_MEDIA_BYTES && events <= MAX_SESSION_MIDI_EVENTS,
            "decoded media exceeds 256 MiB or one million MIDI events"
        );
        Ok(bytes)
    }
    fn check_conversion_budget(&self, rate: u32, limit: u64) -> Result<()> {
        self.bytes()?;
        ensure!(self.rate() > 0 && rate > 0, "sample rates must be non-zero");
        let audio_bytes = |frames: u64| -> Result<u128> {
            Ok(u128::from(scale_duration(frames, self.rate(), rate)?) * 4)
        };
        let estimate = match self {
            Self::Remote(_) => anyhow::bail!("decoded input belongs to the browser codec worker"),
            Self::Midi(midi) => u128::from(midi_bytes(midi)),
            Self::Audio(audio) => audio.channels.iter().try_fold(0u128, |bytes, channel| {
                Ok::<_, anyhow::Error>(bytes + audio_bytes(channel.samples.len() as u64)?)
            })?,
            Self::Session(bundle) => {
                let source = bundle.document.sample_rate;
                let mut lengths = std::collections::BTreeMap::new();
                let mut bytes = 0u128;
                for (id, payload) in &bundle.media {
                    match payload {
                        MediaPayload::Audio(audio) => {
                            lengths.insert(
                                id.as_str(),
                                scale_duration(audio.samples.len() as u64, source, rate)?,
                            );
                        }
                        MediaPayload::Midi(midi) => bytes += u128::from(midi_bytes(midi)),
                    }
                }
                for content in bundle
                    .document
                    .track_groups
                    .iter()
                    .flat_map(|g| &g.tracks)
                    .flat_map(|t| &t.loops)
                {
                    for channel in &content.channels {
                        if channel.capture_alignment_frames == 0 {
                            continue;
                        }
                        let Some(length) = channel
                            .media_id
                            .as_deref()
                            .and_then(|id| lengths.get_mut(id))
                        else {
                            continue;
                        };
                        let required =
                            i128::from(scale_signed_nearest(
                                channel.start_offset_frames,
                                source,
                                rate,
                            )?) + i128::from(scale_signed_nearest(
                                channel.capture_alignment_frames,
                                source,
                                rate,
                            )?) + i128::from(scale_duration(content.length_frames, source, rate)?);
                        *length = (*length).max(u64::try_from(required)?);
                    }
                }
                bytes
                    + lengths
                        .values()
                        .map(|frames| u128::from(*frames) * 4)
                        .sum::<u128>()
            }
        };
        ensure!(
            estimate <= u128::from(limit),
            "converted media would exceed the 256 MiB budget"
        );
        Ok(())
    }
}
pub enum Work {
    Assemble(Vec<shoop_app_protocol::persistence::Object>),
    Decode {
        kind: IoTaskKind,
        bytes: Arc<Vec<u8>>,
        rate: u32,
    },
    Resample {
        decoded: Decoded,
        rate: u32,
    },
    Prepare {
        decoded: Decoded,
        mapping: Vec<u32>,
        update_length: bool,
    },
    Encode {
        image: Arc<image::Image>,
        kind: IoTaskKind,
        wav: bool,
        standard: bool,
        channels: Vec<u32>,
    },
}
pub enum Value {
    Captured(Arc<image::Image>),
    Decoded(Decoded),
    Image(Arc<image::Image>),
    File {
        name: String,
        mime: String,
        bytes: Arc<Vec<u8>>,
        warning: String,
    },
}
impl Work {
    pub fn run(self) -> Result<Value> {
        match self {
            Self::Assemble(objects) => Ok(Value::Captured(
                shoop_app_protocol::persistence_transfer::assemble(objects)
                    .map_err(|r| anyhow!("invalid captured image: {r:?}"))?,
            )),
            Self::Decode { kind, bytes, rate } => {
                ensure!(
                    bytes.len() as u64 <= MAX_SESSION_FILE_BYTES,
                    "file exceeds the 512 MiB input limit"
                );
                let decoded = match kind {
                    IoTaskKind::LoadSession => {
                        Decoded::Session(decode_session_bounded(&bytes, codec_limits(), |doc| {
                            document::requirements(doc)
                                .map_err(|e| SessionError::Validation(e.to_string()))
                        })?)
                    }
                    IoTaskKind::ImportLoopAudio if bytes.starts_with(b"RIFF") => {
                        Decoded::Audio(decode_wav_bounded(&bytes, codec_limits())?)
                    }
                    IoTaskKind::ImportLoopAudio => {
                        Decoded::Audio(decode_loop_audio_bounded(&bytes, codec_limits())?)
                    }
                    IoTaskKind::ImportLoopMidi if bytes.starts_with(b"MThd") => {
                        Decoded::Midi(decode_standard_midi_bounded(&bytes, rate, codec_limits())?)
                    }
                    IoTaskKind::ImportLoopMidi => {
                        Decoded::Midi(decode_exact_midi_bounded(&bytes, codec_limits())?)
                    }
                    _ => anyhow::bail!("unexpected file task"),
                };
                decoded.bytes()?;
                Ok(Value::Decoded(decoded))
            }
            Self::Resample { decoded, rate } => {
                decoded.check_conversion_budget(rate, MAX_SESSION_MEDIA_BYTES)?;
                let decoded = match decoded {
                    Decoded::Remote(_) => {
                        anyhow::bail!("decoded input belongs to the browser codec worker")
                    }
                    Decoded::Session(b) => Decoded::Session(resample_session(&b, rate)?),
                    Decoded::Audio(a) => Decoded::Audio(resample_loop_audio(&a, rate)?),
                    Decoded::Midi(m) => Decoded::Midi(resample_exact_midi(&m, rate)?),
                };
                decoded.bytes()?;
                Ok(Value::Decoded(decoded))
            }
            Self::Prepare {
                decoded,
                mapping,
                update_length,
            } => {
                decoded.bytes()?;
                let image = match decoded {
                    Decoded::Remote(_) => {
                        anyhow::bail!("decoded input belongs to the browser codec worker")
                    }
                    Decoded::Session(b) => document::from_bundle(b)?,
                    Decoded::Audio(audio) => {
                        ensure!(!mapping.is_empty(), "no destination audio channels");
                        let mut payloads = Vec::new();
                        let mut updates = Vec::new();
                        let mut frames = 0;
                        let channels = audio
                            .channels
                            .into_iter()
                            .map(|c| Arc::<Vec<f32>>::from(c.samples))
                            .collect::<Vec<_>>();
                        for (channel, source) in mapping.into_iter().enumerate() {
                            let samples = channels.get(source as usize).ok_or_else(|| {
                                anyhow!("audio mapping refers to a missing source channel")
                            })?;
                            frames = frames.max(samples.len() as u64);
                            let object = if samples.is_empty() {
                                None
                            } else {
                                payloads.push(image::Payload::Audio(Arc::clone(samples)));
                                Some(payloads.len() as u32)
                            };
                            updates.push(image::AudioUpdate {
                                channel: channel as u32,
                                content: image::Channel {
                                    object,
                                    frames: samples.len() as u64,
                                    gain: 1.,
                                    start_offset: 0,
                                    capture_alignment: 0,
                                    preplay: 0,
                                    recording_started_at: None,
                                },
                            });
                        }
                        Arc::new(image::Image {
                            manifest: image::Manifest::Update {
                                sample_rate: audio.sample_rate,
                                frames: update_length.then_some(frames),
                                audio: updates,
                                midi: Vec::new(),
                            },
                            payloads,
                        })
                    }
                    Decoded::Midi(midi) => {
                        let mut payloads = Vec::new();
                        let object = if midi.events.is_empty() {
                            None
                        } else {
                            payloads.push(image::Payload::Midi(
                                midi.events
                                    .into_iter()
                                    .map(|e| {
                                        Ok(MidiEventState {
                                            frame: u32::try_from(e.frame)?,
                                            data: e.data.into(),
                                        })
                                    })
                                    .collect::<Result<Vec<_>>>()?
                                    .into(),
                            ));
                            Some(1)
                        };
                        Arc::new(image::Image {
                            manifest: image::Manifest::Update {
                                sample_rate: midi.sample_rate,
                                frames: update_length.then_some(midi.length_frames),
                                audio: Vec::new(),
                                midi: vec![image::MidiUpdate {
                                    channel: 0,
                                    content: image::MidiChannel {
                                        channel: image::Channel {
                                            object,
                                            frames: midi.length_frames,
                                            gain: 1.,
                                            start_offset: 0,
                                            capture_alignment: 0,
                                            preplay: 0,
                                            recording_started_at: None,
                                        },
                                        initial: midi.start_state,
                                    },
                                }],
                            },
                            payloads,
                        })
                    }
                };
                image
                    .validate()
                    .map_err(|r| anyhow!("invalid or oversized media: {r:?}"))?;
                validate_initials(&image)?;
                Ok(Value::Image(image))
            }
            Self::Encode {
                image,
                kind,
                wav,
                standard,
                channels,
            } => {
                let mut warning = String::new();
                let (name, mime, bytes) = if kind == IoTaskKind::SaveSession {
                    (
                        "session.shoop",
                        "application/x-shoop-session",
                        encode_session_bounded(
                            &document::to_bundle(&image)?,
                            env!("CARGO_PKG_VERSION"),
                            codec_limits(),
                        )?,
                    )
                } else {
                    let image::Manifest::Loop {
                        sample_rate,
                        content,
                    } = &image.manifest
                    else {
                        anyhow::bail!("expected loop capture")
                    };
                    if kind == IoTaskKind::ExportLoopAudio {
                        ensure!(
                            !channels.is_empty()
                                && channels.iter().copied().collect::<BTreeSet<_>>().len()
                                    == channels.len(),
                            "select distinct audio channels"
                        );
                        ensure!(
                            content
                                .frames
                                .saturating_mul(4)
                                .saturating_mul(channels.len() as u64)
                                <= MAX_SESSION_MEDIA_BYTES,
                            "logical export exceeds the media budget"
                        );
                        let mut output = Vec::new();
                        for index in channels {
                            let channel = content.audio.get(index as usize).ok_or_else(|| {
                                anyhow!("audio selection refers to a missing channel")
                            })?;
                            let samples: &[f32] = if let Some(object) = channel.object {
                                let image::Payload::Audio(samples) =
                                    &image.payloads[object as usize - 1]
                                else {
                                    anyhow::bail!("invalid audio capture")
                                };
                                &**samples
                            } else {
                                &[]
                            };
                            let mut normalized = Vec::new();
                            normalized.try_reserve_exact(content.frames as usize)?;
                            for frame in 0..content.frames {
                                let source = i128::from(frame)
                                    + i128::from(channel.start_offset)
                                    + i128::from(channel.capture_alignment);
                                normalized.push(
                                    usize::try_from(source)
                                        .ok()
                                        .and_then(|i| samples.get(i))
                                        .copied()
                                        .unwrap_or(0.),
                                );
                            }
                            output.push(LoopAudioChannel {
                                label: format!("Audio {}", index + 1),
                                role: "direct".into(),
                                samples: normalized,
                            });
                        }
                        let audio = LoopAudio {
                            sample_rate: *sample_rate,
                            channels: output,
                        };
                        if wav {
                            ("loop.wav", "audio/wav", encode_float_wav(&audio)?)
                        } else {
                            (
                                "loop.shoop-audio",
                                "application/x-shoop-audio",
                                encode_loop_audio_bounded(&audio, codec_limits())?,
                            )
                        }
                    } else {
                        let channel = content
                            .midi
                            .first()
                            .ok_or_else(|| anyhow!("loop has no MIDI channel"))?;
                        let mut midi = document::exact_midi(&image, channel, *sample_rate)?;
                        // Export logical time, preserving stable order at equal frames.
                        let origin = i128::from(channel.channel.start_offset)
                            + i128::from(channel.channel.capture_alignment);
                        if origin > 0
                            && midi
                                .events
                                .iter()
                                .any(|event| i128::from(event.frame) < origin)
                        {
                            let mut state = rt_runtime::midi_state::MidiState::default();
                            for packet in midi.start_state.iter().map(Vec::as_slice).chain(
                                midi.events
                                    .iter()
                                    .take_while(|event| i128::from(event.frame) < origin)
                                    .map(|event| event.data.as_slice()),
                            ) {
                                ensure!(matches!(packet.first(), Some(0x80..=0xef | 0xff)), "MIDI preroll requires unsupported vendor/system state; save the complete session to preserve it (P11)");
                                state
                                    .apply_packet(packet, 1024 * 1024)
                                    .map_err(|error| anyhow!("invalid MIDI preroll: {error:?}"))?;
                            }
                            let mut initial =
                                Vec::with_capacity(shoop_loops::midi_restore::RESTORE_MAX_EVENTS);
                            let report = shoop_loops::midi_restore::restore(&state, |packet| {
                                initial.push(packet.to_vec());
                                Ok(())
                            });
                            ensure!(!report.parameter_history_untracked && report.first_error.is_none(), "MIDI preroll requires unsupported parameter history; save the complete session to preserve it (P11)");
                            midi.start_state = initial;
                        }
                        midi.events = midi
                            .events
                            .into_iter()
                            .filter_map(|mut event| {
                                let frame = i128::from(event.frame) - origin;
                                (frame >= 0 && frame < i128::from(content.frames)).then(|| {
                                    event.frame = frame as u64;
                                    event
                                })
                            })
                            .collect();
                        midi.length_frames = content.frames;
                        if standard {
                            let export = encode_standard_midi(&midi)?;
                            warning = format!("Standard MIDI quantizes timing (maximum error {:.3} frames); use exact MIDI to preserve frame positions and full state.", export.max_quantization_error_frames);
                            ("loop.mid", "audio/midi", export.bytes)
                        } else {
                            (
                                "loop.shoop-midi",
                                "application/x-shoop-midi",
                                encode_exact_midi_bounded(&midi, codec_limits())?,
                            )
                        }
                    }
                };
                ensure!(
                    bytes.len() as u64 <= MAX_SESSION_FILE_BYTES,
                    "encoded file exceeds the 512 MiB output limit"
                );
                let name = if let image::Manifest::Loop { content, .. } = &image.manifest {
                    let stem = content
                        .name
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect::<String>();
                    format!(
                        "{}.{}",
                        if stem.is_empty() { "loop" } else { &stem },
                        name.split_once('.').map_or("", |(_, extension)| extension)
                    )
                } else {
                    name.to_owned()
                };
                Ok(Value::File {
                    name,
                    mime: mime.into(),
                    bytes: bytes.into(),
                    warning,
                })
            }
        }
    }
}

fn validate_initials(image: &image::Image) -> Result<()> {
    let channels: Vec<&image::MidiChannel> = match &image.manifest {
        image::Manifest::Session(session) => session
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .flat_map(|l| &l.midi)
            .collect(),
        image::Manifest::Loop { content, .. } => content.midi.iter().collect(),
        image::Manifest::Update { midi, .. } => midi.iter().map(|u| &u.content).collect(),
    };
    for channel in channels {
        ensure!(
            channel.initial.len() <= 16_384,
            "MIDI initial state exceeds the current engine limit (P11)"
        );
        let mut state = rt_runtime::midi_state::MidiState::default();
        let origin = i128::from(channel.channel.start_offset)
            + i128::from(channel.channel.capture_alignment);
        let events = channel
            .channel
            .object
            .and_then(|object| image.payloads.get(object as usize - 1));
        let mut count = 0;
        let mut apply = |packet: &[u8]| -> Result<()> {
            count += 1;
            ensure!(
                count <= 16_384,
                "MIDI initial/preroll state exceeds the current engine limit (P11)"
            );
            ensure!(
                matches!(packet.first(), Some(0x80..=0xef | 0xff)),
                "MIDI initial/preroll vendor or system state is not supported yet (P11)"
            );
            state
                .apply_packet(packet, 3)
                .map_err(|e| anyhow!("unsupported MIDI initial/preroll packet: {e:?} (P11)"))?;
            Ok(())
        };
        for packet in &channel.initial {
            apply(packet)?;
        }
        if let Some(image::Payload::Midi(events)) = events {
            for event in events
                .iter()
                .take_while(|event| i128::from(event.frame) < origin)
            {
                apply(&event.data)?;
            }
        }
        ensure!(
            (0..16).all(|channel| !state
                .channel(channel)
                .unwrap()
                .parameter_history_untracked()),
            "MIDI initial/preroll parameter history is not supported yet (P11)"
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod shared_midi_tests;
