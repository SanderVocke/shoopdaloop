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
    /// Allocation granularity; payload addresses and view chunk IDs stay physical.
    #[cfg_attr(
        feature = "serde",
        serde(
            default = "one",
            skip_serializing_if = "is_one",
            with = "crate::serde_index"
        )
    )]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub block_chunks: usize,
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
    /// Complete blocks in stack order, with ascending physical IDs inside each block.
    pub free_chunks: Vec<ChunkId>,
    pub views: Vec<Option<ViewSnapshot>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BlockId(usize);

struct View {
    blocks: Vec<BlockId>,
    frames: u64,
    range: Option<ReadOnlyRange>,
}

impl View {
    fn offset(&self) -> usize {
        self.range.map_or(0, |range| range.offset)
    }
}

/// A growing mono arena with block ownership and per-view block tables.
/// Snapshot boundaries expand block IDs to physical chunks. View IDs never recycle.
pub struct MediaStore {
    chunk_frames: usize,
    chunk_limit: Option<usize>,
    block_chunks: usize,
    memory: ChunkMemory,
    references: Vec<usize>,
    free_chunks: Vec<BlockId>,
    views: Vec<Option<View>>,
}

impl MediaStore {
    pub fn new(chunk_frames: usize) -> Result<Self, Error> {
        Self::with_block_chunks(chunk_frames, 1)
    }

    pub fn with_block_chunks(chunk_frames: usize, block_chunks: usize) -> Result<Self, Error> {
        if chunk_frames == 0
            || !block_chunks.is_power_of_two()
            || block_chunks > 65536
            || chunk_frames.checked_mul(block_chunks).is_none()
        {
            return Err(Error::InvalidShape);
        }
        Ok(Self {
            chunk_frames,
            chunk_limit: None,
            block_chunks,
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

    pub fn block_chunks(&self) -> usize {
        self.block_chunks
    }

    fn block_frames(&self) -> usize {
        self.chunk_frames * self.block_chunks
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
        self.free_chunks.len() * self.block_chunks
    }

    pub fn chunk_references(&self, id: ChunkId) -> Result<usize, Error> {
        self.references
            .get(id.0 / self.block_chunks)
            .copied()
            .ok_or(Error::InvalidView)
    }

    /// Capacity hints do not allocate IDs or reserve ownership against other views.
    pub fn reserve_chunk_capacity(&mut self, chunks: usize) -> Result<(), Error> {
        let chunks = chunk_count(chunks.max(self.slot_count()), self.block_chunks)
            .checked_mul(self.block_chunks)
            .ok_or(Error::MemoryFull)?;
        if self.chunk_limit.is_some_and(|limit| chunks > limit) {
            return Err(Error::MemoryFull);
        }
        self.memory.reserve_slots(chunks)?;
        let blocks = chunks / self.block_chunks;
        self.references
            .try_reserve(blocks - self.references.len())
            .map_err(|_| Error::MemoryFull)?;
        self.free_chunks
            .try_reserve(blocks.saturating_sub(self.free_chunks.len()))
            .map_err(|_| Error::MemoryFull)?;
        Ok(())
    }

    pub fn reserve_view_capacity(&mut self, id: ViewId, frames: usize) -> Result<(), Error> {
        let chunks = chunk_count(frames, self.block_frames());
        let view = self
            .views
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidView)?;
        if view.range.is_some() {
            return Err(Error::InvalidState);
        }
        view.blocks
            .try_reserve(chunks.saturating_sub(view.blocks.len()))
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
            self.views.push(Some(View {
                blocks: Vec::new(),
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
            .offset()
            % self.chunk_frames)
    }

    pub(crate) fn chunk_at(&self, id: ViewId, ordinal: usize) -> Result<ChunkId, Error> {
        let view = self
            .views
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?;
        let count = chunk_count(
            view.offset() % self.chunk_frames + view.frames as usize,
            self.chunk_frames,
        );
        if ordinal >= count {
            return Err(Error::InvalidView);
        }
        let at = view.offset() / self.chunk_frames + ordinal;
        Ok(ChunkId(
            view.blocks[at / self.block_chunks].0 * self.block_chunks + at % self.block_chunks,
        ))
    }

    /// Materialize physical chunk IDs for diagnostics; playback resolves them directly.
    pub fn chunk_ids(&self, id: ViewId) -> Result<Vec<ChunkId>, Error> {
        let view = self
            .views
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidView)?;
        let count = chunk_count(
            view.offset() % self.chunk_frames + view.frames as usize,
            self.chunk_frames,
        );
        let mut chunks = Vec::new();
        chunks.try_reserve(count).map_err(|_| Error::MemoryFull)?;
        for i in 0..count {
            chunks.push(self.chunk_at(id, i)?);
        }
        Ok(chunks)
    }

    /// Retain a read-only frame range without copying samples. Nested ranges
    /// retain their own block references and do not depend on the source view.
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
            start % self.block_frames()
        };
        let span = offset.checked_add(length).ok_or(Error::InvalidView)?;
        let first = start / self.block_frames();
        let range = &view.blocks[first..first + chunk_count(span, self.block_frames())];
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
        self.views.push(Some(View {
            blocks: chunks,
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
        for chunk in view.blocks.drain(..) {
            release_block(
                &mut self.references,
                &mut self.free_chunks,
                &mut self.memory,
                self.block_chunks,
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
        for chunk in view.blocks {
            release_block(
                &mut self.references,
                &mut self.free_chunks,
                &mut self.memory,
                self.block_chunks,
                chunk,
            )?;
        }
        Ok(())
    }

    /// Release at most one tail allocation block from a quiescent view; report whether empty.
    pub fn reclaim_chunk(&mut self, id: ViewId) -> Result<bool, Error> {
        let block_frames = self.block_frames();
        let view = self
            .views
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidView)?;
        if let Some(chunk) = view.blocks.pop() {
            release_block(
                &mut self.references,
                &mut self.free_chunks,
                &mut self.memory,
                self.block_chunks,
                chunk,
            )?;
            let capacity = (view.blocks.len() * block_frames).saturating_sub(view.offset());
            view.frames = view.frames.min(capacity as u64);
            if view.blocks.is_empty() {
                if let Some(range) = &mut view.range {
                    range.offset = 0;
                }
            }
        }
        Ok(view.blocks.is_empty())
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
        let required = chunk_count(new_frames, self.block_frames())
            - self.views[id.0]
                .as_ref()
                .ok_or(Error::InvalidView)?
                .blocks
                .len();
        let old_slots = self.slot_count();
        let new_slots = old_slots
            .checked_add(
                required
                    .saturating_sub(self.free_chunks.len())
                    .checked_mul(self.block_chunks)
                    .ok_or(Error::MemoryFull)?,
            )
            .ok_or(Error::MemoryFull)?;
        self.reserve_chunk_capacity(new_slots)?;
        self.reserve_view_capacity(id, new_frames)?;

        // All fallible reservations precede changes to samples, IDs, and lengths.
        self.memory.grow_zeroed(new_slots)?;
        self.references.resize(new_slots / self.block_chunks, 0);
        let view = self.views[id.0].as_mut().ok_or(Error::InvalidView)?;
        let mut next_slot = old_slots / self.block_chunks;
        for _ in 0..required {
            let chunk = self.free_chunks.pop().unwrap_or_else(|| {
                let chunk = BlockId(next_slot);
                next_slot += 1;
                chunk
            });
            self.references[chunk.0] = 1;
            view.blocks.push(chunk);
        }
        let mut frame = old_frames;
        let mut remaining = samples;
        while !remaining.is_empty() {
            let offset = frame % self.chunk_frames;
            let count = remaining.len().min(self.chunk_frames - offset);
            let physical = ChunkId(
                view.blocks[frame / (self.chunk_frames * self.block_chunks)].0 * self.block_chunks
                    + (frame / self.chunk_frames) % self.block_chunks,
            );
            self.memory.write(physical, offset, &remaining[..count])?;
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
            let physical = ChunkId(
                view.blocks[ordinal / self.block_chunks].0 * self.block_chunks
                    + ordinal % self.block_chunks,
            );
            self.memory
                .read(physical, offset, &mut remaining[..count])?;
            let next = (ordinal + 1) * self.chunk_frames;
            if next < view.offset() + view.frames as usize {
                self.memory.prefetch(ChunkId(
                    view.blocks[(ordinal + 1) / self.block_chunks].0 * self.block_chunks
                        + (ordinal + 1) % self.block_chunks,
                ))?;
            }
            remaining = &mut remaining[count..];
            frame += count as u64;
            if frame == length {
                frame = 0;
            }
        }
        Ok(())
    }

    /// Unpublished bytes in blocked snapshots are canonical zeroes.
    pub fn snapshot(&self) -> MediaSnapshot {
        let mut samples = if self.block_chunks == 1 {
            self.memory.samples().to_vec()
        } else {
            alloc::vec![Fixed::ZERO; self.memory.samples().len()]
        };
        let views = self
            .views
            .iter()
            .enumerate()
            .map(|(index, view)| {
                view.as_ref().map(|view| {
                    let chunks = self.chunk_ids(ViewId(index)).expect("valid view mapping");
                    if self.block_chunks != 1 {
                        let mut frame = 0;
                        while frame < view.frames as usize {
                            let at = view.offset() + frame;
                            let physical = (view.blocks[at / self.block_frames()].0
                                * self.block_chunks
                                + (at / self.chunk_frames) % self.block_chunks)
                                * self.chunk_frames
                                + at % self.chunk_frames;
                            let count = (view.frames as usize - frame)
                                .min(self.chunk_frames - at % self.chunk_frames);
                            samples[physical..physical + count].copy_from_slice(
                                &self.memory.samples()[physical..physical + count],
                            );
                            frame += count;
                        }
                    }
                    ViewSnapshot {
                        chunks,
                        frames: view.frames,
                        range: view.range.map(|_| ReadOnlyRange {
                            offset: view.offset() % self.chunk_frames,
                        }),
                    }
                })
            })
            .collect();
        MediaSnapshot {
            block_chunks: self.block_chunks,
            chunk_frames: self.chunk_frames,
            chunk_limit: self.chunk_limit,
            samples,
            free_chunks: self
                .free_chunks
                .iter()
                .flat_map(|block| {
                    (0..self.block_chunks).map(move |at| ChunkId(block.0 * self.block_chunks + at))
                })
                .collect(),
            views,
        }
    }

    pub fn from_snapshot(snapshot: MediaSnapshot) -> Result<Self, Error> {
        let mut store = Self::with_block_chunks(snapshot.chunk_frames, snapshot.block_chunks)?;
        let bc = store.block_chunks;
        let bf = store.block_frames();
        if snapshot.samples.len() % bf != 0 {
            return Err(Error::InvalidShape);
        }
        let slots = snapshot.samples.len() / snapshot.chunk_frames;
        if snapshot.chunk_limit.is_some_and(|limit| slots > limit) {
            return Err(Error::InvalidShape);
        }
        let mut assigned = Vec::new();
        assigned
            .try_reserve(slots / bc)
            .map_err(|_| Error::MemoryFull)?;
        assigned.resize(slots / bc, Ownership::default());
        store.reserve_recordings(snapshot.views.len())?;
        for (index, entry) in snapshot.views.iter().enumerate() {
            let Some(view) = entry else {
                store.views.push(None);
                continue;
            };
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
            let first = view.chunks.first().map_or(0, |id| id.0 % bc);
            if view.range.is_none() && first != 0 {
                return Err(Error::InvalidState);
            }
            let offset = first * snapshot.chunk_frames + view.offset();
            let extent = offset.checked_add(frames).ok_or(Error::InvalidShape)?;
            let mut blocks = Vec::new();
            blocks
                .try_reserve(chunk_count(extent, bf))
                .map_err(|_| Error::MemoryFull)?;
            for (ordinal, chunk) in view.chunks.iter().enumerate() {
                let at = first + ordinal;
                if chunk.0 % bc != at % bc {
                    return Err(Error::InvalidState);
                }
                if ordinal == 0 || at % bc == 0 {
                    blocks.push(BlockId(chunk.0 / bc));
                }
                if blocks.last().is_none_or(|block| block.0 != chunk.0 / bc) {
                    return Err(Error::InvalidState);
                }
            }
            for (ordinal, block) in blocks.iter().enumerate() {
                let owner = assigned.get_mut(block.0).ok_or(Error::InvalidState)?;
                if owner.last_view == Some(index) {
                    return Err(Error::InvalidState);
                }
                owner.last_view = Some(index);
                owner.references = owner.references.checked_add(1).ok_or(Error::MemoryFull)?;
                let end = (extent - ordinal * bf).min(bf);
                if view.range.is_some() {
                    owner.read_end = owner.read_end.max(end);
                } else if owner.write_end.replace(end).is_some() {
                    return Err(Error::InvalidState);
                }
            }
            store.views.push(Some(View {
                blocks,
                frames: view.frames,
                range: view.range.map(|_| ReadOnlyRange { offset }),
            }));
        }
        if snapshot.free_chunks.len() % bc != 0 {
            return Err(Error::InvalidState);
        }
        store
            .free_chunks
            .try_reserve(snapshot.free_chunks.len() / bc)
            .map_err(|_| Error::MemoryFull)?;
        for chunks in snapshot.free_chunks.chunks(bc) {
            let id = chunks[0].0;
            if id % bc != 0
                || chunks
                    .iter()
                    .enumerate()
                    .any(|(i, chunk)| chunk.0 != id + i)
            {
                return Err(Error::InvalidState);
            }
            let owner = assigned.get_mut(id / bc).ok_or(Error::InvalidState)?;
            if owner.free || owner.references != 0 {
                return Err(Error::InvalidState);
            }
            owner.free = true;
            store.free_chunks.push(BlockId(id / bc));
        }
        if assigned.iter().any(|owner| {
            (owner.references == 0 && !owner.free)
                || owner.write_end.is_some_and(|end| owner.read_end > end)
        }) {
            return Err(Error::InvalidState);
        }
        store
            .references
            .try_reserve(assigned.len())
            .map_err(|_| Error::MemoryFull)?;
        store
            .references
            .extend(assigned.iter().map(|owner| owner.references));
        store.chunk_limit = snapshot.chunk_limit;
        store.memory = ChunkMemory::from_samples(snapshot.chunk_frames, snapshot.samples)?;
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

fn release_block(
    references: &mut [usize],
    free: &mut Vec<BlockId>,
    memory: &mut ChunkMemory,
    block_chunks: usize,
    chunk: BlockId,
) -> Result<(), Error> {
    if references[chunk.0] == 1 {
        memory.retire_range(ChunkId(chunk.0 * block_chunks), block_chunks)?;
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
    fn begin_capture(&mut self, view: ViewId) -> Result<(), Error> {
        if view.0 > self.views.len() {
            return Err(Error::InvalidView);
        }
        if view.0 == self.views.len() {
            self.create_recording()?;
        }
        self.clear(view)
    }

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

#[cfg(feature = "serde")]
fn one() -> usize {
    1
}
#[cfg(feature = "serde")]
fn is_one(value: &usize) -> bool {
    *value == 1
}
