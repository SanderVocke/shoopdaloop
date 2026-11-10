//! Fixed-point audio payloads with ordinary resource revision and lease ownership.
use super::*;
use rt_fpga_reference::{
    fixed::Fixed,
    storage::{MediaStore, ViewId},
};

#[derive(Clone, Copy)]
pub(super) struct Take {
    pub first: ViewId,
    pub channels: usize,
    pub partial: [u8; 4],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElasticAudioWork {
    pub slots: usize,
    pub free_slots: usize,
    pub capacity_slots: usize,
    pub arena_growths: u64,
    pub read_samples: u64,
    pub captured_samples: u64,
    pub reclaimed_chunks: u64,
    pub reserved_chunks: usize,
}

pub(super) struct ElasticAudio {
    pub media: MediaStore,
    pub reserved_chunks: usize,
    chunk_frames: usize,
    scratch: Vec<Fixed>,
    work: ElasticAudioWork,
}

fn map_error(_: rt_fpga_reference::Error) -> Error {
    Error::Capacity
}

impl ElasticAudio {
    pub fn new(quantum: Quantum, max_frames: usize) -> Self {
        Self {
            media: MediaStore::new(quantum.frames()).expect("positive quantum"),
            reserved_chunks: 0,
            chunk_frames: quantum.frames(),
            scratch: vec![Fixed::ZERO; max_frames],
            work: ElasticAudioWork::default(),
        }
    }
    pub fn reset_work(&mut self) {
        self.work = ElasticAudioWork::default();
    }
    pub fn work(&self) -> ElasticAudioWork {
        ElasticAudioWork {
            slots: self.media.slot_count(),
            free_slots: self.media.free_chunk_count(),
            capacity_slots: self.media.slot_capacity(),
            reserved_chunks: self.reserved_chunks,
            ..self.work
        }
    }
    pub fn create(&mut self, channels: usize) -> Result<Take, Error> {
        Ok(Take {
            first: self.media.create_recordings(channels).map_err(map_error)?,
            channels,
            partial: [0; 4],
        })
    }
    pub fn reclaim(&mut self, take: &mut Take) -> Result<(), Error> {
        let before = self.media.free_chunk_count();
        if self.media.reclaim_chunk(take.first).map_err(map_error)? {
            self.media.remove(take.first).map_err(map_error)?;
            take.first.0 += 1;
            take.channels -= 1;
        }
        self.work.reclaimed_chunks += (self.media.free_chunk_count() - before) as u64;
        Ok(())
    }
    /// Reserve capacity without publishing data or claiming chunk IDs. Pending
    /// capture intervals keep separate capacity credits until commit/cancellation.
    pub fn prepare(&mut self, take: Take, frames: usize, extra: usize) -> Result<usize, Error> {
        let chunks = self.additional_chunks(take, frames)?;
        for channel in 0..take.channels {
            let view = ViewId(take.first.0 + channel);
            let frames = frames.max(self.media.frames(view).map_err(map_error)?);
            self.media
                .reserve_view_capacity(view, frames)
                .map_err(map_error)?;
        }
        let before = self.media.slot_capacity();
        let demand = self
            .reserved_chunks
            .checked_add(extra)
            .and_then(|n| n.checked_add(chunks))
            .ok_or(Error::Capacity)?;
        let capacity = self
            .media
            .slot_count()
            .checked_add(demand.saturating_sub(self.media.free_chunk_count()))
            .ok_or(Error::Capacity)?;
        self.media
            .reserve_chunk_capacity(capacity)
            .map_err(map_error)?;
        if self.media.slot_capacity() != before {
            self.work.arena_growths += 1;
        }
        Ok(chunks)
    }
    pub fn additional_chunks(&self, take: Take, frames: usize) -> Result<usize, Error> {
        let mut chunks = 0usize;
        for channel in 0..take.channels {
            let view = ViewId(take.first.0 + channel);
            let needed = frames
                .max(self.media.frames(view).map_err(map_error)?)
                .div_ceil(self.chunk_frames);
            chunks = chunks
                .checked_add(needed - self.media.chunk_ids(view).map_err(map_error)?.len())
                .ok_or(Error::Capacity)?;
        }
        Ok(chunks)
    }
    pub fn append_bytes(&mut self, version: &mut Version, bytes: &[u8], fixed: bool) {
        let take = version.elastic.as_mut().expect("elastic take");
        for &byte in bytes {
            take.partial[version.written % 4] = byte;
            version.written += 1;
            if version.written % 4 == 0 {
                let channel = (version.written / 4 - 1) % take.channels;
                let value = if fixed {
                    Fixed(i32::from_le_bytes(take.partial))
                } else {
                    Fixed::from_f32(f32::from_le_bytes(take.partial))
                };
                self.media
                    .append(ViewId(take.first.0 + channel), &[value])
                    .expect("reserved elastic append");
            }
        }
    }
    pub fn append_staging(&mut self, take: Take, staging: &[u8]) {
        let frames = staging.len() / (take.channels * 4);
        for channel in 0..take.channels {
            for (frame, sample) in self.scratch[..frames].iter_mut().enumerate() {
                let at = (frame * take.channels + channel) * 4;
                *sample = Fixed(i32::from_le_bytes(
                    staging[at..at + 4].try_into().expect("sample"),
                ));
            }
            self.media
                .append(ViewId(take.first.0 + channel), &self.scratch[..frames])
                .expect("reserved elastic capture");
        }
        self.work.captured_samples += (frames * take.channels) as u64;
    }
    pub fn read_fixed(
        &mut self,
        take: Take,
        channel: usize,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error> {
        if channel >= take.channels {
            return Err(Error::InvalidOffset);
        }
        self.media
            .read_loop(ViewId(take.first.0 + channel), position, length, output)
            .map_err(|_| Error::InvalidState)?;
        self.work.read_samples += output.len() as u64;
        Ok(())
    }
    pub fn read_bytes(
        &mut self,
        take: Take,
        offset: usize,
        output: &mut [u8],
    ) -> Result<(), Error> {
        let mut at = offset;
        let mut remaining = output;
        while !remaining.is_empty() {
            let ordinal = at / 4;
            let channel = ordinal % take.channels;
            let frame = ordinal / take.channels;
            let view = ViewId(take.first.0 + channel);
            let length = self.media.frames(view).map_err(map_error)?;
            let mut sample = [Fixed::ZERO];
            self.media
                .read_loop(view, frame as u64, length as u64, &mut sample)
                .map_err(map_error)?;
            let bytes = sample[0].to_f32().to_le_bytes();
            let first = at % 4;
            let count = remaining.len().min(4 - first);
            remaining[..count].copy_from_slice(&bytes[first..first + count]);
            remaining = &mut remaining[count..];
            at += count;
        }
        Ok(())
    }
}

impl ResourceStore {
    pub fn elastic_audio_work(&self) -> ElasticAudioWork {
        self.elastic_audio
            .as_ref()
            .map(ElasticAudio::work)
            .unwrap_or_default()
    }
    pub fn read_elastic_audio(
        &mut self,
        index: ReadIndex,
        channel: usize,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error> {
        let lease = self.index_lease(index)?;
        if length == 0 || length > lease.snapshot.description.frames || position >= length {
            return Err(Error::InvalidOffset);
        }
        let version = get(&self.versions, lease.version.slot, lease.version.generation)?;
        let take = version.elastic.ok_or(Error::InvalidShape)?;
        self.elastic_audio
            .as_mut()
            .ok_or(Error::InvalidState)?
            .read_fixed(take, channel, position, length, output)
    }
}

#[cfg(test)]
mod tests;
