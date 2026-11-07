//! Prepared interleaved audio history and copy-on-write takes. Handles own one
//! root of a persistent page index; borrowed read indexes acquire no ownership.
//! Capture and edits preflight all channels/pages before publishing a new root.
//! Required copies use render accounting, independent of background transfers.

use super::{
    get, index, release, retirement_stack,
    shared_pages::{LeaseId, SharedPages},
    slots, RenderIo, ResourceStore, Slot, WorkClass,
};
use crate::{Error, Frame, Revision};
use alloc::{boxed::Box, vec, vec::Vec};
use core::ops::Range;

mod capture;
mod layout;
pub(in crate::resource) use capture::CaptureWrite;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub channels: u32,
    /// Zero prepares an edit-only store with no input history.
    pub history_frames: usize,
    pub views: usize,
    /// Includes live history, retained versions and transaction workspace.
    pub pages: usize,
}

/// One channel's nonwrapping raw span within interleaved source samples.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChannelWrite {
    pub channel: usize,
    pub position: u64,
    pub offset: usize,
    pub frames: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Handle {
    scope: u32,
    slot: u32,
    generation: u32,
}

/// An independently editable take. Copying this token does not retain it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TakeId(Handle);

/// An immutable independently owned export, including its content revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportId(Handle);

/// Borrows one owner's current view; release invalidates it. Use an export to
/// keep one revision independently of future take edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadIndex(Handle);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryId {
    scope: u32,
    slot: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Info {
    pub channels: u32,
    pub frames: u64,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioPoolWork {
    pub index_allocated: u64,
    pub index_reclaimed: u64,
    pub shared_payloads: u64,
    pub payload_live: u64,
    pub index_live: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capacity {
    pub histories: usize,
    pub history_pages: usize,
    pub transaction_pages: usize,
    pub index_nodes: usize,
    /// Additional prepared arrays and scratch; excludes allocator overhead and
    /// the resource store's common arena metadata.
    pub metadata_bytes: usize,
}

#[derive(Clone, Copy)]
enum Kind {
    Branch([Option<usize>; 2]),
    Leaf(LeaseId),
}

#[derive(Clone, Copy)]
struct Node {
    references: usize,
    kind: Kind,
}

#[derive(Clone, Copy)]
struct Prefix {
    frames: usize,
    tail: usize,
}
#[derive(Clone, Copy)]
struct Mapping {
    first: usize,
    modulus: usize,
    prefix: Option<Prefix>,
}
impl Mapping {
    fn physical(self, local: usize) -> usize {
        match self.prefix {
            Some(prefix) if local >= prefix.frames => prefix.tail + local - prefix.frames,
            _ => (self.first + local) % self.modulus,
        }
    }
    fn contiguous(self, local: usize) -> usize {
        match self.prefix {
            Some(prefix) if local >= prefix.frames => usize::MAX - self.physical(local),
            Some(prefix) => (prefix.frames - local).min(self.modulus - self.physical(local)),
            None => self.modulus - self.physical(local),
        }
    }
    fn credits(self, frames: usize, page_frames: usize) -> usize {
        let prefix = self.prefix.map_or(frames, |p| frames.min(p.frames));
        let head = if prefix == 0 {
            0
        } else {
            (self.first % page_frames + prefix)
                .div_ceil(page_frames)
                .min(self.modulus / page_frames)
        };
        head + self.prefix.map_or(0, |p| {
            (frames.saturating_sub(p.frames)).div_ceil(page_frames)
        })
    }
}

/// A private root awaiting explicit commit or cancellation.
#[must_use]
pub(in crate::resource) struct Write {
    take: TakeId,
    expected: Revision,
    view: View,
}

#[derive(Clone, Copy)]
struct View {
    root: Option<usize>,
    mapping: Mapping,
    frames: usize,
    growing: Option<usize>,
    zero_from: Option<usize>,
    revision: Revision,
    writable: bool,
    reserved: bool,
    retiring: bool,
    base: Option<usize>,
    credit: usize,
}

#[derive(Clone, Copy)]
struct Base {
    snapshot: super::Snapshot,
    index: super::ReadIndex,
    references: usize,
}

#[derive(Clone, Copy, Default)]
struct Plan {
    page: usize,
}

#[derive(Clone, Copy)]
struct History {
    origin: Frame,
    next: Frame,
    root: Option<usize>,
}

pub struct AudioStore {
    work: AudioPoolWork,
    last_payload: Option<LeaseId>,
    scope: u32,
    store_scope: u32,
    channels: usize,
    keep: usize,
    histories: Box<[History]>,
    ring_pages: usize,
    page_frames: usize,
    page_bytes: usize,
    max_frames: usize,
    depth: usize,
    capacity: Capacity,
    nodes: Vec<Option<Node>>,
    free_nodes: Vec<usize>,
    views: Vec<Slot<View>>,
    free_views: Vec<usize>,
    debt: usize,
    debt_limit: usize,
    reserved: usize,
    pages: SharedPages,
    marks: Box<[(u64, usize)]>,
    generation: u64,
    plans: Vec<Plan>,
    scratch: Box<[u8]>,
    immediate: Vec<usize>,
    retired: Vec<(usize, usize)>,
    pending: Box<[usize]>,
    closed: bool,
    bases: Vec<Option<Base>>,
    retired_bases: Vec<usize>,
    base_turn: bool,
}

impl AudioStore {
    pub(super) fn reset_work(&mut self) {
        self.work = AudioPoolWork::default();
    }
    pub(super) fn work(&self) -> AudioPoolWork {
        AudioPoolWork {
            payload_live: if self.closed {
                0
            } else {
                self.pages.live_payloads() as u64
            },
            index_live: (self.nodes.len() - self.free_nodes.len()) as u64,
            ..self.work
        }
    }

    pub(super) fn chunk_samples(&self, lease: LeaseId) -> Result<&[f32], Error> {
        self.pages.sample_span(lease)
    }
    pub(super) fn retain_chunk(&mut self, lease: LeaseId) -> Result<LeaseId, Error> {
        self.pages.retain(lease)
    }
    pub(super) fn release_chunk(
        &mut self,
        store: &mut ResourceStore,
        lease: LeaseId,
    ) -> Result<(), Error> {
        if !self.pages.chunked() {
            return Err(Error::InvalidState);
        }
        if self.pages.reclaim_render(store, lease)? {
            self.reserved += 1;
        }
        Ok(())
    }
    pub(super) fn chunk_sequence(
        &self,
        store: super::AudioStoreId,
        index: ReadIndex,
        position: u64,
        frames: usize,
        channel: usize,
    ) -> Result<Option<crate::audio_refs::AudioSequence>, Error> {
        use crate::audio_refs::{AudioSequence, ChunkRef, ChunkSpan};
        if !self.pages.chunked() {
            return Ok(None);
        }
        let view = self.view(index.0)?;
        let position = usize::try_from(position).map_err(|_| Error::InvalidOffset)?;
        if channel >= self.channels
            || frames > self.page_frames
            || position
                .checked_add(frames)
                .is_none_or(|end| end > view.frames)
            || view.zero_from.is_some_and(|zero| position + frames > zero)
        {
            return Ok(None);
        }
        let mut result = AudioSequence::default();
        let mut at = 0;
        while at < frames {
            let physical = view.mapping.physical(position + at);
            let count = (frames - at)
                .min(self.page_frames - physical % self.page_frames)
                .min(view.mapping.contiguous(position + at));
            let Some(lease) = self.find(view.root, physical / self.page_frames) else {
                return Ok(None);
            };
            if result.len == result.spans.len() {
                return Ok(None);
            }
            result.spans[result.len] = ChunkSpan {
                chunk: ChunkRef::Persistent(super::AudioChunkRef {
                    store,
                    lease,
                    first: physical % self.page_frames * self.channels + channel,
                    stride: self.channels,
                }),
                frames: count,
            };
            result.len += 1;
            at += count;
        }
        Ok(Some(result))
    }
    pub fn prepare(
        store: &mut ResourceStore,
        config: Config,
        origin: Frame,
    ) -> Result<Self, Error> {
        Self::prepare_histories(store, config, origin, 1)
    }

    /// Prepare independent source histories sharing one page/index/owner budget.
    pub fn prepare_histories(
        store: &mut ResourceStore,
        config: Config,
        origin: Frame,
        histories: usize,
    ) -> Result<Self, Error> {
        let layout = if store.audio_render_mode().chunked() {
            layout::Layout::chunks(
                store.config,
                config,
                histories,
                store.quantum.frames(),
                false,
            )?
        } else {
            layout::Layout::new(store.config, config, histories)?
        };
        Self::prepare_layout(store, config, origin, histories, layout)
    }
    pub fn prepare_histories_windowed(
        store: &mut ResourceStore,
        config: Config,
        origin: Frame,
        histories: usize,
    ) -> Result<Self, Error> {
        let layout = if store.audio_render_mode().chunked() {
            layout::Layout::chunks(
                store.config,
                config,
                histories,
                store.quantum.frames(),
                true,
            )?
        } else {
            layout::Layout::new_windowed(store.config, config, histories)?
        };
        Self::prepare_layout(store, config, origin, histories, layout)
    }
    fn prepare_layout(
        store: &mut ResourceStore,
        config: Config,
        origin: Frame,
        histories: usize,
        layout: layout::Layout,
    ) -> Result<Self, Error> {
        if config.pages > store.free_pages.len() - store.reserved_pages {
            return Err(Error::Capacity);
        }
        let layout::Layout {
            chunked,
            payload_pages,
            channels,
            page_bytes,
            page_frames,
            ring_pages,
            max_frames,
            depth,
            node_limit,
            scratch_bytes,
            lease_limit,
            index_pages,
            transaction_pages,
            debt_limit,
            capacity,
        } = layout;
        let mut scratch = vec![0; scratch_bytes].into_boxed_slice();
        let mut nodes = vec![None; node_limit];
        let mut marks = vec![(0, 0); index_pages].into_boxed_slice();
        let mut views = slots(config.views);
        let mut plans = vec![Plan::default(); transaction_pages];
        let mut retired = vec![(0, 0); node_limit];
        let mut pending = vec![0; config.views + histories].into_boxed_slice();
        for byte in &mut scratch {
            *core::hint::black_box(byte) = 0;
        }
        for node in &mut nodes {
            *core::hint::black_box(node) = None;
        }
        for mark in &mut marks {
            *core::hint::black_box(mark) = (0, 0);
        }
        for view in &mut views {
            *core::hint::black_box(view) = Slot {
                generation: 1,
                value: None,
            };
        }
        for plan in &mut plans {
            *core::hint::black_box(plan) = Plan::default();
        }
        for entry in &mut retired {
            *core::hint::black_box(entry) = (0, 0);
        }
        for count in &mut pending {
            *core::hint::black_box(count) = 0;
        }
        plans.clear();
        retired.clear();
        let pages = if chunked {
            SharedPages::prepare_chunks(
                store,
                config.pages,
                payload_pages,
                page_bytes / 4,
                lease_limit,
            )?
        } else {
            SharedPages::prepare(store, lease_limit)?
        };
        let scope = index::new_scope()?;
        if !chunked {
            store.reserved_pages += config.pages;
        }
        let mut bases = vec![None; config.views];
        for base in &mut bases {
            *core::hint::black_box(base) = None;
        }
        Ok(Self {
            work: AudioPoolWork::default(),
            last_payload: None,
            scope,
            store_scope: store.scope,
            channels,
            keep: config.history_frames,
            histories: vec![
                History {
                    origin,
                    next: origin,
                    root: None
                };
                histories
            ]
            .into_boxed_slice(),
            ring_pages,
            page_frames,
            page_bytes,
            max_frames,
            depth,
            capacity,
            nodes,
            free_nodes: (0..node_limit).rev().collect(),
            views,
            free_views: (0..config.views).rev().collect(),
            debt: 0,
            debt_limit,
            reserved: payload_pages,
            pages,
            marks,
            generation: 0,
            plans,
            scratch,
            immediate: retirement_stack(node_limit),
            retired,
            pending,
            closed: false,
            bases,
            retired_bases: retirement_stack(config.views),
            base_turn: true,
        })
    }

    fn check(&self, store: &ResourceStore) -> Result<(), Error> {
        if self.store_scope != store.scope {
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

    pub fn channels(&self) -> u32 {
        self.channels as u32
    }

    pub fn coverage(&self) -> Result<Range<Frame>, Error> {
        self.history_coverage(self.history_id(0)?)
    }

    pub fn history_id(&self, slot: usize) -> Result<HistoryId, Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        if slot >= self.histories.len() {
            return Err(Error::InvalidId);
        }
        Ok(HistoryId {
            scope: self.scope,
            slot,
        })
    }
    fn history(&self, id: HistoryId) -> Result<History, Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.histories.get(id.slot).copied().ok_or(Error::InvalidId)
    }
    pub fn history_coverage(&self, id: HistoryId) -> Result<Range<Frame>, Error> {
        let history = self.history(id)?;
        Ok(Frame(
            history
                .next
                .0
                .saturating_sub(self.keep as u64)
                .max(history.origin.0),
        )..history.next)
    }

    /// Start a new complete input segment; retained owners keep their old roots.
    pub fn discontinuity(&mut self, io: &mut RenderIo<'_>, next: Frame) -> Result<(), Error> {
        self.history_discontinuity(io, self.history_id(0)?, next)
    }

    pub fn history_discontinuity(
        &mut self,
        io: &mut RenderIo<'_>,
        id: HistoryId,
        next: Frame,
    ) -> Result<(), Error> {
        self.check(io.store)?;
        let history = self.history(id)?;
        if next < history.next {
            return Err(Error::InvalidFrame);
        }
        self.queue(history.root, self.views.len() + id.slot);
        self.histories[id.slot] = History {
            origin: next,
            next,
            root: None,
        };
        Ok(())
    }

    fn view(&self, handle: Handle) -> Result<View, Error> {
        if handle.scope != self.scope {
            return Err(Error::InvalidId);
        }
        get(&self.views, handle.slot, handle.generation).and_then(|view| {
            if view.retiring {
                Err(Error::InvalidId)
            } else {
                Ok(view)
            }
        })
    }

    fn retain_root(&mut self, root: Option<usize>) -> Result<(), Error> {
        if let Some(index) = root {
            let node = self.nodes[index].as_mut().expect("live root");
            node.references = node.references.checked_add(1).ok_or(Error::Exhausted)?;
        }
        Ok(())
    }

    fn own(&mut self, mut view: View, writable: bool) -> Result<Handle, Error> {
        view.credit = view.mapping.credits(view.frames, self.page_frames);
        view.growing = None;
        view.reserved = false;
        if self.closed {
            return Err(Error::InvalidState);
        }
        if self.free_views.is_empty() || view.credit > self.debt_limit - self.debt {
            return Err(Error::Capacity);
        }
        if let Some(base) = view.base {
            if self.bases[base].expect("owned base").references == usize::MAX {
                return Err(Error::Exhausted);
            }
        }
        self.retain_root(view.root)?;
        if let Some(base) = view.base {
            self.bases[base].as_mut().expect("owned base").references += 1;
        }
        Ok(self.insert_view(view, writable))
    }

    fn insert_view(&mut self, mut view: View, writable: bool) -> Handle {
        let slot = self.free_views.pop().expect("prepared view credit");
        view.writable = writable;
        self.views[slot].value = Some(view);
        self.debt += view.credit;
        Handle {
            scope: self.scope,
            slot: slot as u32,
            generation: self.views[slot].generation,
        }
    }

    pub fn grab(&mut self, io: &mut RenderIo<'_>, range: Range<Frame>) -> Result<TakeId, Error> {
        self.grab_history(io, self.history_id(0)?, range)
    }

    pub fn grab_history(
        &mut self,
        io: &mut RenderIo<'_>,
        id: HistoryId,
        range: Range<Frame>,
    ) -> Result<TakeId, Error> {
        self.check(io.store)?;
        let history = self.history(id)?;
        let coverage = self.history_coverage(id)?;
        if range.start < coverage.start || range.end > coverage.end || range.start >= range.end {
            return Err(Error::InvalidFrame);
        }
        let first = (range.start.0 % self.ring_frames() as u64) as usize;
        let frames = (range.end.0 - range.start.0) as usize;
        let last_page = (first + frames - 1) / self.page_frames;
        let ranges = [
            first / self.page_frames..(last_page + 1).min(self.ring_pages),
            0..(last_page + 1).saturating_sub(self.ring_pages),
        ];
        let credit = ranges.iter().map(|range| range.end - range.start).sum();
        if self.free_views.is_empty()
            || credit > self.debt_limit - self.debt
            || self.free_nodes.len() < 4 * (self.depth + 1)
        {
            return Err(Error::Capacity);
        }
        let steps = 8 * (self.depth + 1);
        io.store.charge_work(WorkClass::Render, 0, steps)?;
        io.store.note_operation(WorkClass::Render);
        for _ in 0..steps {
            io.store.index_step(WorkClass::Render);
        }
        let root = self.clip(history.root, 0, self.marks.len(), &ranges);
        Ok(TakeId(self.insert_view(
            View {
                root,
                mapping: Mapping {
                    first,
                    modulus: self.ring_frames(),
                    prefix: Some(Prefix {
                        frames,
                        tail: self.ring_frames(),
                    }),
                },
                frames,
                growing: None,
                zero_from: None,
                revision: Revision(0),
                writable: true,
                reserved: false,
                retiring: false,
                base: None,
                credit,
            },
            true,
        )))
    }

    pub fn grab_history_window(
        &mut self,
        io: &mut RenderIo<'_>,
        id: HistoryId,
        range: Range<Frame>,
        frames: u64,
    ) -> Result<TakeId, Error> {
        self.check(io.store)?;
        let coverage = self.history_coverage(id)?;
        if range.start < coverage.start
            || range.end > coverage.end
            || range.start > range.end
            || range.end.0 - range.start.0 > frames
        {
            return Err(Error::InvalidFrame);
        }
        let take = if range.start == range.end {
            self.empty_take()?
        } else {
            self.grab_history(io, id, range)?
        };
        if let Err(error) = self.pad_take(take, frames) {
            self.release_take(take)?;
            return Err(error);
        }
        Ok(take)
    }

    /// Extend an owned view with declared empty media, never with unknown source history.
    pub fn pad_take(&mut self, take: TakeId, frames: u64) -> Result<(), Error> {
        let old = self.view(take.0)?;
        if frames == old.frames as u64 {
            return Ok(());
        }
        let revision = old.revision.checked_next()?;
        self.reserve_growth(take, frames)?;
        let mut view = self.view(take.0)?;
        view.frames = frames as usize;
        view.growing = None;
        view.zero_from = old.zero_from.or(Some(old.frames));
        view.revision = revision;
        self.views[take.0.slot as usize].value = Some(view);
        Ok(())
    }

    pub fn fork(&mut self, take: TakeId) -> Result<TakeId, Error> {
        self.own(self.view(take.0)?, true).map(TakeId)
    }

    pub(in crate::resource) fn reserve_export(
        &mut self,
        take: TakeId,
        maximum_frames: u64,
    ) -> Result<ExportId, Error> {
        let source = self.view(take.0)?;
        let maximum = usize::try_from(maximum_frames).map_err(|_| Error::Capacity)?;
        let limit = self.marks.len() * self.page_frames;
        if maximum < source.frames {
            return Err(Error::InvalidShape);
        }
        if maximum > limit
            || maximum > 0
                && (source.mapping.physical(maximum - 1) >= limit
                    || source.mapping.prefix.is_none() && maximum > limit - source.mapping.first)
        {
            return Err(Error::Capacity);
        }
        let credit = source.mapping.credits(maximum, self.page_frames);
        if self.free_views.is_empty() || credit > self.debt_limit - self.debt {
            return Err(Error::Capacity);
        }
        Ok(ExportId(self.insert_view(
            View {
                root: None,
                mapping: source.mapping,
                frames: 0,
                growing: None,
                zero_from: Some(0),
                revision: Revision(0),
                writable: false,
                reserved: true,
                retiring: false,
                base: None,
                credit,
            },
            false,
        )))
    }
    pub(in crate::resource) fn publish_reserved_export(
        &mut self,
        export: ExportId,
        take: TakeId,
    ) -> Result<(), Error> {
        let mut source = self.view(take.0)?;
        let target = self.view(export.0)?;
        let credit = source.mapping.credits(source.frames, self.page_frames);
        if !target.reserved || target.root.is_some() || target.base.is_some() {
            return Err(Error::InvalidState);
        }
        if credit > target.credit {
            return Err(Error::Capacity);
        }
        if let Some(base) = source.base {
            if self.bases[base].expect("owned base").references == usize::MAX {
                return Err(Error::Exhausted);
            }
        }
        self.retain_root(source.root)?;
        if let Some(base) = source.base {
            self.bases[base].as_mut().expect("owned base").references += 1;
        }
        source.credit = credit;
        source.writable = false;
        source.reserved = false;
        source.growing = None;
        self.debt -= target.credit - credit;
        self.views[export.0.slot as usize].value = Some(source);
        Ok(())
    }

    pub fn export(&mut self, take: TakeId) -> Result<ExportId, Error> {
        self.own(self.view(take.0)?, false).map(ExportId)
    }

    pub(super) fn fork_export(&mut self, export: ExportId) -> Result<TakeId, Error> {
        self.own(self.view(export.0)?, true).map(TakeId)
    }

    pub(super) fn retain_export(&mut self, export: ExportId) -> Result<ExportId, Error> {
        self.own(self.view(export.0)?, false).map(ExportId)
    }

    pub(super) fn import_snapshot(
        &mut self,
        store: &mut ResourceStore,
        snapshot: super::SnapshotId,
    ) -> Result<TakeId, Error> {
        self.check(store)?;
        let lease = super::get(&store.snapshots, snapshot.slot, snapshot.generation)?;
        let description = lease.snapshot.description;
        if lease.audio.is_some()
            || description.format
                != (super::Format::Audio {
                    channels: self.channels as u32,
                })
            || description.frames == 0
        {
            return Err(Error::InvalidShape);
        }
        let frames = usize::try_from(description.frames).map_err(|_| Error::Capacity)?;
        if frames
            > self
                .marks
                .len()
                .checked_mul(self.page_frames)
                .ok_or(Error::Capacity)?
        {
            return Err(Error::Capacity);
        }
        let credit = frames.div_ceil(self.page_frames);
        if self.free_views.is_empty() || credit > self.debt_limit - self.debt {
            return Err(Error::Capacity);
        }
        let base_slot = self
            .bases
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        let retained = store.retain_snapshot(snapshot)?;
        let index = store.read_index(retained.id)?;
        self.bases[base_slot] = Some(Base {
            snapshot: retained,
            index,
            references: 0,
        });
        self.own(
            View {
                root: None,
                mapping: Mapping {
                    first: 0,
                    modulus: self.marks.len() * self.page_frames,
                    prefix: None,
                },
                frames,
                growing: None,
                zero_from: None,
                revision: Revision(0),
                writable: true,
                reserved: false,
                retiring: false,
                base: Some(base_slot),
                credit,
            },
            true,
        )
        .map(TakeId)
    }

    pub fn take_index(&self, take: TakeId) -> Result<ReadIndex, Error> {
        self.view(take.0)?;
        Ok(ReadIndex(take.0))
    }

    pub fn export_index(&self, export: ExportId) -> Result<ReadIndex, Error> {
        if self.view(export.0)?.reserved {
            return Err(Error::InvalidState);
        }
        Ok(ReadIndex(export.0))
    }

    pub fn info(&self, index: ReadIndex) -> Result<Info, Error> {
        let view = self.view(index.0)?;
        Ok(Info {
            channels: self.channels as u32,
            frames: view.frames as u64,
            revision: view.revision,
        })
    }

    fn ring_frames(&self) -> usize {
        self.ring_pages * self.page_frames
    }

    fn find(&self, root: Option<usize>, page: usize) -> Option<LeaseId> {
        let mut root = root;
        for bit in (0..self.depth).rev() {
            let node = self.nodes[root?].expect("live index node");
            let Kind::Branch(children) = node.kind else {
                unreachable!("branch depth")
            };
            root = children[(page >> bit) & 1];
        }
        let Kind::Leaf(lease) = self.nodes[root?].expect("live leaf").kind else {
            unreachable!("leaf depth")
        };
        Some(lease)
    }

    fn allocate(&mut self, kind: Kind) -> usize {
        let index = self.free_nodes.pop().expect("preflighted index credit");
        self.work.index_allocated += 1;
        self.nodes[index] = Some(Node {
            references: 1,
            kind,
        });
        index
    }

    fn clip(
        &mut self,
        root: Option<usize>,
        low: usize,
        high: usize,
        ranges: &[Range<usize>; 2],
    ) -> Option<usize> {
        let root = root?;
        if ranges
            .iter()
            .all(|range| range.end <= low || range.start >= high)
        {
            return None;
        }
        if ranges
            .iter()
            .any(|range| range.start <= low && range.end >= high)
        {
            self.retain_root(Some(root))
                .expect("prepared reference bound");
            return Some(root);
        }
        let Kind::Branch(children) = self.nodes[root].expect("history branch").kind else {
            unreachable!("partial page-index interval")
        };
        let middle = low + (high - low) / 2;
        let left = self.clip(children[0], low, middle, ranges);
        let right = self.clip(children[1], middle, high, ranges);
        Some(self.allocate(Kind::Branch([left, right])))
    }

    fn set(&mut self, root: Option<usize>, depth: usize, page: usize, lease: LeaseId) -> usize {
        if depth == 0 {
            return self.allocate(Kind::Leaf(lease));
        }
        let mut children = match root.map(|n| self.nodes[n].expect("live index node").kind) {
            Some(Kind::Branch(children)) => children,
            None => [None; 2],
            _ => unreachable!("branch depth"),
        };
        let branch = (page >> (depth - 1)) & 1;
        let child = self.set(children[branch], depth - 1, page, lease);
        self.retain_root(children[1 - branch])
            .expect("prepared reference bound");
        children[branch] = Some(child);
        self.allocate(Kind::Branch(children))
    }

    fn unreference(&mut self, root: Option<usize>) -> Option<usize> {
        root.filter(|&index| {
            let node = self.nodes[index].as_mut().expect("live index owner");
            node.references -= 1;
            node.references == 0
        })
    }

    fn release_immediate(&mut self, store: &mut ResourceStore, root: Option<usize>) {
        if let Some(root) = self.unreference(root) {
            self.immediate.push(root);
        }
        while let Some(index) = self.immediate.pop() {
            let node = self.nodes[index].take().expect("retired index node");
            match node.kind {
                Kind::Branch(children) => {
                    store
                        .charge_work(WorkClass::Render, 0, 1)
                        .expect("required index work");
                    for child in children {
                        if let Some(child) = self.unreference(child) {
                            self.immediate.push(child);
                        }
                    }
                }
                Kind::Leaf(lease) => {
                    if self.pages.reclaim_render(store, lease).expect("owned page") {
                        self.reserved += 1;
                        if !self.pages.chunked() {
                            store.reserved_pages += 1;
                        }
                    }
                }
            }
            self.free_nodes.push(index);
            self.work.index_reclaimed += 1;
        }
    }

    fn stage(
        &mut self,
        store: &mut ResourceStore,
        root: Option<usize>,
        mapping: Mapping,
        frames: usize,
        position: usize,
        samples: &[f32],
        wrapping: bool,
        base: Option<usize>,
        selected_channel: Option<usize>,
    ) -> Result<Option<usize>, Error> {
        if self.pages.chunked()
            && selected_channel.is_none()
            && samples.len() == self.page_frames * self.channels
            && (!wrapping || frames % self.page_frames == 0)
            && mapping.physical(position) % self.page_frames == 0
        {
            return self.stage_chunk(
                store,
                root,
                mapping.physical(position) / self.page_frames,
                samples,
                true,
            );
        }
        self.stage_map(store, root, samples, base, selected_channel, |frame, _| {
            let local = if wrapping {
                (position + frame) % frames
            } else {
                position + frame
            };
            Some(mapping.physical(local))
        })
    }

    fn stage_chunk(
        &mut self,
        store: &mut ResourceStore,
        root: Option<usize>,
        page: usize,
        samples: &[f32],
        reuse: bool,
    ) -> Result<Option<usize>, Error> {
        if page >= self.marks.len()
            || self.reserved == 0
            || self.pages.available_leases() == 0
            || self.free_nodes.len() < self.depth + 1
        {
            return Err(Error::Capacity);
        }
        let shared = self.last_payload.filter(|&lease| {
            reuse
                && self.pages.sample_span(lease).is_ok_and(|previous| {
                    previous
                        .iter()
                        .zip(samples)
                        .all(|(a, b)| a.to_bits() == b.to_bits())
                })
        });
        let lease = if let Some(shared) = shared {
            self.work.shared_payloads += 1;
            self.pages.retain(shared)?
        } else {
            self.reserved -= 1;
            self.pages.insert_samples_render(store, samples)?
        };
        let updated = self.set(root, self.depth, page, lease);
        self.last_payload = Some(lease);
        store.note_operation(WorkClass::Render);
        store.charge_work(WorkClass::Render, 0, 2 * (self.depth + 1))?;
        Ok(Some(updated))
    }
    fn stage_map(
        &mut self,
        store: &mut ResourceStore,
        root: Option<usize>,
        samples: &[f32],
        base: Option<usize>,
        selected_channel: Option<usize>,
        physical: impl Fn(usize, usize) -> Option<usize>,
    ) -> Result<Option<usize>, Error> {
        let stride = if selected_channel.is_some() {
            1
        } else {
            self.channels
        };
        let count = samples.len() / stride;
        self.generation = self.generation.checked_add(1).ok_or(Error::Exhausted)?;
        self.plans.clear();
        let mut writes = 0usize;
        for frame in 0..count {
            for channel in 0..self.channels {
                if selected_channel.is_some_and(|selected| selected != channel) {
                    continue;
                }
                let Some(physical) = physical(frame, channel) else {
                    continue;
                };
                let page = physical / self.page_frames;
                if self.marks[page].0 != self.generation {
                    if self.plans.len() == self.capacity.transaction_pages {
                        return Err(Error::Capacity);
                    }
                    self.marks[page] = (self.generation, self.plans.len());
                    self.plans.push(Plan { page });
                }
                writes += 1;
            }
        }
        let required_nodes = self.plans.len() * (self.depth + 1);
        if self.reserved < self.plans.len()
            || self.pages.available_leases() < self.plans.len()
            || self.free_nodes.len() < required_nodes
        {
            return Err(Error::Capacity);
        }
        store.note_operation(WorkClass::Render);
        for (plan, entry) in self.plans.iter().enumerate() {
            let lease = self.find(root, entry.page);
            let destination =
                &mut self.scratch[plan * self.page_bytes..(plan + 1) * self.page_bytes];
            if let Some(lease) = lease {
                self.pages.copy_span(store, lease, 0, destination)?;
            } else {
                destination.fill(0);
                if let Some(base) = base {
                    let base = self.bases[base].expect("owned base");
                    let offset = entry.page * self.page_frames * self.channels * 4;
                    let count = (self.page_frames * self.channels * 4)
                        .min(base.snapshot.description.bytes.saturating_sub(offset));
                    if count > 0 {
                        store.copy_linear_span(
                            base.index,
                            offset,
                            &mut destination[..count],
                            WorkClass::Render,
                        )?;
                    }
                }
            }
        }
        for frame in 0..count {
            for channel in 0..self.channels {
                if selected_channel.is_some_and(|selected| selected != channel) {
                    continue;
                }
                let Some(physical) = physical(frame, channel) else {
                    continue;
                };
                let plan = self.marks[physical / self.page_frames].1;
                let destination = plan * self.page_bytes
                    + physical % self.page_frames * self.channels * 4
                    + channel * 4;
                self.scratch[destination..destination + 4].copy_from_slice(
                    &samples[frame * stride
                        + if selected_channel.is_some() {
                            0
                        } else {
                            channel
                        }]
                    .to_le_bytes(),
                );
            }
        }
        store
            .charge_work(
                WorkClass::Render,
                self.plans.len() * self.page_bytes + writes * 4,
                2 * required_nodes,
            )
            .expect("required staged work");
        self.retain_root(root)?;
        let mut staged = root;
        for plan in 0..self.plans.len() {
            self.reserved -= 1;
            if !self.pages.chunked() {
                store.reserved_pages -= 1;
            }
            let lease = self
                .pages
                .insert_bytes_render(
                    store,
                    &self.scratch[plan * self.page_bytes..(plan + 1) * self.page_bytes],
                )
                .expect("preflighted page credit");
            let changed = self.set(staged, self.depth, self.plans[plan].page, lease);
            self.release_immediate(store, staged);
            staged = Some(changed);
        }
        Ok(staged)
    }

    pub fn append(
        &mut self,
        io: &mut RenderIo<'_>,
        frame: Frame,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.append_history(io, self.history_id(0)?, frame, samples)
    }

    pub fn append_history(
        &mut self,
        io: &mut RenderIo<'_>,
        id: HistoryId,
        frame: Frame,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.check(io.store)?;
        if self.keep == 0 {
            return Err(Error::InvalidState);
        }
        let history = self.history(id)?;
        if frame != history.next {
            return Err(Error::InvalidFrame);
        }
        if samples.len() % self.channels != 0 || samples.len() / self.channels > self.max_frames {
            return Err(Error::InvalidShape);
        }
        let frames = samples.len() / self.channels;
        if frames == 0 {
            return Ok(());
        }
        let next = frame.checked_add(frames)?;
        let root = if self.pages.chunked()
            && frames == self.page_frames
            && frame.0 % self.page_frames as u64 == 0
        {
            self.stage_chunk(
                io.store,
                history.root,
                (frame.0 as usize % self.ring_frames()) / self.page_frames,
                samples,
                false,
            )?
        } else {
            self.stage(
                io.store,
                history.root,
                Mapping {
                    first: (frame.0 % self.ring_frames() as u64) as usize,
                    modulus: self.ring_frames(),
                    prefix: None,
                },
                frames,
                0,
                samples,
                false,
                None,
                None,
            )?
        };
        self.release_immediate(io.store, history.root);
        self.histories[id.slot].root = root;
        self.histories[id.slot].next = next;
        Ok(())
    }

    pub(in crate::resource) fn stage_write(
        &mut self,
        io: &mut RenderIo<'_>,
        take: TakeId,
        position: Option<u64>,
        samples: &[f32],
    ) -> Result<Write, Error> {
        self.check(io.store)?;
        let mut view = self.view(take.0)?;
        if !view.writable {
            return Err(Error::InvalidState);
        }
        if samples.len() % self.channels != 0 || samples.len() / self.channels > self.max_frames {
            return Err(Error::InvalidShape);
        }
        let expected = view.revision;
        let (position, frames, wrapping) = match position {
            Some(position) => {
                if view.growing.is_some() {
                    return Err(Error::Busy);
                }
                if position >= view.frames as u64 {
                    return Err(Error::InvalidOffset);
                }
                (position as usize, view.frames, true)
            }
            None => {
                let maximum = view.growing.ok_or(Error::InvalidState)?;
                let frames = view
                    .frames
                    .checked_add(samples.len() / self.channels)
                    .ok_or(Error::Capacity)?;
                if frames > maximum {
                    return Err(Error::Capacity);
                }
                (view.frames, frames, false)
            }
        };
        if !samples.is_empty() {
            view.revision = view.revision.checked_next()?;
        }
        view.root = self.stage(
            io.store,
            view.root,
            view.mapping,
            frames,
            position,
            samples,
            wrapping,
            view.base,
            None,
        )?;
        view.frames = frames;
        Ok(Write {
            take,
            expected,
            view,
        })
    }
    pub(in crate::resource) fn stage_window_write(
        &mut self,
        io: &mut RenderIo<'_>,
        take: TakeId,
        samples: &[f32],
        spans: &[ChannelWrite],
    ) -> Result<Write, Error> {
        self.check(io.store)?;
        let mut view = self.view(take.0)?;
        if !view.writable || view.growing.is_some() {
            return Err(Error::Busy);
        }
        if samples.len() % self.channels != 0
            || samples.len() / self.channels > self.max_frames
            || spans.len() > self.channels
        {
            return Err(Error::InvalidShape);
        }
        for (index, span) in spans.iter().enumerate() {
            if span.channel >= self.channels
                || spans[..index].iter().any(|old| old.channel == span.channel)
            {
                return Err(Error::InvalidShape);
            }
            if span
                .position
                .checked_add(span.frames as u64)
                .is_none_or(|end| end > view.frames as u64)
                || span
                    .offset
                    .checked_add(span.frames)
                    .is_none_or(|end| end > samples.len() / self.channels)
            {
                return Err(Error::InvalidFrame);
            }
        }
        let expected = view.revision;
        if spans.iter().any(|span| span.frames > 0) {
            view.revision = view.revision.checked_next()?;
        }
        let mapping = view.mapping;
        view.root = self.stage_map(
            io.store,
            view.root,
            samples,
            view.base,
            None,
            |frame, channel| {
                let span = spans.iter().find(|span| span.channel == channel)?;
                if frame < span.offset || frame >= span.offset + span.frames {
                    return None;
                }
                Some(mapping.physical(span.position as usize + frame - span.offset))
            },
        )?;
        Ok(Write {
            take,
            expected,
            view,
        })
    }
    pub(in crate::resource) fn cancel_write(
        &mut self,
        io: &mut RenderIo<'_>,
        write: Write,
    ) -> Result<(), Error> {
        self.check(io.store)?;
        self.release_immediate(io.store, write.view.root);
        Ok(())
    }
    pub(in crate::resource) fn commit_write(
        &mut self,
        io: &mut RenderIo<'_>,
        write: Write,
    ) -> Result<Info, Error> {
        self.check(io.store)?;
        let current = self.view(write.take.0);
        if !current
            .as_ref()
            .is_ok_and(|view| view.revision == write.expected && view.growing == write.view.growing)
        {
            self.cancel_write(io, write)?;
            return Err(Error::StaleRevision);
        }
        self.release_immediate(io.store, current.unwrap().root);
        self.views[write.take.0.slot as usize].value = Some(write.view);
        self.info(self.take_index(write.take)?)
    }

    /// One atomic edit, including all channels and any loop/page boundary crossings.
    pub fn replace(
        &mut self,
        io: &mut RenderIo<'_>,
        take: TakeId,
        position: u64,
        samples: &[f32],
    ) -> Result<Revision, Error> {
        self.check(io.store)?;
        let mut view = self.view(take.0)?;
        if !view.writable {
            return Err(Error::InvalidState);
        }
        if view.growing.is_some() {
            return Err(Error::Busy);
        }
        if position >= view.frames as u64 {
            return Err(Error::InvalidOffset);
        }
        if samples.len() % self.channels != 0 || samples.len() / self.channels > self.max_frames {
            return Err(Error::InvalidShape);
        }
        if samples.is_empty() {
            return Ok(view.revision);
        }
        let revision = view.revision.checked_next()?;
        let root = self.stage(
            io.store,
            view.root,
            view.mapping,
            view.frames,
            position as usize,
            samples,
            true,
            view.base,
            None,
        )?;
        self.release_immediate(io.store, view.root);
        view.root = root;
        view.revision = revision;
        self.views[take.0.slot as usize].value = Some(view);
        Ok(revision)
    }

    pub fn replace_channel(
        &mut self,
        io: &mut RenderIo<'_>,
        take: TakeId,
        channel: usize,
        position: u64,
        samples: &[f32],
    ) -> Result<Revision, Error> {
        self.check(io.store)?;
        let mut view = self.view(take.0)?;
        if !view.writable {
            return Err(Error::InvalidState);
        }
        if channel >= self.channels || samples.len() > self.max_frames {
            return Err(Error::InvalidShape);
        }
        if position
            .checked_add(samples.len() as u64)
            .is_none_or(|end| end > view.frames as u64)
        {
            return Err(Error::InvalidOffset);
        }
        if samples.is_empty() {
            return Ok(view.revision);
        }
        let revision = view.revision.checked_next()?;
        let root = self.stage(
            io.store,
            view.root,
            view.mapping,
            view.frames,
            position as usize,
            samples,
            false,
            view.base,
            Some(channel),
        )?;
        self.release_immediate(io.store, view.root);
        view.root = root;
        view.revision = revision;
        self.views[take.0.slot as usize].value = Some(view);
        Ok(revision)
    }

    pub fn empty_take(&mut self) -> Result<TakeId, Error> {
        self.own(
            View {
                root: None,
                mapping: Mapping {
                    first: 0,
                    modulus: self.marks.len() * self.page_frames,
                    prefix: None,
                },
                frames: 0,
                growing: None,
                zero_from: None,
                revision: Revision(0),
                writable: true,
                reserved: false,
                retiring: false,
                base: None,
                credit: 0,
            },
            true,
        )
        .map(TakeId)
    }

    /// Largest growth that also reserves a complete final export. Owners share
    /// the remaining page credits; this query does not acquire any of them.
    pub fn growth_limit(&self, take: TakeId, owners: usize) -> Result<u64, Error> {
        let view = self.view(take.0)?;
        if owners == 0 || !view.writable || view.growing.is_some() {
            return Err(Error::InvalidState);
        }
        let limit = self.marks.len() * self.page_frames;
        let available = self.debt_limit - self.debt;
        let prefix = view.mapping.credits(view.frames, self.page_frames);
        let credits = available.saturating_sub(prefix.checked_mul(2).ok_or(Error::Capacity)?)
            / owners.checked_mul(2).ok_or(Error::Capacity)?;
        let mut low = 0;
        let mut high = match view.mapping.prefix {
            Some(prefix) => prefix
                .frames
                .checked_add(limit.checked_sub(prefix.tail).ok_or(Error::Capacity)?)
                .ok_or(Error::Capacity)?
                .min(limit),
            None => limit
                .checked_sub(view.mapping.first)
                .ok_or(Error::Capacity)?,
        };
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if view.mapping.credits(middle, self.page_frames) <= credits {
                low = middle
            } else {
                high = middle - 1
            }
        }
        if low < view.frames {
            return Err(Error::Capacity);
        }
        Ok(low as u64)
    }

    /// Reserve the full growing take before consuming live input. The retained
    /// prefix keeps its physical pages; the tail uses a disjoint linear range.
    pub fn reserve_growth(&mut self, take: TakeId, maximum_frames: u64) -> Result<(), Error> {
        let mut view = self.view(take.0)?;
        if !view.writable || view.growing.is_some() {
            return Err(Error::Busy);
        }
        let maximum = usize::try_from(maximum_frames).map_err(|_| Error::Capacity)?;
        if maximum < view.frames {
            return Err(Error::InvalidShape);
        }
        let limit = self.marks.len() * self.page_frames;
        if maximum > limit {
            return Err(Error::Capacity);
        }
        if maximum > 0
            && (view.mapping.physical(maximum - 1) >= limit
                || view.mapping.prefix.is_none() && maximum > limit - view.mapping.first)
        {
            return Err(Error::Capacity);
        }
        let credit = view.mapping.credits(maximum, self.page_frames);
        if credit - view.credit > self.debt_limit - self.debt {
            return Err(Error::Capacity);
        }
        self.debt += credit - view.credit;
        view.credit = credit;
        view.growing = Some(maximum);
        self.views[take.0.slot as usize].value = Some(view);
        Ok(())
    }

    pub fn finish_growth(&mut self, take: TakeId) -> Result<(), Error> {
        let mut view = self.view(take.0)?;
        if view.growing.is_none() {
            return Err(Error::InvalidState);
        }
        let credit = view.mapping.credits(view.frames, self.page_frames);
        self.debt -= view.credit - credit;
        view.credit = credit;
        view.growing = None;
        self.views[take.0.slot as usize].value = Some(view);
        Ok(())
    }

    pub fn append_take(
        &mut self,
        io: &mut RenderIo<'_>,
        take: TakeId,
        samples: &[f32],
    ) -> Result<Revision, Error> {
        self.check(io.store)?;
        let mut view = self.view(take.0)?;
        let maximum = view.growing.ok_or(Error::InvalidState)?;
        if samples.len() % self.channels != 0 || samples.len() / self.channels > self.max_frames {
            return Err(Error::InvalidShape);
        }
        let frames = view
            .frames
            .checked_add(samples.len() / self.channels)
            .ok_or(Error::Capacity)?;
        if frames > maximum {
            return Err(Error::Capacity);
        }
        if samples.is_empty() {
            return Ok(view.revision);
        }
        let revision = view.revision.checked_next()?;
        let root = self.stage(
            io.store,
            view.root,
            view.mapping,
            frames,
            view.frames,
            samples,
            false,
            view.base,
            None,
        )?;
        self.release_immediate(io.store, view.root);
        view.root = root;
        view.frames = frames;
        view.revision = revision;
        self.views[take.0.slot as usize].value = Some(view);
        Ok(revision)
    }

    pub fn read_bytes(
        &self,
        io: &mut RenderIo<'_>,
        index: ReadIndex,
        position: u64,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        if io.store.scope != self.store_scope {
            return Err(Error::InvalidId);
        }
        let view = self.view(index.0)?;
        let frame_bytes = self.channels * 4;
        if output.len() % frame_bytes != 0 {
            return Err(Error::InvalidShape);
        }
        let frames = output.len() / frame_bytes;
        if frames > self.max_frames {
            return Err(Error::InvalidShape);
        }
        if position
            .checked_add(frames as u64)
            .is_none_or(|end| end > view.frames as u64)
        {
            return Err(Error::InvalidOffset);
        }
        self.read_span(
            io.store,
            index,
            position as usize * frame_bytes,
            output,
            WorkClass::Render,
        )?;
        Ok(frames)
    }

    pub(super) fn read_span(
        &self,
        store: &mut ResourceStore,
        index: ReadIndex,
        position: usize,
        output: &mut [u8],
        class: WorkClass,
    ) -> Result<(), Error> {
        self.check(store)?;
        let view = self.view(index.0)?;
        let frame_bytes = self.channels * 4;
        let length = view.frames * frame_bytes;
        if position
            .checked_add(output.len())
            .is_none_or(|end| end > length)
        {
            return Err(Error::InvalidOffset);
        }
        let base = view.base.map(|slot| self.bases[slot].expect("owned base"));
        let mut copied = 0;
        let mut spans = 0;
        while copied < output.len() {
            let byte = position + copied;
            let physical = view.mapping.physical(byte / frame_bytes);
            let start = physical % self.page_frames * frame_bytes + byte % frame_bytes;
            let count = (output.len() - copied)
                .min(
                    view.zero_from
                        .filter(|&frame| byte < frame * frame_bytes)
                        .map_or(usize::MAX, |frame| frame * frame_bytes - byte),
                )
                .min(self.page_frames * frame_bytes - start)
                .min(
                    view.mapping
                        .contiguous(byte / frame_bytes)
                        .saturating_mul(frame_bytes)
                        .saturating_sub(byte % frame_bytes),
                );
            if let Some(lease) = self.find(view.root, physical / self.page_frames) {
                if self.pages.chunked() {
                    self.pages.sample_span(lease)?;
                } else {
                    self.pages.bytes(store, lease)?;
                }
            } else if !view
                .zero_from
                .is_some_and(|frame| byte >= frame * frame_bytes)
            {
                if base.is_none_or(|base| byte + count > base.snapshot.description.bytes) {
                    return Err(Error::Incomplete);
                }
            }
            copied += count;
            spans += 1;
        }
        store.note_operation(class);
        let index_steps = 2 * spans * (self.depth + 1);
        store.charge_work(
            class,
            output.len(),
            if class == WorkClass::Render {
                index_steps
            } else {
                0
            },
        )?;
        for _ in 0..index_steps {
            store.index_step(class);
        }
        let mut copied = 0;
        while copied < output.len() {
            let byte = position + copied;
            let physical = view.mapping.physical(byte / frame_bytes);
            let start = physical % self.page_frames * frame_bytes + byte % frame_bytes;
            let count = (output.len() - copied)
                .min(
                    view.zero_from
                        .filter(|&frame| byte < frame * frame_bytes)
                        .map_or(usize::MAX, |frame| frame * frame_bytes - byte),
                )
                .min(self.page_frames * frame_bytes - start)
                .min(
                    view.mapping
                        .contiguous(byte / frame_bytes)
                        .saturating_mul(frame_bytes)
                        .saturating_sub(byte % frame_bytes),
                );
            let destination = &mut output[copied..copied + count];
            if let Some(lease) = self.find(view.root, physical / self.page_frames) {
                self.pages
                    .copy_span(store, lease, start, destination)
                    .expect("validated source");
            } else if view
                .zero_from
                .is_some_and(|frame| byte >= frame * frame_bytes)
            {
                destination.fill(0);
            } else {
                store.copy_linear_span(
                    base.expect("validated base").index,
                    byte,
                    destination,
                    class,
                )?;
            }
            copied += count;
        }
        Ok(())
    }

    fn queue(&mut self, root: Option<usize>, owner: usize) {
        if let Some(root) = self.unreference(root) {
            self.retired.push((root, owner));
            self.pending[owner] += 1;
        }
    }

    fn release_view(&mut self, handle: Handle) -> Result<(), Error> {
        let mut view = self.view(handle)?;
        if let Some(base) = view.base {
            let entry = self.bases[base].as_mut().expect("owned base");
            entry.references -= 1;
            if entry.references == 0 {
                self.retired_bases.push(base);
            }
        }
        view.retiring = true;
        self.views[handle.slot as usize].value = Some(view);
        self.queue(view.root, handle.slot as usize);
        self.finish_owner(handle.slot as usize);
        Ok(())
    }

    fn finish_owner(&mut self, owner: usize) {
        if owner < self.views.len() && self.pending[owner] == 0 {
            self.debt -= self.views[owner].value.expect("retired owner").credit;
            release(&mut self.views, &mut self.free_views, owner);
        }
    }

    pub fn release_take(&mut self, take: TakeId) -> Result<(), Error> {
        self.release_view(take.0)
    }
    pub fn release_export(&mut self, export: ExportId) -> Result<(), Error> {
        self.release_view(export.0)
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
        while store.steps_remaining() > 0 && steps < limit {
            if !self.retired_bases.is_empty() && (self.base_turn || self.retired.is_empty()) {
                let base = self.retired_bases.pop().expect("pending base");
                store.charge_external(0, 1)?;
                let snapshot = self.bases[base].take().expect("retired base").snapshot;
                store.release_snapshot(snapshot.id)?;
                self.base_turn = false;
                steps += 1;
                continue;
            }
            let Some(&(index, owner)) = self.retired.last() else {
                break;
            };
            let node = self.nodes[index].expect("retired index node");
            self.base_turn = true;
            match node.kind {
                Kind::Leaf(lease) => {
                    if self.pages.reclaim_lease(store, lease)? {
                        self.reserved += 1;
                        if !self.pages.chunked() {
                            store.reserved_pages += 1;
                        }
                    }
                }
                Kind::Branch(children) => {
                    store.charge_external(0, 1)?;
                    // Remove the parent before enqueuing its children.
                    self.retired.pop();
                    for child in children {
                        self.queue(child, owner);
                    }
                    self.pending[owner] -= 1;
                    self.nodes[index] = None;
                    self.free_nodes.push(index);
                    self.work.index_reclaimed += 1;
                    self.finish_owner(owner);
                    steps += 1;
                    continue;
                }
            }
            self.retired.pop();
            self.pending[owner] -= 1;
            self.nodes[index] = None;
            self.free_nodes.push(index);
            self.work.index_reclaimed += 1;
            self.finish_owner(owner);
            steps += 1;
        }
        if self.closed && self.retired.is_empty() && self.retired_bases.is_empty() {
            if !self.pages.chunked() {
                store.reserved_pages -= self.reserved;
            }
            self.reserved = 0;
        }
        Ok(steps)
    }

    pub fn begin_cleanup(&mut self) -> Result<(), Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        if self.debt != 0 {
            return Err(Error::Busy);
        }
        self.closed = true;
        for slot in 0..self.histories.len() {
            let root = self.histories[slot].root.take();
            self.queue(root, self.views.len() + slot);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
