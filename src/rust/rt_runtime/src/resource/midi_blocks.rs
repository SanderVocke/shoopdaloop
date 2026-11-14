//! Fixed-record MIDI resources and explicitly prepared positions.
use super::{
    indexed_midi::{BlockLimits, WriteSpan},
    RenderIo, ResourceStore, SnapshotId, WorkClass,
};
use crate::{
    buffer::MidiBuffer,
    midi::{EventRecord, ShortMessage},
    midi_runtime::{
        self as runtime, MediaImage, PositionId, PositionJobs, PositionStatus, PreparedPosition,
        Store, Version, WriterId, WriterSet,
    },
    midi_state::MidiState,
    Error, Frame, Quantum, Sample,
};
use alloc::{boxed::Box, vec};
use core::{
    mem::size_of_val,
    ops::{Deref, DerefMut},
};

#[path = "midi_writer/block_edit.rs"]
mod edit;
#[path = "midi_writer/block_history.rs"]
pub(super) mod history;
pub(super) use edit::Patch;

#[derive(Clone, Copy)]
pub(super) struct Config {
    pub blocks: BlockLimits,
    pub events_per_tick: usize,
    pub bytes_per_tick: usize,
    pub writers: usize,
}
pub(super) struct Staging {
    records: Box<[EventRecord]>,
    count: usize,
    frame: Frame,
}
fn empty_record() -> EventRecord {
    EventRecord::new(Frame(0), 0, &[0xff]).unwrap()
}
impl Staging {
    pub fn prepared_bytes(events: usize) -> Result<usize, Error> {
        if events == 0 || events > u16::MAX as usize {
            return Err(Error::InvalidConfig);
        }
        events
            .checked_mul(size_of::<EventRecord>())
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
    pub fn prepare(events: usize) -> Result<Self, Error> {
        Self::prepared_bytes(events)?;
        Ok(Self {
            records: vec![empty_record(); events].into_boxed_slice(),
            count: 0,
            frame: Frame(0),
        })
    }
    pub(crate) fn allocated_bytes(&self) -> usize {
        size_of_val(&*self.records)
    }
}
pub(super) struct Media {
    image: MediaImage,
    identity: Version,
    cache: usize,
    credit: Option<usize>,
}
impl Deref for Media {
    type Target = MediaImage;
    fn deref(&self) -> &MediaImage {
        &self.image
    }
}
impl DerefMut for Media {
    fn deref_mut(&mut self) -> &mut MediaImage {
        &mut self.image
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Fixed,
    Windowed,
    Growing { maximum_frames: u64 },
}
impl From<Kind> for runtime::WriteMode {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Fixed => Self::Fixed,
            Kind::Windowed => Self::Windowed,
            Kind::Growing { maximum_frames } => Self::Growing { maximum_frames },
        }
    }
}
pub(super) struct Admission {
    media: Media,
    kind: Kind,
    ready: bool,
}
pub(super) struct Writer {
    pub media: Media,
    id: WriterId,
    kind: Kind,
    pub position: u64,
    active: bool,
    sequence: u64,
}
#[derive(Clone, Copy, Default)]
pub(super) struct Totals {
    pub bytes: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ImportBase {
    slot: usize,
    generation: u64,
}
struct Base {
    generation: u64,
    snapshot: Option<SnapshotId>,
}
#[derive(Clone, Copy)]
struct Cache {
    identity: Version,
    target: u64,
    prefix: u64,
    warm_prefix: Option<u64>,
    job: Option<PositionId>,
    ready: bool,
    entries: [Option<(u64, u64)>; 2],
    entry_turn: usize,
}
#[derive(Clone, Copy, Default)]
struct Credit {
    references: usize,
    events: usize,
}
pub(super) struct Core {
    blocks: Store,
    writers: WriterSet,
    positions: PositionJobs,
    caches: Box<[Option<Cache>]>,
    cache_states: Box<[MidiState]>,
    bases: Box<[Base]>,
    credits: Box<[Credit]>,
    edit_busy: Box<[bool]>,
    edit_records: Box<[EventRecord]>,
    edit_scratch: Box<[EventRecord]>,
    credit_limit: usize,
    credited: usize,
    pub(super) config: Config,
    q: usize,
}
impl Core {
    fn apply_state<S: Sample>(
        io: &mut RenderIo<'_, S>,
        message: ShortMessage,
        state: &mut MidiState,
    ) -> Result<(), Error> {
        let mut update = crate::midi_state::BasicUpdate::begin(message)?;
        let mut steps = 0;
        while !update.complete() {
            update.step(state);
            steps += 1;
        }
        io.store.charge_work(WorkClass::Render, 0, steps)
    }
    fn profile(config: Config, q: usize) -> Result<runtime::Profile, Error> {
        let p = runtime::Profile {
            pages: config.blocks.record_pages,
            tiles: config.blocks.record_pages,
            directories: config.blocks.directories,
            versions: config.blocks.versions,
            tile_shift: q.trailing_zeros() as u8 + 1,
            pages_per_tile: config
                .events_per_tick
                .checked_mul(6)
                .ok_or(Error::Capacity)?
                .div_ceil(runtime::RECORDS_PER_PAGE)
                .max(1),
            touched_tiles: 2,
        };
        p.validate()
    }
    pub fn prepared_bytes(config: Config) -> Result<usize, Error> {
        Staging::prepared_bytes(config.events_per_tick)?;
        let capacity = config.blocks.media;
        if capacity == 0 || config.blocks.import_bases == 0 || config.bytes_per_tick == 0 {
            return Err(Error::InvalidConfig);
        }
        Store::prepared_bytes(Self::profile(config, 32)?)?
            .checked_add(WriterSet::prepared_bytes(
                config.writers.max(1),
                config.events_per_tick,
            )?)
            .and_then(|n| n.checked_add(PositionJobs::prepared_bytes(capacity).ok()?))
            .and_then(|n| {
                n.checked_add(capacity.checked_mul(
                    size_of::<Option<Cache>>() + size_of::<Credit>() + 4 * size_of::<MidiState>(),
                )?)
            })
            .and_then(|n| n.checked_add(config.blocks.import_bases.checked_mul(size_of::<Base>())?))
            .and_then(|n| n.checked_add(config.writers.max(1).checked_mul(size_of::<bool>())?))
            .and_then(|n| {
                n.checked_add(
                    config
                        .writers
                        .max(1)
                        .checked_add(1)?
                        .checked_mul(config.events_per_tick)?
                        .checked_mul(size_of::<EventRecord>())?,
                )
            })
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
    pub fn prepare<S: Sample>(
        resources: &mut ResourceStore<S>,
        q: Quantum,
        config: Config,
    ) -> Result<Self, Error> {
        Self::prepared_bytes(config)?;
        if resources.quantum != q {
            return Err(Error::InvalidConfig);
        }
        let capacity = config.blocks.media;
        Ok(Self {
            blocks: Store::prepare(Self::profile(config, q.frames())?)?,
            writers: WriterSet::prepare(config.writers.max(1), q.frames(), config.events_per_tick)?,
            positions: PositionJobs::prepare(capacity)?,
            caches: (0..capacity).map(|_| None).collect(),
            cache_states: vec![MidiState::default(); capacity * 4].into_boxed_slice(),
            bases: (0..config.blocks.import_bases)
                .map(|_| Base {
                    generation: 1,
                    snapshot: None,
                })
                .collect(),
            credits: vec![Credit::default(); capacity].into_boxed_slice(),
            edit_busy: vec![false; config.writers.max(1)].into_boxed_slice(),
            edit_records: vec![empty_record(); config.writers.max(1) * config.events_per_tick]
                .into_boxed_slice(),
            edit_scratch: vec![empty_record(); config.events_per_tick].into_boxed_slice(),
            credit_limit: config.blocks.event_credits,
            credited: 0,
            config,
            q: q.frames(),
        })
    }
    pub(crate) fn allocated_bytes(&self) -> usize {
        self.blocks.allocated_bytes()
            + self.writers.allocated_bytes()
            + self.positions.allocated_bytes()
            + size_of_val(&*self.caches)
            + size_of_val(&*self.cache_states)
            + size_of_val(&*self.bases)
            + size_of_val(&*self.credits)
            + size_of_val(&*self.edit_busy)
            + size_of_val(&*self.edit_records)
            + size_of_val(&*self.edit_scratch)
    }
    pub fn owner_credit_bytes(_: usize) -> Result<usize, Error> {
        Ok(0)
    }
    pub fn owner_limits(
        config: Config,
        sources: usize,
        history_events: usize,
        _: usize,
        _: usize,
    ) -> Result<(usize, usize, usize), Error> {
        let reserved = sources
            .checked_mul(history_events)
            .and_then(|n| n.checked_add(config.events_per_tick.checked_mul(sources + 2)?))
            .ok_or(Error::Capacity)?;
        let available = config
            .blocks
            .event_credits
            .checked_sub(reserved)
            .ok_or(Error::Capacity)?;
        Ok((available, available, 0))
    }
    pub fn prepare_owner_credits(
        &mut self,
        sources: usize,
        history_events: usize,
        history_bytes: usize,
    ) -> Result<(), Error> {
        self.credit_limit =
            Self::owner_limits(self.config, sources, history_events, history_bytes, 1)?.0;
        Ok(())
    }
    fn quote_owner(&self, media: &Media, events: usize) -> Result<(usize, usize), Error> {
        let old = media.credit.map(|id| self.credits[id]).unwrap_or_default();
        let unique = old.references == 1;
        let total = self
            .credited
            .checked_sub(if unique { old.events } else { 0 })
            .and_then(|n| n.checked_add(events))
            .ok_or(Error::Capacity)?;
        if total > self.credit_limit {
            return Err(Error::Capacity);
        }
        let id = if unique {
            media.credit.unwrap()
        } else {
            self.credits
                .iter()
                .position(|c| c.references == 0 && c.events == 0)
                .ok_or(Error::Capacity)?
        };
        Ok((id, total))
    }
    fn charge_owner(&mut self, media: &mut Media, events: usize) -> Result<(), Error> {
        let (id, total) = self.quote_owner(media, events)?;
        let unique = media
            .credit
            .is_some_and(|old| self.credits[old].references == 1);
        if let Some(old) = media.credit.filter(|_| !unique) {
            self.credits[old].references -= 1;
        }
        self.credits[id] = Credit {
            references: 1,
            events,
        };
        media.credit = Some(id);
        self.credited = total;
        Ok(())
    }
    pub fn required_maintenance(&self, minimum: usize) -> usize {
        minimum.max(
            self.config
                .events_per_tick
                .saturating_mul(600)
                .saturating_add(2048),
        )
    }
    fn account<S: Sample>(&mut self, io: &mut RenderIo<'_, S>) -> Result<(), Error> {
        let work = self.blocks.work();
        let positions = self.positions.work();
        let state_steps = self
            .writers
            .state_copy_cells
            .checked_add(self.writers.state_update_steps);
        let copies = positions
            .seed_copies
            .checked_add(positions.result_copies)
            .and_then(|n| {
                n.checked_add(
                    self.writers.state_copy_cells / crate::midi_state::BASIC_STATE_CELLS as u64,
                )
            });
        let bytes = copies
            .and_then(|n| n.checked_mul(size_of::<MidiState>() as u64))
            .and_then(|n| usize::try_from(n).ok());
        let steps = work
            .directory_reads
            .checked_add(work.directory_writes)
            .and_then(|n| n.checked_mul(16))
            .and_then(|n| n.checked_add(work.record_reads))
            .and_then(|n| n.checked_add(work.record_writes))
            .and_then(|n| n.checked_add(work.retirement_steps))
            .and_then(|n| n.checked_add(state_steps?))
            .and_then(|n| n.checked_add(positions.state_cells))
            .and_then(|n| n.checked_add(positions.update_steps));
        io.store.charge_work(
            WorkClass::Render,
            bytes.unwrap_or(usize::MAX),
            steps.and_then(|n| n.try_into().ok()).unwrap_or(usize::MAX),
        )?;
        io.store.render_work.counter_overflow |= work.counter_overflow
            || positions.counter_overflow
            || self.writers.counter_overflow
            || steps.is_none()
            || bytes.is_none();
        self.blocks.reset_work();
        self.positions.reset_work();
        self.writers.state_copy_cells = 0;
        self.writers.state_update_steps = 0;
        self.writers.counter_overflow = false;
        Ok(())
    }
    pub fn available_records(&self) -> usize {
        self.config
            .blocks
            .event_credits
            .saturating_sub(self.blocks.usage().pages)
    }
    pub fn available_bodies(&self) -> usize {
        self.available_records()
    }
    pub fn index_leases(&self) -> usize {
        self.caches.iter().filter(|c| c.is_some()).count()
    }
    fn own(&mut self, image: MediaImage) -> Result<Media, (Error, MediaImage)> {
        let Some(cache) = self.caches.iter().position(Option::is_none) else {
            return Err((Error::Capacity, image));
        };
        self.caches[cache] = Some(Cache {
            identity: image.version,
            target: 0,
            prefix: 0,
            warm_prefix: None,
            job: None,
            ready: true,
            entries: [None; 2],
            entry_turn: 0,
        });
        self.cache_states[cache * 4] = image.initial;
        self.cache_states[cache * 4 + 1] = image.initial;
        Ok(Media {
            identity: image.version,
            image,
            cache,
            credit: None,
        })
    }
    pub fn empty(
        &mut self,
        origin: Frame,
        frames: u64,
        initial: &MidiState,
    ) -> Result<Media, Error> {
        origin.0.checked_add(frames).ok_or(Error::InvalidFrame)?;
        if !initial.is_basic() {
            return Err(Error::InvalidParameter);
        }
        if self.caches.iter().all(Option::is_some) {
            return Err(Error::Capacity);
        }
        let version = self.blocks.empty()?;
        Ok(self
            .own(MediaImage {
                version,
                origin,
                frames,
                revision: 0,
                cut: origin,
                initial: *initial,
                frontier: *initial,
                frontier_position: frames,
            })
            .map_err(|(error, _)| error)
            .expect("reserved cache"))
    }
    pub fn retain(&mut self, media: &Media) -> Result<Media, Error> {
        if self.caches.iter().all(Option::is_some) {
            return Err(Error::Capacity);
        }
        let version = self.blocks.retain(media.version)?;
        let mut retained = self
            .own(MediaImage {
                version,
                origin: media.origin,
                frames: media.frames,
                revision: media.revision,
                cut: media.cut,
                initial: media.initial,
                frontier: media.frontier,
                frontier_position: media.frontier_position,
            })
            .map_err(|(error, _)| error)
            .expect("reserved cache");
        retained.identity = media.identity;
        if let Some(id) = media.credit {
            self.credits[id].references += 1;
            retained.credit = Some(id);
        }
        let mut cache = *self.caches[media.cache]
            .as_ref()
            .ok_or(Error::InvalidState)?;
        cache.identity = media.identity;
        cache.job = None;
        self.caches[retained.cache] = Some(cache);
        self.cache_states
            .copy_within(media.cache * 4..media.cache * 4 + 4, retained.cache * 4);
        Ok(retained)
    }
    pub fn retarget_empty(&mut self, target: &mut Media, source: &Media) -> Result<(), Error> {
        if self.blocks.totals(target.version)?.events != 0 {
            return Err(Error::InvalidState);
        }
        let version = self.blocks.retain(source.version)?;
        let mut cache = *self.caches[source.cache]
            .as_ref()
            .ok_or(Error::InvalidState)?;
        cache.job = None;
        self.invalidate(target)?;
        self.blocks.release(target.version)?;
        if let Some(id) = target.credit {
            self.credits[id].references -= 1;
        }
        if let Some(id) = source.credit {
            self.credits[id].references += 1;
        }
        target.image = MediaImage {
            version,
            ..source.image
        };
        target.identity = source.identity;
        target.credit = source.credit;
        self.caches[target.cache] = Some(cache);
        self.cache_states
            .copy_within(source.cache * 4..source.cache * 4 + 4, target.cache * 4);
        Ok(())
    }
    pub fn release(&mut self, media: Media) -> Result<(), Error> {
        let cache = self.caches[media.cache].take().ok_or(Error::InvalidId)?;
        if let Some(job) = cache.job {
            self.positions.cancel(&mut self.blocks, job)?;
        }
        self.blocks.release(media.version)?;
        if let Some(id) = media.credit {
            self.credits[id].references -= 1;
        }
        Ok(())
    }
    pub fn stage<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        staged: &mut Staging,
        frame: Frame,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        if staged.count != 0 {
            return Err(Error::Busy);
        }
        frame.checked_add(self.q)?;
        if input.len() > staged.records.len() || input.byte_len() > self.config.bytes_per_tick {
            return Err(Error::Capacity);
        }
        for index in 0..input.len() {
            let (offset, bytes) = input.get(index).ok_or(Error::InvalidEvent)?;
            if offset as usize >= self.q {
                return Err(Error::InvalidFrame);
            }
            ShortMessage::managed(bytes).map_err(Error::from)?;
        }
        for index in 0..input.len() {
            let (offset, bytes) = input.get(index).unwrap();
            staged.records[index] =
                EventRecord::new(frame.checked_add(offset as usize)?, index as u64, bytes)?;
        }
        staged.count = input.len();
        staged.frame = frame;
        io.store
            .charge_work(WorkClass::Render, staged.count * 16, staged.count)
    }
    pub fn clear_stage(&mut self, staged: &mut Staging) -> Result<(), Error> {
        staged.count = 0;
        Ok(())
    }
    pub fn import_packet<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        media: &mut Media,
        frame: u64,
        order: u64,
        bytes: &[u8],
    ) -> Result<(), Error> {
        if frame >= media.frames {
            return Err(Error::InvalidFrame);
        }
        self.maintain(io, 1024)?;
        let record = EventRecord::new(
            Frame(
                media
                    .origin
                    .0
                    .checked_add(frame)
                    .ok_or(Error::InvalidFrame)?,
            ),
            order,
            bytes,
        )?;
        let events = self.blocks.totals(media.version)?.events as usize + 1;
        self.quote_owner(media, events)?;
        let next = self
            .blocks
            .append(media.version, core::slice::from_ref(&record))?;
        self.charge_owner(media, events)
            .expect("quoted owner credit");
        self.invalidate(media)?;
        self.blocks.release(media.version)?;
        media.image.version = next;
        media.identity = next;
        Self::apply_state(io, record.message(), &mut media.image.frontier)?;
        self.account(io)
    }
    pub fn pin_import<S: Sample>(
        &mut self,
        resources: &mut ResourceStore<S>,
        snapshot: SnapshotId,
    ) -> Result<ImportBase, Error> {
        resources.midi_lease(resources.read_index(snapshot)?)?;
        let slot = self
            .bases
            .iter()
            .position(|base| base.snapshot.is_none() && base.generation != u64::MAX)
            .ok_or(Error::Capacity)?;
        self.bases[slot].snapshot = Some(snapshot);
        Ok(ImportBase {
            slot,
            generation: self.bases[slot].generation,
        })
    }
    pub fn release_import(&mut self, id: ImportBase) -> Result<(), Error> {
        let base = self
            .bases
            .get_mut(id.slot)
            .filter(|b| b.generation == id.generation && b.snapshot.is_some())
            .ok_or(Error::InvalidId)?;
        base.snapshot = None;
        base.generation += 1;
        Ok(())
    }
    fn import_record<S: Sample>(
        &self,
        io: &mut RenderIo<'_, S>,
        base: ImportBase,
        frame: u64,
        order: u64,
        offset: usize,
        bytes: usize,
    ) -> Result<EventRecord, Error> {
        if bytes > 4 {
            return Err(Error::Capacity);
        }
        let snapshot = self
            .bases
            .get(base.slot)
            .filter(|b| b.generation == base.generation)
            .and_then(|b| b.snapshot)
            .ok_or(Error::InvalidId)?;
        let index = io.store.read_index(snapshot)?;
        let mut cursor = io.cursor_at(index, offset)?;
        let mut packet = [0; 4];
        let mut copied = 0;
        while copied < bytes {
            let count = io.read(&mut cursor, &mut packet[copied..bytes])?;
            if count == 0 {
                return Err(Error::Incomplete);
            }
            copied += count;
        }
        EventRecord::new(Frame(frame), order, &packet[..bytes])
    }
    pub fn import_reference<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        media: &mut Media,
        base: ImportBase,
        frame: u64,
        order: u64,
        offset: usize,
        bytes: usize,
    ) -> Result<(), Error> {
        let record = self.import_record(io, base, frame, order, offset, bytes)?;
        self.import_packet(io, media, frame, order, record.message().as_bytes())
    }
    pub fn import_writer_reference<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        writer: &mut Writer,
        prefix_frames: u64,
        base: ImportBase,
        frame: u64,
        order: u64,
        offset: usize,
        bytes: usize,
    ) -> Result<(), Error> {
        let record = self.import_record(
            io,
            base,
            writer
                .media
                .origin
                .0
                .checked_add(frame)
                .ok_or(Error::InvalidFrame)?,
            order,
            offset,
            bytes,
        )?;
        self.writers
            .insert_prefix(&mut self.blocks, writer.id, prefix_frames, record)?;
        self.refresh_writer(writer)?;
        self.account(io)
    }
    pub fn complete_writer_prefix(&self, _: &mut Writer) {}
    fn invalidate(&mut self, media: &Media) -> Result<(), Error> {
        let cache = self.caches[media.cache]
            .as_mut()
            .ok_or(Error::InvalidState)?;
        if let Some(job) = cache.job.take() {
            self.positions.cancel(&mut self.blocks, job)?;
        }
        cache.ready = false;
        cache.warm_prefix = None;
        cache.entries = [None; 2];
        cache.identity = media.identity;
        Ok(())
    }
    pub fn prepare_state<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        media: &Media,
        frame: u64,
        steps: usize,
    ) -> Result<bool, Error> {
        match self.state(io, media, frame) {
            Ok(state) => {
                let prefix = self.blocks.byte_before(
                    media.version,
                    Frame(
                        media
                            .origin
                            .0
                            .checked_add(frame)
                            .ok_or(Error::InvalidFrame)?,
                    ),
                )?;
                let cache = self.caches[media.cache]
                    .as_mut()
                    .ok_or(Error::InvalidState)?;
                if let Some(job) = cache.job.take() {
                    self.positions.cancel(&mut self.blocks, job)?;
                }
                cache.identity = media.identity;
                cache.target = frame;
                cache.prefix = prefix;
                self.cache_states[media.cache * 4] = state;
                cache.ready = true;
                Self::cache_entry(
                    cache,
                    &mut self.cache_states[media.cache * 4..][..4],
                    frame,
                    prefix,
                    state,
                );
                self.account(io)?;
                return Ok(true);
            }
            Err(Error::Busy) => {}
            Err(error) => return Err(error),
        }
        if frame > media.frames {
            return Err(Error::InvalidFrame);
        }
        let cache = self.caches[media.cache]
            .as_mut()
            .ok_or(Error::InvalidState)?;
        if cache.target != frame {
            if let Some(job) = cache.job.take() {
                self.positions.cancel(&mut self.blocks, job)?;
            }
            cache.ready = false;
        }
        let target = Frame(
            media
                .origin
                .0
                .checked_add(frame)
                .ok_or(Error::InvalidFrame)?,
        );
        let job = match cache.job {
            Some(job) => job,
            None => {
                let job = self.positions.begin(
                    &mut self.blocks,
                    media.version,
                    &media.initial,
                    target,
                )?;
                cache.target = frame;
                cache.identity = media.identity;
                cache.job = Some(job);
                job
            }
        };
        let ready = self.positions.progress(&mut self.blocks, job, steps)? == PositionStatus::Ready;
        if ready {
            let position = self.positions.finish(job)?;
            self.blocks.release(position.version)?;
            let cache = self.caches[media.cache].as_mut().unwrap();
            self.cache_states[media.cache * 4] = position.state;
            cache.prefix = self.blocks.byte_before(media.version, target)?;
            cache.job = None;
            cache.ready = true;
            Self::cache_entry(
                cache,
                &mut self.cache_states[media.cache * 4..][..4],
                frame,
                cache.prefix,
                position.state,
            );
        }
        self.account(io)?;
        Ok(ready)
    }
    fn cache_entry(
        cache: &mut Cache,
        states: &mut [MidiState],
        frame: u64,
        prefix: u64,
        state: MidiState,
    ) {
        let slot = cache
            .entries
            .iter()
            .position(|entry| entry.is_some_and(|e| e.0 == frame))
            .unwrap_or(cache.entry_turn);
        cache.entries[slot] = Some((frame, prefix));
        states[slot + 2] = state;
        cache.entry_turn = (slot + 1) % cache.entries.len();
    }
    pub fn replay_bound(&mut self, media: &Media) -> Result<usize, Error> {
        let events = usize::try_from(self.blocks.totals(media.version)?.events)
            .map_err(|_| Error::Capacity)?;
        events
            .checked_mul(crate::midi_state::BASIC_STATE_CELLS + 1)
            .and_then(|n| n.checked_add(crate::midi_state::BASIC_STATE_CELLS + 2))
            .ok_or(Error::Capacity)
    }
    pub fn state<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        media: &Media,
        frame: u64,
    ) -> Result<MidiState, Error> {
        if frame > media.frames {
            return Err(Error::InvalidFrame);
        }
        let state = if frame == 0 {
            media.initial
        } else if frame == media.frontier_position {
            media.frontier
        } else {
            let offset = self.blocks.byte_before(
                media.version,
                Frame(
                    media
                        .origin
                        .0
                        .checked_add(frame)
                        .ok_or(Error::InvalidFrame)?,
                ),
            )?;
            let total = self.blocks.totals(media.version)?;
            self.account(io)?;
            if offset == 0 {
                media.initial
            } else if offset == total.bytes && media.frontier_position == media.frames {
                media.frontier
            } else {
                self.caches
                    .iter()
                    .enumerate()
                    .find_map(|(index, cache)| {
                        let cache = cache.as_ref()?;
                        if cache.identity != media.identity {
                            return None;
                        }
                        if cache.ready && cache.prefix == offset {
                            Some(self.cache_states[index * 4])
                        } else if cache.warm_prefix == Some(offset) {
                            Some(self.cache_states[index * 4 + 1])
                        } else {
                            cache
                                .entries
                                .iter()
                                .position(|entry| entry.is_some_and(|entry| entry.1 == offset))
                                .map(|entry| self.cache_states[index * 4 + 2 + entry])
                        }
                    })
                    .ok_or(Error::Busy)?
            }
        };
        io.store
            .charge_work(WorkClass::Render, size_of::<MidiState>(), 0)?;
        Ok(state)
    }
    pub fn publish_position(
        &mut self,
        media: &Media,
        frame: u64,
        state: &MidiState,
    ) -> Result<(), Error> {
        if frame > media.frames || !state.is_basic() {
            return Err(Error::InvalidFrame);
        }
        let prefix = self.blocks.byte_before(
            media.version,
            Frame(
                media
                    .origin
                    .0
                    .checked_add(frame)
                    .ok_or(Error::InvalidFrame)?,
            ),
        )?;
        let cache = self.caches[media.cache]
            .as_mut()
            .ok_or(Error::InvalidState)?;
        cache.identity = media.identity;
        cache.warm_prefix = Some(prefix);
        self.cache_states[media.cache * 4 + 1] = *state;
        Ok(())
    }
    pub fn flush_work<S: Sample>(&mut self, io: &mut RenderIo<'_, S>) -> Result<(), Error> {
        self.account(io)
    }
    pub fn begin_writer(&mut self, media: Media, kind: Kind) -> Result<Admission, (Error, Media)> {
        if (kind != Kind::Windowed && media.frames % self.q as u64 != 0)
            || kind == Kind::Fixed && media.frames == 0
            || matches!(kind, Kind::Growing { maximum_frames } if maximum_frames < media.frames || media.origin.0.checked_add(maximum_frames).is_none())
        {
            return Err((Error::InvalidShape, media));
        }
        // Every constructor checks density before publishing this descriptor.
        let ready = true;
        Ok(Admission { media, kind, ready })
    }
    pub fn seed_recording(
        &mut self,
        admission: &mut Admission,
        state: &MidiState,
    ) -> Result<(), Error> {
        if !state.is_basic() || !matches!(admission.kind, Kind::Growing { .. }) {
            return Err(Error::InvalidParameter);
        }
        admission.media.image.frontier = *state;
        admission.media.image.frontier_position = admission.media.frames;
        Ok(())
    }
    pub fn progress_writer<S: Sample>(
        &mut self,
        _: &mut RenderIo<'_, S>,
        admission: &mut Admission,
        _: usize,
    ) -> Result<bool, Error> {
        Ok(admission.ready)
    }
    pub fn prepare_admission_position<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        admission: &Admission,
        position: u64,
        steps: usize,
    ) -> Result<bool, Error> {
        self.prepare_state(io, &admission.media, position, steps)
    }
    pub fn admission_replay_bound(&mut self, admission: &Admission) -> Result<usize, Error> {
        self.replay_bound(&admission.media)
    }
    pub fn writer_replay_bound(&mut self, writer: &Writer) -> Result<usize, Error> {
        self.replay_bound(&writer.media)
    }
    pub fn cancel_writer(&mut self, admission: Admission) -> Result<(), Error> {
        self.release(admission.media)
    }
    pub fn prepare_writer_position<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        writer: &Writer,
        position: u64,
        steps: usize,
    ) -> Result<bool, Error> {
        self.prepare_state(io, &writer.media, position, steps)
    }
    pub fn finish_admission(
        &mut self,
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
            return Err((Error::InvalidFrame, admission));
        }
        if position != 0 && admission.media.frontier_position != position {
            let cache = self.caches[admission.media.cache].as_ref().unwrap();
            if !cache.ready || cache.target != position {
                return Err((Error::Busy, admission));
            }
            admission.media.image.frontier = self.cache_states[admission.media.cache * 4];
            admission.media.image.frontier_position = position;
        }
        let id = match self.writers.begin(
            &mut self.blocks,
            &admission.media.image,
            admission.kind.into(),
            position,
        ) {
            Ok(id) => id,
            Err(error) => return Err((error, admission)),
        };
        Ok(Writer {
            media: admission.media,
            id,
            kind: admission.kind,
            position,
            active: false,
            sequence: 0,
        })
    }
    pub fn seek_writer(&mut self, writer: &mut Writer, position: u64) -> Result<(), Error> {
        if writer.kind != Kind::Fixed
            || position >= writer.media.frames
            || position % self.q as u64 != 0
        {
            return Err(Error::InvalidFrame);
        }
        writer.position = position;
        Ok(())
    }
    pub fn fix_writer_window(&mut self, writer: &mut Writer) -> Result<(), Error> {
        if !matches!(writer.kind, Kind::Growing { .. }) || writer.media.frames == 0 {
            return Err(Error::InvalidState);
        }
        self.writers
            .set_mode(writer.id, runtime::WriteMode::Windowed)?;
        writer.kind = Kind::Windowed;
        writer.position = 0;
        writer.active = false;
        Ok(())
    }
    pub fn resume_writer_window(&mut self, writer: &mut Writer) -> Result<(), Error> {
        if writer.kind != Kind::Windowed {
            return Err(Error::InvalidState);
        }
        self.writers
            .set_mode(writer.id, runtime::WriteMode::Windowed)?;
        writer.active = false;
        Ok(())
    }
    fn refresh_writer(&mut self, writer: &mut Writer) -> Result<(), Error> {
        let image = self.writers.snapshot(&mut self.blocks, writer.id)?;
        self.invalidate(&writer.media)?;
        self.blocks.release(writer.media.version)?;
        writer.media.identity = image.version;
        writer.media.image = image;
        Ok(())
    }
    pub fn writer_totals<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        writer: &Writer,
    ) -> Result<Totals, Error> {
        self.totals(io, &writer.media)
    }
    pub fn snapshot(&mut self, writer: &Writer) -> Result<Media, Error> {
        self.retain(&writer.media)
    }
    pub fn finish_writer(&mut self, mut writer: Writer) -> Media {
        let image = self.writers.finish(writer.id).expect("complete writer");
        self.blocks
            .release(writer.media.version)
            .expect("owned mirror");
        writer.media.image = image;
        writer.media
    }
    pub fn write_tick<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        writer: &mut Writer,
        before: &MidiState,
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
        self.write_span(io, writer, before, staged, span)
    }
    pub fn write_window_tick<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        writer: &mut Writer,
        before: &MidiState,
        staged: &Staging,
        span: WriteSpan,
    ) -> Result<(), Error> {
        if writer.kind != Kind::Windowed {
            return Err(Error::InvalidState);
        }
        self.write_span(io, writer, before, staged, span)
    }
    fn write_span<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        writer: &mut Writer,
        before: &MidiState,
        staged: &Staging,
        span: WriteSpan,
    ) -> Result<(), Error> {
        let result = self.write_span_inner(io, writer, before, staged, span);
        if result.is_err() {
            self.writers.freeze(&mut self.blocks, writer.id)?;
            self.account(io)?;
        }
        result
    }
    fn write_span_inner<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        writer: &mut Writer,
        before: &MidiState,
        staged: &Staging,
        span: WriteSpan,
    ) -> Result<(), Error> {
        if span.frames != 0 && writer.kind == Kind::Windowed {
            self.check_window_density(writer, staged, span)?;
        }
        let events = self.blocks.totals(writer.media.version)?.events as usize;
        let reserved_events = events.checked_add(staged.count).ok_or(Error::Capacity)?;
        self.quote_owner(&writer.media, reserved_events)?;
        if span.frames != 0 && span.position != 0 && writer.media.frontier_position != span.position
        {
            if span.position > writer.media.frontier_position
                && span.position - writer.media.frontier_position <= self.q as u64
            {
                self.writers.advance_frontier(
                    &mut self.blocks,
                    writer.id,
                    span.position,
                    self.config.events_per_tick * 2,
                )?;
                self.refresh_writer(writer)?;
            }
            if !self.prepare_state(io, &writer.media, span.position, 0)? {
                return Err(Error::Busy);
            }
            let state = self.state(io, &writer.media, span.position)?;
            let version = self.writers.version(writer.id)?;
            self.writers.handoff_position(
                writer.id,
                &PreparedPosition {
                    expected: version,
                    version,
                    target: Frame(
                        writer
                            .media
                            .origin
                            .0
                            .checked_add(span.position)
                            .ok_or(Error::InvalidFrame)?,
                    ),
                    after: None,
                    state,
                },
            )?;
        }
        if !writer.active {
            self.writers.activate(writer.id, staged.frame)?;
            writer.active = true;
        }
        let sequence = writer.sequence.checked_add(1).ok_or(Error::Exhausted)?;
        let input = runtime::InputStamp {
            sequence,
            frame: staged.frame,
        };
        self.writers
            .begin_interval(&mut self.blocks, writer.id, input, 1)?;
        if let Err(error) = self.writers.stage_span(
            &mut self.blocks,
            writer.id,
            input,
            before,
            &staged.records[..staged.count],
            runtime::WriteSpan {
                position: span.position,
                offset: span.offset,
                frames: span.frames,
            },
        ) {
            self.writers.abort_interval(&mut self.blocks, writer.id)?;
            self.account(io)?;
            return Err(error);
        }
        self.writers.commit_interval(&mut self.blocks, writer.id)?;
        self.charge_owner(&mut writer.media, reserved_events)
            .expect("quoted owner credit");
        writer.sequence = sequence;
        let entries = self.caches[writer.media.cache]
            .as_ref()
            .ok_or(Error::InvalidState)?
            .entries;
        self.refresh_writer(writer)?;
        for (slot, entry) in entries.into_iter().enumerate() {
            let Some((frame, _)) = entry.filter(|entry| entry.0 <= span.position) else {
                continue;
            };
            let prefix = self
                .blocks
                .byte_before(writer.media.version, Frame(writer.media.origin.0 + frame))?;
            self.caches[writer.media.cache].as_mut().unwrap().entries[slot] = Some((frame, prefix));
        }
        let next = span
            .position
            .checked_add(span.frames as u64)
            .ok_or(Error::InvalidFrame)?;
        writer.position = match writer.kind {
            Kind::Fixed => next % writer.media.frames,
            Kind::Windowed => 0,
            Kind::Growing { .. } => next,
        };
        self.account(io)
    }
    fn check_window_density(
        &mut self,
        writer: &Writer,
        staged: &Staging,
        span: WriteSpan,
    ) -> Result<(), Error> {
        if span.frames > self.q
            || span
                .offset
                .checked_add(span.frames)
                .is_none_or(|n| n > self.q)
        {
            return Err(Error::InvalidFrame);
        }
        let next = span
            .position
            .checked_add(span.frames as u64)
            .ok_or(Error::InvalidFrame)?;
        if next > writer.media.frames {
            return Err(Error::InvalidFrame);
        }
        let origin = writer.media.origin.0;
        let begin = origin
            .checked_add(span.position)
            .ok_or(Error::InvalidFrame)?;
        let end = origin.checked_add(next).ok_or(Error::InvalidFrame)?;
        for bucket in span.position / self.q as u64..=(next - 1) / self.q as u64 {
            let low = origin + bucket * self.q as u64;
            let high = origin + ((bucket + 1) * self.q as u64).min(writer.media.frames);
            let old = self
                .blocks
                .range_totals(writer.media.version, Frame(low)..Frame(high))?;
            let removed = self.blocks.range_totals(
                writer.media.version,
                Frame(low.max(begin))..Frame(high.min(end)),
            )?;
            let mut events = old.events - removed.events;
            let mut bytes = old.bytes - removed.bytes;
            for record in &staged.records[..staged.count] {
                let offset = record
                    .frame
                    .0
                    .checked_sub(staged.frame.0)
                    .ok_or(Error::InvalidFrame)? as usize;
                if offset >= span.offset && offset < span.offset + span.frames {
                    let at = begin + (offset - span.offset) as u64;
                    if low <= at && at < high {
                        events += 1;
                        bytes += 12 + record.message().as_bytes().len() as u64;
                    }
                }
            }
            if events > self.config.events_per_tick as u64 * 2
                || bytes - 12 * events > self.config.bytes_per_tick as u64 * 2
            {
                return Err(Error::Capacity);
            }
        }
        Ok(())
    }
    pub fn totals<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        media: &Media,
    ) -> Result<Totals, Error> {
        let total = self.blocks.totals(media.version)?;
        self.account(io)?;
        Ok(Totals {
            bytes: total.bytes.try_into().map_err(|_| Error::Capacity)?,
        })
    }
    pub fn read_bytes_with<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        media: &Media,
        offset: usize,
        output: &mut [u8],
        class: WorkClass,
    ) -> Result<usize, Error> {
        let selected = self.blocks.record_at_byte(media.version, offset as u64)?;
        self.account(io)?;
        let Some((record, within)) = selected else {
            return Ok(0);
        };
        let mut bytes = [0; 16];
        let message = record.message();
        let payload = message.as_bytes();
        bytes[..8].copy_from_slice(&(record.frame.0 - media.origin.0).to_le_bytes());
        bytes[8..12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes[12..12 + payload.len()].copy_from_slice(payload);
        let n = output.len().min(12 + payload.len() - within);
        output[..n].copy_from_slice(&bytes[within..within + n]);
        io.store.charge_work(class, n, 0)?;
        Ok(n)
    }
    pub fn byte_before<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        media: &Media,
        frame: u64,
    ) -> Result<usize, Error> {
        if frame > media.frames {
            return Err(Error::InvalidFrame);
        }
        let offset = self.blocks.byte_before(
            media.version,
            Frame(
                media
                    .origin
                    .0
                    .checked_add(frame)
                    .ok_or(Error::InvalidFrame)?,
            ),
        )?;
        self.account(io)?;
        offset.try_into().map_err(|_| Error::Capacity)
    }
    pub fn maintain<S: Sample>(
        &mut self,
        io: &mut RenderIo<'_, S>,
        budget: usize,
    ) -> Result<usize, Error> {
        let n = self.blocks.maintain(budget);
        if self.blocks.usage().retirement == 0 {
            for credit in &mut self.credits {
                if credit.references == 0 {
                    self.credited -= credit.events;
                    credit.events = 0;
                }
            }
        }
        self.account(io)?;
        Ok(n)
    }
    pub fn begin_cleanup<S: Sample>(&mut self, _: &mut RenderIo<'_, S>) -> Result<(), Error> {
        Ok(())
    }
    pub fn finish_cleanup<S: Sample>(&mut self, _: &mut ResourceStore<S>) -> Result<(), Error> {
        if self.index_leases() != 0
            || self.blocks.usage().retirement != 0
            || self.bases.iter().any(|b| b.snapshot.is_some())
        {
            return Err(Error::Busy);
        }
        Ok(())
    }
}
