//! Asynchronous loop content reads, assembled from chunked bulk reads.
//!
//! A read is started by asking for content and continues whenever the adapter pumps; asking
//! again returns the content once every chunk arrived. Audio is read with every loop channel
//! interleaved, so one engine export serves all channels. After the first chunk announces the
//! length, further chunks are requested ahead as far as the bulk credits allow. A read belongs
//! to one version of the loop's content and is replaced when the content changes.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

mod assembly;
use alloc::collections::BTreeMap;

use shoop_backend::{
    BackendAudioChannelData, BackendAudioData, BackendChannelMode, BackendLoopId,
    BackendMidiChannelData, BackendMidiData,
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
    audio_data: Option<assembly::Audio>,
    midi_data: Option<assembly::Midi>,
    midi_length: u32,
    limit: u64,
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
            audio_data: None,
            midi_data: None,
            midi_length: 0,
            limit: 256 * 1024 * 1024,
            bytes: BTreeMap::new(),
            outcome: Outcome::Reading,
        }
    }

    /// The offset of the next chunk to ask for, if any.
    fn wanted(&self) -> Option<u64> {
        if !matches!(self.outcome, Outcome::Reading) || self.waiting.len() >= 16 {
            return None;
        }
        match self.total {
            None if self.waiting.is_empty() && self.next == 0 => Some(0),
            Some(total) if self.next < total => Some(self.next),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReadKind {
    Audio,
    Midi,
    Initial(u8),
}
pub(crate) struct Reads {
    next_id: u32,
    /// Audio channels of every engine loop.
    loop_channels: usize,
    audio: BTreeMap<BackendLoopId, Read<BackendAudioData>>,
    midi: BTreeMap<BackendLoopId, Read<BackendMidiData>>,
    waiting: BTreeMap<u32, (BackendLoopId, ReadKind)>,
    initial: BTreeMap<(BackendLoopId, u8), Read<Vec<Vec<u8>>>>,
}

impl Default for Reads {
    fn default() -> Self {
        Self {
            next_id: 0,
            loop_channels: 1,
            audio: BTreeMap::new(),
            midi: BTreeMap::new(),
            initial: BTreeMap::new(),
            waiting: BTreeMap::new(),
        }
    }
}

fn refusal_text(refusal: Refusal) -> String {
    format!("the engine refused the content read: {refusal:?}")
}

impl Reads {
    pub fn remember_audio(
        &mut self,
        id: BackendLoopId,
        version: u64,
        slot: LoopSlotId,
        data: BackendAudioData,
    ) {
        let mut read = Read::new(version, slot, data.channels.len());
        read.outcome = Outcome::Done(data);
        self.audio.insert(id, read);
    }
    pub fn remember_midi(
        &mut self,
        id: BackendLoopId,
        version: u64,
        slot: LoopSlotId,
        data: BackendMidiData,
    ) {
        let mut read = Read::new(version, slot, 1);
        read.outcome = Outcome::Done(data);
        self.midi.insert(id, read);
    }
    pub fn advance(&mut self) {
        for read in self.midi.values_mut() {
            if !matches!(read.outcome, Outcome::Reading)
                || !read.total.is_some_and(|total| read.received == total)
            {
                continue;
            }
            let Some(assembly) = read.midi_data.as_mut() else {
                continue;
            };
            match assembly.step(256 * 1024, read.midi_length) {
                Ok(false) => {}
                Ok(true) => {
                    let events = read
                        .midi_data
                        .take()
                        .expect("complete MIDI assembly")
                        .finish();
                    read.outcome = Outcome::Done(BackendMidiData {
                        channels: vec![BackendMidiChannelData {
                            content_revision: read.version,
                            mode: BackendChannelMode::Direct,
                            length: read.midi_length,
                            events,
                            start_offset: 0,
                            capture_alignment_frames: 0,
                            preplay: 0,
                        }],
                    });
                }
                Err(message) => read.outcome = Outcome::Failed(message),
            }
            break;
        }
    }
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
        self.midi_limited(loop_id, version, slot, 256 * 1024 * 1024)
    }
    pub fn midi_limited(
        &mut self,
        loop_id: BackendLoopId,
        version: u64,
        slot: LoopSlotId,
        limit: u64,
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
        read.limit = read.limit.min(limit);
        if read.total.is_some_and(|total| total > read.limit) {
            read.outcome =
                Outcome::Failed("MIDI capture exceeds the remaining session budget".into());
        }

        let outcome = read.outcome.clone();
        if let Outcome::Failed(_) = outcome {
            self.midi.remove(&loop_id);
        }
        outcome
    }

    pub fn initial(
        &mut self,
        loop_id: BackendLoopId,
        version: u64,
        slot: LoopSlotId,
        channel: u8,
    ) -> Outcome<Vec<Vec<u8>>> {
        let read = self
            .initial
            .entry((loop_id, channel))
            .and_modify(|read| {
                if read.version != version {
                    *read = Read::new(version, slot, 1);
                }
            })
            .or_insert_with(|| Read::new(version, slot, 1));
        read.outcome.clone()
    }
    pub fn forget(&mut self, loop_id: BackendLoopId) {
        self.audio.remove(&loop_id);
        self.midi.remove(&loop_id);
        self.initial.retain(|(id, _), _| *id != loop_id);
    }

    pub fn clear(&mut self) {
        self.audio.clear();
        self.midi.clear();
        self.initial.clear();
        self.waiting.clear();
    }

    /// No read waits for a chunk or still has chunks to ask for.
    pub fn idle(&self) -> bool {
        self.initial
            .values()
            .all(|read| !matches!(read.outcome, Outcome::Reading))
            && self
                .audio
                .values()
                .all(|read| !matches!(read.outcome, Outcome::Reading))
            && self
                .midi
                .values()
                .all(|read| !matches!(read.outcome, Outcome::Reading))
    }

    /// The next chunk to ask for. Call [`Reads::sent`] once the read is accepted.
    pub fn next(&mut self) -> Option<(BackendLoopId, ReadKind, BulkRead)> {
        let audio = self
            .audio
            .iter()
            .find_map(|(id, read)| Some((*id, ReadKind::Audio, read.slot, read.wanted()?)));
        let midi = || {
            self.midi
                .iter()
                .find_map(|(id, read)| Some((*id, ReadKind::Midi, read.slot, read.wanted()?)))
        };
        let initial = || {
            self.initial.iter().find_map(|((id, channel), read)| {
                Some((*id, ReadKind::Initial(*channel), read.slot, read.wanted()?))
            })
        };
        let (loop_id, kind, slot, offset) = audio.or_else(midi).or_else(initial)?;
        self.next_id = self.next_id.wrapping_add(1);
        Some((
            loop_id,
            kind,
            BulkRead {
                id: self.next_id,
                loop_id: slot,
                offset,
                selector: match kind {
                    ReadKind::Audio => BulkSelector::AudioFrames,
                    ReadKind::Midi => BulkSelector::Midi,
                    ReadKind::Initial(channel) => BulkSelector::MidiInitial { channel },
                },
            },
        ))
    }
    pub fn sent(&mut self, loop_id: BackendLoopId, kind: ReadKind, id: u32) {
        let step = if kind == ReadKind::Audio {
            self.audio_chunk_frames()
        } else {
            BULK_CHUNK_BYTES as u64
        };
        let read = match kind {
            ReadKind::Audio => self
                .audio
                .get_mut(&loop_id)
                .map(|read| (&mut read.waiting, &mut read.next)),
            ReadKind::Midi => self
                .midi
                .get_mut(&loop_id)
                .map(|read| (&mut read.waiting, &mut read.next)),
            ReadKind::Initial(channel) => self
                .initial
                .get_mut(&(loop_id, channel))
                .map(|read| (&mut read.waiting, &mut read.next)),
        };
        if let Some((waiting, next)) = read {
            waiting.insert(id, *next);
            *next += step;
            self.waiting.insert(id, (loop_id, kind));
        }
    }
    pub fn fail(&mut self, loop_id: BackendLoopId, kind: ReadKind, message: String) {
        match kind {
            ReadKind::Audio => {
                if let Some(read) = self.audio.get_mut(&loop_id) {
                    read.outcome = Outcome::Failed(message);
                }
            }
            ReadKind::Midi => {
                if let Some(read) = self.midi.get_mut(&loop_id) {
                    read.outcome = Outcome::Failed(message);
                }
            }
            ReadKind::Initial(channel) => {
                if let Some(read) = self.initial.get_mut(&(loop_id, channel)) {
                    read.outcome = Outcome::Failed(message);
                }
            }
        }
    }

    /// Takes in a chunk; `length` is the loop's length for MIDI content.
    pub fn receive(&mut self, chunk: BulkChunk, length: impl Fn(BackendLoopId) -> u32) {
        let Some((loop_id, kind)) = self.waiting.remove(&chunk.id) else {
            return;
        };
        let loop_channels = self.loop_channels;
        const MAX_READ_BYTES: u64 = 256 * 1024 * 1024;
        if let ReadKind::Initial(channel) = kind {
            let Some(read) = self.initial.get_mut(&(loop_id, channel)) else {
                return;
            };
            if read.waiting.remove(&chunk.id).is_none() {
                return;
            }
            if chunk.total > 16 * 1024 {
                read.outcome =
                    Outcome::Failed("MIDI initial state exceeds its bounded read limit".to_owned());
                return;
            }
            match chunk.payload {
                BulkPayload::Midi { len, bytes } => {
                    read.received += u64::from(len);
                    read.bytes
                        .insert(chunk.offset, bytes[..usize::from(len)].to_vec());
                    read.total = Some(chunk.total);
                }
                BulkPayload::Refused(refusal) => {
                    read.outcome = Outcome::Failed(refusal_text(refusal))
                }
                _ => {
                    read.outcome =
                        Outcome::Failed("audio payload for MIDI initial state".to_owned())
                }
            }
            if matches!(read.outcome, Outcome::Reading)
                && read.total.is_some_and(|total| read.received >= total)
            {
                let bytes = core::mem::take(&mut read.bytes)
                    .into_values()
                    .flatten()
                    .collect::<Vec<_>>();
                let mut at = 0;
                let mut packets = Vec::new();
                while at < bytes.len() {
                    let len = usize::from(bytes[at]);
                    at += 1;
                    if len == 0 || len > 3 || at + len > bytes.len() {
                        read.outcome = Outcome::Failed("malformed MIDI initial state".to_owned());
                        return;
                    }
                    packets.push(bytes[at..at + len].to_vec());
                    at += len;
                }
                read.outcome = Outcome::Done(packets);
            }
            return;
        }
        if kind == ReadKind::Midi {
            let Some(read) = self.midi.get_mut(&loop_id) else {
                return;
            };
            let Some(expected) = read.waiting.remove(&chunk.id) else {
                return;
            };
            match chunk.payload {
                BulkPayload::Midi { len, bytes } => {
                    let len = usize::from(len);
                    if chunk.offset != expected
                        || read.total.is_some_and(|total| total != chunk.total)
                        || len > bytes.len()
                        || chunk.total > read.limit.min(MAX_READ_BYTES)
                        || chunk
                            .offset
                            .checked_add(len as u64)
                            .is_none_or(|end| end > chunk.total)
                        || len as u64 != (chunk.total - chunk.offset).min(BULK_CHUNK_BYTES as u64)
                    {
                        read.outcome = Outcome::Failed("inconsistent MIDI read chunk".into());
                        return;
                    }
                    if read.midi_data.is_none() {
                        match assembly::Midi::new(chunk.total) {
                            Ok(assembly) => read.midi_data = Some(assembly),
                            Err(message) => {
                                read.outcome = Outcome::Failed(message);
                                return;
                            }
                        }
                        read.midi_length = length(loop_id);
                    }
                    if let Err(message) = read
                        .midi_data
                        .as_mut()
                        .unwrap()
                        .push(chunk.offset, &bytes[..len])
                    {
                        read.outcome = Outcome::Failed(message);
                        return;
                    }
                    read.received += len as u64;
                    read.total = Some(chunk.total);
                }
                BulkPayload::Refused(refusal) => {
                    read.outcome = Outcome::Failed(refusal_text(refusal))
                }
                _ => read.outcome = Outcome::Failed("an audio chunk for a MIDI read".into()),
            }
            return;
        }
        let Some(read) = self.audio.get_mut(&loop_id) else {
            return;
        };
        let Some(expected) = read.waiting.remove(&chunk.id) else {
            return;
        };
        match chunk.payload {
            BulkPayload::Audio { len, samples } => {
                let len = usize::from(len);
                let frames = len / loop_channels;
                if chunk.offset != expected
                    || read.total.is_some_and(|total| total != chunk.total)
                    || len > samples.len()
                    || !len.is_multiple_of(loop_channels)
                    || chunk
                        .offset
                        .checked_add(frames as u64)
                        .is_none_or(|end| end > chunk.total)
                    || frames as u64
                        != (chunk.total - chunk.offset)
                            .min((BULK_CHUNK_SAMPLES / loop_channels).max(1) as u64)
                {
                    read.outcome = Outcome::Failed("inconsistent audio read chunk".into());
                    return;
                }
                if read.audio_data.is_none() {
                    match assembly::Audio::new(chunk.total, loop_channels, read.channels) {
                        Ok(assembly) => read.audio_data = Some(assembly),
                        Err(message) => {
                            read.outcome = Outcome::Failed(message);
                            return;
                        }
                    }
                }
                if let Err(message) = read
                    .audio_data
                    .as_mut()
                    .unwrap()
                    .push(chunk.offset, &samples[..len])
                {
                    read.outcome = Outcome::Failed(message);
                    return;
                }
                read.received += frames as u64;
                read.total = Some(chunk.total);
            }
            BulkPayload::Refused(refusal) => read.outcome = Outcome::Failed(refusal_text(refusal)),
            _ => read.outcome = Outcome::Failed("a MIDI chunk for an audio read".into()),
        }
        if matches!(read.outcome, Outcome::Reading)
            && read.total.is_some_and(|total| read.received == total)
        {
            read.outcome = Outcome::Done(
                read.audio_data
                    .take()
                    .expect("complete audio assembly")
                    .finish(),
            );
        }
    }
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
                events: Vec::new().into(),
                start_offset: 0,
                capture_alignment_frames: 0,
                preplay: 0,
            }]
        } else {
            Vec::new()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn oversized_bulk_totals_fail_before_allocating_media() {
        for midi in [false, true] {
            let mut reads = Reads::default();
            let id = BackendLoopId::from_raw(1);
            let slot = LoopSlotId(shoop_rt_protocol::SlotId {
                index: 0,
                generation: 1,
            });
            if midi {
                reads.midi(id, 1, slot);
            } else {
                reads.audio(id, 1, slot, 1);
            }
            let (loop_id, kind, request) = reads.next().unwrap();
            reads.sent(loop_id, kind, request.id);
            reads.receive(
                BulkChunk {
                    id: request.id,
                    offset: 0,
                    total: u64::MAX,
                    payload: if midi {
                        shoop_rt_protocol::BulkPayload::Midi {
                            len: 0,
                            bytes: [0; BULK_CHUNK_BYTES],
                        }
                    } else {
                        shoop_rt_protocol::BulkPayload::Audio {
                            len: 0,
                            samples: [0.0; BULK_CHUNK_SAMPLES],
                        }
                    },
                },
                |_| 0,
            );
            if midi {
                assert!(matches!(reads.midi(id, 1, slot), Outcome::Failed(_)));
            } else {
                assert!(matches!(reads.audio(id, 1, slot, 1), Outcome::Failed(_)));
            }
        }
    }
}
