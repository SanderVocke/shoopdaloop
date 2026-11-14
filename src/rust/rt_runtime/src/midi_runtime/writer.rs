use super::{
    Pool, PositionId, PositionJobs, PositionStatus, PreparedPosition, Store, Version, NEXT_SCOPE,
};
use crate::midi::EventRecord;
use crate::midi_state::{BasicUpdate, MidiState};
use crate::{Error, Frame};
use alloc::{boxed::Box, vec};
use core::sync::atomic::Ordering;

pub struct MediaImage {
    pub version: Version,
    pub origin: Frame,
    pub frames: u64,
    pub revision: u64,
    pub cut: Frame,
    pub initial: MidiState,
    pub frontier: MidiState,
    pub frontier_position: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    Fixed,
    Windowed,
    Growing { maximum_frames: u64 },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputStamp {
    pub sequence: u64,
    pub frame: Frame,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteSpan {
    pub position: u64,
    pub offset: usize,
    pub frames: usize,
}
#[derive(Clone, Copy)]
struct Content {
    version: Version,
    origin: Frame,
    frames: u64,
    revision: u64,
    cut: Frame,
    frontier: u64,
}
#[derive(Clone, Copy)]
struct Interval {
    content: Content,
    input: InputStamp,
    expected: u8,
    staged: u8,
    after_offset: usize,
}
#[derive(Clone, Copy)]
enum Phase {
    Preparing(PositionId),
    Ready,
    Active,
    Frozen,
}
#[derive(Clone, Copy)]
struct Writer {
    content: Content,
    mode: WriteMode,
    phase: Phase,
    bank: usize,
    next: Option<Frame>,
    interval: Option<Interval>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriterId {
    scope: u32,
    slot: u32,
    generation: u64,
}
pub struct WriterSet {
    scope: u32,
    writers: Pool<Writer>,
    generations: Box<[u64]>,
    states: Box<[MidiState]>,
    scratch: Box<[EventRecord]>,
    positions: PositionJobs,
    quantum: usize,
    events: usize,
    exhausted: bool,
    pub state_copy_cells: u64,
    pub state_update_steps: u64,
}
impl WriterSet {
    pub fn prepare(writers: usize, quantum: usize, events: usize) -> Result<Self, Error> {
        if writers == 0
            || writers > u32::MAX as usize
            || quantum == 0
            || events == 0
            || events > u16::MAX as usize
        {
            return Err(Error::InvalidConfig);
        }
        writers
            .checked_mul(4 * size_of::<MidiState>() + size_of::<Writer>() + 64)
            .and_then(|n| n.checked_add(events.checked_mul(size_of::<EventRecord>())?))
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let scope = NEXT_SCOPE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        Ok(Self {
            scope,
            writers: Pool::new(writers),
            generations: vec![1; writers].into_boxed_slice(),
            states: vec![MidiState::default(); writers * 4].into_boxed_slice(),
            scratch: vec![EventRecord::new(Frame(0), 0, &[0xff])?; events].into_boxed_slice(),
            positions: PositionJobs::prepare(writers)?,
            quantum,
            events,
            exhausted: false,
            state_copy_cells: 0,
            state_update_steps: 0,
        })
    }
    fn writer(&self, id: WriterId) -> Result<Writer, Error> {
        if id.scope != self.scope
            || self.generations.get(id.slot.wrapping_sub(1) as usize) != Some(&id.generation)
        {
            return Err(Error::InvalidId);
        }
        self.writers.live(id.slot)
    }
    fn state_index(id: WriterId, bank: usize, frontier: bool) -> usize {
        (id.slot as usize - 1) * 4 + bank * 2 + usize::from(frontier)
    }
    fn copy_state(&mut self, index: usize, state: MidiState) {
        self.states[index] = state;
        self.state_copy_cells = self
            .state_copy_cells
            .saturating_add(crate::midi_state::BASIC_STATE_CELLS as u64);
    }
    fn apply_state(&mut self, index: usize, event: EventRecord) {
        let mut update = BasicUpdate::begin(event.message()).expect("managed event");
        while !update.complete() {
            update.step(&mut self.states[index]);
            self.state_update_steps = self.state_update_steps.saturating_add(1);
        }
    }
    pub fn begin(
        &mut self,
        store: &mut Store,
        source: &MediaImage,
        mode: WriteMode,
        position: u64,
    ) -> Result<WriterId, Error> {
        if self.writers.available() == 0 {
            return Err(if self.exhausted {
                Error::Exhausted
            } else {
                Error::Capacity
            });
        }
        if position > source.frames
            || matches!(mode, WriteMode::Fixed)
                && (source.frames == 0
                    || position >= source.frames
                    || source.frames % self.quantum as u64 != 0)
            || matches!(mode, WriteMode::Growing { maximum_frames } if maximum_frames < source.frames || position != source.frames)
        {
            return Err(Error::InvalidFrame);
        }
        let end = Frame(
            source
                .origin
                .0
                .checked_add(source.frames)
                .ok_or(Error::InvalidFrame)?,
        );
        let target = Frame(
            source
                .origin
                .0
                .checked_add(position)
                .ok_or(Error::InvalidFrame)?,
        );
        let clip = store.clip(source.version, source.origin..end)?;
        let normalized = store.normalize(clip);
        store.release(clip)?;
        let version = normalized?;
        let prepared = self
            .positions
            .begin(store, version, &source.initial, target);
        let prepared = match prepared {
            Ok(job) => job,
            Err(error) => {
                store.release(version)?;
                return Err(error);
            }
        };
        let slot = self.writers.insert(Writer {
            content: Content {
                version,
                origin: source.origin,
                frames: source.frames,
                revision: source.revision,
                cut: source.cut,
                frontier: position,
            },
            mode,
            phase: Phase::Preparing(prepared),
            bank: 0,
            next: None,
            interval: None,
        });
        let id = WriterId {
            scope: self.scope,
            slot,
            generation: self.generations[slot as usize - 1],
        };
        self.copy_state(Self::state_index(id, 0, false), source.initial);
        Ok(id)
    }
    pub fn progress(
        &mut self,
        store: &mut Store,
        id: WriterId,
        steps: usize,
    ) -> Result<PositionStatus, Error> {
        let mut writer = self.writer(id)?;
        let Phase::Preparing(job) = writer.phase else {
            return Ok(PositionStatus::Ready);
        };
        let status = self.positions.progress(store, job, steps)?;
        if status == PositionStatus::Ready {
            let position = self.positions.finish(job)?;
            self.copy_state(Self::state_index(id, writer.bank, true), position.state);
            store.release(position.version)?;
            writer.phase = Phase::Ready;
            self.writers.replace(id.slot, writer);
        }
        Ok(status)
    }
    pub fn activate(&mut self, id: WriterId, frame: Frame) -> Result<(), Error> {
        let mut writer = self.writer(id)?;
        if !matches!(writer.phase, Phase::Ready) || frame.0 % self.quantum as u64 != 0 {
            return Err(Error::Busy);
        }
        writer.phase = Phase::Active;
        writer.next = Some(frame);
        self.writers.replace(id.slot, writer);
        Ok(())
    }
    pub fn handoff_position(
        &mut self,
        id: WriterId,
        position: &PreparedPosition,
    ) -> Result<(), Error> {
        let mut writer = self.writer(id)?;
        if writer.interval.is_some()
            || position.expected != writer.content.version
            || position.target < writer.content.origin
            || position.target.0 - writer.content.origin.0 > writer.content.frames
        {
            return Err(Error::StaleRevision);
        }
        writer.content.frontier = position.target.0 - writer.content.origin.0;
        self.copy_state(Self::state_index(id, writer.bank, true), position.state);
        self.writers.replace(id.slot, writer);
        Ok(())
    }
    pub fn begin_interval(
        &mut self,
        store: &mut Store,
        id: WriterId,
        input: InputStamp,
        expected_spans: u8,
    ) -> Result<(), Error> {
        let mut writer = self.writer(id)?;
        if !matches!(writer.phase, Phase::Active)
            || writer.interval.is_some()
            || writer.next != Some(input.frame)
            || input.sequence == 0
            || expected_spans > 2
        {
            return Err(Error::InvalidFrame);
        }
        input.frame.checked_add(self.quantum)?;
        let mut content = writer.content;
        content.version = store.retain(content.version)?;
        for frontier in [false, true] {
            let state = self.states[Self::state_index(id, writer.bank, frontier)];
            self.copy_state(Self::state_index(id, writer.bank ^ 1, frontier), state);
        }
        writer.interval = Some(Interval {
            content,
            input,
            expected: expected_spans,
            staged: 0,
            after_offset: 0,
        });
        self.writers.replace(id.slot, writer);
        Ok(())
    }
    pub fn stage_span(
        &mut self,
        store: &mut Store,
        id: WriterId,
        input: InputStamp,
        before_input: &MidiState,
        originals: &[EventRecord],
        span: WriteSpan,
    ) -> Result<(), Error> {
        let mut writer = self.writer(id)?;
        let mut interval = writer.interval.ok_or(Error::InvalidState)?;
        if input != interval.input
            || interval.staged == interval.expected
            || originals.len() > self.events
            || span.offset < interval.after_offset
            || span
                .offset
                .checked_add(span.frames)
                .is_none_or(|n| n > self.quantum)
            || originals.iter().any(|event| {
                event.frame < input.frame || event.frame.0 - input.frame.0 >= self.quantum as u64
            })
            || originals
                .windows(2)
                .any(|pair| (pair[0].frame, pair[0].order) >= (pair[1].frame, pair[1].order))
        {
            return Err(Error::InvalidEvent);
        }
        let next = span
            .position
            .checked_add(span.frames as u64)
            .ok_or(Error::InvalidFrame)?;
        let maximum = match writer.mode {
            WriteMode::Growing { maximum_frames } => maximum_frames,
            _ => interval.content.frames,
        };
        if next > maximum
            || span.frames != 0 && span.position != 0 && span.position != interval.content.frontier
        {
            return Err(Error::InvalidFrame);
        }
        if matches!(writer.mode, WriteMode::Growing { .. })
            && span.position != interval.content.frames
        {
            return Err(Error::InvalidFrame);
        }
        if span.position == 0 && !before_input.is_basic() {
            return Err(Error::InvalidParameter);
        }
        let begin = Frame(
            interval
                .content
                .origin
                .0
                .checked_add(span.position)
                .ok_or(Error::InvalidFrame)?,
        );
        let end = begin.checked_add(span.frames)?;
        let revision = interval
            .content
            .revision
            .checked_add(u64::from(span.frames != 0))
            .ok_or(Error::Exhausted)?;
        let mut count = 0;
        for event in originals {
            let offset = (event.frame.0 - input.frame.0) as usize;
            if offset >= span.offset && offset < span.offset + span.frames {
                self.scratch[count] = EventRecord::new(
                    begin.checked_add(offset - span.offset)?,
                    u64::from(event.order),
                    event.message().as_bytes(),
                )?;
                count += 1;
            }
        }
        let bank = writer.bank ^ 1;
        let mut next_version = interval.content.version;
        if span.frames != 0 {
            next_version =
                store.edit(interval.content.version, begin..end, &self.scratch[..count])?;
            if span.position == 0 {
                self.copy_state(Self::state_index(id, bank, false), *before_input);
                for event in originals
                    .iter()
                    .filter(|event| event.frame.0 - input.frame.0 < span.offset as u64)
                {
                    self.apply_state(Self::state_index(id, bank, false), *event);
                }
                let initial = self.states[Self::state_index(id, bank, false)];
                self.copy_state(Self::state_index(id, bank, true), initial);
            }
            for index in 0..count {
                self.apply_state(Self::state_index(id, bank, true), self.scratch[index]);
            }
            store.release(interval.content.version)?;
        }
        interval.content.version = next_version;
        interval.content.revision = revision;
        if span.frames != 0 {
            interval.content.frontier = next;
        }
        if matches!(writer.mode, WriteMode::Growing { .. }) {
            interval.content.frames = next;
        }
        interval.staged += 1;
        interval.after_offset = span.offset + span.frames;
        writer.interval = Some(interval);
        self.writers.replace(id.slot, writer);
        Ok(())
    }
    pub fn commit_interval(&mut self, store: &mut Store, id: WriterId) -> Result<Frame, Error> {
        let mut writer = self.writer(id)?;
        let mut interval = writer.interval.ok_or(Error::InvalidState)?;
        if interval.staged != interval.expected {
            return Err(Error::Incomplete);
        }
        let end = interval.input.frame.checked_add(self.quantum)?;
        interval.content.cut = end;
        store.release(writer.content.version)?;
        writer.content = interval.content;
        writer.interval = None;
        writer.bank ^= 1;
        writer.next = Some(end);
        self.writers.replace(id.slot, writer);
        Ok(end)
    }
    pub fn abort_interval(&mut self, store: &mut Store, id: WriterId) -> Result<(), Error> {
        let mut writer = self.writer(id)?;
        let interval = writer.interval.take().ok_or(Error::InvalidState)?;
        store.release(interval.content.version)?;
        writer.phase = Phase::Frozen;
        self.writers.replace(id.slot, writer);
        Ok(())
    }
    pub fn snapshot(&mut self, store: &mut Store, id: WriterId) -> Result<MediaImage, Error> {
        let writer = self.writer(id)?;
        if matches!(writer.phase, Phase::Preparing(_)) {
            return Err(Error::Busy);
        }
        Ok(MediaImage {
            version: store.retain(writer.content.version)?,
            origin: writer.content.origin,
            frames: writer.content.frames,
            revision: writer.content.revision,
            cut: writer.content.cut,
            initial: self.states[Self::state_index(id, writer.bank, false)],
            frontier: self.states[Self::state_index(id, writer.bank, true)],
            frontier_position: writer.content.frontier,
        })
    }
    pub fn release(&mut self, store: &mut Store, id: WriterId) -> Result<(), Error> {
        let writer = self.writer(id)?;
        if let Phase::Preparing(job) = writer.phase {
            self.positions.cancel(store, job)?;
        }
        if let Some(interval) = writer.interval {
            store.release(interval.content.version)?;
        }
        store.release(writer.content.version)?;
        assert!(self.writers.release(id.slot));
        let generation = &mut self.generations[id.slot as usize - 1];
        if let Some(next) = generation.checked_add(1) {
            *generation = next;
            self.writers.reclaim(id.slot);
        } else {
            self.exhausted = true;
        }
        Ok(())
    }
}
