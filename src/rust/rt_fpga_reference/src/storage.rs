//! External-memory model. Chunk tables and payloads are accessible only to the job side.

use alloc::vec::Vec;

use crate::{chunk_memory::ChunkMemory, fixed::Fixed, Error};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChunkId(
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub usize,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ViewId(
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub usize,
);

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ViewSnapshot {
    pub chunks: Vec<ChunkId>,
    pub frames: u64,
    /// Present for a read-only view; its offset is within the first chunk.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    #[cfg_attr(feature = "schema", schemars(with = "ReadOnlyRange"))]
    pub range: Option<ReadOnlyRange>,
}

impl ViewSnapshot {
    fn offset(&self) -> usize {
        self.range.map_or(0, |range| range.offset)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReadOnlyRange {
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MediaSnapshot {
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub chunk_frames: usize,
    #[cfg_attr(
        feature = "serde",
        serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "crate::serde_index::optional"
        )
    )]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub chunk_limit: Option<usize>,
    pub samples: Vec<Fixed>,
    pub free_chunks: Vec<ChunkId>,
    pub views: Vec<Option<ViewSnapshot>>,
}

/// A growing mono slot arena and per-view chunk tables. Only IDs cross growth
/// boundaries. Read-only ranges share slots; view IDs never recycle.
pub struct MediaStore {
    chunk_frames: usize,
    chunk_limit: Option<usize>,
    memory: ChunkMemory,
    references: Vec<usize>,
    free_chunks: Vec<ChunkId>,
    views: Vec<Option<ViewSnapshot>>,
}

impl MediaStore {
    pub fn new(chunk_frames: usize) -> Result<Self, Error> {
        if chunk_frames == 0 {
            return Err(Error::InvalidShape);
        }
        Ok(Self {
            chunk_frames,
            chunk_limit: None,
            memory: ChunkMemory::new(chunk_frames)?,
            references: Vec::new(),
            free_chunks: Vec::new(),
            views: Vec::new(),
        })
    }

    pub fn with_chunk_limit(chunk_frames: usize, chunks: usize) -> Result<Self, Error> {
        let mut store = Self::new(chunk_frames)?;
        store.chunk_limit = Some(chunks);
        Ok(store)
    }

    pub fn slot_count(&self) -> usize {
        self.memory.slot_count()
    }

    pub fn slot_capacity(&self) -> usize {
        self.memory.slot_capacity()
    }

    pub fn chunk_memory(&self) -> &ChunkMemory {
        &self.memory
    }

    pub fn fence(&mut self) -> Result<(), Error> {
        self.memory.fence()
    }

    pub fn free_chunk_count(&self) -> usize {
        self.free_chunks.len()
    }

    pub fn chunk_references(&self, id: ChunkId) -> Result<usize, Error> {
        self.references.get(id.0).copied().ok_or(Error::InvalidView)
    }

    /// Capacity hints do not allocate IDs or reserve ownership against other views.
    pub fn reserve_chunk_capacity(&mut self, chunks: usize) -> Result<(), Error> {
        let chunks = chunks.max(self.slot_count());
        if self.chunk_limit.is_some_and(|limit| chunks > limit) {
            return Err(Error::MemoryFull);
        }
        self.memory.reserve_slots(chunks)?;
        self.references
            .try_reserve(chunks - self.references.len())
            .map_err(|_| Error::MemoryFull)?;
        self.free_chunks
            .try_reserve(chunks - self.free_chunks.len())
            .map_err(|_| Error::MemoryFull)?;
        Ok(())
    }

    pub fn reserve_view_capacity(&mut self, id: ViewId, frames: usize) -> Result<(), Error> {
        let chunks = chunk_count(frames, self.chunk_frames);
        let view = self
            .views
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidView)?;
        if view.range.is_some() {
            return Err(Error::InvalidState);
        }
        view.chunks
            .try_reserve(chunks.saturating_sub(view.chunks.len()))
            .map_err(|_| Error::MemoryFull)
    }

    pub fn create_recording(&mut self) -> Result<ViewId, Error> {
        self.create_recordings(1)
    }

    /// Consecutive IDs let a multichannel adapter keep a compact channel range.
    pub fn create_recordings(&mut self, count: usize) -> Result<ViewId, Error> {
        if count == 0 {
            return Err(Error::InvalidShape);
        }
        self.reserve_recordings(count)?;
        let id = ViewId(self.views.len());
        for _ in 0..count {
            self.views.push(Some(ViewSnapshot {
                chunks: Vec::new(),
                frames: 0,
                range: None,
            }));
        }
        Ok(id)
    }

    pub fn reserve_recordings(&mut self, additional: usize) -> Result<(), Error> {
        self.views
            .try_reserve(additional)
            .map_err(|_| Error::MemoryFull)
    }

    pub fn insert(&mut self, samples: &[Fixed]) -> Result<ViewId, Error> {
        let id = self.create_recording()?;
        if let Err(error) = self.append(id, samples) {
            self.views.pop();
            return Err(error);
        }
        Ok(id)
    }

    pub fn frames(&self, id: ViewId) -> Result<usize, Error> {
        let view = self
            .views
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?;
        usize::try_from(view.frames).map_err(|_| Error::InvalidShape)
    }

    pub fn slice_chunks(
        &mut self,
        source: ViewId,
        start: u64,
        chunks: u64,
    ) -> Result<ViewId, Error> {
        let start = start
            .checked_mul(self.chunk_frames as u64)
            .ok_or(Error::InvalidView)?;
        let frames = chunks
            .checked_mul(self.chunk_frames as u64)
            .ok_or(Error::InvalidView)?;
        self.slice(source, start, frames)
    }

    pub(crate) fn range_offset(&self, id: ViewId) -> Result<usize, Error> {
        Ok(self
            .views
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?
            .offset())
    }
    pub fn chunk_ids(&self, id: ViewId) -> Result<&[ChunkId], Error> {
        Ok(&self
            .views
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?
            .chunks)
    }

    /// Retain a read-only frame range without copying samples. Nested ranges
    /// retain their own chunk references and do not depend on the source view.
    pub fn slice(&mut self, source: ViewId, start: u64, frames: u64) -> Result<ViewId, Error> {
        let view = self
            .views
            .get(source.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?;
        if start > view.frames || frames > view.frames - start {
            return Err(Error::InvalidView);
        }
        let start = usize::try_from(start)
            .ok()
            .and_then(|n| n.checked_add(view.offset()))
            .ok_or(Error::InvalidView)?;
        let length = usize::try_from(frames).map_err(|_| Error::InvalidView)?;
        let offset = if length == 0 {
            0
        } else {
            start % self.chunk_frames
        };
        let span = offset.checked_add(length).ok_or(Error::InvalidView)?;
        let first = start / self.chunk_frames;
        let range = &view.chunks[first..first + chunk_count(span, self.chunk_frames)];
        let mut chunks = Vec::new();
        chunks
            .try_reserve(range.len())
            .map_err(|_| Error::MemoryFull)?;
        chunks.extend_from_slice(range);
        self.reserve_recordings(1)?;
        for chunk in &chunks {
            self.references[chunk.0]
                .checked_add(1)
                .ok_or(Error::MemoryFull)?;
        }
        for chunk in &chunks {
            self.references[chunk.0] += 1;
        }
        let id = ViewId(self.views.len());
        self.views.push(Some(ViewSnapshot {
            chunks,
            frames,
            range: Some(ReadOnlyRange { offset }),
        }));
        Ok(id)
    }

    /// Call after jobs using this view have drained. Released IDs enter the LIFO
    /// stack in table order. Clearing retains the view ID and table capacity.
    pub fn clear(&mut self, id: ViewId) -> Result<(), Error> {
        let view = self
            .views
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidView)?;
        for chunk in view.chunks.drain(..) {
            release_chunk(
                &mut self.references,
                &mut self.free_chunks,
                &mut self.memory,
                chunk,
            )?;
        }
        view.frames = 0;
        if let Some(range) = &mut view.range {
            range.offset = 0;
        }
        Ok(())
    }

    /// Retire a view after its jobs have drained; its ID is never reissued.
    pub fn remove(&mut self, id: ViewId) -> Result<(), Error> {
        let view = self
            .views
            .get_mut(id.0)
            .ok_or(Error::InvalidView)?
            .take()
            .ok_or(Error::InvalidView)?;
        for chunk in view.chunks {
            release_chunk(
                &mut self.references,
                &mut self.free_chunks,
                &mut self.memory,
                chunk,
            )?;
        }
        Ok(())
    }

    /// Release at most one tail slot from a quiescent view; report whether empty.
    pub fn reclaim_chunk(&mut self, id: ViewId) -> Result<bool, Error> {
        let view = self
            .views
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidView)?;
        if let Some(chunk) = view.chunks.pop() {
            release_chunk(
                &mut self.references,
                &mut self.free_chunks,
                &mut self.memory,
                chunk,
            )?;
            let capacity = (view.chunks.len() * self.chunk_frames).saturating_sub(view.offset());
            view.frames = view.frames.min(capacity as u64);
            if view.chunks.is_empty() {
                if let Some(range) = &mut view.range {
                    range.offset = 0;
                }
            }
        }
        Ok(view.chunks.is_empty())
    }

    pub fn append(&mut self, id: ViewId, samples: &[Fixed]) -> Result<(), Error> {
        if self
            .views
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?
            .range
            .is_some()
        {
            return Err(Error::InvalidState);
        }
        let old_frames = self.frames(id)?;
        let new_frames = old_frames
            .checked_add(samples.len())
            .ok_or(Error::MemoryFull)?;
        let required = chunk_count(new_frames, self.chunk_frames) - self.chunk_ids(id)?.len();
        let old_slots = self.slot_count();
        let new_slots = old_slots
            .checked_add(required.saturating_sub(self.free_chunks.len()))
            .ok_or(Error::MemoryFull)?;
        self.reserve_chunk_capacity(new_slots)?;
        self.reserve_view_capacity(id, new_frames)?;

        // All fallible reservations precede changes to samples, IDs, and lengths.
        self.memory.grow_zeroed(new_slots)?;
        self.references.resize(new_slots, 0);
        let view = self.views[id.0].as_mut().ok_or(Error::InvalidView)?;
        let mut next_slot = old_slots;
        for _ in 0..required {
            let chunk = self.free_chunks.pop().unwrap_or_else(|| {
                let chunk = ChunkId(next_slot);
                next_slot += 1;
                chunk
            });
            self.references[chunk.0] = 1;
            view.chunks.push(chunk);
        }
        let mut frame = old_frames;
        let mut remaining = samples;
        while !remaining.is_empty() {
            let offset = frame % self.chunk_frames;
            let count = remaining.len().min(self.chunk_frames - offset);
            self.memory.write(
                view.chunks[frame / self.chunk_frames],
                offset,
                &remaining[..count],
            )?;
            remaining = &remaining[count..];
            frame += count;
        }
        view.frames = new_frames as u64;
        Ok(())
    }

    pub fn read_loop(
        &self,
        id: ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error> {
        let view = self
            .views
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?;
        if length == 0 || length > view.frames || position >= length {
            return Err(Error::InvalidView);
        }
        let mut frame = position;
        let mut remaining = output;
        while !remaining.is_empty() {
            let at = view.offset() + frame as usize;
            let offset = at % self.chunk_frames;
            let count = remaining
                .len()
                .min(self.chunk_frames - offset)
                .min((length - frame) as usize);
            let ordinal = at / self.chunk_frames;
            self.memory
                .read(view.chunks[ordinal], offset, &mut remaining[..count])?;
            if let Some(&next) = view.chunks.get(ordinal + 1) {
                self.memory.prefetch(next)?;
            }
            remaining = &mut remaining[count..];
            frame += count as u64;
            if frame == length {
                frame = 0;
            }
        }
        Ok(())
    }

    /// Allocated slots and free-stack order are architectural; reserved capacity is not.
    pub fn snapshot(&self) -> MediaSnapshot {
        MediaSnapshot {
            chunk_frames: self.chunk_frames,
            chunk_limit: self.chunk_limit,
            samples: self.memory.samples().to_vec(),
            free_chunks: self.free_chunks.clone(),
            views: self.views.clone(),
        }
    }

    pub fn from_snapshot(snapshot: MediaSnapshot) -> Result<Self, Error> {
        let mut store = Self::new(snapshot.chunk_frames)?;
        if snapshot.samples.len() % snapshot.chunk_frames != 0 {
            return Err(Error::InvalidShape);
        }
        let slots = snapshot.samples.len() / snapshot.chunk_frames;
        if snapshot.chunk_limit.is_some_and(|limit| slots > limit) {
            return Err(Error::InvalidShape);
        }
        let mut assigned = Vec::new();
        assigned.try_reserve(slots).map_err(|_| Error::MemoryFull)?;
        assigned.resize(slots, Ownership::default());
        for (index, view) in snapshot.views.iter().enumerate() {
            let Some(view) = view else { continue };
            let frames = usize::try_from(view.frames).map_err(|_| Error::InvalidShape)?;
            let span = view
                .offset()
                .checked_add(frames)
                .ok_or(Error::InvalidShape)?;
            if view.offset() >= snapshot.chunk_frames
                || (frames == 0 && view.offset() != 0)
                || view.chunks.len() != chunk_count(span, snapshot.chunk_frames)
            {
                return Err(Error::InvalidState);
            }
            for (ordinal, chunk) in view.chunks.iter().enumerate() {
                let owner = assigned.get_mut(chunk.0).ok_or(Error::InvalidState)?;
                if owner.last_view == Some(index) {
                    return Err(Error::InvalidState);
                }
                owner.last_view = Some(index);
                owner.references = owner.references.checked_add(1).ok_or(Error::MemoryFull)?;
                let end = if ordinal + 1 == view.chunks.len() {
                    span - ordinal * snapshot.chunk_frames
                } else {
                    snapshot.chunk_frames
                };
                if view.range.is_some() {
                    owner.read_end = owner.read_end.max(end);
                } else if owner.write_end.replace(end).is_some() {
                    return Err(Error::InvalidState);
                }
            }
        }
        for chunk in &snapshot.free_chunks {
            let owner = assigned.get_mut(chunk.0).ok_or(Error::InvalidState)?;
            if owner.free || owner.references != 0 {
                return Err(Error::InvalidState);
            }
            owner.free = true;
        }
        // Appends must not overwrite samples visible through a retained range.
        if assigned.iter().any(|owner| {
            (owner.references == 0 && !owner.free)
                || owner.write_end.is_some_and(|end| owner.read_end > end)
        }) {
            return Err(Error::InvalidState);
        }
        store
            .references
            .try_reserve(slots)
            .map_err(|_| Error::MemoryFull)?;
        store
            .references
            .extend(assigned.iter().map(|owner| owner.references));
        store.chunk_limit = snapshot.chunk_limit;
        store.memory = ChunkMemory::from_samples(snapshot.chunk_frames, snapshot.samples)?;
        store.free_chunks = snapshot.free_chunks;
        store.views = snapshot.views;
        store.reserve_chunk_capacity(slots)?;
        Ok(store)
    }
}

#[derive(Clone, Copy, Default)]
struct Ownership {
    references: usize,
    last_view: Option<usize>,
    write_end: Option<usize>,
    read_end: usize,
    free: bool,
}

fn release_chunk(
    references: &mut [usize],
    free: &mut Vec<ChunkId>,
    memory: &mut ChunkMemory,
    chunk: ChunkId,
) -> Result<(), Error> {
    if references[chunk.0] == 1 {
        memory.retire(chunk)?;
    }
    references[chunk.0] -= 1;
    if references[chunk.0] == 0 {
        free.push(chunk);
    }
    Ok(())
}

fn chunk_count(frames: usize, chunk_frames: usize) -> usize {
    frames / chunk_frames + usize::from(frames % chunk_frames != 0)
}

impl crate::jobs::Memory for MediaStore {
    fn chunk_frames(&self) -> usize {
        self.chunk_frames
    }
    fn read(
        &mut self,
        view: ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error> {
        let position = position
            .checked_mul(self.chunk_frames as u64)
            .ok_or(Error::InvalidView)?;
        let length = length
            .checked_mul(self.chunk_frames as u64)
            .ok_or(Error::InvalidView)?;
        self.read_loop(view, position, length, output)
    }
    fn write(&mut self, view: ViewId, samples: &[Fixed]) -> Result<(), Error> {
        self.append(view, samples)
    }
    fn prefetch(&mut self, view: ViewId, position: u64, length: u64, chunks: u64) {
        crate::resolver::resolve(
            crate::resolver::ReadIntent {
                view,
                position,
                length,
                chunks,
            },
            self,
            |chunk| {
                let _ = self.memory.prefetch(chunk);
            },
        );
    }
}
