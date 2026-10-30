//! Asynchronous loop content reads, assembled from chunked bulk reads.
//!
//! A read is started by asking for content and continues whenever the adapter pumps; asking
//! again returns the content once every chunk arrived. A read belongs to one version of the
//! loop's content and is replaced when the content changes.

use std::collections::BTreeMap;
use std::sync::Arc;

use shoop_backend::{
    BackendAudioChannelData, BackendAudioData, BackendChannelMode, BackendLoopId,
    BackendMidiChannelData, BackendMidiData, BackendMidiEvent,
};
use shoop_rt_protocol::{BulkChunk, BulkPayload, BulkRead, BulkSelector, LoopSlotId, Refusal};

#[derive(Clone, Debug)]
pub(crate) enum Outcome<T> {
    Reading,
    Done(T),
    Failed(String),
}

struct Read<T> {
    version: u64,
    slot: LoopSlotId,
    /// Audio channels to read, or one for MIDI.
    count: usize,
    channel: usize,
    offset: u64,
    parts: Vec<Vec<u8>>,
    samples: Vec<Vec<f32>>,
    waiting: Option<u32>,
    outcome: Outcome<T>,
}

impl<T> Read<T> {
    fn new(version: u64, slot: LoopSlotId, count: usize) -> Self {
        Self {
            version,
            slot,
            count,
            channel: 0,
            offset: 0,
            parts: Vec::new(),
            samples: Vec::new(),
            waiting: None,
            outcome: Outcome::Reading,
        }
    }
}

#[derive(Default)]
pub(crate) struct Reads {
    next_id: u32,
    audio: BTreeMap<BackendLoopId, Read<BackendAudioData>>,
    midi: BTreeMap<BackendLoopId, Read<BackendMidiData>>,
    waiting: BTreeMap<u32, (BackendLoopId, bool)>,
}

fn refusal_text(refusal: Refusal) -> String {
    format!("the engine refused the content read: {refusal:?}")
}

impl Reads {
    /// The audio read of a loop's content version, started if needed.
    pub fn audio(
        &mut self,
        loop_id: BackendLoopId,
        version: u64,
        slot: LoopSlotId,
        channels: usize,
    ) -> Outcome<BackendAudioData> {
        let read = self
            .audio
            .entry(loop_id)
            .and_modify(|read| {
                if read.version != version {
                    *read = Read::new(version, slot, channels);
                }
            })
            .or_insert_with(|| Read::new(version, slot, channels));
        let outcome = read.outcome.clone();
        if let Outcome::Failed(_) = outcome {
            self.audio.remove(&loop_id);
        }
        outcome
    }

    /// The MIDI read of a loop's content version, started if needed.
    pub fn midi(
        &mut self,
        loop_id: BackendLoopId,
        version: u64,
        slot: LoopSlotId,
    ) -> Outcome<BackendMidiData> {
        let read = self
            .midi
            .entry(loop_id)
            .and_modify(|read| {
                if read.version != version {
                    *read = Read::new(version, slot, 1);
                }
            })
            .or_insert_with(|| Read::new(version, slot, 1));
        let outcome = read.outcome.clone();
        if let Outcome::Failed(_) = outcome {
            self.midi.remove(&loop_id);
        }
        outcome
    }

    pub fn forget(&mut self, loop_id: BackendLoopId) {
        self.audio.remove(&loop_id);
        self.midi.remove(&loop_id);
    }

    pub fn clear(&mut self) {
        self.audio.clear();
        self.midi.clear();
        self.waiting.clear();
    }

    /// No read waits for a chunk or still has chunks to ask for.
    pub fn idle(&self) -> bool {
        self.audio
            .values()
            .all(|read| !matches!(read.outcome, Outcome::Reading))
            && self
                .midi
                .values()
                .all(|read| !matches!(read.outcome, Outcome::Reading))
    }

    /// The next chunk to ask for. Call [`Reads::sent`] once the read is accepted.
    pub fn next(&mut self) -> Option<(BackendLoopId, bool, BulkRead)> {
        let audio = self
            .audio
            .iter()
            .find(|(_, read)| matches!(read.outcome, Outcome::Reading) && read.waiting.is_none())
            .map(|(id, read)| {
                (
                    *id,
                    false,
                    read.slot,
                    BulkSelector::Audio {
                        channel: read.channel as u8,
                    },
                    read.offset,
                )
            });
        let candidate = audio.or_else(|| {
            self.midi
                .iter()
                .find(|(_, read)| {
                    matches!(read.outcome, Outcome::Reading) && read.waiting.is_none()
                })
                .map(|(id, read)| (*id, true, read.slot, BulkSelector::Midi, read.offset))
        });
        let (loop_id, midi, slot, selector, offset) = candidate?;
        self.next_id = self.next_id.wrapping_add(1);
        Some((
            loop_id,
            midi,
            BulkRead {
                id: self.next_id,
                loop_id: slot,
                selector,
                offset,
            },
        ))
    }

    pub fn sent(&mut self, loop_id: BackendLoopId, midi: bool, id: u32) {
        let waiting = if midi {
            self.midi.get_mut(&loop_id).map(|read| &mut read.waiting)
        } else {
            self.audio.get_mut(&loop_id).map(|read| &mut read.waiting)
        };
        if let Some(waiting) = waiting {
            *waiting = Some(id);
            self.waiting.insert(id, (loop_id, midi));
        }
    }

    pub fn fail(&mut self, loop_id: BackendLoopId, midi: bool, message: String) {
        if midi {
            if let Some(read) = self.midi.get_mut(&loop_id) {
                read.outcome = Outcome::Failed(message);
            }
        } else if let Some(read) = self.audio.get_mut(&loop_id) {
            read.outcome = Outcome::Failed(message);
        }
    }

    /// Takes in a chunk; `length` is the loop's length for MIDI content.
    pub fn receive(&mut self, chunk: BulkChunk, length: impl Fn(BackendLoopId) -> u32) {
        let Some((loop_id, midi)) = self.waiting.remove(&chunk.id) else {
            return;
        };
        if midi {
            let Some(read) = self.midi.get_mut(&loop_id) else {
                return;
            };
            if read.waiting != Some(chunk.id) {
                return;
            }
            read.waiting = None;
            match chunk.payload {
                BulkPayload::Midi { len, bytes } => {
                    read.parts
                        .push(bytes[..usize::from(len).min(bytes.len())].to_vec());
                    read.offset = chunk.offset + u64::from(len);
                    if read.offset >= chunk.total {
                        let bytes = read.parts.concat();
                        read.outcome = match midi_data(&bytes, read.version, length(loop_id)) {
                            Ok(data) => Outcome::Done(data),
                            Err(message) => Outcome::Failed(message),
                        };
                    } else if len == 0 {
                        read.outcome = Outcome::Failed("an empty MIDI chunk".to_owned());
                    }
                }
                BulkPayload::Refused(refusal) => {
                    read.outcome = Outcome::Failed(refusal_text(refusal))
                }
                BulkPayload::Audio { .. } => {
                    read.outcome = Outcome::Failed("an audio chunk for a MIDI read".to_owned())
                }
            }
            return;
        }
        let Some(read) = self.audio.get_mut(&loop_id) else {
            return;
        };
        if read.waiting != Some(chunk.id) {
            return;
        }
        read.waiting = None;
        match chunk.payload {
            BulkPayload::Audio { len, samples } => {
                if read.samples.len() <= read.channel {
                    read.samples.resize(read.channel + 1, Vec::new());
                }
                read.samples[read.channel]
                    .extend_from_slice(&samples[..usize::from(len).min(samples.len())]);
                read.offset = chunk.offset + u64::from(len);
                if read.offset >= chunk.total {
                    read.channel += 1;
                    read.offset = 0;
                    if read.channel >= read.count {
                        let mut channels = std::mem::take(&mut read.samples);
                        channels.resize(read.count, Vec::new());
                        read.outcome = Outcome::Done(BackendAudioData {
                            channels: channels
                                .into_iter()
                                .map(|samples| BackendAudioChannelData {
                                    samples: Arc::from(samples),
                                    ..Default::default()
                                })
                                .collect(),
                        });
                    }
                } else if len == 0 {
                    read.outcome = Outcome::Failed("an empty audio chunk".to_owned());
                }
            }
            BulkPayload::Refused(refusal) => read.outcome = Outcome::Failed(refusal_text(refusal)),
            BulkPayload::Midi { .. } => {
                read.outcome = Outcome::Failed("a MIDI chunk for an audio read".to_owned())
            }
        }
    }
}

/// Decodes the recorded MIDI records: a little-endian `u64` frame, a little-endian `u32`
/// length and the message bytes each.
fn midi_data(bytes: &[u8], version: u64, length: u32) -> Result<BackendMidiData, String> {
    let mut events = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let malformed = || "malformed MIDI content".to_owned();
        let frame = u64::from_le_bytes(rest.get(..8).ok_or_else(malformed)?.try_into().unwrap());
        let len = u32::from_le_bytes(rest.get(8..12).ok_or_else(malformed)?.try_into().unwrap());
        let end = 12 + len as usize;
        let data = rest.get(12..end).ok_or_else(malformed)?.to_vec();
        events.push(BackendMidiEvent {
            time: u32::try_from(frame).map_err(|_| malformed())?,
            data,
        });
        rest = &rest[end..];
    }
    Ok(BackendMidiData {
        channels: vec![BackendMidiChannelData {
            content_revision: version,
            mode: BackendChannelMode::Direct,
            length,
            events,
            start_offset: 0,
            capture_alignment_frames: 0,
            preplay: 0,
        }],
    })
}

/// Content of a loop without recorded material.
pub(crate) fn empty_audio(channels: usize) -> BackendAudioData {
    BackendAudioData {
        channels: vec![BackendAudioChannelData::default(); channels],
    }
}

pub(crate) fn empty_midi(version: u64, midi: bool) -> BackendMidiData {
    BackendMidiData {
        channels: if midi {
            vec![BackendMidiChannelData {
                content_revision: version,
                mode: BackendChannelMode::Direct,
                length: 0,
                events: Vec::new(),
                start_offset: 0,
                capture_alignment_frames: 0,
                preplay: 0,
            }]
        } else {
            Vec::new()
        },
    }
}
