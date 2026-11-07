//! Single-owner fixed-page resource storage. All allocation and final destruction
//! happen at preparation/host teardown; rendering uses fixed pools and bounded copies.
//! Raw resource/snapshot IDs require same-store caller discipline. Scoped indexes
//! additionally check instance and lease generation; copying either acquires no lease.

mod reservation;
pub use reservation::ReservedSnapshotId;
mod boundary;
pub use boundary::{CaptureBoundaryIo, PlannerResourceIo};
mod capture;
mod format;
mod group;
use group::CapturePool;
pub use group::{
    CaptureGroupConfig, CaptureGroupId, CaptureGroupInfo, CaptureGroupUsage, CaptureMember,
    CapturePhase, CaptureTakeId, CaptureTickId,
};
mod index;
mod midi_index;
mod midi_read;
pub use index::{IndexWork, ReadIndex};
use index::{PageIndex, VersionIndex};
pub use midi_index::{MidiIndexConfig, MidiIndexInfo, MidiIndexUsage, MidiIndexWork};
use midi_index::{MidiOwner, MidiPool};
pub use midi_read::{MidiCursor, MidiEvent, MidiPosition};
mod audio_backing;
pub mod indexed_midi;
mod midi_backing;
pub use midi_backing::{MidiEditId, MidiImportId, MidiStoreId};
#[allow(dead_code)] // Continuous shared MIDI storage is integrated in stages.
mod midi_packets;
#[allow(dead_code)] // Continuous writer integration is staged separately.
mod midi_writer;
mod render;
mod request;
#[allow(dead_code)] // Prepared history storage; engine integration is staged separately.
mod rolling_history;
pub mod shared_audio;
pub mod shared_midi;
#[allow(dead_code)] // Prepared shared backing; consumers are introduced separately.
mod shared_pages;
pub use audio_backing::{AudioChunkRef, AudioHistoryId, AudioStoreId, AudioWrite};
pub use capture::{CaptureId, CaptureInfo, ResourceInfo};
use render::WorkClass;
pub use render::{RenderIo, RenderWork};
pub use request::{Reply, Request, ResourceCommand, ResourceIo};
#[cfg(test)]
mod tests;
use crate::{Error, Frame, PreparedConfig, Quantum, ResourceId, Revision};
use alloc::{boxed::Box, vec, vec::Vec};
use format::Parser;
pub use format::{encode_midi_record, Description, Format};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadId {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotId {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub id: SnapshotId,
    pub resource: ResourceId,
    pub revision: Revision,
    pub description: Description,
    pub cut_frame: Frame,
    pub capture: Option<CaptureInfo>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Currentness {
    Current,
    Stale,
    ResourceRemoved,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Work {
    pub copied_bytes: usize,
    pub validation_steps: usize,
    pub reclaimed_pages: usize,
    pub reclaimed_versions: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PoolUsage {
    pub free_pages: usize,
    pub reserved_pages: usize,
    pub free_versions: usize,
    pub free_snapshots: usize,
    pub retired_versions: usize,
    pub retired_pages: usize,
}
#[derive(Clone, Copy)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}
#[derive(Clone, Copy)]
struct Resource {
    active: Option<UploadId>,
    revision: Revision,
    claim: Option<CaptureGroupId>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    PreparedCapture,
    Receiving,
    Complete,
    Active,
    Capturing,
    Retained,
    Retiring,
}
#[derive(Clone, Copy)]
struct Version {
    resource: ResourceId,
    expected_revision: Revision,
    revision: Revision,
    description: Description,
    written: usize,
    reserved: usize,
    head: Option<usize>,
    tail: Option<usize>,
    references: usize,
    parser: Parser,
    state: State,
    capture: Option<CaptureInfo>,
    index: VersionIndex,
    midi: Option<MidiOwner>,
    audio: Option<audio_backing::Take>,
    shared_midi: Option<midi_backing::Take>,
    indexed_midi: Option<indexed_midi::Take>,
}
#[derive(Clone, Copy)]
struct Lease {
    reservation: Option<reservation::Reservation>,
    snapshot: Snapshot,
    version: UploadId,
    private_tail: Option<usize>,
    tail_start: usize,
    audio: Option<audio_backing::Export>,
    shared_midi: Option<midi_backing::Export>,
    indexed_midi: Option<indexed_midi::Take>,
}
#[derive(Clone, Copy, Default)]
struct Page {
    next: Option<usize>,
    used: usize,
    index: PageIndex,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cursor {
    scope: u32,
    snapshot: SnapshotId,
    page: Option<usize>,
    position: usize,
}
impl Cursor {
    pub fn position(self) -> usize {
        self.position
    }
}

/// Exclusive metadata credit. Copying this token acquires no additional credit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotReservation(SnapshotId);

pub struct ResourceStore {
    audio_graph_work: crate::audio_refs::AudioGraphWork,
    audio_render_mode: crate::AudioRenderMode,
    quantum: Quantum,
    scope: u32,
    config: PreparedConfig,
    data: Box<[u8]>,
    pages: Vec<Page>,
    free_pages: Vec<usize>,
    reserved_pages: usize,
    resources: Vec<Slot<Resource>>,
    free_resources: Vec<usize>,
    versions: Vec<Slot<Version>>,
    free_versions: Vec<usize>,
    snapshots: Vec<Slot<Lease>>,
    free_snapshots: Vec<usize>,
    snapshot_reservations: Box<[bool]>,
    retired_versions: Vec<usize>,
    retired_pages: Vec<usize>,
    copy_remaining: usize,
    steps_remaining: usize,
    work: Work,
    render_work: RenderWork,
    index_work: IndexWork,
    midi_indexes: MidiPool,
    capture_groups: CapturePool,
    control_phase: bool,
    audio_stores: Vec<Option<Box<shared_audio::AudioStore>>>,
    audio_maintenance: usize,
    midi_stores: Vec<Option<Box<shared_midi::MidiStore>>>,
    midi_maintenance: usize,
    indexed_midi_stores: Vec<Option<Box<indexed_midi::Store>>>,
    indexed_midi_maintenance: usize,
    maintenance_kind: usize,
}
fn retirement_stack(count: usize) -> Vec<usize> {
    let mut stack = vec![0; count];
    for index in &mut stack {
        *core::hint::black_box(index) = 0;
    }
    stack.clear();
    stack
}
fn slots<T>(count: usize) -> Vec<Slot<T>> {
    (0..count)
        .map(|_| Slot {
            generation: 1,
            value: None,
        })
        .collect()
}
fn get<T: Copy>(slots: &[Slot<T>], slot: u32, generation: u32) -> Result<T, Error> {
    slots
        .get(slot as usize)
        .filter(|entry| entry.generation == generation)
        .and_then(|entry| entry.value)
        .ok_or(Error::InvalidId)
}
fn release<T: Copy>(slots: &mut [Slot<T>], free: &mut Vec<usize>, index: usize) {
    slots[index].value = None;
    if let Some(next) = slots[index].generation.checked_add(1) {
        slots[index].generation = next;
        free.push(index);
    }
}

impl ResourceStore {
    pub fn new(config: PreparedConfig, quantum: Quantum) -> Result<Self, Error> {
        let limits = config.limits();
        if quantum.frames() > limits.max_frames {
            return Err(Error::InvalidConfig);
        }
        for (count, size) in [
            (limits.resource_slots, size_of::<Slot<Version>>()),
            (limits.resource_slots, size_of::<Slot<Lease>>()),
            (limits.resource_pages, size_of::<Page>()),
        ] {
            if count
                .checked_mul(size)
                .is_none_or(|bytes| bytes > i32::MAX as usize)
            {
                return Err(Error::Capacity);
            }
        }
        let scope = index::new_scope()?;
        let mut data = vec![0u8; config.resource_bytes()].into_boxed_slice();
        for byte in &mut data {
            *core::hint::black_box(byte) = 0;
        }
        let mut pages = vec![Page::default(); limits.resource_pages];
        for page in &mut pages {
            *core::hint::black_box(page) = Page::default();
        }
        Ok(Self {
            audio_graph_work: Default::default(),
            audio_render_mode: crate::AudioRenderMode::Current,
            scope,
            config,
            quantum,
            data,
            pages,
            free_pages: (0..limits.resource_pages).rev().collect(),
            reserved_pages: 0,
            audio_stores: Vec::with_capacity(limits.resource_slots),
            audio_maintenance: 0,
            midi_stores: Vec::with_capacity(limits.resource_slots),
            midi_maintenance: 0,
            indexed_midi_stores: Vec::with_capacity(limits.resource_slots),
            indexed_midi_maintenance: 0,
            maintenance_kind: 0,
            resources: slots(limits.resource_slots),
            free_resources: (0..limits.resource_slots).rev().collect(),
            versions: slots(limits.resource_slots),
            free_versions: (0..limits.resource_slots).rev().collect(),
            snapshots: slots(limits.resource_slots),
            free_snapshots: (0..limits.resource_slots).rev().collect(),
            snapshot_reservations: vec![false; limits.resource_slots].into_boxed_slice(),
            retired_versions: retirement_stack(limits.resource_slots),
            retired_pages: retirement_stack(limits.resource_pages),
            copy_remaining: 0,
            steps_remaining: 0,
            work: Work::default(),
            render_work: RenderWork::default(),
            index_work: IndexWork::default(),
            midi_indexes: MidiPool::default(),
            capture_groups: CapturePool::default(),
            control_phase: false,
        })
    }

    /// Host boundary only; call once per callback, not per command or processor.
    pub fn begin_callback(&mut self) {
        self.audio_graph_work = Default::default();
        for audio in self.audio_stores.iter_mut().flatten() {
            audio.reset_work();
        }
        self.copy_remaining = self.config.limits().budgets.transfer_bytes;
        self.steps_remaining = self.config.limits().budgets.validation_steps;
        self.work = Work::default();
        self.render_work = RenderWork::default();
        self.index_work = IndexWork::default();
        self.midi_indexes.work = MidiIndexWork::default();
        self.control_phase = false;
    }
    pub fn quantum(&self) -> Quantum {
        self.quantum
    }
    pub fn set_audio_render_mode(&mut self, mode: crate::AudioRenderMode) {
        self.audio_render_mode = mode;
    }

    pub fn audio_render_mode(&self) -> crate::AudioRenderMode {
        self.audio_render_mode
    }
    pub fn audio_graph_work(&self) -> crate::audio_refs::AudioGraphWork {
        self.audio_graph_work
    }
    pub fn audio_pool_work(&self) -> shared_audio::AudioPoolWork {
        let mut result = shared_audio::AudioPoolWork::default();
        for store in self.audio_stores.iter().flatten() {
            let work = store.work();
            result.index_allocated += work.index_allocated;
            result.index_reclaimed += work.index_reclaimed;
            result.shared_payloads += work.shared_payloads;
            result.payload_live += work.payload_live;
            result.index_live += work.index_live;
        }
        result
    }
    pub fn add_audio_graph_work(&mut self, work: crate::audio_refs::AudioGraphWork) {
        self.audio_graph_work.arena_peak_samples = self
            .audio_graph_work
            .arena_peak_samples
            .max(work.arena_peak_samples);
        self.audio_graph_work.forwarded += work.forwarded;
        self.audio_graph_work.materialized += work.materialized;
        self.audio_graph_work.sample_reads += work.sample_reads;
        self.audio_graph_work.sample_writes += work.sample_writes;
    }

    fn detach_audio_pages(&mut self, count: usize) -> Result<(), Error> {
        let page_bytes = self.config.limits().page_bytes;
        let end = self.data.len() / page_bytes;
        let first = end.checked_sub(count).ok_or(Error::Capacity)?;
        if count > self.free_pages.len().saturating_sub(self.reserved_pages) {
            return Err(Error::Capacity);
        }
        let mut available = vec![false; end];
        for &page in &self.free_pages {
            if page < end {
                available[page] = true;
            }
        }
        if !available[first..].iter().all(|&free| free) {
            return Err(Error::Busy);
        }
        self.free_pages.retain(|&page| page < first);
        let mut data = core::mem::take(&mut self.data).into_vec();
        data.truncate(first * page_bytes);
        self.data = data.into_boxed_slice();
        Ok(())
    }
    pub fn config(&self) -> PreparedConfig {
        self.config
    }
    pub(crate) fn control_phase(&mut self, active: bool) {
        self.control_phase = active;
    }
    pub(crate) fn charge_external(&mut self, bytes: usize, steps: usize) -> Result<(), Error> {
        if bytes > self.copy_remaining() || steps > self.steps_remaining() {
            return Err(Error::Budget);
        }
        self.copy_remaining -= bytes;
        self.work.copied_bytes += bytes;
        self.steps_remaining -= steps;
        self.work.validation_steps += steps;
        Ok(())
    }
    pub fn work(&self) -> Work {
        self.work
    }
    pub fn copy_remaining(&self) -> usize {
        self.copy_remaining.saturating_sub(if self.control_phase {
            self.config.limits().resource_io_bytes
        } else {
            0
        })
    }
    pub fn steps_remaining(&self) -> usize {
        self.steps_remaining.saturating_sub(if self.control_phase {
            self.config.limits().resource_io_steps
        } else {
            0
        })
    }
    pub fn usage(&self) -> PoolUsage {
        PoolUsage {
            free_pages: self.free_pages.len(),
            reserved_pages: self.reserved_pages,
            free_versions: self.free_versions.len(),
            free_snapshots: self.free_snapshots.len(),
            retired_versions: self.retired_versions.len(),
            retired_pages: self.retired_pages.len(),
        }
    }
    pub fn create(&mut self) -> Result<ResourceId, Error> {
        let index = self.free_resources.pop().ok_or(Error::Capacity)?;
        let slot = &mut self.resources[index];
        slot.value = Some(Resource {
            active: None,
            revision: Revision(0),
            claim: None,
        });
        ResourceId::from_parts(index as u32, slot.generation)
    }
    pub fn revision(&self, id: ResourceId) -> Result<Revision, Error> {
        Ok(get(&self.resources, id.slot(), id.generation())?.revision)
    }
    fn check_writer(
        &self,
        id: ResourceId,
        claim: Option<CaptureGroupId>,
    ) -> Result<Resource, Error> {
        let resource = get(&self.resources, id.slot(), id.generation())?;
        if resource.claim != claim {
            return Err(Error::InvalidState);
        }
        Ok(resource)
    }
    pub fn remove(&mut self, id: ResourceId) -> Result<(), Error> {
        let resource = self.check_writer(id, None)?;
        if let Some(version) = resource.active {
            self.detach(version)?;
        }
        release(
            &mut self.resources,
            &mut self.free_resources,
            id.slot() as usize,
        );
        Ok(())
    }
    /// Permanent owner teardown outside rendering. Pending uploads are cancelled
    /// before the resource is removed; independent immutable leases remain valid.
    pub fn remove_with_uploads(&mut self, id: ResourceId) -> Result<(), Error> {
        self.check_writer(id, None)?;
        for slot in 0..self.versions.len() {
            if self.versions[slot].value.is_some_and(|version| {
                version.resource == id
                    && matches!(version.state, State::Receiving | State::Complete)
            }) {
                self.cancel_upload(UploadId {
                    slot: slot as u32,
                    generation: self.versions[slot].generation,
                })?;
            }
        }
        self.remove(id)
    }

    pub fn begin_upload(
        &mut self,
        resource: ResourceId,
        expected_revision: Revision,
        description: Description,
    ) -> Result<UploadId, Error> {
        self.begin_upload_with(resource, expected_revision, description, None)
    }
    fn begin_upload_with(
        &mut self,
        resource: ResourceId,
        expected_revision: Revision,
        description: Description,
        claim: Option<CaptureGroupId>,
    ) -> Result<UploadId, Error> {
        if self.check_writer(resource, claim)?.revision != expected_revision {
            return Err(Error::StaleRevision);
        }
        description.validate(self.config.resource_bytes())?;
        let needed = description.bytes.div_ceil(self.config.limits().page_bytes);
        if needed > self.free_pages.len() - self.reserved_pages {
            return Err(Error::Capacity);
        }
        let index = self.free_versions.pop().ok_or(Error::Capacity)?;
        self.reserved_pages += needed;
        let slot = &mut self.versions[index];
        slot.value = Some(Version {
            resource,
            expected_revision,
            revision: expected_revision,
            description,
            written: 0,
            reserved: needed,
            head: None,
            tail: None,
            references: 1,
            parser: Parser::default(),
            state: State::Receiving,
            capture: None,
            index: VersionIndex::default(),
            midi: None,
            audio: None,
            shared_midi: None,
            indexed_midi: None,
        });
        Ok(UploadId {
            slot: index as u32,
            generation: slot.generation,
        })
    }

    /// A chunk may not cross a page boundary. Validate a copy of parser state so
    /// malformed chunks leave both the received prefix and parser unchanged.
    pub fn write_upload(&mut self, id: UploadId, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        let mut version = get(&self.versions, id.slot, id.generation)?;
        if version.state != State::Receiving {
            return Err(Error::InvalidState);
        }
        let page_bytes = self.config.limits().page_bytes;
        if offset != version.written {
            return Err(Error::InvalidOffset);
        }
        if bytes.is_empty()
            || bytes.len() > version.description.bytes - version.written
            || bytes.len() > page_bytes - offset % page_bytes
        {
            return Err(Error::InvalidShape);
        }
        let validation = if version.description.format == Format::Midi {
            bytes.len()
        } else {
            0
        };
        if bytes.len() > self.copy_remaining() || validation > self.steps_remaining() {
            return Err(Error::Budget);
        }
        self.steps_remaining -= validation;
        self.work.validation_steps += validation;
        let prior_parser = version.parser;
        if version.description.format == Format::Midi {
            version.parser.consume(
                bytes,
                version.description.frames,
                self.config.limits().midi_bytes_per_port,
            )?;
        }
        if let Some(owner) = version.midi {
            self.midi_indexes.preflight(
                owner,
                id,
                version.parser.events() - prior_parser.events(),
            )?;
        }
        let page = if offset % page_bytes == 0 {
            if version.reserved == 0 {
                return Err(Error::InvalidState);
            }
            let page = self.free_pages.pop().ok_or(Error::InvalidState)?;
            self.pages[page] = Page::default();
            self.index_append(&mut version, page, WorkClass::Background);
            if let Some(tail) = version.tail {
                self.pages[tail].next = Some(page);
            } else {
                version.head = Some(page);
            }
            version.tail = Some(page);
            version.reserved -= 1;
            self.reserved_pages -= 1;
            page
        } else {
            version.tail.ok_or(Error::InvalidState)?
        };
        let start = page * page_bytes + self.pages[page].used;
        self.data[start..start + bytes.len()].copy_from_slice(bytes);
        self.pages[page].used += bytes.len();
        version.written += bytes.len();
        if let Some(owner) = version.midi {
            let mut parser = prior_parser;
            parser
                .consume_records(
                    bytes,
                    version.description.frames,
                    self.config.limits().midi_bytes_per_port,
                    offset,
                    |record| {
                        self.midi_indexes
                            .append(owner, record, WorkClass::Background)
                    },
                )
                .expect("validated MIDI chunk and reserved checkpoints");
            debug_assert_eq!(parser, version.parser);
        }
        self.versions[id.slot as usize].value = Some(version);
        self.copy_remaining -= bytes.len();
        self.work.copied_bytes += bytes.len();
        Ok(())
    }
    pub fn finish_upload(&mut self, id: UploadId) -> Result<(), Error> {
        let mut version = get(&self.versions, id.slot, id.generation)?;
        if version.state != State::Receiving {
            return Err(Error::InvalidState);
        }
        if version.written != version.description.bytes {
            return Err(Error::Incomplete);
        }
        if version.description.format == Format::Midi && !version.parser.complete() {
            return Err(Error::Incomplete);
        }
        version.state = State::Complete;
        self.versions[id.slot as usize].value = Some(version);
        Ok(())
    }
    pub fn commit(&mut self, id: UploadId) -> Result<Revision, Error> {
        let mut version = get(&self.versions, id.slot, id.generation)?;
        if version.state != State::Complete {
            return Err(Error::Incomplete);
        }
        let mut resource = self.check_writer(version.resource, None)?;
        if resource.revision != version.expected_revision {
            return Err(Error::StaleRevision);
        }
        let revision = resource.revision.checked_next()?;
        let previous = resource.active.replace(id);
        resource.revision = revision;
        version.revision = revision;
        version.state = State::Active;
        self.versions[id.slot as usize].value = Some(version);
        self.resources[version.resource.slot() as usize].value = Some(resource);
        if let Some(previous) = previous {
            self.detach(previous)?;
        }
        Ok(revision)
    }
    pub fn cancel_upload(&mut self, id: UploadId) -> Result<(), Error> {
        let mut version = get(&self.versions, id.slot, id.generation)?;
        if !matches!(version.state, State::Receiving | State::Complete) {
            return Err(Error::InvalidState);
        }
        self.reserved_pages -= version.reserved;
        version.reserved = 0;
        self.versions[id.slot as usize].value = Some(version);
        self.unreference(id)
    }
    fn detach(&mut self, id: UploadId) -> Result<(), Error> {
        let mut version = get(&self.versions, id.slot, id.generation)?;
        version.state = State::Retained;
        if let Some(capture) = &mut version.capture {
            capture.active = false;
        }
        self.versions[id.slot as usize].value = Some(version);
        self.unreference(id)
    }
    fn unreference(&mut self, id: UploadId) -> Result<(), Error> {
        let mut version = get(&self.versions, id.slot, id.generation)?;
        version.references = version
            .references
            .checked_sub(1)
            .ok_or(Error::InvalidState)?;
        if version.references == 0 {
            if let Some(audio) = version.audio.take() {
                self.with_audio(audio.store, |_, audio_store| {
                    audio_store.release_take(audio.take)
                })?;
            }
            if let Some(take) = version.shared_midi.take() {
                self.with_midi(take.store, |_, midi| midi.release_take(take.take))?;
            }
            if let Some(take) = version.indexed_midi.take() {
                self.with_indexed_midi(take.store, |_, pool| pool.release(take.media))?;
            }
            version.state = State::Retiring;
            if let Some(owner) = version.midi.take() {
                self.midi_indexes.release(owner);
            }
            self.retired_versions.push(id.slot as usize);
        }
        self.versions[id.slot as usize].value = Some(version);
        Ok(())
    }

    pub fn reserve_snapshot(&mut self) -> Result<SnapshotReservation, Error> {
        let slot = self.free_snapshots.pop().ok_or(Error::Capacity)?;
        self.snapshot_reservations[slot] = true;
        Ok(SnapshotReservation(SnapshotId {
            slot: slot as u32,
            generation: self.snapshots[slot].generation,
        }))
    }
    fn check_snapshot_reservation(&self, reservation: SnapshotReservation) -> Result<usize, Error> {
        let id = reservation.0;
        let slot = self
            .snapshots
            .get(id.slot as usize)
            .ok_or(Error::InvalidId)?;
        if slot.generation != id.generation
            || !self.snapshot_reservations[id.slot as usize]
            || slot.value.is_some()
        {
            return Err(Error::InvalidId);
        }
        Ok(id.slot as usize)
    }
    pub fn cancel_snapshot_reservation(
        &mut self,
        reservation: SnapshotReservation,
    ) -> Result<(), Error> {
        let slot = self.check_snapshot_reservation(reservation)?;
        self.snapshot_reservations[slot] = false;
        release(&mut self.snapshots, &mut self.free_snapshots, slot);
        Ok(())
    }
    pub fn snapshot_reserved(
        &mut self,
        reservation: SnapshotReservation,
        resource: ResourceId,
        cut_frame: Frame,
    ) -> Result<Snapshot, Error> {
        let slot = self.check_snapshot_reservation(reservation)?;
        self.snapshot_in_slot(resource, cut_frame, Some(slot))
    }
    pub fn snapshot(&mut self, resource: ResourceId, cut_frame: Frame) -> Result<Snapshot, Error> {
        self.snapshot_in_slot(resource, cut_frame, None)
    }
    fn snapshot_in_slot(
        &mut self,
        resource: ResourceId,
        cut_frame: Frame,
        reserved: Option<usize>,
    ) -> Result<Snapshot, Error> {
        let active = get(&self.resources, resource.slot(), resource.generation())?
            .active
            .ok_or(Error::InvalidState)?;
        let mut version = get(&self.versions, active.slot, active.generation)?;
        let refs = version.references.checked_add(1).ok_or(Error::Exhausted)?;
        let cut_frame = if let Some(capture) = version.capture {
            if cut_frame < capture.next_frame {
                return Err(Error::InvalidFrame);
            }
            capture.next_frame
        } else {
            cut_frame
        };
        if reserved.is_none() && self.free_snapshots.is_empty() {
            return Err(Error::Capacity);
        }
        let audio = match version.audio {
            Some(take) => Some(self.with_audio(take.store, |_, store| {
                let export = store.export(take.take)?;
                Ok(audio_backing::Export {
                    store: take.store,
                    export,
                    index: store.export_index(export)?,
                })
            })?),
            None => None,
        };
        let shared_midi = match version.shared_midi {
            Some(take) => Some(self.with_midi(take.store, |_, midi| {
                let export = midi.export(take.take)?;
                Ok(midi_backing::Export {
                    store: take.store,
                    export,
                    index: midi.export_index(export)?,
                })
            })?),
            None => None,
        };
        let indexed_midi = version
            .indexed_midi
            .map(|take| {
                self.with_indexed_midi(take.store, |_, pool| {
                    pool.retain(take.media).map(|media| indexed_midi::Take {
                        store: take.store,
                        media,
                    })
                })
            })
            .transpose()?;
        let mutable_tail = version.tail.filter(|&page| {
            version.state == State::Capturing
                && self.pages[page].used < self.config.limits().page_bytes
        });
        let private_tail = if let Some(tail) = mutable_tail {
            let used = self.pages[tail].used;
            if self.free_pages.len() == self.reserved_pages {
                return Err(Error::Capacity);
            }
            if used > self.copy_remaining() {
                return Err(Error::Budget);
            }
            let page = self.free_pages.pop().ok_or(Error::InvalidState)?;
            let start = tail * self.config.limits().page_bytes;
            self.data
                .copy_within(start..start + used, page * self.config.limits().page_bytes);
            self.pages[page] = Page {
                used,
                ..Page::default()
            };
            self.copy_remaining -= used;
            self.work.copied_bytes += used;
            Some(page)
        } else {
            None
        };
        let index = if let Some(index) = reserved {
            self.snapshot_reservations[index] = false;
            index
        } else {
            self.free_snapshots.pop().ok_or(Error::InvalidState)?
        };
        let snapshot = Snapshot {
            id: SnapshotId {
                slot: index as u32,
                generation: self.snapshots[index].generation,
            },
            resource,
            revision: version.revision,
            description: version.description,
            cut_frame,
            capture: version.capture,
        };
        self.snapshots[index].value = Some(Lease {
            reservation: None,
            snapshot,
            version: active,
            private_tail,
            tail_start: version.written / self.config.limits().page_bytes
                * self.config.limits().page_bytes,
            audio,
            shared_midi,
            indexed_midi,
        });
        version.references = refs;
        self.versions[active.slot as usize].value = Some(version);
        Ok(snapshot)
    }
    pub fn currentness(&self, id: SnapshotId) -> Result<Currentness, Error> {
        let lease = get(&self.snapshots, id.slot, id.generation)?;
        let resource = match get(
            &self.resources,
            lease.snapshot.resource.slot(),
            lease.snapshot.resource.generation(),
        ) {
            Ok(resource) => resource,
            Err(Error::InvalidId) => return Ok(Currentness::ResourceRemoved),
            Err(error) => return Err(error),
        };
        Ok(
            if resource.active == Some(lease.version)
                && resource.revision == lease.snapshot.revision
                && get(&self.versions, lease.version.slot, lease.version.generation)?.capture
                    == lease.snapshot.capture
            {
                Currentness::Current
            } else {
                Currentness::Stale
            },
        )
    }
    pub fn cursor(&self, snapshot: SnapshotId) -> Result<Cursor, Error> {
        let lease = get(&self.snapshots, snapshot.slot, snapshot.generation)?;
        let version = get(&self.versions, lease.version.slot, lease.version.generation)?;
        Ok(Cursor {
            scope: self.scope,
            snapshot,
            page: version.head,
            position: 0,
        })
    }
    /// Copy at most the remainder of one page. No whole-chain walk or implicit seek.
    pub fn read(&mut self, cursor: &mut Cursor, output: &mut [u8]) -> Result<usize, Error> {
        self.read_with(cursor, output, WorkClass::Background)
    }
    fn read_with(
        &mut self,
        cursor: &mut Cursor,
        output: &mut [u8],
        class: WorkClass,
    ) -> Result<usize, Error> {
        self.note_operation(class);
        if cursor.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let lease = get(
            &self.snapshots,
            cursor.snapshot.slot,
            cursor.snapshot.generation,
        )?;
        if cursor.position == lease.snapshot.description.bytes || output.is_empty() {
            return Ok(0);
        }
        if let Some(take) = lease.indexed_midi {
            let count = self.read_indexed_midi(take, cursor.position, output, class)?;
            cursor.position += count;
            return Ok(count);
        }
        if let Some(midi) = lease.shared_midi {
            let count = self.with_midi(midi.store, |store, midi_store| {
                midi_store.read_bytes(store, midi.index, cursor.position, output, class)
            })?;
            cursor.position += count;
            return Ok(count);
        }
        if let Some(audio) = lease.audio {
            let count = output
                .len()
                .min(self.config.limits().page_bytes)
                .min(lease.snapshot.description.bytes - cursor.position);
            self.with_audio(audio.store, |store, audio_store| {
                audio_store.read_span(
                    store,
                    audio.index,
                    cursor.position,
                    &mut output[..count],
                    class,
                )
            })?;
            cursor.position += count;
            return Ok(count);
        }
        let page_bytes = self.config.limits().page_bytes;
        let private_tail = lease
            .private_tail
            .filter(|_| cursor.position >= lease.tail_start);
        let page = private_tail.or(cursor.page).ok_or(Error::InvalidState)?;
        let offset = cursor.position % page_bytes;
        let count = output
            .len()
            .min(
                self.pages[page]
                    .used
                    .checked_sub(offset)
                    .ok_or(Error::InvalidState)?,
            )
            .min(lease.snapshot.description.bytes - cursor.position);
        if count == 0 {
            return Err(Error::InvalidState);
        }
        self.charge_work(class, count, 0)?;
        let start = page * page_bytes + offset;
        output[..count].copy_from_slice(&self.data[start..start + count]);
        cursor.position += count;
        if cursor.position % page_bytes == 0 {
            cursor.page = self.pages[page].next;
        }
        Ok(count)
    }
    pub fn release_snapshot(&mut self, id: SnapshotId) -> Result<(), Error> {
        let lease = get(&self.snapshots, id.slot, id.generation)?;
        if let Some(reservation) = lease.reservation {
            self.cancel_backing_reservation(reservation)?;
        }
        if let Some(audio) = lease.audio {
            self.with_audio(audio.store, |_, store| store.release_export(audio.export))?;
        }
        if let Some(midi) = lease.shared_midi {
            self.with_midi(midi.store, |_, store| store.release_export(midi.export))?;
        }
        if let Some(take) = lease.indexed_midi {
            self.with_indexed_midi(take.store, |_, pool| pool.release(take.media))?;
        }
        if let Some(page) = lease.private_tail {
            self.retired_pages.push(page);
        }
        self.unreference(lease.version)?;
        release(
            &mut self.snapshots,
            &mut self.free_snapshots,
            id.slot as usize,
        );
        Ok(())
    }
    /// Recycle at most the remaining maintenance/validation step budget. A large
    /// revision's last release never walks or frees its entire page chain.
    pub fn reclaim(&mut self) -> usize {
        let mut steps = 0;
        while self.steps_remaining() > 0 {
            let mut advanced = false;
            for _ in 0..4 {
                let kind = self.maintenance_kind;
                self.maintenance_kind = (kind + 1) % 4;
                advanced = match kind {
                    0 => self.maintain_audio(),
                    1 => self.maintain_midi(),
                    2 => self.maintain_indexed_midi(),
                    _ => self.reclaim_linear_step(),
                };
                if advanced {
                    steps += 1;
                    break;
                }
            }
            if !advanced {
                break;
            }
        }
        steps
    }
    fn reclaim_linear_step(&mut self) -> bool {
        if let Some(page) = self.retired_pages.pop() {
            self.pages[page] = Page::default();
            self.free_pages.push(page);
            self.work.reclaimed_pages += 1;
        } else if let Some(&index) = self.retired_versions.last() {
            let Some(mut version) = self.versions[index].value else {
                return false;
            };
            if let Some(page) = version.head {
                version.head = self.pages[page].next;
                self.pages[page] = Page::default();
                self.free_pages.push(page);
                self.work.reclaimed_pages += 1;
                self.versions[index].value = Some(version);
            } else {
                self.reserved_pages -= version.reserved;
                self.retired_versions.pop();
                release(&mut self.versions, &mut self.free_versions, index);
                self.work.reclaimed_versions += 1;
            }
        } else {
            return false;
        }
        self.steps_remaining -= 1;
        self.work.validation_steps += 1;
        true
    }
}
