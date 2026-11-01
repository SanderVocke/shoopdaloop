use super::document;
use anyhow::{anyhow, ensure, Result};
use shoop_app_api::*;
use shoop_app_protocol::{persistence::*, persistence_image as image};
use shoop_session::*;
use std::sync::Arc;

pub enum Decoded {
    Remote(crate::worker::Summary),
    Session(SessionBundle),
    Audio(LoopAudio),
    Midi(ExactMidi),
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
        fn media_bytes(m: &ExactMidi) -> u64 {
            m.events
                .iter()
                .map(|e| e.data.len() as u64 + 12)
                .sum::<u64>()
                + m.start_state
                    .iter()
                    .map(|p| p.len() as u64 + 4)
                    .sum::<u64>()
        }
        let (bytes, events) = match self {
            Self::Remote(_) => anyhow::bail!("decoded input belongs to the browser codec worker"),
            Self::Session(b) => {
                b.media
                    .values()
                    .fold((0, 0), |(bytes, events), payload| match payload {
                        MediaPayload::Audio(a) => (bytes + a.samples.len() as u64 * 4, events),
                        MediaPayload::Midi(m) => {
                            (bytes + media_bytes(m), events + m.events.len() as u64)
                        }
                    })
            }
            Self::Audio(a) => (
                a.channels.iter().map(|c| c.samples.len() as u64 * 4).sum(),
                0,
            ),
            Self::Midi(m) => (media_bytes(m), m.events.len() as u64),
        };
        ensure!(
            bytes <= MAX_SESSION_MEDIA_BYTES && events <= MAX_SESSION_MIDI_EVENTS,
            "decoded media exceeds 256 MiB or one million MIDI events"
        );
        Ok(bytes)
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
                    IoTaskKind::LoadSession => Decoded::Session(decode_session_with_limits(
                        &bytes,
                        DecodeLimits {
                            max_entries: MAX_SESSION_OBJECTS as usize + 1,
                            max_uncompressed_bytes: MAX_SESSION_FILE_BYTES,
                        },
                    )?),
                    IoTaskKind::ImportLoopAudio if bytes.starts_with(b"RIFF") => {
                        Decoded::Audio(decode_wav(&bytes)?)
                    }
                    IoTaskKind::ImportLoopAudio => Decoded::Audio(decode_loop_audio(&bytes)?),
                    IoTaskKind::ImportLoopMidi if bytes.starts_with(b"MThd") => {
                        Decoded::Midi(decode_standard_midi(&bytes, rate)?)
                    }
                    IoTaskKind::ImportLoopMidi => Decoded::Midi(decode_exact_midi(&bytes)?),
                    _ => anyhow::bail!("unexpected file task"),
                };
                decoded.bytes()?;
                Ok(Value::Decoded(decoded))
            }
            Self::Resample { decoded, rate } => {
                let estimate = (u128::from(decoded.bytes()?) * u128::from(rate))
                    .div_ceil(u128::from(decoded.rate()));
                ensure!(
                    estimate <= u128::from(MAX_SESSION_MEDIA_BYTES),
                    "converted media would exceed the 256 MiB budget"
                );
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
                        encode_session(&document::to_bundle(&image)?, env!("CARGO_PKG_VERSION"))?,
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
                        ensure!(!channels.is_empty(), "select at least one audio channel");
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
                            ensure!(
                                content
                                    .frames
                                    .saturating_mul(4)
                                    .saturating_mul(content.audio.len() as u64)
                                    <= MAX_SESSION_MEDIA_BYTES,
                                "logical export exceeds the media budget"
                            );
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
                                encode_loop_audio(&audio)?,
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
                        ensure!(origin <= 0 || !midi.events.iter().any(|e| i128::from(e.frame) < origin), "MIDI export needs preroll state folding; save the session to preserve this take");
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
                                encode_exact_midi(&midi)?,
                            )
                        }
                    }
                };
                ensure!(
                    bytes.len() as u64 <= MAX_SESSION_FILE_BYTES,
                    "encoded file exceeds the 512 MiB output limit"
                );
                Ok(Value::File {
                    name: name.into(),
                    mime: mime.into(),
                    bytes: bytes.into(),
                    warning,
                })
            }
        }
    }
}
