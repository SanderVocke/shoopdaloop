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

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub channels: u32,
    pub history_frames: usize,
    pub views: usize,
    /// Includes live history, retained versions and transaction workspace.
    pub pages: usize,
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
pub struct Info {
    pub channels: u32,
    pub frames: u64,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capacity {
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
struct View {
    root: Option<usize>,
    first: usize,
    frames: usize,
    revision: Revision,
    writable: bool,
    retiring: bool,
    base: Option<usize>,
    modulus: usize,
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

pub struct AudioStore {
    scope: u32,
    store_scope: u32,
    channels: usize,
    keep: usize,
    origin: Frame,
    next: Frame,
    history: Option<usize>,
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
    pub fn prepare(
        store: &mut ResourceStore,
        config: Config,
        origin: Frame,
    ) -> Result<Self, Error> {
        let channels = config.channels as usize;
        let page_bytes = store.config.limits().page_bytes;
        if channels == 0
            || config.history_frames == 0
            || config.views == 0
            || page_bytes / 4 < channels
        {
            return Err(Error::InvalidConfig);
        }
        let page_frames = page_bytes / 4 / channels;
        store
            .config
            .limits()
            .max_frames
            .checked_mul(channels)
            .and_then(|n| n.checked_mul(4))
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let ring_pages = config
            .history_frames
            .div_ceil(page_frames)
            .checked_add(1)
            .and_then(usize::checked_next_power_of_two)
            .ok_or(Error::Capacity)?;
        let max_frames = store.config.limits().max_frames;
        let transaction_pages = ring_pages.min(
            max_frames
                .div_ceil(page_frames)
                .checked_add(2)
                .ok_or(Error::Capacity)?,
        );
        let baseline = ring_pages
            .checked_add(transaction_pages)
            .ok_or(Error::Capacity)?;
        if config.pages < baseline || config.pages > store.free_pages.len() - store.reserved_pages {
            return Err(Error::Capacity);
        }
        let index_pages = (store.config.resource_bytes() / (channels * 4))
            .div_ceil(page_frames)
            .max(ring_pages)
            .checked_next_power_of_two()
            .ok_or(Error::Capacity)?;
        let depth = index_pages.trailing_zeros() as usize;
        let node_limit = config
            .views
            .checked_add(2)
            .and_then(|n| n.checked_mul(ring_pages.checked_mul(2)?.checked_sub(1)?))
            .and_then(|n| n.checked_add(transaction_pages.checked_mul(depth + 1)?))
            .ok_or(Error::Capacity)?;
        let scratch_bytes = transaction_pages
            .checked_mul(page_bytes)
            .ok_or(Error::Capacity)?;
        let lease_limit = config
            .pages
            .checked_add(transaction_pages)
            .ok_or(Error::Capacity)?;
        let metadata_bytes = node_limit
            .checked_mul(size_of::<Option<Node>>() + size_of::<usize>() * 4)
            .and_then(|n| {
                n.checked_add(
                    config
                        .views
                        .checked_mul(size_of::<Slot<View>>() + size_of::<usize>() * 2)?,
                )
            })
            .and_then(|n| n.checked_add(index_pages.checked_mul(size_of::<(u64, usize)>())?))
            .and_then(|n| {
                n.checked_add(
                    config
                        .views
                        .checked_mul(size_of::<Option<Base>>() + size_of::<usize>())?,
                )
            })
            .and_then(|n| n.checked_add(scratch_bytes))
            .and_then(|n| n.checked_add(transaction_pages.checked_mul(size_of::<Plan>())?))
            .and_then(|n| n.checked_add(size_of::<usize>()))
            .and_then(|n| n.checked_add(store.pages.len().checked_mul(size_of::<usize>())?))
            .and_then(|n| {
                n.checked_add(
                    lease_limit.checked_mul(size_of::<Slot<usize>>() + size_of::<usize>())?,
                )
            })
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        ring_pages
            .checked_mul(page_frames)
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let mut scratch = vec![0; scratch_bytes].into_boxed_slice();
        let mut nodes = vec![None; node_limit];
        let mut marks = vec![(0, 0); index_pages].into_boxed_slice();
        let mut views = slots(config.views);
        let mut plans = vec![Plan::default(); transaction_pages];
        let mut retired = vec![(0, 0); node_limit];
        let mut pending = vec![0; config.views + 1].into_boxed_slice();
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
        let pages = SharedPages::prepare(store, lease_limit)?;
        let scope = index::new_scope()?;
        store.reserved_pages += config.pages;
        let mut bases = vec![None; config.views];
        for base in &mut bases {
            *core::hint::black_box(base) = None;
        }
        Ok(Self {
            scope,
            store_scope: store.scope,
            channels,
            keep: config.history_frames,
            origin,
            next: origin,
            history: None,
            ring_pages,
            page_frames,
            page_bytes,
            max_frames,
            depth,
            capacity: Capacity {
                history_pages: ring_pages,
                transaction_pages,
                index_nodes: node_limit,
                metadata_bytes,
            },
            nodes,
            free_nodes: (0..node_limit).rev().collect(),
            views,
            free_views: (0..config.views).rev().collect(),
            debt: 0,
            debt_limit: config.pages - baseline,
            reserved: config.pages,
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
        if self.closed {
            return Err(Error::InvalidState);
        }
        Ok(Frame(
            self.next
                .0
                .saturating_sub(self.keep as u64)
                .max(self.origin.0),
        )..self.next)
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
        let slot = self.free_views.pop().expect("prepared view credit");
        view.writable = writable;
        self.views[slot].value = Some(view);
        self.debt += view.credit;
        Ok(Handle {
            scope: self.scope,
            slot: slot as u32,
            generation: self.views[slot].generation,
        })
    }

    pub fn grab(&mut self, range: Range<Frame>) -> Result<TakeId, Error> {
        let coverage = self.coverage()?;
        if range.start < coverage.start || range.end > coverage.end || range.start >= range.end {
            return Err(Error::InvalidFrame);
        }
        let first = (range.start.0 % self.ring_frames() as u64) as usize;
        self.own(
            View {
                root: self.history,
                first,
                frames: (range.end.0 - range.start.0) as usize,
                revision: Revision(0),
                writable: true,
                retiring: false,
                base: None,
                modulus: self.ring_frames(),
                credit: self.ring_pages,
            },
            true,
        )
        .map(TakeId)
    }

    pub fn fork(&mut self, take: TakeId) -> Result<TakeId, Error> {
        self.own(self.view(take.0)?, true).map(TakeId)
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
        let credit = self.ring_pages.max(frames.div_ceil(self.page_frames));
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
                first: 0,
                frames,
                revision: Revision(0),
                writable: true,
                retiring: false,
                base: Some(base_slot),
                modulus: self.marks.len() * self.page_frames,
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
        self.view(export.0)?;
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
        self.nodes[index] = Some(Node {
            references: 1,
            kind,
        });
        index
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
                        store.reserved_pages += 1;
                    }
                }
            }
            self.free_nodes.push(index);
        }
    }

    fn stage(
        &mut self,
        store: &mut ResourceStore,
        root: Option<usize>,
        first: usize,
        frames: usize,
        position: usize,
        samples: &[f32],
        wrapping: bool,
        modulus: usize,
        base: Option<usize>,
    ) -> Result<Option<usize>, Error> {
        let count = samples.len() / self.channels;
        self.generation = self.generation.checked_add(1).ok_or(Error::Exhausted)?;
        self.plans.clear();
        for frame in 0..count {
            let local = if wrapping {
                (position + frame) % frames
            } else {
                position + frame
            };
            let physical = (first + local) % modulus;
            let page = physical / self.page_frames;
            if self.marks[page].0 != self.generation {
                if self.plans.len() == self.capacity.transaction_pages {
                    return Err(Error::Capacity);
                }
                self.marks[page] = (self.generation, self.plans.len());
                self.plans.push(Plan { page });
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
                destination.copy_from_slice(self.pages.bytes(store, lease)?);
            } else {
                destination.fill(0);
                if let Some(base) = base {
                    let base = self.bases[base].expect("owned base");
                    let offset = entry.page * self.page_frames * self.channels * 4;
                    let count = (self.page_frames * self.channels * 4)
                        .min(base.snapshot.description.bytes.saturating_sub(offset));
                    store.copy_linear_span(
                        base.index,
                        offset,
                        &mut destination[..count],
                        WorkClass::Render,
                    )?;
                }
            }
        }
        for frame in 0..count {
            let local = if wrapping {
                (position + frame) % frames
            } else {
                position + frame
            };
            let physical = (first + local) % modulus;
            let plan = self.marks[physical / self.page_frames].1;
            let destination =
                plan * self.page_bytes + physical % self.page_frames * self.channels * 4;
            for channel in 0..self.channels {
                self.scratch[destination + channel * 4..destination + channel * 4 + 4]
                    .copy_from_slice(&samples[frame * self.channels + channel].to_le_bytes());
            }
        }
        store
            .charge_work(
                WorkClass::Render,
                self.plans.len() * self.page_bytes + samples.len() * 4,
                2 * required_nodes,
            )
            .expect("required staged work");
        self.retain_root(root)?;
        let mut staged = root;
        for plan in 0..self.plans.len() {
            self.reserved -= 1;
            store.reserved_pages -= 1;
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
        self.check(io.store)?;
        if frame != self.next {
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
        let root = self.stage(
            io.store,
            self.history,
            (frame.0 % self.ring_frames() as u64) as usize,
            frames,
            0,
            samples,
            false,
            self.ring_frames(),
            None,
        )?;
        let old = core::mem::replace(&mut self.history, root);
        self.release_immediate(io.store, old);
        self.next = next;
        Ok(())
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
            view.first,
            view.frames,
            position as usize,
            samples,
            true,
            view.modulus,
            view.base,
        )?;
        self.release_immediate(io.store, view.root);
        view.root = root;
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
            let physical = (view.first + byte / frame_bytes) % view.modulus;
            let start = physical % self.page_frames * frame_bytes + byte % frame_bytes;
            let count = (output.len() - copied)
                .min(self.page_frames * frame_bytes - start)
                .min((view.modulus - physical) * frame_bytes - byte % frame_bytes);
            if let Some(lease) = self.find(view.root, physical / self.page_frames) {
                self.pages.bytes(store, lease)?;
            } else if base.is_none() {
                return Err(Error::Incomplete);
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
            let physical = (view.first + byte / frame_bytes) % view.modulus;
            let start = physical % self.page_frames * frame_bytes + byte % frame_bytes;
            let count = (output.len() - copied)
                .min(self.page_frames * frame_bytes - start)
                .min((view.modulus - physical) * frame_bytes - byte % frame_bytes);
            let destination = &mut output[copied..copied + count];
            if let Some(lease) = self.find(view.root, physical / self.page_frames) {
                let bytes = self.pages.bytes(store, lease).expect("validated source");
                destination.copy_from_slice(&bytes[start..start + count]);
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
                        store.reserved_pages += 1;
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
                    self.finish_owner(owner);
                    steps += 1;
                    continue;
                }
            }
            self.retired.pop();
            self.pending[owner] -= 1;
            self.nodes[index] = None;
            self.free_nodes.push(index);
            self.finish_owner(owner);
            steps += 1;
        }
        if self.closed && self.retired.is_empty() && self.retired_bases.is_empty() {
            store.reserved_pages -= self.reserved;
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
        let root = self.history.take();
        self.queue(root, self.views.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests;
