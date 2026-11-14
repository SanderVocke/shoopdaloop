//! Continuous MIDI media exposed through ordinary resource and snapshot leases.

use super::{midi_writer as writer, *};
use crate::Sample;
use crate::{buffer::MidiBuffer, midi_state::MidiState};
use core::mem::size_of_val;

/// Fixed pools; each record page holds 32 inline 16-byte records.
#[derive(Clone, Copy, Debug)]
pub struct BlockLimits {
    pub event_credits: usize,
    pub record_pages: usize,
    pub directories: usize,
    pub versions: usize,
    pub media: usize,
    pub import_bases: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub blocks: BlockLimits,
    pub sources: usize,
    pub history_frames: u64,
    pub history_events: usize,
    /// Serialized history bytes per source, including record headers.
    pub history_bytes: usize,
    pub views: usize,
    pub writers: usize,
    pub imports: usize,
    pub events_per_tick: usize,
    pub bytes_per_tick: usize,
    /// Required cleanup per source/writer tick and idle callback progress.
    pub maintenance_steps_per_tick: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoreId {
    scope: u32,
    slot: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Key {
    slot: usize,
    generation: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceId {
    store: StoreId,
    key: Key,
}
impl SourceId {
    pub fn store(self) -> StoreId {
        self.store
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriterId {
    store: StoreId,
    key: Key,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportId {
    store: StoreId,
    key: Key,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrabId {
    store: StoreId,
    key: Key,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatchId {
    store: StoreId,
    key: Key,
}
struct PatchJob {
    edit: writer::Patch,
    resource: ResourceId,
    expected: Revision,
    version: UploadId,
    original: Key,
    ready: bool,
}
mod patch;
/// One nonwrapping raw-media span within a complete source tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WriteSpan {
    pub position: u64,
    pub offset: usize,
    pub frames: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteKind {
    Fixed,
    Growing { maximum_frames: u64 },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Building,
    Ready,
}
#[derive(Clone, Copy)]
pub(super) struct Take {
    pub store: StoreId,
    pub(super) media: Key,
}
struct Entry<T> {
    generation: u64,
    value: Option<T>,
}
pub(super) struct Arena<T> {
    entries: Box<[Entry<T>]>,
}
impl<T> Arena<T> {
    fn prepare(count: usize) -> Result<Self, Error> {
        count
            .checked_mul(size_of::<Entry<T>>())
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        Ok(Self {
            entries: (0..count)
                .map(|_| Entry {
                    generation: 1,
                    value: None,
                })
                .collect(),
        })
    }
    fn available(&self) -> Result<(), Error> {
        if self
            .entries
            .iter()
            .any(|e| e.value.is_none() && e.generation != u64::MAX)
        {
            Ok(())
        } else {
            Err(Error::Capacity)
        }
    }
    fn insert(&mut self, value: T) -> Key {
        let slot = self
            .entries
            .iter()
            .position(|e| e.value.is_none() && e.generation != u64::MAX)
            .expect("preflighted owner credit");
        self.entries[slot].value = Some(value);
        Key {
            slot,
            generation: self.entries[slot].generation,
        }
    }
    pub(super) fn get(&self, key: Key) -> Result<&T, Error> {
        self.entries
            .get(key.slot)
            .filter(|e| e.generation == key.generation)
            .and_then(|e| e.value.as_ref())
            .ok_or(Error::InvalidId)
    }
    fn get_mut(&mut self, key: Key) -> Result<&mut T, Error> {
        self.entries
            .get_mut(key.slot)
            .filter(|e| e.generation == key.generation)
            .and_then(|e| e.value.as_mut())
            .ok_or(Error::InvalidId)
    }
    fn distinct(&mut self, source: Key, target: Key) -> Result<(&T, &mut T), Error> {
        self.get(source)?;
        self.get(target)?;
        if source.slot == target.slot {
            return Err(Error::InvalidId);
        }
        if source.slot < target.slot {
            let (left, right) = self.entries.split_at_mut(target.slot);
            Ok((
                left[source.slot].value.as_ref().unwrap(),
                right[0].value.as_mut().unwrap(),
            ))
        } else {
            let (left, right) = self.entries.split_at_mut(source.slot);
            Ok((
                right[0].value.as_ref().unwrap(),
                left[target.slot].value.as_mut().unwrap(),
            ))
        }
    }
    fn remove(&mut self, key: Key) -> Result<T, Error> {
        self.get(key)?;
        let entry = &mut self.entries[key.slot];
        entry.generation += 1;
        Ok(entry.value.take().expect("validated owner"))
    }
    fn empty(&self) -> bool {
        self.entries.iter().all(|e| e.value.is_none())
    }
}
struct Source {
    history: Option<writer::history::Source>,
    staged: writer::Staging,
    before: MidiState,
    before_dirty: bool,
    cut: Option<Frame>,
}
struct Job {
    resource: ResourceId,
    expected: Revision,
    original: Key,
    admission: Option<writer::Admission>,
    writer: Option<writer::Writer>,
    source: Option<SourceId>,
    next: Option<Frame>,
    complete: bool,
    prefix: Option<Key>,
}
struct Import {
    snapshot: SnapshotId,
    cursor: MidiCursor,
    base: writer::ImportBase,
    media: Option<writer::Media>,
    writer: Option<Key>,
    frames: u64,
    quantum: Option<u64>,
    density: usize,
    bytes: usize,
    order: u64,
    last_frame: Option<u64>,
    ready: bool,
}
pub(super) struct Store {
    pub(super) core: writer::Core,
    pub(super) views: Arena<writer::Media>,
    reserved_views: Box<[bool]>,
    sources: Arena<Source>,
    writers: Arena<Job>,
    imports: Arena<Import>,
    grabs: Arena<writer::history::Grab>,
    patches: Arena<PatchJob>,
    closed: bool,
    maintenance_steps: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capacity {
    /// Prepared arrays and scratch, excluding allocator and common arena overhead.
    pub metadata_bytes: usize,
    pub reserved_payload_bytes: usize,
    pub state_bytes: usize,
    pub history_frames: u64,
}
impl Config {
    fn core(self) -> writer::Config {
        writer::Config {
            blocks: self.blocks,
            events_per_tick: self.events_per_tick,
            bytes_per_tick: self.bytes_per_tick,
            writers: self.writers,
        }
    }
    /// Allocation-free sizing of protected capture and retained-owner pools.
    pub fn capacity(self, runtime: crate::PreparedConfig, q: Quantum) -> Result<Capacity, Error> {
        self.sources
            .checked_add(self.writers)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(self.maintenance_steps_per_tick))
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let tick_bytes = self
            .events_per_tick
            .checked_mul(12)
            .and_then(|n| n.checked_add(self.bytes_per_tick))
            .ok_or(Error::Capacity)?;
        if self.maintenance_steps_per_tick == 0
            || self.views == 0
            || self.events_per_tick == 0
            || self.bytes_per_tick == 0
            || self.events_per_tick > runtime.limits().midi_events_per_port
            || self.bytes_per_tick > runtime.limits().midi_bytes_per_port
            || q.frames() > runtime.limits().max_frames
            || self.sources > self.blocks.media
            || self.blocks.record_pages < self.blocks.event_credits
            || self.sources > 0
                && (self.history_frames == 0
                    || self.history_events < self.events_per_tick
                    || self.history_bytes < tick_bytes)
        {
            return Err(Error::InvalidConfig);
        }
        writer::Core::owner_limits(
            self.core(),
            self.sources,
            self.history_events,
            self.history_bytes,
            runtime.limits().page_bytes,
        )?;
        let mut bytes = size_of::<Store>()
            .checked_add(writer::Core::prepared_bytes(self.core())?)
            .and_then(|n| n.checked_add(writer::Core::owner_credit_bytes(self.blocks.media).ok()?))
            .ok_or(Error::Capacity)?;
        for (count, size) in [
            (
                self.views,
                size_of::<Entry<writer::Media>>() + size_of::<bool>(),
            ),
            (
                self.sources,
                size_of::<Entry<Source>>()
                    .checked_add(writer::Staging::prepared_bytes(self.events_per_tick)?)
                    .ok_or(Error::Capacity)?,
            ),
            (self.writers, size_of::<Entry<Job>>()),
            (self.imports, size_of::<Entry<Import>>()),
            (self.imports, size_of::<Entry<writer::history::Grab>>()),
            (self.writers.max(1), size_of::<Entry<PatchJob>>()),
        ] {
            bytes = count
                .checked_mul(size)
                .and_then(|more| bytes.checked_add(more))
                .filter(|&n| n <= i32::MAX as usize)
                .ok_or(Error::Capacity)?;
        }
        Ok(Capacity {
            metadata_bytes: bytes
                .checked_sub(
                    self.blocks
                        .record_pages
                        .checked_mul(
                            crate::midi_runtime::RECORDS_PER_PAGE
                                * crate::midi::EventRecord::ENCODED_BYTES,
                        )
                        .ok_or(Error::Capacity)?,
                )
                .ok_or(Error::Capacity)?,
            reserved_payload_bytes: self
                .blocks
                .record_pages
                .checked_mul(
                    crate::midi_runtime::RECORDS_PER_PAGE * crate::midi::EventRecord::ENCODED_BYTES,
                )
                .ok_or(Error::Capacity)?,
            state_bytes: self
                .writers
                .max(1)
                .checked_mul(6)
                .and_then(|n| n.checked_add(self.blocks.media.checked_mul(6)?))
                .and_then(|n| n.checked_add(self.views.checked_mul(2)?))
                .and_then(|n| n.checked_add(self.sources.checked_mul(4)?))
                .and_then(|n| n.checked_add(self.writers.checked_mul(4)?))
                .and_then(|n| n.checked_add(self.writers.max(1).checked_mul(2)?))
                .and_then(|n| n.checked_add(self.imports.checked_mul(6)?))
                .and_then(|n| n.checked_mul(size_of::<MidiState>()))
                .ok_or(Error::Capacity)?,
            history_frames: q.ceil(Frame(self.history_frames))?.0,
        })
    }
}
impl Store {
    pub(super) fn allocated_bytes(&self) -> usize {
        self.core.allocated_bytes()
            + size_of_val(&*self.views.entries)
            + size_of_val(&*self.reserved_views)
            + size_of_val(&*self.sources.entries)
            + size_of_val(&*self.writers.entries)
            + size_of_val(&*self.imports.entries)
            + size_of_val(&*self.grabs.entries)
            + size_of_val(&*self.patches.entries)
            + self
                .sources
                .entries
                .iter()
                .filter_map(|entry| entry.value.as_ref())
                .map(|source| source.staged.allocated_bytes())
                .sum::<usize>()
    }
    fn prepare<S: Sample>(resources: &mut ResourceStore<S>, config: Config) -> Result<Self, Error> {
        config.capacity(resources.config, resources.quantum)?;
        let mut views = Arena::prepare(config.views)?;
        let mut sources = Arena::prepare(config.sources)?;
        let writers = Arena::prepare(config.writers)?;
        let imports = Arena::prepare(config.imports)?;
        let grabs = Arena::prepare(config.imports)?;
        let patches = Arena::prepare(config.writers.max(1))?;
        let q = resources.quantum;
        let mut core = writer::Core::prepare(resources, q, config.core())?;
        core.prepare_owner_credits(config.sources, config.history_events, config.history_bytes)?;
        for _ in 0..config.sources {
            sources.insert(Source {
                history: Some(core.begin_source_with_bytes(
                    Frame(0),
                    config.history_frames,
                    config.history_events,
                    config.history_bytes,
                    &MidiState::default(),
                )?),
                staged: writer::Staging::prepare(config.events_per_tick)?,
                before: MidiState::default(),
                before_dirty: false,
                cut: None,
            });
        }
        // Touch the prepared view array before handing it to rendering.
        for entry in &mut views.entries {
            core::hint::black_box(entry);
        }
        let maintenance_steps = core.required_maintenance(config.maintenance_steps_per_tick);
        Ok(Self {
            core,
            views,
            reserved_views: vec![false; config.views].into_boxed_slice(),
            sources,
            writers,
            imports,
            grabs,
            patches,
            closed: false,
            maintenance_steps,
        })
    }
    pub(super) fn reserve_export(&mut self) -> Result<Key, Error> {
        self.views.available()?;
        let media = self.core.empty(Frame(0), 0, &MidiState::default())?;
        let key = self.views.insert(media);
        self.reserved_views[key.slot] = true;
        Ok(key)
    }
    pub(super) fn publish_export(&mut self, key: Key, source: Key) -> Result<(), Error> {
        self.views.get(key)?;
        self.views.get(source)?;
        if !self.reserved_views[key.slot] || self.reserved_views[source.slot] {
            return Err(Error::InvalidState);
        }
        let (source, target) = self.views.distinct(source, key)?;
        self.core.retarget_empty(target, source)?;
        self.reserved_views[key.slot] = false;
        Ok(())
    }
    pub(super) fn retain(&mut self, key: Key) -> Result<Key, Error> {
        self.views.available()?;
        let media = self.core.retain(self.views.get(key)?)?;
        Ok(self.views.insert(media))
    }
    pub(super) fn release(&mut self, key: Key) -> Result<(), Error> {
        let media = self.views.remove(key)?;
        self.reserved_views[key.slot] = false;
        self.core.release(media)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    pub views: usize,
    pub reserved_views: usize,
    pub writers: usize,
    pub imports: usize,
    pub index_leases: usize,
    pub available_records: usize,
    pub available_bodies: usize,
}
impl<S: Sample> ResourceStore<S> {
    pub fn indexed_midi_usage(&mut self, id: StoreId) -> Result<Usage, Error> {
        self.with_indexed_midi(id, |_, pool| {
            Ok(Usage {
                views: pool
                    .views
                    .entries
                    .iter()
                    .filter(|entry| entry.value.is_some())
                    .count(),
                reserved_views: pool
                    .reserved_views
                    .iter()
                    .filter(|&&reserved| reserved)
                    .count(),
                writers: pool
                    .writers
                    .entries
                    .iter()
                    .filter(|entry| entry.value.is_some())
                    .count(),
                imports: pool
                    .imports
                    .entries
                    .iter()
                    .filter(|entry| entry.value.is_some())
                    .count(),
                index_leases: pool.core.index_leases(),
                available_records: pool.core.available_records(),
                available_bodies: pool.core.available_bodies(),
            })
        })
    }
    pub fn prepare_indexed_midi_store(&mut self, config: Config) -> Result<StoreId, Error> {
        if self.indexed_midi_stores.len() == self.config.limits().resource_slots {
            return Err(Error::Capacity);
        }
        let pool = Store::prepare(self, config)?;
        let id = StoreId {
            scope: self.scope,
            slot: self.indexed_midi_stores.len(),
        };
        self.indexed_midi_stores.push(Some(Box::new(pool)));
        Ok(id)
    }
    pub(super) fn with_indexed_midi<T>(
        &mut self,
        id: StoreId,
        operation: impl FnOnce(&mut ResourceStore<S>, &mut Store) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let mut pool = self
            .indexed_midi_stores
            .get_mut(id.slot)
            .and_then(Option::take)
            .ok_or(Error::InvalidId)?;
        let result = operation(self, &mut pool);
        self.indexed_midi_stores[id.slot] = Some(pool);
        result
    }
    pub fn indexed_midi_source(&self, id: StoreId, slot: usize) -> Result<SourceId, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let pool = self
            .indexed_midi_stores
            .get(id.slot)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidId)?;
        if pool.closed {
            return Err(Error::InvalidState);
        }
        let entry = pool.sources.entries.get(slot).ok_or(Error::InvalidId)?;
        let key = Key {
            slot,
            generation: entry.generation,
        };
        pool.sources.get(key)?;
        Ok(SourceId { store: id, key })
    }
    /// Preparation-only adjustment for sources sharing a larger common pool.
    pub fn configure_indexed_midi_source(
        &mut self,
        source: SourceId,
        frames: u64,
    ) -> Result<(), Error> {
        self.with_indexed_midi(source.store, |_, pool| {
            pool.sources
                .get_mut(source.key)?
                .history
                .as_mut()
                .ok_or(Error::InvalidState)?
                .configure_horizon(frames)
        })
    }
    pub fn indexed_midi_coverage(
        &self,
        source: SourceId,
    ) -> Result<core::ops::Range<Frame>, Error> {
        if source.store.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let pool = self
            .indexed_midi_stores
            .get(source.store.slot)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidId)?;
        Ok(pool
            .sources
            .get(source.key)?
            .history
            .as_ref()
            .ok_or(Error::InvalidState)?
            .coverage())
    }
    fn indexed_midi_resource(&mut self, take: Take) -> Result<ResourceId, Error> {
        let description = self.with_indexed_midi(take.store, |resources, pool| {
            let media = pool.views.get(take.media)?;
            let totals = pool.core.totals(&mut resources.render_io(), media)?;
            Ok(Description {
                format: Format::Midi,
                frames: media.frames,
                bytes: totals.bytes,
            })
        })?;
        let resource = self.create()?;
        let slot = self.free_versions.pop().expect("preflighted version");
        let active = UploadId {
            slot: slot as u32,
            generation: self.versions[slot].generation,
        };
        let revision = Revision(1);
        self.versions[slot].value = Some(Version {
            resource,
            expected_revision: Revision(0),
            revision,
            description,
            written: 0,
            reserved: 0,
            head: None,
            tail: None,
            references: 1,
            parser: Parser::default(),
            state: State::Active,
            capture: None,
            index: VersionIndex::default(),
            midi: None,
            audio: None,
            shared_midi: None,
            indexed_midi: Some(take),
        });
        self.resources[resource.slot() as usize].value = Some(Resource {
            active: Some(active),
            revision,
            claim: None,
        });
        Ok(resource)
    }
    fn indexed_midi_credit(&self) -> Result<(), Error> {
        if self.free_resources.is_empty() || self.free_versions.is_empty() {
            Err(Error::Capacity)
        } else {
            Ok(())
        }
    }
    pub fn grab_indexed_midi(
        &mut self,
        source: SourceId,
        range: core::ops::Range<Frame>,
        frames: u64,
    ) -> Result<ResourceId, Error> {
        self.indexed_midi_credit()?;
        let media = self.with_indexed_midi(source.store, |resources, pool| {
            pool.views.available()?;
            let history = pool
                .sources
                .get(source.key)?
                .history
                .as_ref()
                .ok_or(Error::InvalidState)?;
            let media =
                pool.core
                    .grab_source(&mut resources.render_io(), history, range, frames)?;
            Ok(pool.views.insert(media))
        })?;
        self.indexed_midi_resource(Take {
            store: source.store,
            media,
        })
    }
    pub fn prepare_indexed_midi_grab(
        &mut self,
        source: SourceId,
        range: core::ops::Range<Frame>,
        frames: u64,
    ) -> Result<GrabId, Error> {
        self.with_indexed_midi(source.store, |resources, pool| {
            if pool.closed {
                return Err(Error::InvalidState);
            }
            pool.grabs.available()?;
            let history = pool
                .sources
                .get(source.key)?
                .history
                .as_ref()
                .ok_or(Error::InvalidState)?;
            let grab = pool
                .core
                .begin_grab(&mut resources.render_io(), history, range, frames)?;
            Ok(GrabId {
                store: source.store,
                key: pool.grabs.insert(grab),
            })
        })
    }
    pub fn progress_indexed_midi_grab(
        &mut self,
        id: GrabId,
        steps: usize,
    ) -> Result<Status, Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let ready = pool.core.progress_grab(
                &mut resources.render_io(),
                pool.grabs.get_mut(id.key)?,
                steps,
            )?;
            Ok(if ready {
                Status::Ready
            } else {
                Status::Building
            })
        })
    }
    pub fn indexed_midi_grab_replay_bound(&mut self, id: GrabId) -> Result<usize, Error> {
        self.with_indexed_midi(id.store, |_, pool| {
            pool.core.grab_replay_bound(pool.grabs.get(id.key)?)
        })
    }
    pub fn finish_indexed_midi_grab(&mut self, id: GrabId) -> Result<ResourceId, Error> {
        self.indexed_midi_credit()?;
        let media = self.with_indexed_midi(id.store, |_, pool| {
            pool.views.available()?;
            if !pool.core.grab_ready(pool.grabs.get(id.key)?) {
                return Err(Error::Busy);
            }
            let media = pool.core.finish_grab(pool.grabs.remove(id.key)?);
            Ok(pool.views.insert(media))
        })?;
        self.indexed_midi_resource(Take {
            store: id.store,
            media,
        })
    }
    pub fn cancel_indexed_midi_grab(&mut self, id: GrabId) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |_, pool| {
            pool.core.cancel_grab(pool.grabs.remove(id.key)?)
        })
    }
    pub fn fork_indexed_midi(
        &mut self,
        id: StoreId,
        snapshot: SnapshotId,
    ) -> Result<ResourceId, Error> {
        self.indexed_midi_credit()?;
        let source = get(&self.snapshots, snapshot.slot, snapshot.generation)?
            .indexed_midi
            .ok_or(Error::InvalidShape)?;
        if source.store != id {
            return Err(Error::InvalidShape);
        }
        let media = self.with_indexed_midi(id, |_, pool| pool.retain(source.media))?;
        self.indexed_midi_resource(Take { store: id, media })
    }
    pub fn prepare_indexed_midi_import(
        &mut self,
        id: StoreId,
        snapshot: SnapshotId,
        initial: &MidiState,
    ) -> Result<ImportId, Error> {
        self.with_indexed_midi(id, |resources, pool| {
            if pool.closed {
                return Err(Error::InvalidState);
            }
            pool.imports.available()?;
            let index = resources.read_index(snapshot)?;
            let lease = resources.midi_lease(index)?;
            if lease.indexed_midi.is_some() || lease.shared_midi.is_some() {
                return Err(Error::InvalidShape);
            }
            let retained = resources.retain_snapshot(snapshot)?;
            let base = match pool.core.pin_import(resources, retained.id) {
                Ok(base) => base,
                Err(error) => {
                    resources.release_snapshot(retained.id)?;
                    return Err(error);
                }
            };
            let media = match pool
                .core
                .empty(Frame(0), retained.description.frames, initial)
            {
                Ok(media) => media,
                Err(error) => {
                    pool.core.release_import(base)?;
                    resources.release_snapshot(retained.id)?;
                    return Err(error);
                }
            };
            let cursor = resources.midi_cursor_impl(resources.read_index(retained.id)?)?;
            Ok(ImportId {
                store: id,
                key: pool.imports.insert(Import {
                    snapshot: retained.id,
                    cursor,
                    base,
                    media: Some(media),
                    writer: None,
                    frames: retained.description.frames,
                    quantum: None,
                    density: 0,
                    bytes: 0,
                    order: 0,
                    last_frame: None,
                    ready: false,
                }),
            })
        })
    }
    pub fn progress_indexed_midi_import(
        &mut self,
        id: ImportId,
        events: usize,
    ) -> Result<Status, Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.imports.get_mut(id.key)?;
            for _ in 0..events {
                let mut next = job.cursor;
                let Some(event) = resources.midi_next_impl(&mut next)? else {
                    job.ready = true;
                    break;
                };
                let order = if job.last_frame == Some(event.frame()) {
                    job.order.checked_add(1).ok_or(Error::Exhausted)?
                } else {
                    0
                };
                if order > u16::MAX as u64 {
                    return Err(Error::Exhausted);
                }
                let quantum = event.frame() / resources.quantum.frames() as u64;
                let density = if job.quantum == Some(quantum) {
                    job.density + 1
                } else {
                    1
                };
                let bytes = if job.quantum == Some(quantum) {
                    job.bytes
                        .checked_add(event.byte_len())
                        .ok_or(Error::Capacity)?
                } else {
                    event.byte_len()
                };
                if density > pool.core.config.events_per_tick
                    || bytes > pool.core.config.bytes_per_tick
                {
                    return Err(Error::Capacity);
                }
                if let Some(writer) = job.writer {
                    let target = pool.writers.get_mut(writer)?;
                    if resources.revision(target.resource)? != target.expected {
                        return Err(Error::StaleRevision);
                    }
                    pool.core.import_writer_reference(
                        &mut resources.render_io(),
                        target.writer.as_mut().ok_or(Error::Busy)?,
                        job.frames,
                        job.base,
                        event.frame(),
                        order,
                        job.cursor.byte_position() + 12,
                        event.byte_len(),
                    )?;
                } else {
                    pool.core.import_reference(
                        &mut resources.render_io(),
                        job.media.as_mut().ok_or(Error::InvalidState)?,
                        job.base,
                        event.frame(),
                        order,
                        job.cursor.byte_position() + 12,
                        event.byte_len(),
                    )?;
                }
                job.quantum = Some(quantum);
                job.density = density;
                job.bytes = bytes;
                job.cursor = next;
                job.order = order;
                job.last_frame = Some(event.frame());
            }
            Ok(if job.ready {
                Status::Ready
            } else {
                Status::Building
            })
        })
    }
    pub fn cancel_indexed_midi_import(&mut self, id: ImportId) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.imports.remove(id.key)?;
            if let Some(media) = job.media {
                pool.core.release(media)?;
            }
            if let Some(writer) = job.writer {
                pool.writers.get_mut(writer)?.prefix = None;
            }
            pool.core.release_import(job.base)?;
            resources.release_snapshot(job.snapshot)
        })
    }
    pub fn finish_indexed_midi_import(&mut self, id: ImportId) -> Result<ResourceId, Error> {
        self.indexed_midi_credit()?;
        let media = self.with_indexed_midi(id.store, |resources, pool| {
            pool.views.available()?;
            if pool.imports.get(id.key)?.writer.is_some() {
                return Err(Error::InvalidState);
            }
            if !pool.imports.get(id.key)?.ready {
                return Err(Error::Busy);
            }
            let job = pool.imports.remove(id.key)?;
            pool.core.release_import(job.base)?;
            resources.release_snapshot(job.snapshot)?;
            Ok(pool.views.insert(job.media.ok_or(Error::InvalidState)?))
        })?;
        self.indexed_midi_resource(Take {
            store: id.store,
            media,
        })
    }
    /// Build an immutable prefix while the private writer captures its live tail.
    pub fn prepare_indexed_midi_recording(
        &mut self,
        store: StoreId,
        snapshot: SnapshotId,
        initial: &MidiState,
    ) -> Result<(ResourceId, WriterId, ImportId), Error> {
        let index = self.read_index(snapshot)?;
        let frames = self.midi_lease(index)?.snapshot.description.frames;
        let end_state = self.midi_seek_impl(index, frames)?.state.basic_projection();
        let import = self.prepare_indexed_midi_import(store, snapshot, initial)?;
        let mut resource = None;
        let mut writer = None;
        let result = (|| {
            self.indexed_midi_credit()?;
            let (media, frames) = self.with_indexed_midi(store, |_, pool| {
                pool.views.available()?;
                let job = pool.imports.get(import.key)?;
                let media = pool
                    .core
                    .retain(job.media.as_ref().ok_or(Error::InvalidState)?)?;
                Ok((pool.views.insert(media), job.frames))
            })?;
            let target = self.indexed_midi_resource(Take { store, media })?;
            resource = Some(target);
            let id = self.prepare_indexed_midi_writer(
                target,
                self.revision(target)?,
                WriteKind::Growing {
                    maximum_frames: u64::MAX,
                },
            )?;
            writer = Some(id);
            self.with_indexed_midi(store, |_, pool| {
                let admission = pool
                    .writers
                    .get_mut(id.key)?
                    .admission
                    .as_mut()
                    .ok_or(Error::InvalidState)?;
                pool.core.seed_recording(admission, &end_state)
            })?;
            self.start_indexed_midi_writer(id, frames)?;
            self.with_indexed_midi(store, |_, pool| {
                let job = pool.imports.get_mut(import.key)?;
                pool.core
                    .release(job.media.take().ok_or(Error::InvalidState)?)?;
                job.writer = Some(id.key);
                let target = pool.writers.get_mut(id.key)?;
                target.complete = false;
                target.prefix = Some(import.key);
                Ok(())
            })?;
            Ok((target, id, import))
        })();
        if result.is_err() {
            if let Some(writer) = writer {
                self.cancel_indexed_midi_writer(writer)?;
            }
            if let Some(resource) = resource {
                self.remove(resource)?;
            }
            self.cancel_indexed_midi_import(import)?;
        }
        result
    }
    pub fn finish_indexed_midi_recording_import(&mut self, id: ImportId) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.imports.get(id.key)?;
            if !job.ready {
                return Err(Error::Busy);
            }
            let target = pool
                .writers
                .get_mut(job.writer.ok_or(Error::InvalidState)?)?;
            if target.prefix != Some(id.key) {
                return Err(Error::InvalidState);
            }
            pool.core
                .complete_writer_prefix(target.writer.as_mut().ok_or(Error::Busy)?);
            target.complete = true;
            target.prefix = None;
            let job = pool.imports.remove(id.key)?;
            pool.core.release_import(job.base)?;
            resources.release_snapshot(job.snapshot)
        })
    }
    pub fn fix_indexed_midi_writer_window(&mut self, id: WriterId) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.writers.get_mut(id.key)?;
            if !job.complete {
                return Err(Error::Busy);
            }
            if resources.revision(job.resource)? != job.expected {
                return Err(Error::StaleRevision);
            }
            pool.core
                .fix_writer_window(job.writer.as_mut().ok_or(Error::Busy)?)
        })
    }
    pub fn prepare_indexed_midi_writer(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        kind: WriteKind,
    ) -> Result<WriterId, Error> {
        let kind = match kind {
            WriteKind::Fixed => writer::Kind::Fixed,
            WriteKind::Growing { maximum_frames } => writer::Kind::Growing { maximum_frames },
        };
        self.prepare_indexed_writer(resource, expected, kind)
    }
    pub fn prepare_indexed_midi_window_writer(
        &mut self,
        resource: ResourceId,
        expected: Revision,
    ) -> Result<WriterId, Error> {
        self.prepare_indexed_writer(resource, expected, writer::Kind::Windowed)
    }
    fn prepare_indexed_writer(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        kind: writer::Kind,
    ) -> Result<WriterId, Error> {
        let owner = self.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let version = get(
            &self.versions,
            owner.active.ok_or(Error::InvalidState)?.slot,
            owner.active.unwrap().generation,
        )?;
        let take = version.indexed_midi.ok_or(Error::InvalidShape)?;
        self.with_indexed_midi(take.store, |resources, pool| {
            pool.core
                .maintain(&mut resources.render_io(), pool.maintenance_steps)?;
            pool.writers.available()?;
            if pool
                .patches
                .entries
                .iter()
                .filter_map(|e| e.value.as_ref())
                .any(|j| j.resource == resource)
            {
                return Err(Error::Busy);
            }
            if pool
                .writers
                .entries
                .iter()
                .filter_map(|e| e.value.as_ref())
                .any(|j| j.resource == resource)
            {
                return Err(Error::Busy);
            }
            let media = pool.core.retain(pool.views.get(take.media)?)?;
            let admission = match pool.core.begin_writer(media, kind) {
                Ok(admission) => admission,
                Err((error, media)) => {
                    pool.core.release(media)?;
                    return Err(error);
                }
            };
            Ok(WriterId {
                store: take.store,
                key: pool.writers.insert(Job {
                    resource,
                    expected,
                    original: take.media,
                    admission: Some(admission),
                    writer: None,
                    source: None,
                    next: None,
                    complete: true,
                    prefix: None,
                }),
            })
        })
    }
    pub fn progress_indexed_midi_writer(
        &mut self,
        id: WriterId,
        events: usize,
    ) -> Result<Status, Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.writers.get_mut(id.key)?;
            let owner = resources.check_writer(job.resource, None)?;
            if owner.revision != job.expected {
                return Err(Error::StaleRevision);
            }
            if job.writer.is_some() {
                return Ok(Status::Ready);
            }
            let ready = pool.core.progress_writer(
                &mut resources.render_io(),
                job.admission.as_mut().ok_or(Error::InvalidState)?,
                events,
            )?;
            Ok(if ready {
                Status::Ready
            } else {
                Status::Building
            })
        })
    }
    pub fn start_indexed_midi_writer(&mut self, id: WriterId, position: u64) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.writers.get_mut(id.key)?;
            if resources.revision(job.resource)? != job.expected {
                return Err(Error::StaleRevision);
            }
            let admission = job.admission.take().ok_or(Error::InvalidState)?;
            match pool.core.finish_admission(admission, position) {
                Ok(writer) => {
                    job.writer = Some(writer);
                    Ok(())
                }
                Err((error, admission)) => {
                    job.admission = Some(admission);
                    Err(error)
                }
            }
        })
    }
    pub fn prepare_indexed_midi_writer_position(
        &mut self,
        id: WriterId,
        position: u64,
        steps: usize,
    ) -> Result<bool, Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.writers.get(id.key)?;
            if resources.revision(job.resource)? != job.expected {
                return Err(Error::StaleRevision);
            }
            if let Some(admission) = job.admission.as_ref() {
                pool.core.prepare_admission_position(
                    &mut resources.render_io(),
                    admission,
                    position,
                    steps,
                )
            } else {
                pool.core.prepare_writer_position(
                    &mut resources.render_io(),
                    job.writer.as_ref().ok_or(Error::InvalidState)?,
                    position,
                    steps,
                )
            }
        })
    }
    pub fn indexed_midi_writer_replay_bound(&mut self, id: WriterId) -> Result<usize, Error> {
        self.with_indexed_midi(id.store, |_, pool| {
            let job = pool.writers.get(id.key)?;
            if let Some(admission) = &job.admission {
                pool.core.admission_replay_bound(admission)
            } else {
                pool.core
                    .writer_replay_bound(job.writer.as_ref().ok_or(Error::InvalidState)?)
            }
        })
    }
    pub fn seek_indexed_midi_writer(&mut self, id: WriterId, position: u64) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |_, pool| {
            let job = pool.writers.get_mut(id.key)?;
            pool.core
                .seek_writer(job.writer.as_mut().ok_or(Error::Busy)?, position)
        })
    }
    /// Resume a prepared writer after an unwritten source interval.
    pub fn resume_indexed_midi_window_writer(&mut self, id: WriterId) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.writers.get_mut(id.key)?;
            if resources.revision(job.resource)? != job.expected {
                return Err(Error::StaleRevision);
            }
            if !job.complete {
                return Err(Error::Busy);
            }
            pool.core
                .resume_writer_window(job.writer.as_mut().ok_or(Error::Busy)?)?;
            job.source = None;
            job.next = None;
            Ok(())
        })
    }
    /// Retain the last complete private cut without consuming the writer.
    pub fn snapshot_indexed_midi_writer(&mut self, id: WriterId) -> Result<ResourceId, Error> {
        self.indexed_midi_credit()?;
        let media = self.with_indexed_midi(id.store, |_, pool| {
            pool.views.available()?;
            let job = pool.writers.get(id.key)?;
            if !job.complete {
                return Err(Error::Busy);
            }
            let media = pool
                .core
                .snapshot(job.writer.as_ref().ok_or(Error::Busy)?)?;
            Ok(pool.views.insert(media))
        })?;
        self.indexed_midi_resource(Take {
            store: id.store,
            media,
        })
    }
    pub fn cancel_indexed_midi_writer(&mut self, id: WriterId) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.writers.remove(id.key)?;
            if let Some(key) = job.prefix {
                let import = pool.imports.remove(key)?;
                pool.core.release_import(import.base)?;
                resources.release_snapshot(import.snapshot)?;
            }
            if let Some(admission) = job.admission {
                pool.core.cancel_writer(admission)?;
            }
            if let Some(writer) = job.writer {
                let media = pool.core.finish_writer(writer);
                pool.core.release(media)?;
            }
            Ok(())
        })
    }
    /// Publishes a complete view using the existing resource/version credit.
    pub fn finish_indexed_midi_writer(&mut self, id: WriterId) -> Result<Revision, Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.writers.get(id.key)?;
            if !job.complete {
                return Err(Error::Busy);
            }
            let mut owner = resources.check_writer(job.resource, None)?;
            if owner.revision != job.expected {
                return Err(Error::StaleRevision);
            }
            let active = owner.active.ok_or(Error::InvalidState)?;
            let mut version = get(&resources.versions, active.slot, active.generation)?;
            let take = version.indexed_midi.ok_or(Error::InvalidShape)?;
            if take.store != id.store || take.media != job.original {
                return Err(Error::StaleRevision);
            }
            let revision = owner.revision.checked_next()?;
            let writer = job.writer.as_ref().ok_or(Error::Busy)?;
            let totals = pool
                .core
                .writer_totals(&mut resources.render_io(), writer)?;
            let job = pool.writers.remove(id.key)?;
            let media = pool
                .core
                .finish_writer(job.writer.expect("validated writer"));
            version.description = Description {
                format: Format::Midi,
                frames: media.frames,
                bytes: totals.bytes,
            };
            let old = core::mem::replace(pool.views.get_mut(job.original)?, media);
            pool.core.release(old)?;
            owner.revision = revision;
            version.revision = revision;
            resources.resources[job.resource.slot() as usize].value = Some(owner);
            resources.versions[active.slot as usize].value = Some(version);
            Ok(revision)
        })
    }
    pub(super) fn read_indexed_midi(
        &mut self,
        take: Take,
        offset: usize,
        output: &mut [u8],
        class: WorkClass,
    ) -> Result<usize, Error> {
        // Bound background transfers before index lookup; required index work stays separately visible.
        let limit = if class == WorkClass::Background {
            output.len().min(self.copy_remaining())
        } else {
            output.len()
        };
        if limit == 0 {
            return Err(Error::Budget);
        }
        self.with_indexed_midi(take.store, |resources, pool| {
            pool.core.read_bytes_with(
                &mut resources.render_io(),
                pool.views.get(take.media)?,
                offset,
                &mut output[..limit],
                class,
            )
        })
    }
    pub fn cleanup_indexed_midi_store(&mut self, id: StoreId) -> Result<(), Error> {
        self.with_indexed_midi(id, |resources, pool| {
            if pool.closed {
                return Err(Error::InvalidState);
            }
            if !pool.views.empty()
                || !pool.writers.empty()
                || !pool.imports.empty()
                || !pool.grabs.empty()
                || !pool.patches.empty()
            {
                return Err(Error::Busy);
            }
            for slot in 0..pool.sources.entries.len() {
                let source = pool.sources.entries[slot]
                    .value
                    .as_mut()
                    .ok_or(Error::InvalidState)?;
                pool.core.clear_stage(&mut source.staged)?;
                pool.core
                    .release_source(source.history.take().ok_or(Error::InvalidState)?)?;
            }
            pool.core.begin_cleanup(&mut resources.render_io())?;
            pool.closed = true;
            Ok(())
        })
    }
    pub(super) fn maintain_indexed_midi(&mut self) -> bool {
        let count = self.indexed_midi_stores.len();
        for _ in 0..count {
            let slot = self.indexed_midi_maintenance;
            self.indexed_midi_maintenance = (slot + 1) % count;
            if self.indexed_midi_stores[slot].is_some() {
                let progressed = self
                    .with_indexed_midi(
                        StoreId {
                            scope: self.scope,
                            slot,
                        },
                        |resources, pool| {
                            let steps = pool.core.maintain(&mut resources.render_io(), 1)?;
                            if steps > 0 {
                                resources.charge_external(0, 1)?;
                            }
                            if pool.closed {
                                match pool.core.finish_cleanup(resources) {
                                    Ok(()) | Err(Error::Busy) => {}
                                    Err(error) => return Err(error),
                                }
                            }
                            Ok(steps > 0)
                        },
                    )
                    .expect("prepared indexed MIDI store");
                if progressed {
                    return true;
                }
            }
        }
        false
    }
}
impl<S: Sample> RenderIo<'_, S> {
    pub fn append_indexed_midi_source(
        &mut self,
        id: SourceId,
        frame: Frame,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        self.store.with_indexed_midi(id.store, |resources, pool| {
            if pool.closed {
                return Err(Error::InvalidState);
            }
            let source = pool.sources.get_mut(id.key)?;
            if source
                .history
                .as_ref()
                .ok_or(Error::InvalidState)?
                .coverage()
                .end
                != frame
            {
                return Err(Error::InvalidFrame);
            }
            pool.core.clear_stage(&mut source.staged)?;
            source.cut = None;
            pool.core
                .maintain(&mut resources.render_io(), pool.maintenance_steps)?;
            pool.core
                .stage(&mut resources.render_io(), &mut source.staged, frame, input)?;
            if source.before_dirty {
                source.before = *source.history.as_ref().ok_or(Error::InvalidState)?.state();
                resources.charge_work(WorkClass::Render, size_of::<MidiState>(), 0)?;
            }
            if let Err(error) = pool.core.commit_source(
                &mut resources.render_io(),
                source.history.as_mut().ok_or(Error::InvalidState)?,
                &source.staged,
            ) {
                pool.core.clear_stage(&mut source.staged)?;
                return Err(error);
            }
            source.before_dirty = !input.is_empty();
            source.cut = Some(frame);
            Ok(())
        })
    }
    pub fn reset_indexed_midi_source(
        &mut self,
        id: SourceId,
        frame: Frame,
        initial: &MidiState,
    ) -> Result<SourceId, Error> {
        self.store.with_indexed_midi(id.store, |resources, pool| {
            let source = pool.sources.get_mut(id.key)?;
            if id.key.generation == u64::MAX - 1 {
                return Err(Error::Exhausted);
            }
            if frame
                < source
                    .history
                    .as_ref()
                    .ok_or(Error::InvalidState)?
                    .coverage()
                    .end
            {
                return Err(Error::InvalidFrame);
            }
            pool.core.reset_source(
                source.history.as_mut().ok_or(Error::InvalidState)?,
                frame,
                initial,
            )?;
            pool.core.clear_stage(&mut source.staged)?;
            source.cut = None;
            source.before = *initial;
            source.before_dirty = false;
            resources.charge_work(WorkClass::Render, 3 * size_of::<MidiState>(), 0)?;
            pool.sources.entries[id.key.slot].generation += 1;
            Ok(SourceId {
                store: id.store,
                key: Key {
                    slot: id.key.slot,
                    generation: id.key.generation + 1,
                },
            })
        })
    }
    pub fn write_indexed_midi_tick(
        &mut self,
        id: WriterId,
        input: SourceId,
        frame: Frame,
    ) -> Result<u64, Error> {
        self.write_indexed_midi_tick_impl(id, input, frame, None)
    }
    pub fn write_indexed_midi_window_tick(
        &mut self,
        id: WriterId,
        input: SourceId,
        frame: Frame,
        span: WriteSpan,
    ) -> Result<(), Error> {
        self.write_indexed_midi_tick_impl(id, input, frame, Some(span))
            .map(|_| ())
    }
    fn write_indexed_midi_tick_impl(
        &mut self,
        id: WriterId,
        input: SourceId,
        frame: Frame,
        span: Option<WriteSpan>,
    ) -> Result<u64, Error> {
        if id.store != input.store {
            return Err(Error::InvalidShape);
        }
        self.store.with_indexed_midi(id.store, |resources, pool| {
            let source = pool.sources.get(input.key)?;
            let job = pool.writers.get_mut(id.key)?;
            if resources.revision(job.resource)? != job.expected {
                return Err(Error::StaleRevision);
            }
            if source.cut != Some(frame)
                || job.next.is_some_and(|next| next != frame)
                || job.source.is_some_and(|source| source != input)
            {
                return Err(Error::InvalidFrame);
            }
            pool.core
                .maintain(&mut resources.render_io(), pool.maintenance_steps)?;
            let next = frame.checked_add(resources.quantum.frames())?;
            let writer = job.writer.as_mut().ok_or(Error::Busy)?;
            if let Some(span) = span {
                pool.core.write_window_tick(
                    &mut resources.render_io(),
                    writer,
                    &source.before,
                    &source.staged,
                    span,
                )?;
            } else {
                pool.core.write_tick(
                    &mut resources.render_io(),
                    writer,
                    &source.before,
                    &source.staged,
                )?;
            }
            job.source = Some(input);
            job.next = Some(next);
            Ok(writer.position)
        })
    }
}

impl<S: Sample> ResourceIo<'_, S> {
    pub fn prepare_indexed_midi_grab(
        &mut self,
        source: SourceId,
        range: core::ops::Range<Frame>,
        frames: u64,
    ) -> Result<GrabId, Error> {
        self.store.prepare_indexed_midi_grab(source, range, frames)
    }
    pub fn progress_indexed_midi_grab(
        &mut self,
        id: GrabId,
        steps: usize,
    ) -> Result<Status, Error> {
        self.store.progress_indexed_midi_grab(id, steps)
    }
    pub fn finish_indexed_midi_grab(&mut self, id: GrabId) -> Result<ResourceId, Error> {
        self.store.finish_indexed_midi_grab(id)
    }
    pub fn indexed_midi_grab_replay_bound(&mut self, id: GrabId) -> Result<usize, Error> {
        self.store.indexed_midi_grab_replay_bound(id)
    }
    pub fn cancel_indexed_midi_grab(&mut self, id: GrabId) -> Result<(), Error> {
        self.store.cancel_indexed_midi_grab(id)
    }
    pub fn prepare_indexed_midi_writer_position(
        &mut self,
        id: WriterId,
        position: u64,
        steps: usize,
    ) -> Result<bool, Error> {
        self.store
            .prepare_indexed_midi_writer_position(id, position, steps)
    }
    pub fn resume_indexed_midi_window_writer(&mut self, id: WriterId) -> Result<(), Error> {
        self.store.resume_indexed_midi_window_writer(id)
    }
    pub fn indexed_midi_writer_replay_bound(&mut self, id: WriterId) -> Result<usize, Error> {
        self.store.indexed_midi_writer_replay_bound(id)
    }
    pub fn prepare_indexed_midi_recording(
        &mut self,
        store: StoreId,
        snapshot: SnapshotId,
        initial: &MidiState,
    ) -> Result<(ResourceId, WriterId, ImportId), Error> {
        self.store
            .prepare_indexed_midi_recording(store, snapshot, initial)
    }
    pub fn finish_indexed_midi_recording_import(&mut self, id: ImportId) -> Result<(), Error> {
        self.store.finish_indexed_midi_recording_import(id)
    }
    pub fn fix_indexed_midi_writer_window(&mut self, id: WriterId) -> Result<(), Error> {
        self.store.fix_indexed_midi_writer_window(id)
    }

    pub fn prepare_indexed_midi_window_writer(
        &mut self,
        resource: ResourceId,
        expected: Revision,
    ) -> Result<WriterId, Error> {
        self.store
            .prepare_indexed_midi_window_writer(resource, expected)
    }

    pub fn maintain_indexed_midi_required(&mut self, id: StoreId) -> Result<usize, Error> {
        self.store.with_indexed_midi(id, |resources, pool| {
            pool.core
                .maintain(&mut resources.render_io(), pool.maintenance_steps)
        })
    }

    pub fn seek_indexed_midi_writer(&mut self, id: WriterId, position: u64) -> Result<(), Error> {
        self.store.seek_indexed_midi_writer(id, position)
    }
    pub fn snapshot_indexed_midi_writer(&mut self, id: WriterId) -> Result<ResourceId, Error> {
        self.store.snapshot_indexed_midi_writer(id)
    }

    pub fn fork_indexed_midi(
        &mut self,
        id: StoreId,
        snapshot: SnapshotId,
    ) -> Result<ResourceId, Error> {
        self.store.fork_indexed_midi(id, snapshot)
    }
    pub fn prepare_indexed_midi_import(
        &mut self,
        id: StoreId,
        snapshot: SnapshotId,
        initial: &MidiState,
    ) -> Result<ImportId, Error> {
        self.store
            .prepare_indexed_midi_import(id, snapshot, initial)
    }
    pub fn progress_indexed_midi_import(
        &mut self,
        id: ImportId,
        events: usize,
    ) -> Result<Status, Error> {
        self.store.progress_indexed_midi_import(id, events)
    }
    pub fn cancel_indexed_midi_import(&mut self, id: ImportId) -> Result<(), Error> {
        self.store.cancel_indexed_midi_import(id)
    }
    pub fn finish_indexed_midi_import(&mut self, id: ImportId) -> Result<ResourceId, Error> {
        self.store.finish_indexed_midi_import(id)
    }
    pub fn prepare_indexed_midi_writer(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        kind: WriteKind,
    ) -> Result<WriterId, Error> {
        self.store
            .prepare_indexed_midi_writer(resource, expected, kind)
    }
    pub fn progress_indexed_midi_writer(
        &mut self,
        id: WriterId,
        events: usize,
    ) -> Result<Status, Error> {
        self.store.progress_indexed_midi_writer(id, events)
    }
    pub fn start_indexed_midi_writer(&mut self, id: WriterId, position: u64) -> Result<(), Error> {
        self.store.start_indexed_midi_writer(id, position)
    }
    pub fn cancel_indexed_midi_writer(&mut self, id: WriterId) -> Result<(), Error> {
        self.store.cancel_indexed_midi_writer(id)
    }
    pub fn finish_indexed_midi_writer(&mut self, id: WriterId) -> Result<Revision, Error> {
        self.store.finish_indexed_midi_writer(id)
    }

    pub fn indexed_midi_coverage(
        &self,
        source: SourceId,
    ) -> Result<core::ops::Range<Frame>, Error> {
        self.store.indexed_midi_coverage(source)
    }
    pub fn grab_indexed_midi(
        &mut self,
        source: SourceId,
        range: core::ops::Range<Frame>,
        frames: u64,
    ) -> Result<ResourceId, Error> {
        self.store.grab_indexed_midi(source, range, frames)
    }
}

#[cfg(test)]
mod tests;
