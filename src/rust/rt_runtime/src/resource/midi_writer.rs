//! Complete shared MIDI versions after each successful fixed tick.

use super::{
    midi_packets::{self, Packets, RecordId},
    RenderIo, ResourceStore, WorkClass,
};
use crate::{
    buffer::MidiBuffer,
    midi::Message,
    midi_state::{
        indexed::{
            self, store,
            tree::{Lookup, Totals},
            Cut, Event, Timeline,
        },
        interval::{HoldSummary, Point, SostenutoSummary},
        MidiState,
    },
    Error, Frame, Quantum, Revision,
};
use alloc::{boxed::Box, vec, vec::Vec};
use core::num::NonZeroU32;

#[derive(Clone, Copy)]
pub(super) struct Config {
    pub packets: midi_packets::Config,
    pub indexes: store::Config,
    pub events_per_tick: usize,
    pub bytes_per_tick: usize,
}
pub(super) struct Staging {
    records: Box<[Option<RecordId>]>,
    values: Box<[NonZeroU32]>,
    count: usize,
    frame: Frame,
}
impl Staging {
    pub fn prepare(events: usize) -> Result<Self, Error> {
        if events == 0 {
            return Err(Error::InvalidConfig);
        }
        events
            .checked_mul(size_of::<Option<RecordId>>() + size_of::<NonZeroU32>())
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let mut records = vec![None; events].into_boxed_slice();
        for record in &mut records {
            *core::hint::black_box(record) = None;
        }
        let mut values = vec![NonZeroU32::new(1).unwrap(); events].into_boxed_slice();
        for value in &mut values {
            *core::hint::black_box(value) = NonZeroU32::new(1).unwrap();
        }
        Ok(Self {
            records,
            values,
            count: 0,
            frame: Frame(0),
        })
    }
}
pub(super) struct Media {
    root: store::Version,
    origin: Frame,
    pub frames: u64,
    pub initial: MidiState,
    pub revision: Revision,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Fixed,
    Growing { maximum_frames: u64 },
}
pub(super) struct Admission {
    media: Media,
    kind: Kind,
    after: Cut,
    end: Cut,
    quantum: Option<u64>,
    density: usize,
    bytes: usize,
    ready: bool,
}
pub(super) struct Writer {
    media: Media,
    kind: Kind,
    pub position: u64,
}
#[derive(Clone, Copy)]
pub(super) struct Packet {
    value: NonZeroU32,
    pub frame: u64,
    pub order: u64,
    pub bytes: usize,
}
pub(super) struct Core {
    indexes: store::Store<NonZeroU32>,
    packets: Packets,
    config: Config,
    q: usize,
    removals: Vec<NonZeroU32>,
    insertions: Box<[NonZeroU32]>,
    records: Box<[Option<RecordId>]>,
}
impl Core {
    pub fn prepare(
        resources: &mut ResourceStore,
        q: Quantum,
        config: Config,
    ) -> Result<Self, Error> {
        if config.events_per_tick == 0 || config.bytes_per_tick == 0 || resources.quantum != q {
            return Err(Error::InvalidConfig);
        }
        config
            .events_per_tick
            .checked_mul(size_of::<NonZeroU32>() * 2 + size_of::<Option<RecordId>>())
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let indexes = store::Store::prepare(config.indexes)?;
        let packets = Packets::prepare(resources, config.packets)?;
        let mut removals = vec![NonZeroU32::new(1).unwrap(); config.events_per_tick];
        for value in &mut removals {
            *core::hint::black_box(value) = NonZeroU32::new(1).unwrap();
        }
        removals.clear();
        let mut insertions =
            vec![NonZeroU32::new(1).unwrap(); config.events_per_tick].into_boxed_slice();
        for value in &mut insertions {
            *core::hint::black_box(value) = NonZeroU32::new(1).unwrap();
        }
        let mut records = vec![None; config.events_per_tick].into_boxed_slice();
        for record in &mut records {
            *core::hint::black_box(record) = None;
        }
        Ok(Self {
            indexes,
            packets,
            config,
            q: q.frames(),
            removals,
            insertions,
            records,
        })
    }
    fn account(&self, io: &mut RenderIo<'_>) {
        let (pages, trees) = self.indexes.work();
        let mut overflow = pages.counter_overflow || trees.iter().any(|tree| tree.counter_overflow);
        let mut steps = 0usize;
        for amount in [
            pages.table_copies.saturating_mul(66),
            pages.category_copies.saturating_mul(128),
            pages.root_retains,
            pages.retired_tables.saturating_mul(66),
            pages.retired_categories.saturating_mul(128),
        ]
        .into_iter()
        .chain(
            trees
                .into_iter()
                .flat_map(|tree| [tree.visited, tree.copied, tree.rotations, tree.reclaimed]),
        ) {
            match steps.checked_add(amount) {
                Some(next) => steps = next,
                None => {
                    overflow = true;
                    steps = usize::MAX;
                }
            }
        }
        io.store
            .charge_work(WorkClass::Render, 0, steps)
            .expect("required index accounting");
        io.store.render_work.counter_overflow |= overflow;
    }
    pub fn empty(
        &mut self,
        origin: Frame,
        frames: u64,
        initial: &MidiState,
    ) -> Result<Media, Error> {
        origin.0.checked_add(frames).ok_or(Error::InvalidEvent)?;
        Ok(Media {
            root: self.indexes.empty()?,
            origin,
            frames,
            initial: *initial,
            revision: Revision(0),
        })
    }
    pub fn retain(&mut self, media: &Media) -> Result<Media, Error> {
        Ok(Media {
            root: self.indexes.retain(media.root)?,
            origin: media.origin,
            frames: media.frames,
            initial: media.initial,
            revision: media.revision,
        })
    }
    pub fn release(&mut self, media: Media) -> Result<(), Error> {
        self.indexes.release(media.root)
    }
    pub fn stage(
        &mut self,
        io: &mut RenderIo<'_>,
        staged: &mut Staging,
        frame: Frame,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        if staged.count != 0 {
            return Err(Error::Busy);
        }
        if input.len() > self.config.events_per_tick
            || input.len() > staged.records.len()
            || input.byte_len() > self.config.bytes_per_tick
            || input.len() > self.packets.available_records()
        {
            return Err(Error::Capacity);
        }
        for i in 0..input.len() {
            let (offset, bytes) = input.get(i).unwrap();
            if offset as usize >= self.q || frame.0.checked_add(u64::from(offset)).is_none() {
                return Err(Error::InvalidEvent);
            }
            Message::parse(bytes, self.config.packets.max_packet)?;
        }
        self.packets
            .preflight_append((0..input.len()).map(|i| input.get(i).unwrap().1.len()))?;
        staged.frame = frame;
        for i in 0..input.len() {
            let (offset, bytes) = input.get(i).unwrap();
            let body = self.packets.append(io, bytes)?;
            let record = self
                .packets
                .make_record(
                    body,
                    Point {
                        frame: Frame(frame.0 + u64::from(offset)),
                        order: i as u64,
                    },
                )
                .expect("preflighted staged record");
            self.packets.release_body(body).unwrap();
            staged.values[i] = self.packets.value(record).unwrap();
            staged.records[i] = Some(record);
            staged.count += 1;
        }
        Ok(())
    }
    pub fn clear_stage(&mut self, staged: &mut Staging) -> Result<(), Error> {
        for record in &mut staged.records[..staged.count] {
            self.packets
                .release_record(record.take().ok_or(Error::InvalidState)?)?;
        }
        staged.count = 0;
        Ok(())
    }
    /// One packet per call supports bounded import jobs; the owned view is private.
    pub fn import_packet(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &mut Media,
        frame: u64,
        order: u64,
        bytes: &[u8],
    ) -> Result<(), Error> {
        if frame >= media.frames {
            return Err(Error::InvalidEvent);
        }
        if self.packets.available_records() == 0 {
            return Err(Error::Capacity);
        }
        self.indexes.reset_work();
        let result = (|| {
            let body = self.packets.append(io, bytes)?;
            let record = self
                .packets
                .make_record(
                    body,
                    Point {
                        frame: Frame(media.origin.0 + frame),
                        order,
                    },
                )
                .expect("reserved import record");
            self.packets.release_body(body).unwrap();
            let result = self.indexes.insert(
                media.root,
                self.packets.value(record).unwrap(),
                &mut self.packets,
            );
            self.packets.release_record(record).unwrap();
            media.root = result?;
            Ok(())
        })();
        self.account(io);
        result
    }
    pub fn begin_writer(&self, media: Media, kind: Kind) -> Result<Admission, (Error, Media)> {
        if let Err(error) = self.indexes.len(media.root) {
            return Err((error, media));
        }
        if media.frames % self.q as u64 != 0
            || matches!(kind, Kind::Fixed) && media.frames == 0
            || matches!(kind, Kind::Growing { maximum_frames } if maximum_frames < media.frames || media.origin.0.checked_add(maximum_frames).is_none())
        {
            return Err((Error::InvalidShape, media));
        }
        let after = Cut::frame(media.origin);
        let end = Cut::frame(Frame(match kind {
            Kind::Fixed => media.origin.0 + media.frames,
            Kind::Growing { .. } => u64::MAX,
        }));
        Ok(Admission {
            media,
            kind,
            after,
            end,
            quantum: None,
            density: 0,
            bytes: 0,
            ready: false,
        })
    }
    pub fn progress_writer(
        &mut self,
        io: &mut RenderIo<'_>,
        admission: &mut Admission,
        budget: usize,
    ) -> Result<bool, Error> {
        self.indexes.reset_work();
        let result = (|| {
            for _ in 0..budget {
                let Some(value) = self.indexes.first(
                    admission.media.root,
                    admission.after,
                    admission.end,
                    &self.packets,
                )?
                else {
                    admission.ready = true;
                    break;
                };
                let point = self.packets.point(value);
                let quantum = (point.frame.0 - admission.media.origin.0) / self.q as u64;
                admission.density = if admission.quantum == Some(quantum) {
                    admission.density + 1
                } else {
                    1
                };
                admission.bytes = if admission.quantum == Some(quantum) {
                    admission
                        .bytes
                        .checked_add(self.packets.packet_len(value))
                        .ok_or(Error::Capacity)?
                } else {
                    self.packets.packet_len(value)
                };
                admission.quantum = Some(quantum);
                if admission.density > self.config.events_per_tick
                    || admission.bytes > self.config.bytes_per_tick
                    || self.packets.packet_len(value) > self.config.packets.max_packet
                {
                    return Err(Error::Capacity);
                }
                admission.after = Cut::at(point);
            }
            Ok(admission.ready)
        })();
        self.account(io);
        result
    }
    pub fn cancel_writer(&mut self, admission: Admission) -> Result<(), Error> {
        self.release(admission.media)
    }
    pub fn finish_admission(
        &self,
        admission: Admission,
        position: u64,
    ) -> Result<Writer, (Error, Admission)> {
        if !admission.ready {
            return Err((Error::Busy, admission));
        }
        if position % self.q as u64 != 0
            || match admission.kind {
                Kind::Fixed => position >= admission.media.frames,
                Kind::Growing { .. } => position != admission.media.frames,
            }
        {
            return Err((Error::InvalidEvent, admission));
        }
        Ok(Writer {
            media: admission.media,
            kind: admission.kind,
            position,
        })
    }
    pub fn snapshot(&mut self, writer: &Writer) -> Result<Media, Error> {
        self.retain(&writer.media)
    }
    pub fn finish_writer(&self, writer: Writer) -> Media {
        writer.media
    }
    pub fn write_tick(
        &mut self,
        io: &mut RenderIo<'_>,
        writer: &mut Writer,
        before_input: &MidiState,
        staged: &Staging,
    ) -> Result<(), Error> {
        self.indexes.reset_work();
        let logical_zero = writer.position == 0;
        let result = self.write_inner(writer, before_input, staged);
        if result.is_ok() && logical_zero {
            io.store
                .charge_work(WorkClass::Render, size_of::<MidiState>(), 0)?;
        }
        self.account(io);
        result
    }
    fn write_inner(
        &mut self,
        writer: &mut Writer,
        before_input: &MidiState,
        staged: &Staging,
    ) -> Result<(), Error> {
        let next = writer
            .position
            .checked_add(self.q as u64)
            .ok_or(Error::InvalidEvent)?;
        let frames = match writer.kind {
            Kind::Fixed => writer.media.frames,
            Kind::Growing { maximum_frames } if next <= maximum_frames => next,
            Kind::Growing { .. } => return Err(Error::Capacity),
        };
        let revision = writer
            .media
            .revision
            .0
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        let begin = writer.media.origin.0 + writer.position;
        let end = begin
            .checked_add(self.q as u64)
            .ok_or(Error::InvalidEvent)?;
        self.removals.clear();
        let mut after = Cut::frame(Frame(begin));
        while let Some(value) = self.indexes.first(
            writer.media.root,
            after,
            Cut::frame(Frame(end)),
            &self.packets,
        )? {
            if self.removals.len() == self.config.events_per_tick {
                return Err(Error::Capacity);
            }
            self.removals.push(value);
            after = Cut::at(self.packets.point(value));
        }
        if staged.count > self.packets.available_records() {
            return Err(Error::Capacity);
        }
        for i in 0..staged.count {
            self.packets
                .value(staged.records[i].ok_or(Error::InvalidId)?)?;
            let point = self.packets.point(staged.values[i]);
            if point.frame.0 < staged.frame.0 || point.frame.0 - staged.frame.0 >= self.q as u64 {
                return Err(Error::InvalidEvent);
            }
        }
        for i in 0..staged.count {
            let offset = self.packets.point(staged.values[i]).frame.0 - staged.frame.0;
            let record = self
                .packets
                .fork_record(
                    staged.values[i],
                    Point {
                        frame: Frame(begin + offset),
                        order: i as u64,
                    },
                )
                .expect("preflighted writer record");
            self.insertions[i] = self.packets.value(record).unwrap();
            self.records[i] = Some(record);
        }
        let result = self.indexes.replace_records(
            writer.media.root,
            &self.removals,
            &self.insertions[..staged.count],
            &mut self.packets,
        );
        for record in &mut self.records[..staged.count] {
            self.packets.release_record(record.take().unwrap()).unwrap();
        }
        writer.media.root = result?;
        if writer.position == 0 {
            writer.media.initial = *before_input;
        }
        writer.media.frames = frames;
        writer.media.revision = Revision(revision);
        writer.position = if next == frames && matches!(writer.kind, Kind::Fixed) {
            0
        } else {
            next
        };
        Ok(())
    }
    pub fn totals(&mut self, io: &mut RenderIo<'_>, media: &Media) -> Result<Totals, Error> {
        self.indexes.reset_work();
        let result = self.indexes.totals(
            media.root,
            Cut::frame(media.origin),
            Cut::frame(Frame(media.origin.0 + media.frames)),
            &self.packets,
        );
        self.account(io);
        result
    }
    pub fn first(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &Media,
        after: Option<Packet>,
    ) -> Result<Option<Packet>, Error> {
        self.indexes.reset_work();
        let after = after.map_or(Cut::frame(media.origin), |packet| {
            Cut::at(Point {
                frame: Frame(media.origin.0 + packet.frame),
                order: packet.order,
            })
        });
        let value = self.indexes.first(
            media.root,
            after,
            Cut::frame(Frame(media.origin.0 + media.frames)),
            &self.packets,
        );
        self.account(io);
        Ok(value?.map(|value| {
            let point = self.packets.point(value);
            Packet {
                value,
                frame: point.frame.0 - media.origin.0,
                order: point.order,
                bytes: self.packets.packet_len(value),
            }
        }))
    }
    pub fn read_packet(
        &self,
        io: &mut RenderIo<'_>,
        packet: Packet,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        self.packets.read(io, packet.value, offset, output)
    }
    pub fn state(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &Media,
        frame: u64,
    ) -> Result<MidiState, Error> {
        if frame > media.frames {
            return Err(Error::InvalidEvent);
        }
        self.indexes.reset_work();
        let timeline = Window {
            inner: self.indexes.timeline(media.root, &self.packets)?,
            origin: media.origin.0,
            frames: media.frames,
        };
        let result = indexed::state(&timeline, &media.initial, Cut::frame(Frame(frame)));
        self.account(io);
        Ok(result)
    }
    pub fn maintain(&mut self, io: &mut RenderIo<'_>, budget: usize) -> Result<usize, Error> {
        self.indexes.reset_work();
        let mut work = 0;
        for _ in 0..budget {
            let indexes = self.indexes.maintain(1, &mut self.packets);
            let packets = self.packets.maintain(io, 1)?;
            if indexes == 0 && packets == 0 {
                break;
            }
            work += 1;
        }
        self.account(io);
        Ok(work)
    }
    pub fn begin_cleanup(&mut self, io: &mut RenderIo<'_>) -> Result<(), Error> {
        self.packets.begin_cleanup(io)
    }
    pub fn finish_cleanup(&mut self, resources: &mut ResourceStore) -> Result<(), Error> {
        if self.indexes.leases() != 0 {
            return Err(Error::Busy);
        }
        self.packets.finish_cleanup(resources)
    }
}
struct Window<T> {
    inner: T,
    origin: u64,
    frames: u64,
}
impl<T: Timeline> Window<T> {
    fn shift(&self, cut: Cut) -> Cut {
        cut.min(Cut::frame(Frame(self.frames)))
            .shifted(self.origin)
            .expect("validated view range")
    }
    fn local(&self, event: Event) -> Event {
        event.packet().at(Point {
            frame: Frame(event.point.frame.0 - self.origin),
            order: event.point.order,
        })
    }
}
impl<T: Timeline> Timeline for Window<T> {
    fn last(&self, key: indexed::Key, cut: Cut) -> Option<Event> {
        self.inner
            .last(key, self.shift(cut))
            .filter(|event| event.point.frame.0 >= self.origin)
            .map(|event| self.local(event))
    }
    fn first(&self, key: indexed::Key, after: Cut, cut: Cut) -> Option<Event> {
        self.inner
            .first(key, self.shift(after), self.shift(cut))
            .map(|event| self.local(event))
    }
    fn holds(&self, ch: u8, after: Cut, cut: Cut) -> HoldSummary {
        self.inner.holds(ch, self.shift(after), self.shift(cut))
    }
    fn sostenuto(&self, ch: u8, after: Cut, cut: Cut) -> SostenutoSummary {
        self.inner
            .sostenuto(ch, self.shift(after), self.shift(cut))
            .map(|point| Point {
                frame: Frame(point.frame.0 - self.origin),
                order: point.order,
            })
    }
}

#[cfg(test)]
mod tests;
