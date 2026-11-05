use super::document;
use anyhow::{anyhow, ensure, Result};
use shoop_app_api::*;
use shoop_app_protocol::{persistence::*, persistence_image as image};
use shoop_session::*;
use std::{collections::BTreeSet, sync::Arc};
pub mod family;

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
    pub fn frames(&self) -> Option<u64> {
        match self {
            Self::Remote(summary) => summary.frames,
            Self::Audio(audio) => Some(
                audio
                    .channels
                    .iter()
                    .map(|channel| channel.samples.len() as u64)
                    .max()
                    .unwrap_or(0),
            ),
            Self::Midi(midi) => Some(midi.length_frames),
            Self::Session(_) => None,
        }
    }
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
    fn standalone_scale(
        &self,
        target: ConversionTarget,
        update_length: bool,
    ) -> Result<FrameScale> {
        ensure!(
            target.is_valid() && self.rate() > 0,
            "invalid conversion format"
        );
        let frames = match self {
            Self::Audio(audio) => audio
                .channels
                .iter()
                .map(|channel| channel.samples.len() as u64)
                .max()
                .unwrap_or(0),
            Self::Midi(midi) => midi.length_frames,
            _ => anyhow::bail!("expected standalone media"),
        };
        if update_length && frames > 0 {
            let plan = ConversionPlan::new(
                target,
                &[LoopTiming {
                    id: 1,
                    sync_source: None,
                    frames,
                    sample_rate: self.rate(),
                }],
            )?;
            Ok(plan.loops[0].scale)
        } else {
            Ok(FrameScale::new(
                u128::from(target.sample_rate),
                u128::from(self.rate()),
            )?)
        }
    }
    fn check_conversion_budget(
        &self,
        target: ConversionTarget,
        update_length: bool,
        limit: u64,
    ) -> Result<bool> {
        self.bytes()?;
        ensure!(target.is_valid(), "invalid conversion format");
        if let Self::Session(bundle) = self {
            return Ok(SessionFormatPlan::new(bundle, target, limit)?
                .timing
                .requires_confirmation);
        }
        let scale = self.standalone_scale(target, update_length)?;
        let map = MediaTiming::new(scale, 0)?;
        let estimate = match self {
            Self::Midi(midi) => u128::from(midi_bytes(midi)),
            Self::Audio(audio) => audio.channels.iter().try_fold(0_u128, |bytes, channel| {
                Ok::<_, anyhow::Error>(
                    bytes + u128::from(map.length(channel.samples.len() as u64)?) * 4,
                )
            })?,
            _ => anyhow::bail!("decoded input belongs to the browser codec worker"),
        };
        ensure!(
            estimate <= u128::from(limit),
            "converted media would exceed the 256 MiB budget"
        );
        Ok(self.rate() != target.sample_rate || !scale.is_identity())
    }
    fn select_audio(self, mapping: &[u32]) -> Result<Self> {
        let Self::Audio(mut audio) = self else {
            return Ok(self);
        };
        ensure!(
            !mapping.is_empty() && mapping.len() <= 16,
            "invalid destination channel count"
        );
        audio.channels = mapping
            .iter()
            .map(|source| {
                audio
                    .channels
                    .get(*source as usize)
                    .cloned()
                    .ok_or_else(|| anyhow!("audio mapping refers to a missing source channel"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self::Audio(audio))
    }
}
pub enum Work {
    ConvertFamily {
        decoded: Decoded,
        base: Arc<image::Image>,
    },
    GenerateClick {
        request: ClickTrackRequest,
        rate: u32,
    },
    Assemble(Vec<shoop_app_protocol::persistence::Object>),
    Decode {
        kind: IoTaskKind,
        bytes: Arc<Vec<u8>>,
        rate: u32,
    },
    Inspect {
        decoded: Decoded,
        target: ConversionTarget,
        mapping: Vec<u32>,
        update_length: bool,
    },
    Convert {
        decoded: Decoded,
        target: ConversionTarget,
        update_length: bool,
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
    Inspected {
        decoded: Decoded,
        conversion_required: bool,
    },
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
        self.run_with_cancel(|| false)
    }
    pub fn run_with_cancel(self, mut cancelled: impl FnMut() -> bool) -> Result<Value> {
        self.run_cancelled(&mut cancelled)
    }
    fn run_cancelled(self, mut cancelled: &mut dyn FnMut() -> bool) -> Result<Value> {
        match self {
            Self::ConvertFamily { decoded, base } => {
                family::convert(decoded, base, &mut cancelled).map(Value::Image)
            }
            Self::GenerateClick { request, rate } => {
                let timing = ClickTrackTimingSpec {
                    bpm: request.bpm,
                    click_count: request.click_count,
                    odd_click_delay_percent: request.odd_click_delay_percent,
                };
                let decoded = match request.kind {
                    ClickTrackKind::Audio => Decoded::Audio(generate_audio_click_track(
                        &AudioClickTrackSpec {
                            timing,
                            primary_sound: request.primary_sound_id,
                            secondary_sound: request.secondary_sound_id,
                            secondary_clicks_per_primary: request.secondary_clicks_per_primary,
                        },
                        rate,
                    )?),
                    ClickTrackKind::Midi => Decoded::Midi(generate_midi_click_track(
                        MidiClickTrackSpec {
                            timing,
                            note: request.midi_note,
                            channel: 0,
                            velocity: 127,
                            note_length_seconds: request.midi_note_length_seconds,
                        },
                        rate,
                    )?),
                };
                decoded.bytes()?;
                Ok(Value::Decoded(decoded))
            }
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
            Self::Inspect {
                decoded,
                target,
                mapping,
                update_length,
            } => {
                let decoded = decoded.select_audio(&mapping)?;
                let conversion_required = decoded.check_conversion_budget(
                    target,
                    update_length,
                    MAX_SESSION_MEDIA_BYTES,
                )?;
                Ok(Value::Inspected {
                    decoded,
                    conversion_required,
                })
            }
            Self::Convert {
                decoded,
                target,
                update_length,
            } => {
                decoded.check_conversion_budget(target, update_length, MAX_SESSION_MEDIA_BYTES)?;
                let scale = if matches!(decoded, Decoded::Audio(_) | Decoded::Midi(_)) {
                    Some(decoded.standalone_scale(target, update_length)?)
                } else {
                    None
                };
                let decoded = match decoded {
                    Decoded::Remote(_) => {
                        anyhow::bail!("decoded input belongs to the browser codec worker")
                    }
                    Decoded::Session(bundle) => Decoded::Session(
                        SessionFormatPlan::new(&bundle, target, MAX_SESSION_MEDIA_BYTES)?
                            .convert(&bundle, &mut cancelled)?,
                    ),
                    Decoded::Audio(mut audio) => {
                        let map = MediaTiming::new(scale.expect("standalone scale"), 0)?;
                        let length = audio
                            .channels
                            .iter()
                            .map(|channel| channel.samples.len())
                            .max()
                            .unwrap_or(0);
                        for channel in &mut audio.channels {
                            let periodic = update_length && channel.samples.len() == length;
                            let mut conversion = AudioConversion::new(
                                &channel.samples,
                                map,
                                periodic,
                                MAX_SESSION_MEDIA_BYTES / 4,
                            )?;
                            while !conversion.step(4096)? {
                                ensure!(!cancelled(), "audio conversion cancelled");
                            }
                            channel.samples = conversion.finish()?;
                        }
                        audio.sample_rate = target.sample_rate;
                        Decoded::Audio(audio)
                    }
                    Decoded::Midi(midi) => Decoded::Midi(convert_midi_with_cancel(
                        &midi,
                        MediaTiming::new(scale.expect("standalone scale"), 0)?,
                        target.sample_rate,
                        &mut cancelled,
                    )?),
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
                                    mode: image::ChannelMode::Direct,
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
                                            mode: image::ChannelMode::Direct,
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
                        let labels = content.audio_labels();
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
                                label: labels[index as usize].clone(),
                                role: channel.mode.role().into(),
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
        image::Manifest::FamilyUpdate { members, .. } => members
            .iter()
            .filter_map(|member| member.content.as_ref())
            .flat_map(|content| &content.midi)
            .collect(),
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
