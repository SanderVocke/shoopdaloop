//! Immutable packet ranges in append-only slabs shared by ordered MIDI records.

use super::{index, Page, RenderIo, ResourceStore, WorkClass};
use crate::{
    midi::Message,
    midi_state::{
        indexed::{
            tree::{Lookup, Values},
            Event, StatePacket,
        },
        interval::Point,
    },
    Error,
};
use alloc::{vec, vec::Vec};
use core::num::NonZeroU32;

#[derive(Clone, Copy)]
pub(super) struct Config {
    pub pages: usize,
    pub bodies: usize,
    pub records: usize,
    pub max_packet: usize,
    pub bases: usize,
}
impl Config {
    pub fn metadata_bytes(self) -> Result<usize, Error> {
        if [self.pages, self.bodies, self.records, self.max_packet].contains(&0) {
            return Err(Error::InvalidConfig);
        }
        self.pages
            .checked_mul(size_of::<Option<Slab>>() + size_of::<NonZeroU32>())
            .and_then(|bytes| {
                self.bodies
                    .checked_mul(size_of::<Slot<Body>>() + 2 * size_of::<NonZeroU32>())
                    .and_then(|more| bytes.checked_add(more))
            })
            .and_then(|bytes| {
                self.records
                    .checked_mul(size_of::<Slot<Record>>() + 2 * size_of::<NonZeroU32>())
                    .and_then(|more| bytes.checked_add(more))
            })
            .and_then(|bytes| {
                self.bases
                    .checked_mul(size_of::<Slot<Base>>() + 2 * size_of::<NonZeroU32>())
                    .and_then(|more| bytes.checked_add(more))
            })
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Handle {
    scope: u32,
    slot: NonZeroU32,
    generation: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BodyId(Handle);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecordId(Handle);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BaseId(Handle);
#[derive(Clone, Copy)]
struct Base {
    snapshot: super::Snapshot,
    index: super::ReadIndex,
    references: u32,
}
#[derive(Clone, Copy)]
struct Slab {
    physical: usize,
    next: Option<NonZeroU32>,
    used: usize,
    references: u32,
}
#[derive(Clone, Copy)]
struct Body {
    first: NonZeroU32,
    offset: usize,
    bytes: usize,
    references: u32,
    packet: Option<StatePacket>,
    base: Option<NonZeroU32>,
}
#[derive(Clone, Copy)]
struct Record {
    point: Point,
    packet: Option<StatePacket>,
    body: NonZeroU32,
    references: u32,
}
#[derive(Clone, Copy)]
struct Slot<T> {
    generation: u64,
    value: Option<T>,
}
pub(super) struct Packets {
    scope: u32,
    store_scope: u32,
    config: Config,
    page_bytes: usize,
    reserved: usize,
    closed: bool,
    tail: Option<NonZeroU32>,
    slabs: Vec<Option<Slab>>,
    free_slabs: Vec<NonZeroU32>,
    bodies: Vec<Slot<Body>>,
    free_bodies: Vec<NonZeroU32>,
    retired_bodies: Vec<NonZeroU32>,
    records: Vec<Slot<Record>>,
    body_count: usize,
    record_count: usize,
    free_records: Vec<NonZeroU32>,
    retired_records: Vec<NonZeroU32>,
    bases: Vec<Slot<Base>>,
    free_bases: Vec<NonZeroU32>,
    retired_bases: Vec<NonZeroU32>,
    base_count: usize,
    cursor: u8,
}
fn slot(id: NonZeroU32) -> usize {
    (id.get() - 1) as usize
}
fn free_ids(count: usize) -> Vec<NonZeroU32> {
    (1..=count as u32)
        .rev()
        .map(|id| NonZeroU32::new(id).unwrap())
        .collect()
}
fn slots<T: Copy>(count: usize) -> Vec<Slot<T>> {
    let mut slots = vec![
        Slot {
            generation: 1,
            value: None
        };
        count
    ];
    for entry in &mut slots {
        *core::hint::black_box(entry) = Slot {
            generation: 1,
            value: None,
        };
    }
    slots
}
fn retired(count: usize) -> Vec<NonZeroU32> {
    let mut queue = vec![NonZeroU32::new(1).unwrap(); count];
    for id in &mut queue {
        *core::hint::black_box(id) = NonZeroU32::new(1).unwrap();
    }
    queue.clear();
    queue
}
impl Packets {
    pub fn prepare(resources: &mut ResourceStore, config: Config) -> Result<Self, Error> {
        config.metadata_bytes()?;
        if config.pages > resources.free_pages.len() - resources.reserved_pages {
            return Err(Error::Capacity);
        }
        let scope = index::new_scope()?;
        let mut slabs = vec![None; config.pages];
        for entry in &mut slabs {
            *core::hint::black_box(entry) = None;
        }
        let result = Self {
            scope,
            store_scope: resources.scope,
            config,
            page_bytes: resources.config.limits().page_bytes,
            reserved: config.pages,
            closed: false,
            tail: None,
            slabs,
            free_slabs: free_ids(config.pages),
            bodies: slots(config.bodies),
            free_bodies: free_ids(config.bodies),
            retired_bodies: retired(config.bodies),
            records: slots(config.records),
            body_count: 0,
            record_count: 0,
            free_records: free_ids(config.records),
            retired_records: retired(config.records),
            bases: slots(config.bases),
            free_bases: free_ids(config.bases),
            retired_bases: retired(config.bases),
            base_count: 0,
            cursor: 0,
        };
        resources.reserved_pages += config.pages;
        Ok(result)
    }
    fn check_store(&self, resources: &ResourceStore) -> Result<(), Error> {
        if self.store_scope != resources.scope {
            return Err(Error::InvalidId);
        }
        Ok(())
    }
    fn body(&self, id: BodyId) -> Result<Body, Error> {
        let entry = self.bodies.get(slot(id.0.slot)).ok_or(Error::InvalidId)?;
        if id.0.scope != self.scope || id.0.generation != entry.generation {
            return Err(Error::InvalidId);
        }
        entry
            .value
            .filter(|body| body.references > 0)
            .ok_or(Error::InvalidId)
    }
    fn record(&self, id: RecordId) -> Result<Record, Error> {
        let entry = self.records.get(slot(id.0.slot)).ok_or(Error::InvalidId)?;
        if id.0.scope != self.scope || id.0.generation != entry.generation {
            return Err(Error::InvalidId);
        }
        entry
            .value
            .filter(|record| record.references > 0)
            .ok_or(Error::InvalidId)
    }
    fn body_id(&self, id: NonZeroU32) -> BodyId {
        BodyId(Handle {
            scope: self.scope,
            slot: id,
            generation: self.bodies[slot(id)].generation,
        })
    }
    pub fn value(&self, id: RecordId) -> Result<NonZeroU32, Error> {
        self.record(id)?;
        Ok(id.0.slot)
    }
    fn base(&self, id: BaseId) -> Result<Base, Error> {
        let entry = self.bases.get(slot(id.0.slot)).ok_or(Error::InvalidId)?;
        if id.0.scope != self.scope || id.0.generation != entry.generation {
            return Err(Error::InvalidId);
        }
        entry
            .value
            .filter(|base| base.references > 0)
            .ok_or(Error::InvalidId)
    }
    pub fn pin_base(
        &mut self,
        resources: &mut ResourceStore,
        snapshot: super::SnapshotId,
    ) -> Result<BaseId, Error> {
        self.check_store(resources)?;
        if self.closed {
            return Err(Error::InvalidState);
        }
        let id = *self.free_bases.last().ok_or(Error::Capacity)?;
        let lease = resources.index_lease(resources.read_index(snapshot)?)?;
        if lease.audio.is_some()
            || lease.shared_midi.is_some()
            || lease.snapshot.description.format != super::Format::Midi
        {
            return Err(Error::InvalidShape);
        }
        let snapshot = resources.retain_snapshot(snapshot)?;
        let index = resources.read_index(snapshot.id)?;
        self.free_bases.pop();
        self.base_count += 1;
        self.bases[slot(id)].value = Some(Base {
            snapshot,
            index,
            references: 1,
        });
        Ok(BaseId(Handle {
            scope: self.scope,
            slot: id,
            generation: self.bases[slot(id)].generation,
        }))
    }
    pub fn release_base(&mut self, id: BaseId) -> Result<(), Error> {
        self.base(id)?;
        self.release_base_slot(id.0.slot);
        Ok(())
    }
    fn release_base_slot(&mut self, id: NonZeroU32) {
        let base = self.bases[slot(id)].value.as_mut().unwrap();
        base.references -= 1;
        if base.references == 0 {
            self.retired_bases.push(id);
        }
    }
    /// The caller supplies a validated record boundary from this immutable body.
    pub fn reference_packet(
        &mut self,
        io: &mut RenderIo<'_>,
        id: BaseId,
        offset: usize,
        bytes: usize,
    ) -> Result<BodyId, Error> {
        self.check_store(io.store)?;
        if self.closed {
            return Err(Error::InvalidState);
        }
        let base = self.base(id)?;
        if bytes == 0
            || offset
                .checked_add(bytes)
                .is_none_or(|end| end > base.snapshot.description.bytes)
        {
            return Err(Error::InvalidOffset);
        }
        if bytes > self.config.max_packet || self.free_bodies.is_empty() {
            return Err(Error::Capacity);
        }
        let references = base.references.checked_add(1).ok_or(Error::Exhausted)?;
        let mut prefix = [0; 3];
        let count = bytes.min(3);
        io.store.charge_work(WorkClass::Render, count, 0)?;
        io.store
            .copy_linear_span(base.index, offset, &mut prefix[..count], WorkClass::Render)?;
        let packet = if matches!(prefix[0], 0x80..=0xef | 0xff) {
            Event::from_message(
                Point {
                    frame: crate::Frame(0),
                    order: 0,
                },
                Message::parse(&prefix[..count], 3)?,
            )
            .map(Event::packet)
        } else {
            None
        };
        let body = self.free_bodies.pop().unwrap();
        self.bodies[slot(body)].value = Some(Body {
            first: NonZeroU32::new(1).unwrap(),
            offset,
            bytes,
            references: 1,
            packet,
            base: Some(id.0.slot),
        });
        self.bases[slot(id.0.slot)]
            .value
            .as_mut()
            .unwrap()
            .references = references;
        self.body_count += 1;
        Ok(self.body_id(body))
    }
    pub fn available_records(&self) -> usize {
        self.free_records.len()
    }
    pub fn apply(&self, value: NonZeroU32, state: &mut crate::midi_state::MidiState) {
        if let Some(packet) = self.records[slot(value)]
            .value
            .expect("owned ordered record")
            .packet
        {
            packet.apply(state);
        }
    }
    pub fn fork_record(&mut self, value: NonZeroU32, point: Point) -> Result<RecordId, Error> {
        let body = self.records[slot(value)]
            .value
            .ok_or(Error::InvalidId)?
            .body;
        self.make_record(self.body_id(body), point)
    }
    pub fn available_bodies(&self) -> usize {
        self.free_bodies.len()
    }
    pub fn available_pages(&self) -> usize {
        self.free_slabs.len()
    }
    pub fn preflight_append(&self, lengths: impl IntoIterator<Item = usize>) -> Result<(), Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        let mut bytes = 0usize;
        let mut bodies = 0usize;
        for len in lengths {
            if len == 0 {
                return Err(Error::InvalidEvent);
            }
            if len > self.config.max_packet {
                return Err(Error::Capacity);
            }
            bytes = bytes.checked_add(len).ok_or(Error::Capacity)?;
            bodies = bodies.checked_add(1).ok_or(Error::Capacity)?;
        }
        let tail_space = self
            .tail
            .map_or(0, |id| self.page_bytes - self.slabs[slot(id)].unwrap().used);
        let pages = bytes.saturating_sub(tail_space).div_ceil(self.page_bytes);
        if pages > self.free_slabs.len() || bodies > self.free_bodies.len() {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    fn allocate_slab(&mut self, resources: &mut ResourceStore) -> NonZeroU32 {
        let id = self.free_slabs.pop().expect("preflighted packet slab");
        self.reserved -= 1;
        resources.reserved_pages -= 1;
        let physical = resources
            .free_pages
            .pop()
            .expect("reserved physical packet page");
        resources.pages[physical] = Page {
            used: self.page_bytes,
            ..Page::default()
        };
        self.slabs[slot(id)] = Some(Slab {
            physical,
            next: None,
            used: 0,
            references: 1,
        });
        id
    }
    fn release_slab(&mut self, resources: &mut ResourceStore, id: NonZeroU32) {
        let slab = self.slabs[slot(id)].as_mut().expect("owned packet slab");
        slab.references -= 1;
        if slab.references == 0 {
            let physical = slab.physical;
            self.slabs[slot(id)] = None;
            self.free_slabs.push(id);
            resources.pages[physical] = Page::default();
            resources.free_pages.push(physical);
            self.reserved += 1;
            resources.reserved_pages += 1;
        }
    }
    pub fn append(&mut self, io: &mut RenderIo<'_>, bytes: &[u8]) -> Result<BodyId, Error> {
        self.check_store(io.store)?;
        let packet = Event::from_message(
            Point {
                frame: crate::Frame(0),
                order: 0,
            },
            Message::parse(bytes, self.config.max_packet)?,
        )
        .map(Event::packet);
        self.preflight_append([bytes.len()])?;
        io.store.note_operation(WorkClass::Render);
        io.store.charge_work(
            WorkClass::Render,
            bytes.len(),
            bytes.len().div_ceil(self.page_bytes) + 1,
        )?;
        let mut first = None;
        let mut first_offset = 0;
        let mut copied = 0;
        while copied < bytes.len() {
            let tail = match self.tail {
                Some(tail) if self.slabs[slot(tail)].unwrap().used < self.page_bytes => tail,
                old => {
                    let next = self.allocate_slab(io.store);
                    if let Some(old) = old {
                        self.slabs[slot(old)].as_mut().unwrap().next = Some(next);
                        self.release_slab(io.store, old);
                    }
                    self.tail = Some(next);
                    next
                }
            };
            let slab = self.slabs[slot(tail)].as_mut().unwrap();
            if first.is_none() {
                first = Some(tail);
                first_offset = slab.used;
            }
            let n = (bytes.len() - copied).min(self.page_bytes - slab.used);
            let offset = slab.physical * self.page_bytes + slab.used;
            io.store.data[offset..offset + n].copy_from_slice(&bytes[copied..copied + n]);
            slab.used += n;
            slab.references += 1;
            copied += n;
        }
        let id = self.free_bodies.pop().expect("preflighted body descriptor");
        self.bodies[slot(id)].value = Some(Body {
            first: first.unwrap(),
            offset: first_offset,
            bytes: bytes.len(),
            references: 1,
            packet,
            base: None,
        });
        self.body_count += 1;
        Ok(self.body_id(id))
    }
    pub fn make_record(&mut self, body: BodyId, point: Point) -> Result<RecordId, Error> {
        if self.closed {
            return Err(Error::InvalidState);
        }
        let value = self.body(body)?;
        let references = value.references.checked_add(1).ok_or(Error::Exhausted)?;
        let id = self.free_records.pop().ok_or(Error::Capacity)?;
        self.bodies[slot(body.0.slot)]
            .value
            .as_mut()
            .unwrap()
            .references = references;
        self.records[slot(id)].value = Some(Record {
            point,
            packet: value.packet,
            body: body.0.slot,
            references: 1,
        });
        self.record_count += 1;
        Ok(RecordId(Handle {
            scope: self.scope,
            slot: id,
            generation: self.records[slot(id)].generation,
        }))
    }
    fn release_body_slot(&mut self, id: NonZeroU32) {
        let body = self.bodies[slot(id)]
            .value
            .as_mut()
            .expect("owned packet body");
        body.references -= 1;
        if body.references == 0 {
            self.retired_bodies.push(id);
        }
    }
    pub fn release_body(&mut self, id: BodyId) -> Result<(), Error> {
        self.body(id)?;
        self.release_body_slot(id.0.slot);
        Ok(())
    }
    pub fn release_record(&mut self, id: RecordId) -> Result<(), Error> {
        self.record(id)?;
        self.release(id.0.slot);
        Ok(())
    }
    pub fn packet_len(&self, value: NonZeroU32) -> usize {
        let record = self.records[slot(value)]
            .value
            .expect("owned ordered record");
        self.bodies[slot(record.body)]
            .value
            .expect("owned packet body")
            .bytes
    }
    fn read_body(
        &self,
        io: &mut RenderIo<'_>,
        body: Body,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        self.check_store(io.store)?;
        if offset > body.bytes {
            return Err(Error::InvalidEvent);
        }
        let count = output.len().min(body.bytes - offset);
        if count == 0 {
            io.store.note_operation(WorkClass::Render);
            return Ok(0);
        }
        if let Some(base) = body.base {
            let base = self.bases[slot(base)]
                .value
                .expect("owned immutable MIDI base");
            io.store.note_operation(WorkClass::Render);
            io.store.charge_work(WorkClass::Render, count, 0)?;
            io.store.copy_linear_span(
                base.index,
                body.offset + offset,
                &mut output[..count],
                WorkClass::Render,
            )?;
            return Ok(count);
        }
        let mut skip = body.offset + offset;
        let mut id = body.first;
        let mut steps = 0;
        while skip >= self.page_bytes {
            id = self.slabs[slot(id)]
                .unwrap()
                .next
                .expect("owned packet continuation");
            skip -= self.page_bytes;
            steps += 1;
        }
        let mut copied = 0;
        while copied < count {
            let slab = self.slabs[slot(id)].expect("owned packet slab");
            let n = (count - copied).min(self.page_bytes - skip);
            let start = slab.physical * self.page_bytes + skip;
            output[copied..copied + n].copy_from_slice(&io.store.data[start..start + n]);
            copied += n;
            steps += 1;
            if copied < count {
                id = slab.next.expect("owned packet continuation");
                skip = 0;
            }
        }
        io.store.note_operation(WorkClass::Render);
        io.store.charge_work(WorkClass::Render, count, steps)?;
        Ok(count)
    }
    pub fn read(
        &self,
        io: &mut RenderIo<'_>,
        value: NonZeroU32,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        let record = self.records[slot(value)]
            .value
            .expect("owned ordered record");
        self.read_body(
            io,
            self.bodies[slot(record.body)]
                .value
                .expect("owned packet body"),
            offset,
            output,
        )
    }
    pub fn read_retained_body(
        &self,
        io: &mut RenderIo<'_>,
        id: BodyId,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        self.read_body(io, self.body(id)?, offset, output)
    }
    pub fn maintain(&mut self, io: &mut RenderIo<'_>, budget: usize) -> Result<usize, Error> {
        self.check_store(io.store)?;
        let mut reclaimed = 0;
        while reclaimed < budget {
            let records_first = self.cursor == 0;
            let bases_first = self.cursor == 2;
            self.cursor = (self.cursor + 1) % 3;
            if !self.retired_bases.is_empty()
                && (bases_first
                    || self.retired_records.is_empty() && self.retired_bodies.is_empty())
            {
                let id = self.retired_bases.pop().unwrap();
                let entry = &mut self.bases[slot(id)];
                let base = entry.value.take().unwrap();
                if let Some(generation) = entry.generation.checked_add(1) {
                    entry.generation = generation;
                    self.free_bases.push(id);
                }
                self.base_count -= 1;
                io.store.release_snapshot(base.snapshot.id)?;
            } else if !self.retired_records.is_empty()
                && (records_first || self.retired_bodies.is_empty())
            {
                let id = self.retired_records.pop().unwrap();
                let entry = &mut self.records[slot(id)];
                let record = entry.value.take().unwrap();
                self.record_count -= 1;
                if let Some(generation) = entry.generation.checked_add(1) {
                    entry.generation = generation;
                    self.free_records.push(id);
                }
                self.release_body_slot(record.body);
            } else if let Some(&id) = self.retired_bodies.last() {
                let body = self.bodies[slot(id)].value.unwrap();
                if let Some(base) = body.base {
                    self.retired_bodies.pop();
                    let entry = &mut self.bodies[slot(id)];
                    entry.value = None;
                    if let Some(generation) = entry.generation.checked_add(1) {
                        entry.generation = generation;
                        self.free_bodies.push(id);
                    }
                    self.body_count -= 1;
                    self.release_base_slot(base);
                    io.store.charge_work(WorkClass::Render, 0, 1)?;
                    reclaimed += 1;
                    continue;
                }
                let slab = self.slabs[slot(body.first)].unwrap();
                let bytes = body.bytes.min(self.page_bytes - body.offset);
                self.release_slab(io.store, body.first);
                let entry = &mut self.bodies[slot(id)];
                if bytes == body.bytes {
                    self.retired_bodies.pop();
                    entry.value = None;
                    self.body_count -= 1;
                    if let Some(generation) = entry.generation.checked_add(1) {
                        entry.generation = generation;
                        self.free_bodies.push(id);
                    }
                } else {
                    let body = entry.value.as_mut().unwrap();
                    body.first = slab.next.expect("owned retiring continuation");
                    body.offset = 0;
                    body.bytes -= bytes;
                }
            } else {
                break;
            }
            io.store.charge_work(WorkClass::Render, 0, 1)?;
            reclaimed += 1;
        }
        Ok(reclaimed)
    }
    pub fn begin_cleanup(&mut self, io: &mut RenderIo<'_>) -> Result<(), Error> {
        self.check_store(io.store)?;
        self.closed = true;
        if let Some(tail) = self.tail.take() {
            self.release_slab(io.store, tail);
            io.store.charge_work(WorkClass::Render, 0, 1)?;
        }
        Ok(())
    }
    pub fn finish_cleanup(&mut self, resources: &mut ResourceStore) -> Result<(), Error> {
        self.check_store(resources)?;
        if !self.closed || self.body_count != 0 || self.record_count != 0 || self.base_count != 0 {
            return Err(Error::Busy);
        }
        resources.reserved_pages -= self.reserved;
        self.reserved = 0;
        Ok(())
    }
}
impl Lookup<NonZeroU32> for Packets {
    fn packet_bytes(&self, value: NonZeroU32) -> usize {
        self.packet_len(value)
    }
    fn point(&self, value: NonZeroU32) -> Point {
        self.records[slot(value)]
            .value
            .expect("owned ordered record")
            .point
    }
    fn event(&self, value: NonZeroU32) -> Option<Event> {
        let record = self.records[slot(value)]
            .value
            .expect("owned ordered record");
        record.packet.map(|packet| packet.at(record.point))
    }
}
impl Values<NonZeroU32> for Packets {
    fn retain(&mut self, value: NonZeroU32) {
        let references = &mut self.records[slot(value)]
            .value
            .as_mut()
            .expect("owned ordered record")
            .references;
        *references = references
            .checked_add(1)
            .expect("prepared ordered-reference ceiling");
    }
    fn release(&mut self, value: NonZeroU32) {
        let references = &mut self.records[slot(value)]
            .value
            .as_mut()
            .expect("owned ordered record")
            .references;
        *references -= 1;
        if *references == 0 {
            self.retired_records.push(value);
        }
    }
}

#[cfg(test)]
mod tests;
