//! Core application state and immutable media, independent of file formats and engines.
use crate::persistence::{MAX_SESSION_MEDIA_BYTES, MAX_SESSION_MIDI_EVENTS, MAX_SESSION_OBJECTS};
use crate::Refusal;
use serde::{Deserialize, Serialize};
use shoop_app_api::*;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Controls {
    pub input_gain_db: f32,
    pub input_balance: f32,
    pub monitoring: bool,
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub muted: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Channel {
    pub object: Option<u32>,
    pub frames: u64,
    pub gain: f32,
    pub start_offset: i64,
    pub capture_alignment: i64,
    pub preplay: u64,
    pub recording_started_at: Option<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MidiChannel {
    pub channel: Channel,
    pub initial: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Loop {
    pub id: LoopId,
    pub name: String,
    pub sync: bool,
    pub frames: u64,
    pub gain: f32,
    pub balance: f32,
    pub audio: Vec<Channel>,
    pub midi: Vec<MidiChannel>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Port {
    pub state: ApplicationPortState,
    pub external_connections: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub port_name_base: String,
    pub sync: bool,
    pub audio_channels: u32,
    pub midi: bool,
    pub controls: Controls,
    pub ports: Vec<Port>,
    pub loops: Vec<Loop>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MasterChannel {
    pub id: BusChannelId,
    pub label: String,
    pub port: Port,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Master {
    pub id: BusId,
    pub name: String,
    pub gain_db: f32,
    pub balance: f32,
    pub muted: bool,
    pub channels: Vec<MasterChannel>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Session {
    pub sample_rate: u32,
    pub global: GlobalControlState,
    pub tracks: Vec<Track>,
    pub master: Master,
    pub global_ports: Vec<Port>,
    pub routes: Vec<MixerRouteState>,
    pub selected: Vec<LoopId>,
    pub target: Option<LoopId>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioUpdate {
    pub channel: u32,
    pub content: Channel,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MidiUpdate {
    pub channel: u32,
    pub content: MidiChannel,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Manifest {
    Session(Session),
    Loop {
        sample_rate: u32,
        content: Loop,
    },
    Update {
        sample_rate: u32,
        frames: Option<u64>,
        audio: Vec<AudioUpdate>,
        midi: Vec<MidiUpdate>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    Audio(Arc<Vec<f32>>),
    Midi(Arc<[MidiEventState]>),
}
impl Payload {
    pub fn bytes(&self) -> Option<u64> {
        match self {
            Self::Audio(samples) => (samples.len() as u64).checked_mul(4),
            Self::Midi(events) => events
                .iter()
                .try_fold(0u64, |n, event| n.checked_add(12 + event.data.len() as u64)),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub manifest: Manifest,
    /// Object zero is metadata; these payloads have indices starting at one.
    pub payloads: Vec<Payload>,
}
impl Image {
    pub fn validate(&self) -> Result<u64, Refusal> {
        if self.payloads.len() >= MAX_SESSION_OBJECTS as usize {
            return Err(Refusal::Invalid);
        }
        let mut bytes = 0u64;
        let mut events = 0u64;
        for payload in &self.payloads {
            bytes = bytes
                .checked_add(payload.bytes().ok_or(Refusal::Invalid)?)
                .ok_or(Refusal::Invalid)?;
            match payload {
                Payload::Audio(samples) if samples.iter().any(|sample| !sample.is_finite()) => {
                    return Err(Refusal::Invalid)
                }
                Payload::Midi(midi) => {
                    events += midi.len() as u64;
                }
                _ => {}
            }
        }
        if bytes > MAX_SESSION_MEDIA_BYTES || events > MAX_SESSION_MIDI_EVENTS {
            return Err(Refusal::Overloaded);
        }
        let mut referenced = BTreeSet::new();
        let mut check = |channel: &Channel, midi: bool| -> Result<(), Refusal> {
            if !channel.gain.is_finite()
                || channel.frames > u32::MAX as u64
                || i32::try_from(channel.start_offset).is_err()
                || i32::try_from(channel.capture_alignment).is_err()
                || channel.preplay != 0
                || channel
                    .recording_started_at
                    .as_ref()
                    .is_some_and(|value| value.len() > 128)
            {
                return Err(Refusal::Invalid);
            }
            if let Some(object) = channel.object {
                if object == 0 || !referenced.insert(object) {
                    return Err(Refusal::Identity);
                }
                match self.payloads.get(object as usize - 1) {
                    Some(Payload::Audio(samples))
                        if !midi && samples.len() as u64 == channel.frames => {}
                    Some(Payload::Midi(events)) if midi => {
                        let mut previous = 0;
                        for event in events.iter() {
                            if u64::from(event.frame) >= channel.frames
                                || event.frame < previous
                                || event.data.is_empty()
                                || event.data.len() > 1024 * 1024
                            {
                                return Err(Refusal::Invalid);
                            }
                            previous = event.frame;
                        }
                    }
                    _ => return Err(Refusal::Invalid),
                }
            } else if !midi && channel.frames != 0 {
                return Err(Refusal::Invalid);
            }
            Ok(())
        };
        let loops: Vec<&Loop> = match &self.manifest {
            Manifest::Session(session) => {
                if session.sample_rate == 0
                    || session.tracks.len() > 64
                    || session
                        .tracks
                        .iter()
                        .map(|track| track.loops.len())
                        .sum::<usize>()
                        > 256
                {
                    return Err(Refusal::Invalid);
                }
                session
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .collect()
            }
            Manifest::Loop {
                sample_rate,
                content,
            } => {
                if *sample_rate == 0 {
                    return Err(Refusal::Invalid);
                }
                vec![content]
            }
            Manifest::Update {
                sample_rate,
                frames,
                audio,
                midi,
            } => {
                if *sample_rate == 0 || frames.is_some_and(|frames| frames > u32::MAX as u64) {
                    return Err(Refusal::Invalid);
                }
                for channel in audio {
                    check(&channel.content, false)?;
                }
                for channel in midi {
                    check(&channel.content.channel, true)?;
                }
                Vec::new()
            }
        };
        for content in loops {
            if !content.id.is_valid()
                || content.frames > u32::MAX as u64
                || !content.gain.is_finite()
                || !(-1.0..=1.0).contains(&content.balance)
                || content.name.len() > 128
            {
                return Err(Refusal::Invalid);
            }
            for channel in &content.audio {
                check(channel, false)?;
            }
            for channel in &content.midi {
                check(&channel.channel, true)?;
            }
        }
        if referenced.len() != self.payloads.len() {
            return Err(Refusal::Invalid);
        }
        Ok(bytes)
    }
}
