//! Single-owner fixed-page resource storage. All allocation and final destruction
//! happen at preparation/host teardown; rendering uses fixed pools and bounded copies.
//! Raw resource/snapshot IDs require same-store caller discipline. Scoped indexes
//! additionally check instance and lease generation; copying either acquires no lease.

mod boundary;
pub use boundary::{CaptureBoundaryIo, PlannerResourceIo};
mod capture;
mod format;
mod group;
use group::CapturePool;
pub use group::{
    CaptureGroupConfig, CaptureGroupId, CaptureGroupInfo, CaptureGroupUsage, CaptureIntervalId,
    CaptureMember, CapturePhase, CaptureTakeId,
};
mod index;
mod midi_index;
mod midi_read;
pub use index::{IndexWork, ReadIndex};
use index::{PageIndex, VersionIndex};
pub use midi_index::{MidiIndexConfig, MidiIndexInfo, MidiIndexUsage, MidiIndexWork};
use midi_index::{MidiOwner, MidiPool};
pub use midi_read::{MidiCursor, MidiEvent, MidiPosition};
mod render;
mod request;
pub use capture::{CaptureId, CaptureInfo, ResourceInfo};
use render::WorkClass;
pub use render::{RenderIo, RenderWork};
pub use request::{Reply, Request, ResourceCommand, ResourceIo};
#[cfg(test)]
mod tests;
use crate::{Error, Frame, PreparedConfig, ResourceId, Revision};
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
}
#[derive(Clone, Copy)]
struct Lease {
    snapshot: Snapshot,
    version: UploadId,
    private_tail: Option<usize>,
    tail_start: usize,
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

pub struct ResourceStore {
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
}
fn retirement_stack(count: usize) -> Vec<usize> {
    let mut stack = vec![0; count];
    for index in &mut stack {
        *std::hint::black_box(index) = 0;
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
    pub fn new(config: PreparedConfig) -> Result<Self, Error> {
        let limits = config.limits();
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
            *std::hint::black_box(byte) = 0;
        }
        let mut pages = vec![Page::default(); limits.resource_pages];
        for page in &mut pages {
            *std::hint::black_box(page) = Page::default();
        }
        Ok(Self {
            scope,
            config,
            data,
            pages,
            free_pages: (0..limits.resource_pages).rev().collect(),
            reserved_pages: 0,
            resources: slots(limits.resource_slots),
            free_resources: (0..limits.resource_slots).rev().collect(),
            versions: slots(limits.resource_slots),
            free_versions: (0..limits.resource_slots).rev().collect(),
            snapshots: slots(limits.resource_slots),
            free_snapshots: (0..limits.resource_slots).rev().collect(),
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
        self.copy_remaining = self.config.limits().budgets.transfer_bytes;
        self.steps_remaining = self.config.limits().budgets.validation_steps;
        self.work = Work::default();
        self.render_work = RenderWork::default();
        self.index_work = IndexWork::default();
        self.midi_indexes.work = MidiIndexWork::default();
        self.control_phase = false;
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
            version.state = State::Retiring;
            if let Some(owner) = version.midi.take() {
                self.midi_indexes.release(owner);
            }
            self.retired_versions.push(id.slot as usize);
        }
        self.versions[id.slot as usize].value = Some(version);
        Ok(())
    }

    pub fn snapshot(&mut self, resource: ResourceId, cut_frame: Frame) -> Result<Snapshot, Error> {
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
        if self.free_snapshots.is_empty() {
            return Err(Error::Capacity);
        }
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
        let index = self.free_snapshots.pop().ok_or(Error::InvalidState)?;
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
            snapshot,
            version: active,
            private_tail,
            tail_start: version.written / self.config.limits().page_bytes
                * self.config.limits().page_bytes,
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
            if let Some(page) = self.retired_pages.pop() {
                self.pages[page] = Page::default();
                self.free_pages.push(page);
                self.work.reclaimed_pages += 1;
            } else if let Some(&index) = self.retired_versions.last() {
                let Some(mut version) = self.versions[index].value else {
                    break;
                };
                if let Some(page) = version.head {
                    version.head = self.pages[page].next;
                    self.pages[page] = Page::default();
                    self.free_pages.push(page);
                    self.work.reclaimed_pages += 1;
                    self.versions[index].value = Some(version);
                } else {
                    self.retired_versions.pop();
                    release(&mut self.versions, &mut self.free_versions, index);
                    self.work.reclaimed_versions += 1;
                }
            } else {
                break;
            }
            self.steps_remaining -= 1;
            self.work.validation_steps += 1;
            steps += 1;
        }
        steps
    }
}
