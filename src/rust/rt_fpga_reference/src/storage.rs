//! External-memory model. Chunk tables and payloads are accessible only to the job side.

use alloc::vec::Vec;

use crate::{fixed::Fixed, Error};

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
/// boundaries. Views exclusively own their slots; view IDs never recycle.
pub struct MediaStore {
    chunk_frames: usize,
    chunk_limit: Option<usize>,
    samples: Vec<Fixed>,
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
            samples: Vec::new(),
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
        self.samples.len() / self.chunk_frames
    }

    pub fn slot_capacity(&self) -> usize {
        self.samples.capacity() / self.chunk_frames
    }

    pub fn free_chunk_count(&self) -> usize {
        self.free_chunks.len()
    }

    /// Capacity hints do not allocate IDs or reserve ownership against other views.
    pub fn reserve_chunk_capacity(&mut self, chunks: usize) -> Result<(), Error> {
        let chunks = chunks.max(self.slot_count());
        if self.chunk_limit.is_some_and(|limit| chunks > limit) {
            return Err(Error::MemoryFull);
        }
        let samples = chunks
            .checked_mul(self.chunk_frames)
            .ok_or(Error::MemoryFull)?;
        self.samples
            .try_reserve(samples - self.samples.len())
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

    pub fn chunk_ids(&self, id: ViewId) -> Result<&[ChunkId], Error> {
        Ok(&self
            .views
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?
            .chunks)
    }

    /// Call after jobs using this view have drained. Released IDs enter the LIFO
    /// stack in table order. Clearing retains the view ID and table capacity.
    pub fn clear(&mut self, id: ViewId) -> Result<(), Error> {
        let view = self
            .views
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidView)?;
        self.free_chunks.extend(view.chunks.drain(..));
        view.frames = 0;
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
        self.free_chunks.extend(view.chunks);
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
            self.free_chunks.push(chunk);
            view.frames = view
                .frames
                .min((view.chunks.len() * self.chunk_frames) as u64);
        }
        Ok(view.chunks.is_empty())
    }

    pub fn append(&mut self, id: ViewId, samples: &[Fixed]) -> Result<(), Error> {
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
        self.samples
            .resize(new_slots * self.chunk_frames, Fixed::ZERO);
        let view = self.views[id.0].as_mut().ok_or(Error::InvalidView)?;
        let mut next_slot = old_slots;
        for _ in 0..required {
            let chunk = self.free_chunks.pop().unwrap_or_else(|| {
                let chunk = ChunkId(next_slot);
                next_slot += 1;
                chunk
            });
            view.chunks.push(chunk);
        }
        let mut frame = old_frames;
        let mut remaining = samples;
        while !remaining.is_empty() {
            let offset = frame % self.chunk_frames;
            let count = remaining.len().min(self.chunk_frames - offset);
            let address = view.chunks[frame / self.chunk_frames].0 * self.chunk_frames + offset;
            self.samples[address..address + count].copy_from_slice(&remaining[..count]);
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
            let at = frame as usize;
            let offset = at % self.chunk_frames;
            let count = remaining
                .len()
                .min(self.chunk_frames - offset)
                .min((length - frame) as usize);
            let address = view.chunks[at / self.chunk_frames].0 * self.chunk_frames + offset;
            remaining[..count].copy_from_slice(&self.samples[address..address + count]);
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
            samples: self.samples.clone(),
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
        assigned.resize(slots, false);
        for view in snapshot.views.iter().flatten() {
            let frames = usize::try_from(view.frames).map_err(|_| Error::InvalidShape)?;
            if view.chunks.len() != chunk_count(frames, snapshot.chunk_frames) {
                return Err(Error::InvalidState);
            }
        }
        for chunk in snapshot
            .views
            .iter()
            .flatten()
            .flat_map(|view| &view.chunks)
            .chain(&snapshot.free_chunks)
        {
            let used = assigned.get_mut(chunk.0).ok_or(Error::InvalidState)?;
            if *used {
                return Err(Error::InvalidState);
            }
            *used = true;
        }
        if assigned.contains(&false) {
            return Err(Error::InvalidState);
        }
        store.chunk_limit = snapshot.chunk_limit;
        store.samples = snapshot.samples;
        store.free_chunks = snapshot.free_chunks;
        store.views = snapshot.views;
        store.reserve_chunk_capacity(slots)?;
        Ok(store)
    }
}

fn chunk_count(frames: usize, chunk_frames: usize) -> usize {
    frames / chunk_frames + usize::from(frames % chunk_frames != 0)
}

impl crate::jobs::Memory for MediaStore {
    fn read(
        &mut self,
        view: ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error> {
        self.read_loop(view, position, length, output)
    }
    fn write(&mut self, view: ViewId, samples: &[Fixed]) -> Result<(), Error> {
        self.append(view, samples)
    }
}
