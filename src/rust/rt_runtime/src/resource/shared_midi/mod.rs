//! Prepared shared MIDI packets, immutable windows and revision-owned state.
use super::{
    get, index, release,
    shared_pages::{LeaseId, SharedPages},
    RenderIo, ResourceStore, Slot, WorkClass,
};
use crate::{buffer::MidiBuffer, midi::Message, midi_state::MidiState, Error, Frame, Revision};
use alloc::{boxed::Box, vec, vec::Vec};
use core::ops::Range;

mod edit;
mod history;
mod import;
mod layout;
mod read;
mod storage;
pub use edit::{EditId, EditStatus};
pub use import::ImportId;
pub use read::{Cursor, Event, Position};

pub const EVENTS_PER_CHUNK: usize = 8;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub history_frames: u64,
    pub chunks: usize,
    pub views: usize,
    pub pages: usize,
    pub max_events: usize,
    pub max_bytes: usize,
    pub max_packet: usize,
    pub transactions: usize,
    pub edit_steps: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Handle {
    scope: u32,
    slot: u32,
    generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TakeId(Handle);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportId(Handle);
/// Borrowed access. Copying acquires no owner; take edits invalidate old cursors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadIndex(Handle);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Info {
    pub frames: u64,
    pub bytes: usize,
    pub events: usize,
    pub checkpoints: usize,
    pub revision: Revision,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capacity {
    /// Prepared view slots; private transactions share the retained-chunk budget.
    pub owners: usize,
    pub chunks: usize,
    pub index_nodes: usize,
    pub state_bytes: usize,
    pub metadata_bytes: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    pub free_nodes: usize,
    pub free_chunks: usize,
    pub free_blocks: usize,
    pub free_views: usize,
    pub reserved_pages: usize,
    pub retiring: usize,
    pub owners: usize,
    pub retained_chunks: usize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Work {
    pub records: usize,
    pub state_copies: usize,
    pub index_steps: usize,
    pub reclaimed: usize,
    pub counter_overflow: bool,
}
impl Work {
    fn add(&mut self, records: usize, states: usize, indexes: usize, reclaimed: usize) {
        for (counter, amount) in [
            (&mut self.records, records),
            (&mut self.state_copies, states),
            (&mut self.index_steps, indexes),
            (&mut self.reclaimed, reclaimed),
        ] {
            match counter.checked_add(amount) {
                Some(n) => *counter = n,
                None => {
                    *counter = usize::MAX;
                    self.counter_overflow = true;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Point {
    serial: u64,
    event: usize,
}
#[derive(Clone, Copy, Default)]
struct Record {
    frame: u64,
    block: usize,
    offset: usize,
    len: usize,
}
#[derive(Clone, Copy)]
struct Chunk {
    references: usize,
    serial: u64,
    count: usize,
    records: [Record; EVENTS_PER_CHUNK],
    bytes: usize,
}
#[derive(Clone, Copy)]
struct Base {
    snapshot: crate::resource::Snapshot,
    index: crate::resource::ReadIndex,
    references: usize,
}
#[derive(Clone, Copy)]
struct Block {
    references: usize,
    bytes: usize,
    pages: usize,
    base: Option<(usize, usize)>,
}
#[derive(Clone, Copy)]
enum Kind {
    Branch([Option<usize>; 2]),
    Leaf(usize),
}
#[derive(Clone, Copy)]
struct Node {
    references: usize,
    kind: Kind,
    bytes: usize,
    events: usize,
}
#[derive(Clone, Copy)]
struct View {
    root: Option<usize>,
    first: Point,
    last: Point,
    origin: u64,
    frames: u64,
    bytes: usize,
    events: usize,
    revision: Revision,
    writable: bool,
    retiring: bool,
    credit: usize,
}
#[derive(Clone, Copy)]
enum Garbage {
    Node(usize),
    Chunk(usize, usize),
    Block(usize, usize),
    Edit(usize, usize),
}
#[derive(Clone, Copy)]
struct Job {
    garbage: Garbage,
    owner: usize,
}

pub struct MidiStore {
    scope: u32,
    store_scope: u32,
    config: Config,
    page_bytes: usize,
    block_pages: usize,
    depth: usize,
    origin: Frame,
    next: Frame,
    pressure_start: Frame,
    end: u64,
    root: Option<usize>,
    current: Box<MidiState>,
    capacity: Capacity,
    nodes: Vec<Option<Node>>,
    free_nodes: Vec<usize>,
    chunks: Vec<Option<Chunk>>,
    chunk_states: Box<[MidiState]>,
    free_chunks: Vec<usize>,
    blocks: Vec<Option<Block>>,
    block_map: Box<[Option<LeaseId>]>,
    free_blocks: Vec<usize>,
    views: Vec<Slot<View>>,
    initial: Box<[MidiState]>,
    free_views: Vec<usize>,
    pages: SharedPages,
    reserved: usize,
    debt: usize,
    owners: usize,
    debt_limit: usize,
    scratch: Box<[u8]>,
    records: [Record; EVENTS_PER_CHUNK],
    retired: Vec<Job>,
    immediate: Vec<Job>,
    pending: Box<[usize]>,
    closed: bool,
    work: Work,
    edits: Vec<Slot<edit::Edit>>,
    free_edits: Vec<usize>,
    edit_records: Box<[Record]>,
    edit_states: Box<[MidiState]>,
    bases: Vec<Option<Base>>,
    free_bases: Vec<usize>,
}

fn touched<T: Copy>(count: usize, value: T) -> Vec<T> {
    let mut entries = vec![value; count];
    for entry in &mut entries {
        *core::hint::black_box(entry) = value;
    }
    entries
}

fn jobs(count: usize) -> Vec<Job> {
    let mut result = vec![
        Job {
            garbage: Garbage::Node(0),
            owner: 0
        };
        count
    ];
    for job in &mut result {
        *core::hint::black_box(job) = Job {
            garbage: Garbage::Node(0),
            owner: 0,
        };
    }
    result.clear();
    result
}

impl MidiStore {
    pub fn prepare(
        store: &mut ResourceStore,
        config: Config,
        origin: Frame,
        initial: &MidiState,
    ) -> Result<Self, Error> {
        let layout = layout::Layout::new(store.config, config)?;
        if config.pages > store.free_pages.len() - store.reserved_pages {
            return Err(Error::Capacity);
        }
        let page_bytes = store.config.limits().page_bytes;
        let layout::Layout {
            block_pages,
            depth,
            chunks,
            nodes,
            blocks,
            base_slots,
            map,
            garbage,
            views,
            debt_limit,
            capacity,
        } = layout;
        let mut chunk_states = vec![MidiState::default(); chunks].into_boxed_slice();
        let mut states = vec![MidiState::default(); views].into_boxed_slice();
        let mut scratch = vec![0; page_bytes].into_boxed_slice();
        let mut edit_states =
            vec![MidiState::default(); config.transactions * 3].into_boxed_slice();
        for state in &mut edit_states {
            *core::hint::black_box(state) = MidiState::default();
        }
        for state in chunk_states.iter_mut().chain(states.iter_mut()) {
            *core::hint::black_box(state) = MidiState::default();
        }
        for byte in &mut scratch {
            *core::hint::black_box(byte) = 0;
        }
        let mut block_map = vec![None; map].into_boxed_slice();
        for page in &mut block_map {
            *core::hint::black_box(page) = None;
        }
        let pages = SharedPages::prepare(store, config.pages)?;
        let scope = index::new_scope()?;
        store.reserved_pages += config.pages;
        Ok(Self {
            scope,
            store_scope: store.scope,
            config,
            page_bytes,
            block_pages,
            depth,
            origin,
            next: origin,
            pressure_start: origin,
            end: 0,
            root: None,
            current: Box::new(*initial),
            capacity,
            nodes: touched(nodes, None),
            free_nodes: (0..nodes).rev().collect(),
            chunks: touched(chunks, None),
            chunk_states,
            free_chunks: (0..chunks).rev().collect(),
            blocks: touched(blocks, None),
            block_map,
            free_blocks: (0..blocks).rev().collect(),
            views: touched(
                views,
                Slot {
                    generation: 1,
                    value: None,
                },
            ),
            initial: states,
            free_views: (0..views).rev().collect(),
            pages,
            reserved: config.pages,
            debt: 0,
            owners: 0,
            debt_limit,
            scratch,
            records: [Record::default(); EVENTS_PER_CHUNK],
            retired: jobs(garbage),
            immediate: jobs(garbage),
            pending: touched(views + config.transactions + 2, 0).into_boxed_slice(),
            closed: false,
            work: Work::default(),
            edits: touched(
                config.transactions,
                Slot {
                    generation: 1,
                    value: None,
                },
            ),
            free_edits: (0..config.transactions).rev().collect(),
            edit_records: touched(config.transactions * config.max_events, Record::default())
                .into_boxed_slice(),
            edit_states,
            bases: touched(base_slots, None),
            free_bases: (0..base_slots).rev().collect(),
        })
    }
    fn check(&self, store: &ResourceStore) -> Result<(), Error> {
        if store.scope != self.store_scope {
            return Err(Error::InvalidId);
        }
        if self.closed {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
    pub fn capacity(&self) -> Capacity {
        self.capacity
    }
    pub fn work(&self) -> Work {
        self.work
    }
    pub fn usage(&self) -> Usage {
        Usage {
            free_nodes: self.free_nodes.len(),
            free_chunks: self.free_chunks.len(),
            free_blocks: self.free_blocks.len(),
            free_views: self.free_views.len(),
            reserved_pages: self.reserved,
            retiring: self.retired.len(),
            owners: self.owners,
            retained_chunks: self.debt,
        }
    }
    pub fn coverage(&self) -> Result<Range<Frame>, Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        Ok(Frame(
            self.next
                .0
                .saturating_sub(self.config.history_frames)
                .max(self.origin.0)
                .max(self.pressure_start.0),
        )..self.next)
    }
    fn view(&self, h: Handle) -> Result<View, Error> {
        if h.scope != self.scope {
            return Err(Error::InvalidId);
        }
        get(&self.views, h.slot, h.generation).and_then(|v| {
            if v.retiring {
                Err(Error::InvalidId)
            } else {
                Ok(v)
            }
        })
    }
    fn own(&mut self, view: View, state: &MidiState, writable: bool) -> Result<Handle, Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        if self.free_views.is_empty() || view.credit > self.debt_limit - self.debt {
            return Err(Error::Capacity);
        }
        self.retain(view.root)?;
        Ok(self.insert_view(view, state, writable))
    }
    fn insert_view(&mut self, mut view: View, state: &MidiState, writable: bool) -> Handle {
        let slot = self.free_views.pop().expect("prepared view");
        view.writable = writable;
        self.views[slot].value = Some(view);
        self.initial[slot] = *state;
        self.debt += view.credit;
        self.owners += 1;
        self.work.add(0, 1, 0, 0);
        Handle {
            scope: self.scope,
            slot: slot as u32,
            generation: self.views[slot].generation,
        }
    }
    pub fn fork(&mut self, take: TakeId) -> Result<TakeId, Error> {
        let v = self.view(take.0)?;
        let state = self.initial[take.0.slot as usize];
        self.work.add(0, 1, 0, 0);
        self.own(v, &state, true).map(TakeId)
    }
    pub fn export(&mut self, take: TakeId) -> Result<ExportId, Error> {
        let v = self.view(take.0)?;
        let state = self.initial[take.0.slot as usize];
        self.work.add(0, 1, 0, 0);
        self.own(v, &state, false).map(ExportId)
    }
    pub fn retain_export(&mut self, export: ExportId) -> Result<ExportId, Error> {
        let view = self.view(export.0)?;
        let state = self.initial[export.0.slot as usize];
        self.work.add(0, 1, 0, 0);
        self.own(view, &state, false).map(ExportId)
    }
    pub fn fork_export(&mut self, export: ExportId) -> Result<TakeId, Error> {
        let view = self.view(export.0)?;
        let state = self.initial[export.0.slot as usize];
        self.work.add(0, 1, 0, 0);
        self.own(view, &state, true).map(TakeId)
    }
    pub fn take_index(&self, take: TakeId) -> Result<ReadIndex, Error> {
        self.view(take.0)?;
        Ok(ReadIndex(take.0))
    }
    pub fn export_index(&self, export: ExportId) -> Result<ReadIndex, Error> {
        self.view(export.0)?;
        Ok(ReadIndex(export.0))
    }
    pub fn info(&self, index: ReadIndex) -> Result<Info, Error> {
        let v = self.view(index.0)?;
        Ok(Info {
            frames: v.frames,
            bytes: v.bytes,
            events: v.events,
            checkpoints: if v.first == v.last {
                0
            } else {
                (v.last.serial - v.first.serial) as usize + usize::from(v.last.event > 0)
            },
            revision: v.revision,
        })
    }
    pub fn initial_state(&self, index: ReadIndex) -> Result<&MidiState, Error> {
        self.view(index.0)?;
        Ok(&self.initial[index.0.slot as usize])
    }
    pub fn release_take(&mut self, take: TakeId) -> Result<(), Error> {
        self.release_view(take.0)
    }
    pub fn release_export(&mut self, export: ExportId) -> Result<(), Error> {
        self.release_view(export.0)
    }
    fn release_view(&mut self, h: Handle) -> Result<(), Error> {
        let mut v = self.view(h)?;
        v.retiring = true;
        self.views[h.slot as usize].value = Some(v);
        self.drop_root(v.root, h.slot as usize, false);
        self.finish_owner(h.slot as usize);
        Ok(())
    }
    fn finish_owner(&mut self, owner: usize) {
        if owner < self.views.len() && self.pending[owner] == 0 {
            self.debt -= self.views[owner].value.expect("retiring owner").credit;
            self.owners -= 1;
            release(&mut self.views, &mut self.free_views, owner);
        } else if owner >= self.views.len() + 2 && self.pending[owner] == 0 {
            self.debt -= self.edits[owner - self.views.len() - 2]
                .value
                .expect("retiring edit")
                .credit;
            self.owners -= 1;
            release(
                &mut self.edits,
                &mut self.free_edits,
                owner - self.views.len() - 2,
            );
        }
    }
    pub fn begin_cleanup(&mut self) -> Result<(), Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        if self.debt != 0 {
            return Err(Error::Busy);
        }
        self.closed = true;
        let root = self.root.take();
        self.drop_root(root, self.views.len(), false);
        Ok(())
    }
    pub fn maintain(&mut self, store: &mut ResourceStore) -> Result<usize, Error> {
        self.maintain_limit(store, usize::MAX)
    }
    pub(super) fn maintain_limit(
        &mut self,
        store: &mut ResourceStore,
        limit: usize,
    ) -> Result<usize, Error> {
        if store.scope != self.store_scope {
            return Err(Error::InvalidId);
        }
        let mut steps = 0;
        while steps < limit && store.steps_remaining() > 0 {
            let Some(job) = self.retired.pop() else {
                break;
            };
            self.process_job(store, job, false)?;
            steps += 1;
        }
        if self.closed && self.retired.is_empty() {
            store.reserved_pages -= self.reserved;
            self.reserved = 0;
        }
        Ok(steps)
    }
}

#[cfg(test)]
mod tests;
