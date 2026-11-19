use super::{Error, Message, MidiState, Record, TakeStatus};
use alloc::{boxed::Box, collections::VecDeque, vec::Vec};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ViewId {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SnapshotId {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct StoreLimits {
    pub block_events: u32,
    pub blocks: u32,
    pub max_events: u32,
    pub views: u32,
    pub snapshots: u32,
    pub retirements: u32,
}
impl StoreLimits {
    pub fn session() -> Self {
        Self {
            block_events: 256,
            blocks: 4864,
            max_events: 8192,
            views: 512,
            snapshots: 768,
            retirements: 256,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Description {
    pub events: u32,
    pub frames: u64,
    pub initial: SnapshotId,
    pub status: TakeStatus,
}
struct View {
    generation: u32,
    refs: u32,
    retired: bool,
    table: Box<[u32]>,
    blocks: usize,
    first: usize,
    origin: u64,
    next_order: u32,
    event_limit: u32,
    description: Description,
}
struct Snapshot {
    active: usize,
    generation: u32,
    refs: u32,
    state: MidiState,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoreUsage {
    pub blocks: usize,
    pub views: usize,
    pub snapshots: usize,
    pub retirements: usize,
}
pub struct ContentStore {
    limits: StoreLimits,
    records: Box<[Record]>,
    references: Box<[u32]>,
    free_blocks: Vec<u32>,
    views: Box<[View]>,
    free_views: Vec<u32>,
    snapshots: Box<[Snapshot]>,
    free_snapshots: Vec<u32>,
    live_snapshots: Vec<u32>,
    retired: VecDeque<ViewId>,
    work: u64,
    snapshot_bytes: u64,
}
fn prepared<T>(count: usize, mut value: impl FnMut() -> T) -> Result<Vec<T>, Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(count).map_err(|_| Error::Capacity)?;
    v.extend((0..count).map(|_| value()));
    Ok(v)
}
fn free_ids(count: u32) -> Result<Vec<u32>, Error> {
    let mut v = prepared(count as usize, || 0)?;
    for (i, id) in v.iter_mut().enumerate() {
        *id = count - i as u32 - 1;
    }
    Ok(v)
}
impl ContentStore {
    pub fn prepare(limits: StoreLimits) -> Result<Self, Error> {
        if limits.block_events == 0
            || limits.blocks == 0
            || limits.max_events == 0
            || limits.views == 0
            || limits.snapshots == 0
            || limits.retirements == 0
        {
            return Err(Error::Capacity);
        }
        let count = (limits.blocks as usize)
            .checked_mul(limits.block_events as usize)
            .ok_or(Error::Capacity)?;
        let capacity = limits
            .max_events
            .checked_add(limits.block_events.checked_mul(2).ok_or(Error::Capacity)?)
            .ok_or(Error::Capacity)?;
        let table = (capacity as usize)
            .div_ceil(limits.block_events as usize)
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        let mut views = Vec::new();
        views
            .try_reserve_exact(limits.views as usize)
            .map_err(|_| Error::Capacity)?;
        for _ in 0..limits.views {
            views.push(View {
                generation: 0,
                refs: 0,
                retired: false,
                table: prepared(table, || 0)?.into_boxed_slice(),
                blocks: 0,
                first: 0,
                origin: 0,
                next_order: 0,
                event_limit: limits.max_events,
                description: Description {
                    events: 0,
                    frames: 0,
                    initial: SnapshotId {
                        slot: 0,
                        generation: 0,
                    },
                    status: TakeStatus::Empty,
                },
            });
        }
        let mut retired = VecDeque::new();
        retired
            .try_reserve(limits.retirements as usize)
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            limits,
            records: prepared(count, || Record {
                frame: 0,
                order: 0,
                message: Message {
                    bytes: [0x80, 0, 0],
                    len: 3,
                },
            })?
            .into_boxed_slice(),
            references: prepared(limits.blocks as usize, || 0)?.into_boxed_slice(),
            free_blocks: free_ids(limits.blocks)?,
            views: views.into_boxed_slice(),
            free_views: free_ids(limits.views)?,
            snapshots: prepared(limits.snapshots as usize, || Snapshot {
                active: 0,
                generation: 0,
                refs: 0,
                state: MidiState::default(),
            })?
            .into_boxed_slice(),
            free_snapshots: free_ids(limits.snapshots)?,
            live_snapshots: {
                let mut ids = Vec::new();
                ids.try_reserve_exact(limits.snapshots as usize)
                    .map_err(|_| Error::Capacity)?;
                ids
            },
            retired,
            work: 0,
            snapshot_bytes: 0,
        })
    }
    pub fn limits(&self) -> StoreLimits {
        self.limits
    }
    pub(super) fn view_blocks(&self, id: ViewId) -> Result<u32, Error> {
        Ok(self.view(id)?.blocks as u32)
    }
    pub(super) fn snapshot_bytes(&self) -> u64 {
        self.snapshot_bytes
    }
    pub fn work(&self) -> u64 {
        self.work
    }
    pub fn usage(&self) -> StoreUsage {
        StoreUsage {
            blocks: self.references.len() - self.free_blocks.len(),
            views: self.views.len() - self.free_views.len(),
            snapshots: self.snapshots.len() - self.free_snapshots.len(),
            retirements: self.retired.len(),
        }
    }
    pub fn prepared_bytes(&self) -> usize {
        size_of::<Self>()
            + self.records.len() * size_of::<Record>()
            + self.references.len() * size_of::<u32>()
            + self.free_blocks.capacity() * size_of::<u32>()
            + self.views.len() * size_of::<View>()
            + self
                .views
                .iter()
                .map(|v| v.table.len() * size_of::<u32>())
                .sum::<usize>()
            + self.free_views.capacity() * size_of::<u32>()
            + self.snapshots.len() * size_of::<Snapshot>()
            + (self.free_snapshots.capacity() + self.live_snapshots.capacity()) * size_of::<u32>()
            + self.retired.capacity() * size_of::<ViewId>()
    }
    pub fn snapshot(&mut self, state: &MidiState) -> Result<SnapshotId, Error> {
        if !state.is_basic() {
            return Err(Error::Unsupported);
        }
        let slot = *self.free_snapshots.last().ok_or(Error::Capacity)?;
        let s = &mut self.snapshots[slot as usize];
        let generation = s.generation.checked_add(1).ok_or(Error::Overflow)?;
        self.free_snapshots.pop();
        s.active = self.live_snapshots.len();
        self.live_snapshots.push(slot);
        s.generation = generation;
        s.refs = 1;
        s.state = *state;
        self.work = self.work.saturating_add(size_of::<MidiState>() as u64);
        self.snapshot_bytes = self
            .snapshot_bytes
            .saturating_add(size_of::<MidiState>() as u64);
        Ok(SnapshotId { slot, generation })
    }
    pub fn state(&self, id: SnapshotId) -> Result<&MidiState, Error> {
        let s = self
            .snapshots
            .get(id.slot as usize)
            .ok_or(Error::InvalidState)?;
        if s.generation != id.generation || s.refs == 0 {
            return Err(Error::InvalidState);
        }
        Ok(&s.state)
    }
    pub fn retain_snapshot(&mut self, id: SnapshotId) -> Result<(), Error> {
        self.state(id)?;
        let s = &mut self.snapshots[id.slot as usize];
        s.refs = s.refs.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn release_snapshot(&mut self, id: SnapshotId) -> Result<(), Error> {
        self.state(id)?;
        let s = &mut self.snapshots[id.slot as usize];
        s.refs -= 1;
        if s.refs == 0 {
            let at = s.active;
            self.live_snapshots.swap_remove(at);
            if let Some(&moved) = self.live_snapshots.get(at) {
                self.snapshots[moved as usize].active = at;
            }
            self.free_snapshots.push(id.slot);
        }
        Ok(())
    }
    pub fn snapshot_ids(&self) -> impl Iterator<Item = SnapshotId> + '_ {
        self.live_snapshots.iter().map(|&slot| SnapshotId {
            slot,
            generation: self.snapshots[slot as usize].generation,
        })
    }
    fn view(&self, id: ViewId) -> Result<&View, Error> {
        let v = self
            .views
            .get(id.slot as usize)
            .ok_or(Error::InvalidState)?;
        if v.generation != id.generation || v.refs == 0 || v.retired {
            return Err(Error::InvalidState);
        }
        Ok(v)
    }
    fn allocate_view(&mut self, initial: SnapshotId) -> Result<ViewId, Error> {
        self.state(initial)?;
        let slot = *self.free_views.last().ok_or(Error::Capacity)?;
        let generation = self.views[slot as usize]
            .generation
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.retain_snapshot(initial)?;
        self.free_views.pop();
        let v = &mut self.views[slot as usize];
        v.generation = generation;
        v.refs = 1;
        v.retired = false;
        v.blocks = 0;
        v.first = 0;
        v.origin = 0;
        v.next_order = 0;
        v.event_limit = self.limits.max_events;
        v.description = Description {
            events: 0,
            frames: 0,
            initial,
            status: TakeStatus::Recording,
        };
        Ok(ViewId { slot, generation })
    }
    pub fn writer(&mut self, initial: SnapshotId) -> Result<ViewId, Error> {
        self.allocate_view(initial)
    }
    pub(super) fn history_writer(&mut self, initial: SnapshotId) -> Result<ViewId, Error> {
        let id = self.allocate_view(initial)?;
        self.views[id.slot as usize].event_limit =
            self.limits.max_events + 2 * self.limits.block_events;
        Ok(id)
    }
    pub fn describe(&self, id: ViewId) -> Result<Description, Error> {
        Ok(self.view(id)?.description)
    }
    pub fn retain(&mut self, id: ViewId) -> Result<ViewId, Error> {
        let v = self.view(id)?;
        if v.description.status != TakeStatus::Complete {
            return Err(Error::Incomplete);
        }
        let refs = v.refs.checked_add(1).ok_or(Error::Overflow)?;
        self.views[id.slot as usize].refs = refs;
        self.work = self.work.saturating_add(1);
        Ok(id)
    }
    pub(super) fn can_retain(&self, id: ViewId, count: u32) -> Result<(), Error> {
        let v = self.view(id)?;
        if v.description.status != TakeStatus::Complete {
            return Err(Error::Incomplete);
        }
        v.refs.checked_add(count).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn can_release(&self, ids: &[ViewId]) -> Result<(), Error> {
        let mut needed = 0;
        for (i, id) in ids.iter().enumerate() {
            let v = self.view(*id)?;
            if ids[..i].contains(id) {
                continue;
            }
            let releases = ids.iter().filter(|other| **other == *id).count();
            if releases > v.refs as usize {
                return Err(Error::InvalidState);
            }
            if releases == v.refs as usize {
                needed += 1;
            }
        }
        if self.retired.len() + needed > self.limits.retirements as usize {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    pub fn release(&mut self, id: ViewId) -> Result<(), Error> {
        self.can_release(core::slice::from_ref(&id))?;
        let v = &mut self.views[id.slot as usize];
        v.refs -= 1;
        if v.refs == 0 {
            v.retired = true;
            self.retired.push_back(id);
        }
        self.work = self.work.saturating_add(1);
        Ok(())
    }
    fn release_block(&mut self, block: u32) {
        self.references[block as usize] -= 1;
        if self.references[block as usize] == 0 {
            self.free_blocks.push(block);
        }
    }
    pub fn maintenance(&mut self, budget: u32) -> u32 {
        let mut steps = 0;
        while steps < budget {
            let Some(id) = self.retired.front().copied() else {
                break;
            };
            let v = &mut self.views[id.slot as usize];
            if v.blocks != 0 {
                v.blocks -= 1;
                let b = v.table[v.blocks];
                self.release_block(b);
            } else {
                let initial = v.description.initial;
                self.release_snapshot(initial)
                    .expect("retired snapshot owner");
                self.free_views.push(id.slot);
                self.retired.pop_front();
            }
            steps += 1;
        }
        self.work = self.work.saturating_add(u64::from(steps));
        steps
    }
    pub fn read(&self, id: ViewId, index: u32) -> Result<Option<Record>, Error> {
        let v = self.view(id)?;
        if index >= v.description.events {
            return Ok(None);
        }
        let at = v.first + index as usize;
        let block = v.table[at / self.limits.block_events as usize] as usize;
        let mut record = self.records
            [block * self.limits.block_events as usize + at % self.limits.block_events as usize];
        record.frame -= v.origin;
        Ok(Some(record))
    }
    pub(super) fn contiguous(&self, id: ViewId) -> &[Record] {
        let v = self.view(id).expect("owned take view");
        if v.description.events == 0 {
            return &[];
        }
        assert_eq!(v.first, 0);
        assert_eq!(v.origin, 0);
        assert_eq!(v.blocks, 1);
        let start = v.table[0] as usize * self.limits.block_events as usize;
        &self.records[start..start + v.description.events as usize]
    }
    pub fn lower_bound(&self, id: ViewId, frame: u64) -> Result<u32, Error> {
        let mut low = 0;
        let mut high = self.describe(id)?.events;
        while low < high {
            let mid = low + (high - low) / 2;
            if self.read(id, mid)?.unwrap().frame < frame {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        Ok(low)
    }
    pub(super) fn can_append(&self, id: ViewId) -> Result<bool, Error> {
        let v = self.view(id)?;
        let at = v.first + v.description.events as usize;
        Ok(v.description.status == TakeStatus::Recording
            && v.description.events < v.event_limit
            && (at / (self.limits.block_events as usize) < v.blocks
                || !self.free_blocks.is_empty()))
    }
    pub fn append(&mut self, id: ViewId, frame: u64, message: Message) -> Result<(), Error> {
        Message::managed(message.as_bytes())?;
        let v = self.view(id)?;
        if v.description.status != TakeStatus::Recording {
            return Err(Error::InvalidState);
        }
        if v.description.events > 0
            && self.read(id, v.description.events - 1)?.unwrap().frame > frame
        {
            return Err(Error::InvalidEvent);
        }
        let frame = frame.checked_add(v.origin).ok_or(Error::Overflow)?;
        let order = v.next_order.checked_add(1).ok_or(Error::Overflow)?;
        let at = v.first + v.description.events as usize;
        let new_block = at / self.limits.block_events as usize == v.blocks;
        if v.description.events == v.event_limit || (new_block && self.free_blocks.is_empty()) {
            self.views[id.slot as usize].description.status = TakeStatus::Incomplete;
            return Err(Error::Capacity);
        }
        let v = &mut self.views[id.slot as usize];
        if new_block {
            let block = self.free_blocks.pop().unwrap();
            self.references[block as usize] = 1;
            v.table[v.blocks] = block;
            v.blocks += 1;
        }
        let block = v.table[at / self.limits.block_events as usize] as usize;
        self.records
            [block * self.limits.block_events as usize + at % self.limits.block_events as usize] =
            Record {
                frame,
                order: order - 1,
                message,
            };
        v.next_order = order;
        v.description.events += 1;
        self.work = self.work.saturating_add(1);
        Ok(())
    }
    pub fn advance(&mut self, id: ViewId, frames: u64) -> Result<(), Error> {
        let v = self.view(id)?;
        if v.description.status != TakeStatus::Recording || frames < v.description.frames {
            return Err(Error::InvalidState);
        }
        if v.description.events != 0
            && self.read(id, v.description.events - 1)?.unwrap().frame >= frames
        {
            return Err(Error::InvalidEvent);
        }
        self.views[id.slot as usize].description.frames = frames;
        Ok(())
    }
    pub(super) fn mark_incomplete(&mut self, id: ViewId) -> Result<(), Error> {
        if self.describe(id)?.status != TakeStatus::Recording {
            return Err(Error::InvalidState);
        }
        self.views[id.slot as usize].description.status = TakeStatus::Incomplete;
        Ok(())
    }
    pub fn finish(&mut self, id: ViewId, frames: u64) -> Result<(), Error> {
        if self.describe(id)?.status == TakeStatus::Incomplete {
            return Err(Error::Incomplete);
        }
        self.advance(id, frames)?;
        self.views[id.slot as usize].description.status = TakeStatus::Complete;
        Ok(())
    }
    pub fn range(
        &mut self,
        source: ViewId,
        start: u64,
        end: u64,
        initial: SnapshotId,
    ) -> Result<ViewId, Error> {
        let v = self.view(source)?;
        if start > end
            || end > v.description.frames
            || v.description.status == TakeStatus::Incomplete
        {
            return Err(Error::InvalidEvent);
        }
        let first_event = self.lower_bound(source, start)?;
        let end_event = self.lower_bound(source, end)?;
        let count = end_event - first_event;
        if count > self.limits.max_events {
            return Err(Error::Capacity);
        }
        let offset = v.first + first_event as usize;
        let first_block = offset / self.limits.block_events as usize;
        let blocks = if count == 0 {
            0
        } else {
            (offset % self.limits.block_events as usize + count as usize)
                .div_ceil(self.limits.block_events as usize)
        };
        let origin = v.origin.checked_add(start).ok_or(Error::Overflow)?;
        for block in &v.table[first_block..first_block + blocks] {
            self.references[*block as usize]
                .checked_add(1)
                .ok_or(Error::Overflow)?;
        }
        let id = self.allocate_view(initial)?;
        for i in 0..blocks {
            let b = self.views[source.slot as usize].table[first_block + i];
            self.views[id.slot as usize].table[i] = b;
            self.references[b as usize] += 1;
        }
        let target = &mut self.views[id.slot as usize];
        target.blocks = blocks;
        target.first = offset % self.limits.block_events as usize;
        target.origin = origin;
        target.description = Description {
            events: count,
            frames: end - start,
            initial,
            status: TakeStatus::Complete,
        };
        self.work = self.work.saturating_add(blocks as u64);
        Ok(id)
    }
    pub fn discard_before(&mut self, id: ViewId, frame: u64) -> Result<u32, Error> {
        let v = self.view(id)?;
        if v.description.status != TakeStatus::Recording || frame > v.description.frames {
            return Err(Error::InvalidState);
        }
        let count = self.lower_bound(id, frame)?;
        self.discard_events(id, count)?;
        Ok(count)
    }
    pub(super) fn discard_events(&mut self, id: ViewId, count: u32) -> Result<(), Error> {
        let v = self.view(id)?;
        if v.description.status != TakeStatus::Recording || count > v.description.events {
            return Err(Error::InvalidState);
        }
        let at = v.first + count as usize;
        let remove = at / self.limits.block_events as usize;
        for i in 0..remove {
            let b = self.views[id.slot as usize].table[i];
            self.release_block(b);
        }
        let v = &mut self.views[id.slot as usize];
        v.table.copy_within(remove..v.blocks, 0);
        v.blocks -= remove;
        v.first = at % self.limits.block_events as usize;
        v.description.events -= count;
        self.work = self.work.saturating_add((remove + v.blocks) as u64);
        Ok(())
    }
}
