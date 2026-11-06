//! Complete shared MIDI versions after each successful fixed tick.

use super::{
    indexed_midi::WriteSpan,
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

pub(super) mod history;

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
    pub fn prepared_bytes(events: usize) -> Result<usize, Error> {
        if events == 0 {
            return Err(Error::InvalidConfig);
        }
        events
            .checked_mul(size_of::<Option<RecordId>>() + size_of::<NonZeroU32>())
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
    pub fn prepare(events: usize) -> Result<Self, Error> {
        Self::prepared_bytes(events)?;
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
    density_checked: bool,
    credit: Option<usize>,
    retained_events: usize,
    retained_pages: usize,
    categories: u128,
    retained_epoch: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Fixed,
    Windowed,
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
#[derive(Clone, Copy, Default)]
struct OwnerCredit {
    references: usize,
    events: usize,
    pages: usize,
    categories: usize,
    desired_events: usize,
    desired_pages: usize,
}
struct Credits {
    owners: Box<[OwnerCredit]>,
    events: usize,
    pages: usize,
    event_limit: usize,
    page_limit: usize,
    categories: usize,
    category_limit: usize,
}
pub(super) struct Core {
    indexes: store::Store<NonZeroU32>,
    packets: Packets,
    pub(super) config: Config,
    q: usize,
    credits: Option<Credits>,
    retirement_epoch: u64,
    removals: Vec<NonZeroU32>,
    insertions: Box<[NonZeroU32]>,
    records: Box<[Option<RecordId>]>,
}
impl Core {
    pub fn prepared_bytes(config: Config) -> Result<usize, Error> {
        let scratch = config
            .events_per_tick
            .checked_mul(size_of::<NonZeroU32>() * 3 + size_of::<Option<RecordId>>())
            .ok_or(Error::Capacity)?;
        config
            .packets
            .metadata_bytes()?
            .checked_add(config.indexes.prepared_bytes::<NonZeroU32>()?)
            .and_then(|bytes| bytes.checked_add(scratch))
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
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
            .checked_mul(size_of::<NonZeroU32>() * 3 + size_of::<Option<RecordId>>())
            .filter(|&bytes| bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let indexes = store::Store::prepare(config.indexes)?;
        let packets = Packets::prepare(resources, config.packets)?;
        let mut removals = vec![NonZeroU32::new(1).unwrap(); config.events_per_tick * 2];
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
            credits: None,
            retirement_epoch: 0,
            removals,
            insertions,
            records,
        })
    }
    pub fn owner_limits(
        config: Config,
        sources: usize,
        history_events: usize,
        history_bytes: usize,
        page_bytes: usize,
    ) -> Result<(usize, usize, usize), Error> {
        let live = sources.checked_mul(history_events).ok_or(Error::Capacity)?;
        let staged = sources
            .checked_add(1)
            .and_then(|n| n.checked_mul(config.events_per_tick))
            .ok_or(Error::Capacity)?;
        let path_workspace =
            super::indexed_midi::Config::index_workspace(config.events_per_tick, history_events)?;
        let mut limit = config
            .packets
            .records
            .min(config.packets.bodies)
            .checked_sub(live)
            .and_then(|n| n.checked_sub(staged))
            .ok_or(Error::Capacity)?;
        for (nodes, multiplicity) in [
            (config.indexes.packet_nodes, 1),
            (config.indexes.scalar_nodes, 2),
            (config.indexes.hold_nodes, 1),
            (config.indexes.sostenuto_nodes, 1),
        ] {
            let owner = nodes
                .checked_sub(
                    path_workspace
                        .checked_mul(multiplicity)
                        .ok_or(Error::Capacity)?,
                )
                .map(|n| n / multiplicity)
                .and_then(|n| n.checked_sub(live))
                .ok_or(Error::Capacity)?;
            limit = limit.min(owner);
        }
        let workspace_pages = config.bytes_per_tick.div_ceil(page_bytes) + 2;
        let source_pages = sources
            .checked_mul(history_bytes.div_ceil(page_bytes))
            .ok_or(Error::Capacity)?;
        let pages = config
            .packets
            .pages
            .checked_sub(source_pages)
            .and_then(|n| n.checked_sub(workspace_pages))
            .ok_or(Error::Capacity)?;
        let categories = config
            .indexes
            .category_pages
            .checked_sub(sources.checked_mul(66).ok_or(Error::Capacity)?)
            .and_then(|n| n.checked_sub(config.events_per_tick.saturating_mul(3).min(66)))
            .ok_or(Error::Capacity)?;
        Ok((limit, pages, categories))
    }
    fn packet_categories(&self, value: NonZeroU32) -> u128 {
        self.packets
            .event(value)
            .map_or([None; 3], Event::keys)
            .into_iter()
            .flatten()
            .fold(0u128, |mask, key| mask | (1u128 << (key.slot() / 128)))
    }
    pub fn prepare_owner_credits(
        &mut self,
        sources: usize,
        history_events: usize,
        history_bytes: usize,
    ) -> Result<(), Error> {
        let (event_limit, page_limit, category_limit) = Self::owner_limits(
            self.config,
            sources,
            history_events,
            history_bytes,
            self.packets.page_bytes(),
        )?;
        self.credits = Some(Credits {
            owners: vec![OwnerCredit::default(); self.config.indexes.leases].into_boxed_slice(),
            events: 0,
            pages: 0,
            categories: 0,
            event_limit,
            page_limit,
            category_limit,
        });
        Ok(())
    }
    pub fn owner_credit_bytes(leases: usize) -> Result<usize, Error> {
        leases
            .checked_mul(size_of::<OwnerCredit>())
            .ok_or(Error::Capacity)
    }
    fn owner_credit(
        &mut self,
        media: &mut Media,
        events: usize,
        pages: usize,
    ) -> Result<(), Error> {
        let Some(credits) = self.credits.as_mut() else {
            return Ok(());
        };
        let old = media
            .credit
            .map(|id| credits.owners[id])
            .unwrap_or_default();
        let events = events.max(old.events);
        let pages = pages.max(old.pages);
        let unique = old.references == 1;
        let total_events = credits
            .events
            .checked_sub(if unique { old.events } else { 0 })
            .and_then(|n| n.checked_add(events))
            .ok_or(Error::Capacity)?;
        let total_pages = credits
            .pages
            .checked_sub(if unique { old.pages } else { 0 })
            .and_then(|n| n.checked_add(pages))
            .ok_or(Error::Capacity)?;
        let total_categories = credits.categories - if unique { old.categories } else { 0 }
            + media.categories.count_ones() as usize;
        if total_events > credits.event_limit
            || total_pages > credits.page_limit
            || total_categories > credits.category_limit
        {
            return Err(Error::Capacity);
        }
        let id = if unique {
            media.credit.unwrap()
        } else {
            credits
                .owners
                .iter()
                .position(|c| {
                    c.references == 0 && c.events == 0 && c.pages == 0 && c.categories == 0
                })
                .ok_or(Error::Capacity)?
        };
        if let Some(old_id) = media.credit.filter(|_| !unique) {
            credits.owners[old_id].references -= 1;
        }
        credits.owners[id] = OwnerCredit {
            references: 1,
            events,
            pages,
            categories: media.categories.count_ones() as usize,
            desired_events: events
                .saturating_sub(media.retained_events)
                .max(old.desired_events),
            desired_pages: pages
                .saturating_sub(media.retained_pages)
                .max(old.desired_pages),
        };
        credits.events = total_events;
        credits.pages = total_pages;
        credits.categories = total_categories;
        media.credit = Some(id);
        Ok(())
    }
    fn settled_media(&self, media: &mut Media) {
        if media.retained_epoch != self.retirement_epoch {
            media.retained_events = 0;
            media.retained_pages = 0;
            media.retained_epoch = self.retirement_epoch;
        }
    }
    fn retain_credit(&mut self, media: &Media) {
        if let Some(id) = media.credit {
            self.credits.as_mut().unwrap().owners[id].references += 1;
        }
    }
    fn release_credit(&mut self, media: &Media) {
        if let Some(id) = media.credit {
            let credits = self.credits.as_mut().unwrap();
            let owner = &mut credits.owners[id];
            owner.references -= 1;
            // Keep debt charged until deferred roots, packets and slabs have retired.
        }
    }
    pub fn required_maintenance(&self, minimum: usize) -> usize {
        let height = 2 * self.config.indexes.packet_nodes.ilog2() as usize + 2;
        // Three semantic categories, packet paths, packet/body retirement and category tables.
        minimum.max(
            self.config
                .events_per_tick
                .saturating_mul(24 * (height + 1) + 16)
                .saturating_add(256),
        )
    }
    fn account(&self, io: &mut RenderIo<'_>) {
        let (pages, trees) = self.indexes.work();
        let mut overflow = pages.counter_overflow || trees.iter().any(|tree| tree.counter_overflow);
        let mut steps = 0usize;
        for amount in [
            pages.table_copies.saturating_mul(66),
            pages.category_copies.saturating_mul(128),
            pages.root_retains,
            pages.metadata_queries,
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
    pub fn available_records(&self) -> usize {
        self.packets.available_records()
    }
    pub fn available_bodies(&self) -> usize {
        self.packets.available_bodies()
    }
    pub fn index_leases(&self) -> usize {
        self.indexes.leases()
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
            density_checked: true,
            credit: None,
            retained_events: 0,
            retained_pages: 0,
            categories: 0,
            retained_epoch: self.retirement_epoch,
        })
    }
    pub fn retain(&mut self, media: &Media) -> Result<Media, Error> {
        let root = self.indexes.retain(media.root)?;
        self.retain_credit(media);
        Ok(Media {
            root,
            credit: media.credit,
            retained_events: media.retained_events,
            retained_pages: media.retained_pages,
            categories: media.categories,
            retained_epoch: media.retained_epoch,
            origin: media.origin,
            frames: media.frames,
            initial: media.initial,
            revision: media.revision,
            density_checked: media.density_checked,
        })
    }
    pub fn retarget_empty(&mut self, empty: &mut Media, source: &Media) -> Result<(), Error> {
        if self.indexes.len(empty.root)? != 0 {
            return Err(Error::InvalidState);
        }
        self.indexes.len(source.root)?;
        self.indexes.release(empty.root)?;
        empty.root = self
            .indexes
            .retain(source.root)
            .expect("reserved lease credit");
        self.release_credit(empty);
        self.retain_credit(source);
        empty.credit = source.credit;
        empty.retained_events = source.retained_events;
        empty.retained_pages = source.retained_pages;
        empty.categories = source.categories;
        empty.retained_epoch = source.retained_epoch;
        empty.origin = source.origin;
        empty.frames = source.frames;
        empty.initial = source.initial;
        empty.revision = source.revision;
        empty.density_checked = source.density_checked;
        Ok(())
    }
    pub fn release(&mut self, media: Media) -> Result<(), Error> {
        self.indexes.release(media.root)?;
        self.release_credit(&media);
        Ok(())
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
        frame
            .0
            .checked_add(self.q as u64)
            .ok_or(Error::InvalidFrame)?;
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
        let totals = self.indexes.totals(
            media.root,
            Cut::frame(media.origin),
            Cut::frame(Frame(media.origin.0 + media.frames)),
            &self.packets,
        )?;
        self.owner_credit(
            media,
            totals.events + 1,
            totals.pages + bytes.len().div_ceil(self.packets.page_bytes()) + 1,
        )?;
        media.density_checked = false;
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
            let value = self.packets.value(record).unwrap();
            media.categories |= self.packet_categories(value);
            let result = self
                .owner_credit(
                    media,
                    totals.events + 1,
                    totals.pages + bytes.len().div_ceil(self.packets.page_bytes()) + 1,
                )
                .and_then(|()| self.indexes.insert(media.root, value, &mut self.packets));
            self.packets.release_record(record).unwrap();
            media.root = result?;
            Ok(())
        })();
        self.account(io);
        result
    }
    pub fn pin_import(
        &mut self,
        resources: &mut ResourceStore,
        snapshot: super::SnapshotId,
    ) -> Result<midi_packets::BaseId, Error> {
        self.packets.pin_base(resources, snapshot)
    }
    pub fn release_import(&mut self, base: midi_packets::BaseId) -> Result<(), Error> {
        self.packets.release_base(base)
    }
    pub fn import_reference(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &mut Media,
        base: midi_packets::BaseId,
        frame: u64,
        order: u64,
        offset: usize,
        bytes: usize,
    ) -> Result<(), Error> {
        if frame >= media.frames {
            return Err(Error::InvalidFrame);
        }
        if self.packets.available_records() == 0 {
            return Err(Error::Capacity);
        }
        let totals = self.indexes.totals(
            media.root,
            Cut::frame(media.origin),
            Cut::frame(Frame(media.origin.0 + media.frames)),
            &self.packets,
        )?;
        self.owner_credit(media, totals.events + 1, totals.pages)?;
        media.density_checked = false;
        self.indexes.reset_work();
        let result = (|| {
            let body = self.packets.reference_packet(io, base, offset, bytes)?;
            let row = self
                .packets
                .make_record(
                    body,
                    Point {
                        frame: Frame(media.origin.0 + frame),
                        order,
                    },
                )
                .expect("preflighted imported reference");
            self.packets.release_body(body).unwrap();
            let value = self.packets.value(row).unwrap();
            media.categories |= self.packet_categories(value);
            let result = self
                .owner_credit(media, totals.events + 1, totals.pages)
                .and_then(|()| self.indexes.insert(media.root, value, &mut self.packets));
            self.packets.release_record(row).unwrap();
            media.root = result?;
            Ok(())
        })();
        self.account(io);
        result
    }
    pub fn import_writer_reference(
        &mut self,
        io: &mut RenderIo<'_>,
        writer: &mut Writer,
        prefix_frames: u64,
        base: midi_packets::BaseId,
        frame: u64,
        order: u64,
        offset: usize,
        bytes: usize,
    ) -> Result<(), Error> {
        if !matches!(writer.kind, Kind::Growing { .. }) || frame >= prefix_frames {
            return Err(Error::InvalidFrame);
        }
        self.import_reference(io, &mut writer.media, base, frame, order, offset, bytes)
    }
    pub fn complete_writer_prefix(&self, writer: &mut Writer) {
        writer.media.density_checked = true;
    }
    pub fn fix_writer_window(&self, writer: &mut Writer) -> Result<(), Error> {
        if !matches!(writer.kind, Kind::Growing { .. }) || writer.media.frames == 0 {
            return Err(Error::InvalidState);
        }
        writer.kind = Kind::Windowed;
        writer.position = 0;
        Ok(())
    }
    pub fn resume_writer_window(&self, writer: &mut Writer) -> Result<(), Error> {
        if writer.kind != Kind::Windowed {
            return Err(Error::InvalidState);
        }
        writer.position = 0;
        Ok(())
    }
    pub fn begin_writer(
        &mut self,
        mut media: Media,
        kind: Kind,
    ) -> Result<Admission, (Error, Media)> {
        if let Err(error) = self.indexes.len(media.root) {
            return Err((error, media));
        }
        if kind != Kind::Windowed && media.frames % self.q as u64 != 0
            || matches!(kind, Kind::Fixed) && media.frames == 0
            || matches!(kind, Kind::Growing { maximum_frames } if maximum_frames < media.frames || media.origin.0.checked_add(maximum_frames).is_none())
        {
            return Err((Error::InvalidShape, media));
        }
        self.settled_media(&mut media);
        let admission = (|| {
            let totals = self.indexes.totals(
                media.root,
                Cut::frame(media.origin),
                Cut::frame(Frame(media.origin.0 + media.frames)),
                &self.packets,
            )?;
            let events = totals
                .events
                .checked_add(media.retained_events)
                .and_then(|n| n.checked_add(self.config.events_per_tick))
                .ok_or(Error::Capacity)?;
            let pages = totals
                .pages
                .checked_add(media.retained_pages)
                .and_then(|n| {
                    n.checked_add(
                        self.config
                            .bytes_per_tick
                            .div_ceil(self.packets.page_bytes())
                            + 1,
                    )
                })
                .ok_or(Error::Capacity)?;
            self.owner_credit(&mut media, events, pages)
        })();
        if let Err(error) = admission {
            return Err((error, media));
        }
        let after = Cut::frame(media.origin);
        let end = Cut::frame(Frame(match kind {
            Kind::Fixed | Kind::Windowed => media.origin.0 + media.frames,
            Kind::Growing { .. } => u64::MAX,
        }));
        let ready = media.density_checked;
        Ok(Admission {
            media,
            kind,
            after,
            end,
            quantum: None,
            density: 0,
            bytes: 0,
            ready,
        })
    }
    pub fn progress_writer(
        &mut self,
        io: &mut RenderIo<'_>,
        admission: &mut Admission,
        budget: usize,
    ) -> Result<bool, Error> {
        if admission.ready {
            return Ok(true);
        }
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
        mut admission: Admission,
        position: u64,
    ) -> Result<Writer, (Error, Admission)> {
        if !admission.ready {
            return Err((Error::Busy, admission));
        }
        if position % self.q as u64 != 0
            || match admission.kind {
                Kind::Fixed => position >= admission.media.frames,
                Kind::Windowed => position != 0,
                Kind::Growing { .. } => position != admission.media.frames,
            }
        {
            return Err((Error::InvalidEvent, admission));
        }
        admission.media.density_checked = true;
        Ok(Writer {
            media: admission.media,
            kind: admission.kind,
            position,
        })
    }
    pub fn seek_writer(&self, writer: &mut Writer, position: u64) -> Result<(), Error> {
        if writer.kind != Kind::Fixed
            || position >= writer.media.frames
            || position % self.q as u64 != 0
        {
            return Err(Error::InvalidFrame);
        }
        writer.position = position;
        Ok(())
    }
    pub fn writer_totals(
        &mut self,
        io: &mut RenderIo<'_>,
        writer: &Writer,
    ) -> Result<Totals, Error> {
        self.totals(io, &writer.media)
    }
    pub fn snapshot(&mut self, writer: &Writer) -> Result<Media, Error> {
        self.retain(&writer.media)
    }
    pub fn finish_writer(&mut self, writer: Writer) -> Media {
        if let Some(id) = writer.media.credit {
            let totals = self
                .indexes
                .totals(
                    writer.media.root,
                    Cut::frame(writer.media.origin),
                    Cut::frame(Frame(writer.media.origin.0 + writer.media.frames)),
                    &self.packets,
                )
                .expect("complete writer root");
            let owner = &mut self.credits.as_mut().unwrap().owners[id];
            owner.desired_events = totals.events;
            owner.desired_pages = totals.pages;
        }
        writer.media
    }
    pub fn write_tick(
        &mut self,
        io: &mut RenderIo<'_>,
        writer: &mut Writer,
        before_input: &MidiState,
        staged: &Staging,
    ) -> Result<(), Error> {
        if writer.kind == Kind::Windowed {
            return Err(Error::InvalidState);
        }
        let span = WriteSpan {
            position: writer.position,
            offset: 0,
            frames: self.q,
        };
        self.write_span(io, writer, before_input, staged, span)
    }
    pub fn write_window_tick(
        &mut self,
        io: &mut RenderIo<'_>,
        writer: &mut Writer,
        before_input: &MidiState,
        staged: &Staging,
        span: WriteSpan,
    ) -> Result<(), Error> {
        if writer.kind != Kind::Windowed {
            return Err(Error::InvalidState);
        }
        self.write_span(io, writer, before_input, staged, span)
    }
    fn write_span(
        &mut self,
        io: &mut RenderIo<'_>,
        writer: &mut Writer,
        before_input: &MidiState,
        staged: &Staging,
        span: WriteSpan,
    ) -> Result<(), Error> {
        self.indexes.reset_work();
        let result = self.write_inner(writer, before_input, staged, span);
        if result.is_ok() && span.position == 0 && span.frames > 0 {
            io.store
                .charge_work(WorkClass::Render, size_of::<MidiState>(), staged.count)
                .expect("required input projection");
        }
        self.account(io);
        result
    }
    fn write_inner(
        &mut self,
        writer: &mut Writer,
        before_input: &MidiState,
        staged: &Staging,
        span: WriteSpan,
    ) -> Result<(), Error> {
        self.settled_media(&mut writer.media);
        let next = span
            .position
            .checked_add(span.frames as u64)
            .ok_or(Error::InvalidFrame)?;
        if span
            .offset
            .checked_add(span.frames)
            .is_none_or(|end| end > self.q)
        {
            return Err(Error::InvalidFrame);
        }
        let frames = match writer.kind {
            Kind::Fixed | Kind::Windowed if next <= writer.media.frames => writer.media.frames,
            Kind::Fixed | Kind::Windowed => return Err(Error::InvalidFrame),
            Kind::Growing { maximum_frames }
                if span.position == writer.media.frames && next <= maximum_frames =>
            {
                next
            }
            Kind::Growing { .. } => return Err(Error::Capacity),
        };
        let revision = writer.media.revision.checked_next()?;
        if span.frames == 0 {
            writer.media.revision = revision;
            return Ok(());
        }
        let begin = writer
            .media
            .origin
            .0
            .checked_add(span.position)
            .ok_or(Error::InvalidFrame)?;
        let end = begin
            .checked_add(span.frames as u64)
            .ok_or(Error::InvalidFrame)?;
        self.removals.clear();
        let mut after = Cut::frame(Frame(begin));
        while let Some(value) = self.indexes.first(
            writer.media.root,
            after,
            Cut::frame(Frame(end)),
            &self.packets,
        )? {
            if self.removals.len() == self.removals.capacity() {
                return Err(Error::Capacity);
            }
            self.removals.push(value);
            after = Cut::at(self.packets.point(value));
        }
        let mut selected = 0;
        for i in 0..staged.count {
            self.packets
                .value(staged.records[i].ok_or(Error::InvalidId)?)?;
            let point = self.packets.point(staged.values[i]);
            if point.frame.0 < staged.frame.0 || point.frame.0 - staged.frame.0 >= self.q as u64 {
                return Err(Error::InvalidEvent);
            }
            let offset = (point.frame.0 - staged.frame.0) as usize;
            if offset >= span.offset && offset < span.offset + span.frames {
                selected += 1;
            }
        }
        let totals = self.indexes.totals(
            writer.media.root,
            Cut::frame(writer.media.origin),
            Cut::frame(Frame(writer.media.origin.0 + writer.media.frames)),
            &self.packets,
        )?;
        let mut incoming = Totals::default();
        for &value in &staged.values[..staged.count] {
            let offset = (self.packets.point(value).frame.0 - staged.frame.0) as usize;
            if offset >= span.offset && offset < span.offset + span.frames {
                incoming = <Totals as crate::midi_state::indexed::tree::Summary<NonZeroU32>>::then(
                    incoming,
                    <Totals as crate::midi_state::indexed::tree::Summary<NonZeroU32>>::value(
                        value,
                        &self.packets,
                    ),
                );
            }
        }
        for &value in &staged.values[..staged.count] {
            writer.media.categories |= self.packet_categories(value);
        }
        let owned_events =
            totals.events - self.removals.len() + selected + writer.media.retained_events;
        let owned_pages = totals
            .pages
            .checked_add(writer.media.retained_pages)
            .and_then(|n| n.checked_add(incoming.pages))
            .and_then(|n| n.checked_add(2))
            .ok_or(Error::Capacity)?;
        self.owner_credit(&mut writer.media, owned_events, owned_pages)?;
        if selected > self.packets.available_records() {
            return Err(Error::Capacity);
        }
        if writer.kind == Kind::Windowed {
            self.check_window_density(writer, staged, span)?;
        }
        let mut inserted = 0;
        for i in 0..staged.count {
            let offset = (self.packets.point(staged.values[i]).frame.0 - staged.frame.0) as usize;
            if offset < span.offset || offset >= span.offset + span.frames {
                continue;
            }
            let record = self
                .packets
                .fork_record(
                    staged.values[i],
                    Point {
                        frame: Frame(begin + (offset - span.offset) as u64),
                        order: i as u64,
                    },
                )
                .expect("preflighted writer record");
            self.insertions[inserted] = self.packets.value(record).unwrap();
            self.records[inserted] = Some(record);
            inserted += 1;
        }
        let result = self.indexes.replace_records(
            writer.media.root,
            &self.removals,
            &self.insertions[..inserted],
            &mut self.packets,
        );
        for record in &mut self.records[..inserted] {
            self.packets.release_record(record.take().unwrap()).unwrap();
        }
        writer.media.root = result?;
        if span.position == 0 {
            writer.media.initial = *before_input;
            for &value in &staged.values[..staged.count] {
                let offset = self.packets.point(value).frame.0 - staged.frame.0;
                if offset < span.offset as u64 {
                    self.packets.apply(value, &mut writer.media.initial);
                }
            }
        }
        writer.media.frames = frames;
        writer.media.revision = revision;
        writer.position = match writer.kind {
            Kind::Fixed => next % frames,
            Kind::Windowed => 0,
            Kind::Growing { .. } => next,
        };
        Ok(())
    }
    fn check_window_density(
        &self,
        writer: &Writer,
        staged: &Staging,
        span: WriteSpan,
    ) -> Result<(), Error> {
        let first = span.position / self.q as u64;
        let last = (span.position + span.frames as u64 - 1) / self.q as u64;
        for bucket in first..=last {
            let begin = writer.media.origin.0 + bucket * self.q as u64;
            let end = writer.media.origin.0
                + (bucket * self.q as u64)
                    .checked_add(self.q as u64)
                    .unwrap_or(writer.media.frames)
                    .min(writer.media.frames);
            let totals = self.indexes.totals(
                writer.media.root,
                Cut::frame(Frame(begin)),
                Cut::frame(Frame(end)),
                &self.packets,
            )?;
            let (mut events, mut bytes) = (totals.events, totals.bytes);
            for &value in &self.removals {
                if (begin..end).contains(&self.packets.point(value).frame.0) {
                    events = events.checked_sub(1).ok_or(Error::InvalidState)?;
                    bytes = bytes
                        .checked_sub(12 + self.packets.packet_len(value))
                        .ok_or(Error::InvalidState)?;
                }
            }
            for &value in &staged.values[..staged.count] {
                let offset = (self.packets.point(value).frame.0 - staged.frame.0) as usize;
                if offset >= span.offset && offset < span.offset + span.frames {
                    let at = writer.media.origin.0 + span.position + (offset - span.offset) as u64;
                    if (begin..end).contains(&at) {
                        events = events.checked_add(1).ok_or(Error::Capacity)?;
                        bytes = bytes
                            .checked_add(12 + self.packets.packet_len(value))
                            .ok_or(Error::Capacity)?;
                    }
                }
            }
            let payload = bytes
                .checked_sub(events.checked_mul(12).ok_or(Error::Capacity)?)
                .ok_or(Error::InvalidState)?;
            // An offset span can combine portions of two admitted source quanta.
            if events
                > self
                    .config
                    .events_per_tick
                    .checked_mul(2)
                    .ok_or(Error::Capacity)?
                || payload
                    > self
                        .config
                        .bytes_per_tick
                        .checked_mul(2)
                        .ok_or(Error::Capacity)?
            {
                return Err(Error::Capacity);
            }
        }
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
    pub fn read_bytes(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &Media,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, Error> {
        self.read_bytes_with(io, media, offset, output, WorkClass::Render)
    }
    pub fn read_bytes_with(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &Media,
        offset: usize,
        output: &mut [u8],
        class: WorkClass,
    ) -> Result<usize, Error> {
        self.indexes.reset_work();
        let selected = self.indexes.record_at(media.root, offset, &self.packets);
        self.account(io);
        let Some((value, within)) = selected? else {
            return Ok(0);
        };
        if within < 12 {
            let point = self.packets.point(value);
            let bytes =
                u32::try_from(self.packets.packet_len(value)).map_err(|_| Error::Capacity)?;
            let mut header = [0; 12];
            header[..8].copy_from_slice(&(point.frame.0 - media.origin.0).to_le_bytes());
            header[8..].copy_from_slice(&bytes.to_le_bytes());
            let count = output.len().min(12 - within);
            io.store.charge_work(class, count, 0)?;
            output[..count].copy_from_slice(&header[within..within + count]);
            return Ok(count);
        }
        self.packets
            .read_with(io, value, within - 12, output, class)
    }
    pub fn byte_before(
        &mut self,
        io: &mut RenderIo<'_>,
        media: &Media,
        frame: u64,
    ) -> Result<usize, Error> {
        if frame > media.frames {
            return Err(Error::InvalidFrame);
        }
        self.indexes.reset_work();
        let result = self.indexes.totals(
            media.root,
            Cut::frame(media.origin),
            Cut::frame(Frame(media.origin.0 + frame)),
            &self.packets,
        );
        self.account(io);
        Ok(result?.bytes)
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
                if !self.indexes.has_pending_retirement() {
                    if let Some(credits) = self.credits.as_mut() {
                        for owner in &mut credits.owners {
                            if owner.references == 0 {
                                credits.events -= owner.events;
                                credits.pages -= owner.pages;
                                credits.categories -= owner.categories;
                                *owner = OwnerCredit::default();
                            } else {
                                credits.events -= owner.events - owner.desired_events;
                                credits.pages -= owner.pages - owner.desired_pages;
                                owner.events = owner.desired_events;
                                owner.pages = owner.desired_pages;
                            }
                        }
                    }
                    self.retirement_epoch = self
                        .retirement_epoch
                        .checked_add(1)
                        .ok_or(Error::Exhausted)?;
                }
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
