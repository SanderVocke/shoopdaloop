//! Asynchronous loop content reads, assembled from chunked bulk reads.
//!
//! A read is started by asking for content and continues whenever the adapter pumps; asking
//! again returns the content once every chunk arrived. Audio is read with every loop channel
//! interleaved, so one engine export serves all channels. After the first chunk announces the
//! length, further chunks are requested ahead as far as the bulk credits allow. A read belongs
//! to one version of the loop's content and is replaced when the content changes.

use std::collections::BTreeMap;
use std::sync::Arc;

use shoop_backend::{
    BackendAudioChannelData, BackendAudioData, BackendChannelMode, BackendLoopId,
    BackendMidiChannelData, BackendMidiData, BackendMidiEvent,
};
use shoop_rt_protocol::{
    BulkChunk, BulkPayload, BulkRead, BulkSelector, LoopSlotId, Refusal, BULK_CHUNK_BYTES,
    BULK_CHUNK_SAMPLES,
};

#[derive(Clone, Debug)]
pub(crate) enum Outcome<T> {
    Reading,
    Done(T),
    Failed(String),
}

struct Read<T> {
    version: u64,
    slot: LoopSlotId,
    /// Audio channels the application wants, out of the interleaved loop channels.
    channels: usize,
    /// Offset of the next chunk to ask for: frames for audio, bytes for MIDI.
    next: u64,
    total: Option<u64>,
    /// Frames or bytes received.
    received: u64,
    waiting: BTreeMap<u32, u64>,
    samples: BTreeMap<u64, Vec<f32>>,
    bytes: BTreeMap<u64, Vec<u8>>,
    outcome: Outcome<T>,
}

impl<T> Read<T> {
    fn new(version: u64, slot: LoopSlotId, channels: usize) -> Self {
        Self {
            version,
            slot,
            channels,
            next: 0,
            total: None,
            received: 0,
            waiting: BTreeMap::new(),
            samples: BTreeMap::new(),
            bytes: BTreeMap::new(),
            outcome: Outcome::Reading,
        }
    }

    /// The offset of the next chunk to ask for, if any.
    fn wanted(&self) -> Option<u64> {
        if !matches!(self.outcome, Outcome::Reading) {
            return None;
        }
        match self.total {
            None if self.waiting.is_empty() && self.next == 0 => Some(0),
            Some(total) if self.next < total => Some(self.next),
            _ => None,
        }
    }
}

pub(crate) struct Reads {
    next_id: u32,
    /// Audio channels of every engine loop.
    loop_channels: usize,
    audio: BTreeMap<BackendLoopId, Read<BackendAudioData>>,
    midi: BTreeMap<BackendLoopId, Read<BackendMidiData>>,
    waiting: BTreeMap<u32, (BackendLoopId, bool)>,
}

impl Default for Reads {
    fn default() -> Self {
        Self {
            next_id: 0,
            loop_channels: 1,
            audio: BTreeMap::new(),
            midi: BTreeMap::new(),
            waiting: BTreeMap::new(),
        }
    }
}

fn refusal_text(refusal: Refusal) -> String {
    format!("the engine refused the content read: {refusal:?}")
}

impl Reads {
    pub fn set_loop_channels(&mut self, channels: u8) {
        self.loop_channels = usize::from(channels).max(1);
    }

    fn audio_chunk_frames(&self) -> u64 {
        (BULK_CHUNK_SAMPLES / self.loop_channels).max(1) as u64
    }

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
            .find_map(|(id, read)| Some((*id, false, read.slot, read.wanted()?)));
        let (loop_id, midi, slot, offset) = audio.or_else(|| {
            self.midi
                .iter()
                .find_map(|(id, read)| Some((*id, true, read.slot, read.wanted()?)))
        })?;
        self.next_id = self.next_id.wrapping_add(1);
        Some((
            loop_id,
            midi,
            BulkRead {
                id: self.next_id,
                loop_id: slot,
                selector: if midi {
                    BulkSelector::Midi
                } else {
                    BulkSelector::AudioFrames
                },
                offset,
            },
        ))
    }

    pub fn sent(&mut self, loop_id: BackendLoopId, midi: bool, id: u32) {
        let step = if midi {
            BULK_CHUNK_BYTES as u64
        } else {
            self.audio_chunk_frames()
        };
        let read = if midi {
            self.midi
                .get_mut(&loop_id)
                .map(|read| (&mut read.waiting, &mut read.next))
        } else {
            self.audio
                .get_mut(&loop_id)
                .map(|read| (&mut read.waiting, &mut read.next))
        };
        if let Some((waiting, next)) = read {
            waiting.insert(id, *next);
            *next += step;
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
        let loop_channels = self.loop_channels;
        if midi {
            let Some(read) = self.midi.get_mut(&loop_id) else {
                return;
            };
            if read.waiting.remove(&chunk.id).is_none() {
                return;
            }
            match chunk.payload {
                BulkPayload::Midi { len, bytes } => {
                    let data = bytes[..usize::from(len).min(bytes.len())].to_vec();
                    read.received += data.len() as u64;
                    read.bytes.insert(chunk.offset, data);
                    read.total = Some(chunk.total);
                }
                BulkPayload::Refused(refusal) => {
                    read.outcome = Outcome::Failed(refusal_text(refusal))
                }
                BulkPayload::Audio { .. } => {
                    read.outcome = Outcome::Failed("an audio chunk for a MIDI read".to_owned())
                }
            }
            if matches!(read.outcome, Outcome::Reading)
                && read.total.is_some_and(|total| read.received >= total)
            {
                let bytes = std::mem::take(&mut read.bytes)
                    .into_values()
                    .collect::<Vec<_>>();
                read.outcome = match midi_data(&bytes.concat(), read.version, length(loop_id)) {
                    Ok(data) => Outcome::Done(data),
                    Err(message) => Outcome::Failed(message),
                };
            }
            return;
        }
        let Some(read) = self.audio.get_mut(&loop_id) else {
            return;
        };
        if read.waiting.remove(&chunk.id).is_none() {
            return;
        }
        match chunk.payload {
            BulkPayload::Audio { len, samples } => {
                let data = samples[..usize::from(len).min(samples.len())].to_vec();
                read.received += (data.len() / loop_channels) as u64;
                read.samples.insert(chunk.offset, data);
                read.total = Some(chunk.total);
            }
            BulkPayload::Refused(refusal) => read.outcome = Outcome::Failed(refusal_text(refusal)),
            BulkPayload::Midi { .. } => {
                read.outcome = Outcome::Failed("a MIDI chunk for an audio read".to_owned())
            }
        }
        if matches!(read.outcome, Outcome::Reading)
            && read.total.is_some_and(|total| read.received >= total)
        {
            let frames = read.received as usize;
            let mut channels = vec![Vec::with_capacity(frames); read.channels];
            for part in std::mem::take(&mut read.samples).into_values() {
                for frame in part.chunks_exact(loop_channels) {
                    for (channel, samples) in channels.iter_mut().enumerate() {
                        samples.push(frame.get(channel).copied().unwrap_or(0.0));
                    }
                }
            }
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
