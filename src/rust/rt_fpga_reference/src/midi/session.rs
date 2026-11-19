mod control;
use super::{
    history::History, ComparisonId, ContentStore, DifferenceService, Differences, Error,
    Interpreter, Message, MidiState, Record, SnapshotId, StateTransition, StoreLimits, StoreUsage,
    TakeStatus, ViewId,
};
use crate::{
    records::{Identity, Mode},
    state::StateProcessor,
};
use alloc::{boxed::Box, collections::BinaryHeap, vec::Vec};
use core::cmp::Reverse;

#[derive(Clone, Copy, Debug)]
pub struct SessionLimits {
    pub store: StoreLimits,
    pub loops: u16,
    pub inputs: u16,
    pub destinations: u16,
    pub input_events_per_quantum: u32,
    pub history_frames: u64,
    pub maintenance_steps: u32,
}
impl SessionLimits {
    pub fn session() -> Self {
        Self {
            store: StoreLimits::session(),
            loops: 128,
            inputs: 1,
            destinations: 8,
            input_events_per_quantum: 256,
            history_frames: 30 * 48_000,
            maintenance_steps: 64,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub input: u16,
    pub offset: u32,
    pub message: Message,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputEvent {
    pub destination: u16,
    pub offset: u32,
    pub message: Message,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Binding {
    pub loop_id: Identity,
    pub input: u16,
    pub destination: u16,
    pub channels: u16,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionWork {
    pub input_steps: u64,
    pub destination_steps: u64,
    pub activity_steps: u64,
    pub transition_steps: u64,
    pub record_reads: u64,
    pub context_visits: u64,
    pub published_bindings: u64,
    pub state_copy_bytes: u64,
    pub replay_steps: u64,
    pub comparison_work: u64,
    pub storage_work: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionReport {
    pub error: Option<Error>,
    pub work: SessionWork,
    pub usage: StoreUsage,
}
struct Context {
    binding: Option<Binding>,
    view: Option<ViewId>,
    writer: Option<ViewId>,
    active: Option<ViewId>,
    activity: Interpreter,
    transition: Option<StateTransition>,
    cleaning: bool,
    cursor: u32,
    position: u64,
    mode: Mode,
    desynchronized: bool,
    status: TakeStatus,
    record_limit: u32,
}
impl Context {
    fn new(record_limit: u32) -> Self {
        Self {
            binding: None,
            view: None,
            writer: None,
            active: None,
            activity: Interpreter::default(),
            transition: None,
            cleaning: false,
            cursor: 0,
            position: 0,
            mode: Mode::Stopped,
            desynchronized: false,
            status: TakeStatus::Empty,
            record_limit,
        }
    }
}
pub struct MidiSession {
    pub(super) store: ContentStore,
    pub(super) maps: DifferenceService,
    pub(super) outputs: Box<[Interpreter]>,
    inputs: Box<[Interpreter]>,
    histories: Box<[History]>,
    contexts: Box<[Context]>,
    recovery: Box<[u8]>,
    limits: SessionLimits,
    next_frame: Option<u64>,
    work: SessionWork,
    old_views: Vec<ViewId>,
    seeds: Box<[Option<SnapshotId>]>,
    owners: Box<[u16]>,
    heap: BinaryHeap<Reverse<(u32, u16)>>,
    transfer_events: u32,
    transfer_cells: u32,
    transfer_metadata: u32,
    publication_bindings: usize,
}
fn array<T>(n: usize, mut create: impl FnMut() -> T) -> Result<Box<[T]>, Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| Error::Capacity)?;
    v.extend((0..n).map(|_| create()));
    Ok(v.into_boxed_slice())
}
impl MidiSession {
    pub fn prepare(limits: SessionLimits) -> Result<Self, Error> {
        if limits.loops == 0
            || limits.loops > 128
            || limits.inputs == 0
            || limits.destinations == 0
            || limits.input_events_per_quantum == 0
            || limits.history_frames == 0
        {
            return Err(Error::Capacity);
        }
        let mut store = ContentStore::prepare(limits.store)?;
        let mut maps = DifferenceService::prepare(limits.store.snapshots, limits.destinations)?;
        let outputs = array(usize::from(limits.destinations), Interpreter::default)?;
        let mut histories = Vec::new();
        histories
            .try_reserve_exact(usize::from(limits.inputs))
            .map_err(|_| Error::Capacity)?;
        for _ in 0..limits.inputs {
            histories.push(History::prepare(
                &mut store,
                &mut maps,
                &outputs,
                limits.history_frames,
            )?);
        }
        let mut old_views = Vec::new();
        old_views
            .try_reserve_exact(usize::from(limits.loops) * 2)
            .map_err(|_| Error::Capacity)?;
        let mut heap = BinaryHeap::new();
        heap.try_reserve(usize::from(limits.loops))
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            store,
            maps,
            outputs,
            inputs: array(usize::from(limits.inputs), Interpreter::default)?,
            histories: histories.into_boxed_slice(),
            contexts: array(usize::from(limits.loops), || {
                Context::new(limits.store.max_events)
            })?,
            recovery: array(usize::from(limits.destinations), || 32)?,
            limits,
            next_frame: None,
            work: SessionWork::default(),
            old_views,
            seeds: array(usize::from(limits.inputs), || None)?,
            owners: array(usize::from(limits.destinations), || 0)?,
            heap,
            transfer_events: 64,
            transfer_cells: 256,
            transfer_metadata: 64,
            publication_bindings: 128,
        })
    }
    pub fn store(&self) -> &ContentStore {
        &self.store
    }
    pub fn work(&self) -> SessionWork {
        SessionWork {
            comparison_work: self.maps.work(),
            storage_work: self.store.work(),
            replay_steps: self.histories.iter().map(|h| h.replay_work).sum(),
            ..self.work
        }
    }
    pub fn report(&self, error: Option<Error>) -> SessionReport {
        SessionReport {
            error,
            work: self.work(),
            usage: self.store.usage(),
        }
    }
    pub fn prepared_bytes(&self) -> usize {
        size_of::<Self>() + self.store.prepared_bytes() - size_of::<ContentStore>()
            + self.maps.prepared_bytes()
            - size_of::<DifferenceService>()
            + (self.inputs.len() + self.outputs.len()) * size_of::<Interpreter>()
            + self
                .histories
                .iter()
                .map(History::prepared_bytes)
                .sum::<usize>()
            + self.contexts.len() * size_of::<Context>()
            + self.recovery.len()
            + self.old_views.capacity() * size_of::<ViewId>()
            + self.seeds.len() * size_of::<Option<SnapshotId>>()
            + self.owners.len() * size_of::<u16>()
            + self.heap.capacity() * size_of::<Reverse<(u32, u16)>>()
    }
    pub fn bind(&mut self, slot: u16, binding: Binding) -> Result<(), Error> {
        if binding.input >= self.limits.inputs
            || binding.destination >= self.limits.destinations
            || binding.channels == 0
            || binding.loop_id.generation == 0
        {
            return Err(Error::InvalidState);
        }
        if self
            .contexts
            .iter()
            .any(|c| c.binding.is_some_and(|b| b.loop_id == binding.loop_id))
        {
            return Err(Error::InvalidState);
        }
        let context = self
            .contexts
            .get_mut(usize::from(slot))
            .ok_or(Error::InvalidState)?;
        if context.binding.is_some() {
            return Err(Error::Busy);
        }
        context.binding = Some(binding);
        Ok(())
    }
    pub fn set_record_limit(&mut self, slot: u16, events: u32) -> Result<(), Error> {
        if events == 0 || events > self.limits.store.max_events {
            return Err(Error::Capacity);
        }
        let c = self
            .contexts
            .get_mut(usize::from(slot))
            .ok_or(Error::InvalidState)?;
        if c.mode != Mode::Stopped || c.writer.is_some() {
            return Err(Error::Busy);
        }
        c.record_limit = events;
        Ok(())
    }
    pub fn recovery_pending(&self, destination: u16) -> Result<bool, Error> {
        Ok(*self
            .recovery
            .get(usize::from(destination))
            .ok_or(Error::InvalidState)?
            != 32)
    }
    fn update_activity(&mut self, index: usize, message: Message) -> Result<(), Error> {
        let a = &mut self.contexts[index].activity;
        let before = a.steps();
        a.apply(message)?;
        self.work.activity_steps = self.work.activity_steps.saturating_add(a.steps() - before);
        Ok(())
    }
    pub fn content(&self, slot: u16) -> Result<Option<ViewId>, Error> {
        Ok(self
            .contexts
            .get(usize::from(slot))
            .ok_or(Error::InvalidState)?
            .view)
    }
    pub fn take_status(&self, slot: u16) -> Result<TakeStatus, Error> {
        Ok(self
            .contexts
            .get(usize::from(slot))
            .ok_or(Error::InvalidState)?
            .status)
    }
    pub fn input_state(&self, input: u16) -> Result<&MidiState, Error> {
        self.inputs
            .get(usize::from(input))
            .ok_or(Error::InvalidState)?
            .state()
    }
    pub fn output_state(&self, destination: u16) -> Result<&MidiState, Error> {
        if *self
            .recovery
            .get(usize::from(destination))
            .ok_or(Error::InvalidState)?
            != 32
        {
            return Err(Error::Busy);
        }
        if self.contexts.iter().any(|c| {
            c.binding.is_some_and(|b| b.destination == destination) && c.transition.is_some()
        }) {
            return Err(Error::Busy);
        }
        self.outputs[usize::from(destination)].state()
    }
    pub fn send(
        &mut self,
        event: OutputEvent,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if event.destination >= self.limits.destinations {
            return Err(Error::InvalidState);
        }
        if self.recovery[usize::from(event.destination)] != 32 {
            return Err(Error::Busy);
        }
        if self.contexts.iter().any(|c| {
            c.binding
                .is_some_and(|b| b.destination == event.destination)
                && c.transition.is_some()
        }) {
            return Err(Error::Busy);
        }
        self.emit(event, sink)
    }
    fn emit(
        &mut self,
        event: OutputEvent,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        sink(event)?;
        let output = &mut self.outputs[usize::from(event.destination)];
        let before = output.steps();
        output.begin(event.message)?;
        while !output
            .step_observed(|s, cell| self.maps.changed(&self.store, event.destination, s, cell))
        {
        }
        self.work.destination_steps = self
            .work
            .destination_steps
            .saturating_add(output.steps() - before);
        Ok(())
    }
    pub fn fault(&mut self) {
        for destination in 0..self.limits.destinations {
            self.fail_destination(destination);
        }
    }
    fn fail_destination(&mut self, destination: u16) {
        if self.recovery[usize::from(destination)] == 32 {
            self.recovery[usize::from(destination)] = 0;
        }
        for c in &mut self.contexts {
            if c.binding.is_some_and(|b| b.destination == destination) {
                c.desynchronized = true;
                c.transition = None;
                c.cleaning = false;
            }
        }
    }
    pub fn drain_recovery(
        &mut self,
        offset: u32,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        for destination in 0..self.limits.destinations {
            while self.recovery[usize::from(destination)] < 32 {
                let n = self.recovery[usize::from(destination)];
                let message =
                    Message::managed(&[0xb0 | (n / 2), if n % 2 == 0 { 120 } else { 123 }, 0])?;
                self.emit(
                    OutputEvent {
                        destination,
                        offset,
                        message,
                    },
                    sink,
                )?;
                for i in 0..self.contexts.len() {
                    if self.contexts[i].binding.is_some_and(|b| {
                        b.destination == destination && b.channels & (1 << (n / 2)) != 0
                    }) {
                        self.update_activity(i, message)?;
                    }
                }
                self.recovery[usize::from(destination)] += 1;
            }
        }
        Ok(())
    }
    pub(crate) fn validate(
        &mut self,
        state: &StateProcessor,
        frames: usize,
        events: &[InputEvent],
    ) -> Result<(), Error> {
        let q = state.quantum().ok_or(Error::InvalidState)?;
        if frames == 0
            || frames % q as usize != 0
            || self.next_frame.is_some_and(|f| f != state.frame())
        {
            return Err(Error::InvalidState);
        }
        for e in events {
            if e.input >= self.limits.inputs || e.offset as usize >= frames {
                return Err(Error::InvalidEvent);
            }
        }
        if events.windows(2).any(|w| w[0].offset > w[1].offset) {
            return Err(Error::InvalidEvent);
        }
        for input in 0..self.limits.inputs {
            let mut window = 0;
            let mut count = 0;
            for e in events.iter().filter(|e| e.input == input) {
                if e.offset / q != window {
                    window = e.offset / q;
                    count = 0;
                }
                count += 1;
                if count > self.limits.input_events_per_quantum {
                    return Err(Error::Capacity);
                }
            }
        }
        for c in &self.contexts {
            if let Some(b) = c.binding {
                let slot = state
                    .loop_state(b.loop_id)
                    .map_err(|_| Error::InvalidState)?;
                for plan in slot.planned.iter().flatten() {
                    match plan.operation {
                        crate::transitions::Operation::Seek { .. } => {
                            return Err(Error::Unsupported)
                        }
                        crate::transitions::Operation::Play { position, .. } if position != 0 => {
                            return Err(Error::Unsupported)
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
    fn transition(
        &mut self,
        index: usize,
        offset: u32,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        loop {
            let c = &self.contexts[index];
            let Some(transition) = c.transition else {
                return Ok(());
            };
            if transition.complete() {
                self.contexts[index].transition = None;
                return Ok(());
            }
            let b = c.binding.unwrap();
            let (next, message) = if c.cleaning {
                transition.propose(
                    &MidiState::default(),
                    c.activity.state()?,
                    &Differences::default(),
                )
            } else {
                let view = c.active.ok_or(Error::InvalidState)?;
                let initial = self.store.describe(view)?.initial;
                transition.propose(
                    self.store.state(initial)?,
                    self.outputs[usize::from(b.destination)].state()?,
                    self.maps.get(
                        &self.store,
                        ComparisonId {
                            target: initial,
                            destination: b.destination,
                        },
                    )?,
                )
            };
            if let Some(message) = message {
                self.emit(
                    OutputEvent {
                        destination: b.destination,
                        offset,
                        message,
                    },
                    sink,
                )?;
                self.update_activity(index, message)?;
            }
            self.work.transition_steps = self
                .work
                .transition_steps
                .saturating_add(next.steps() - transition.steps());
            self.contexts[index].transition = Some(next);
        }
    }
    fn start_recording(&mut self, index: usize, frame: u64) -> Result<(), Error> {
        let maximum = self
            .limits
            .store
            .max_events
            .div_ceil(self.limits.store.block_events);
        if self
            .limits
            .store
            .blocks
            .saturating_sub(self.store.usage().blocks as u32)
            < self.reserved_blocks()?.saturating_add(maximum)
        {
            return Err(Error::Capacity);
        }
        let b = self.contexts[index].binding.unwrap();
        let input = usize::from(b.input);
        let initial = if let Some(id) = self.seeds[input] {
            id
        } else {
            let id = self.histories[input].state_at(
                &mut self.store,
                &mut self.maps,
                &self.outputs,
                frame,
            )?;
            self.seeds[input] = Some(id);
            id
        };
        if let Some(old) = self.contexts[index].writer {
            self.store.can_release(core::slice::from_ref(&old))?;
        }
        let writer = self.store.writer(initial)?;
        if let Some(old) = self.contexts[index].writer {
            if let Err(error) = self.store.release(old) {
                self.store.release(writer)?;
                return Err(error);
            }
        }
        let c = &mut self.contexts[index];
        c.writer = Some(writer);
        c.status = TakeStatus::Recording;
        Ok(())
    }
    fn finish_recording(&mut self, index: usize) -> Result<(), Error> {
        let writer = self.contexts[index].writer.ok_or(Error::Incomplete)?;
        let frames = self.store.describe(writer)?.frames;
        self.store.finish(writer, frames)?;
        if let Some(old) = self.contexts[index].view {
            self.store.release(old)?;
        }
        let c = &mut self.contexts[index];
        c.view = Some(writer);
        c.writer = None;
        c.status = TakeStatus::Complete;
        Ok(())
    }
    fn prepare_playback(
        &mut self,
        index: usize,
        record: crate::records::LoopRecord,
        base: u32,
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let playing = record.mode == Mode::Playing && record.position < record.content_length;
        let boundary = playing && record.position == 0;
        let c = &mut self.contexts[index];
        if c.active.is_some() && (!playing || boundary || c.active != c.view) && !c.cleaning {
            c.transition = Some(StateTransition::cleanup(c.binding.unwrap().channels));
            c.cleaning = true;
        }
        if c.cleaning {
            self.transition(index, base, sink)?;
            let active = self.contexts[index].active.unwrap();
            self.store.release(active)?;
            let c = &mut self.contexts[index];
            c.active = None;
            c.cleaning = false;
            c.activity = Interpreter::default();
        }
        if !playing {
            return Ok(());
        }
        if self.contexts[index].desynchronized && !boundary {
            return Err(Error::Busy);
        }
        if self.contexts[index].active.is_none() {
            if !boundary {
                return Err(Error::Unsupported);
            }
            let c = &self.contexts[index];
            let view = c.view.ok_or(Error::Incomplete)?;
            let d = self.store.describe(view)?;
            if d.frames != record.content_length || c.status != TakeStatus::Complete {
                return Err(Error::Incomplete);
            }
            self.maps.get(
                &self.store,
                ComparisonId {
                    target: d.initial,
                    destination: c.binding.unwrap().destination,
                },
            )?;
            self.store.retain(view)?;
            let c = &mut self.contexts[index];
            c.active = Some(view);
            c.cursor = 0;
            c.position = 0;
            c.desynchronized = false;
            c.activity = Interpreter::new(*self.store.state(d.initial)?);
            self.work.state_copy_bytes = self
                .work
                .state_copy_bytes
                .saturating_add(size_of::<MidiState>() as u64);
            c.transition = Some(StateTransition::restore(c.binding.unwrap().channels));
        }
        self.transition(index, base, sink)?;
        if self.contexts[index].position != record.position {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
    fn push_head(&mut self, index: usize, base: u32, q: u32) -> Result<(), Error> {
        let c = &mut self.contexts[index];
        if c.desynchronized {
            return Ok(());
        }
        let Some(view) = c.active else {
            return Ok(());
        };
        while let Some(event) = self.store.read(view, c.cursor)? {
            self.work.record_reads = self.work.record_reads.saturating_add(1);
            if event.frame < c.position {
                return Err(Error::InvalidState);
            }
            if event.frame >= c.position + u64::from(q) {
                break;
            }
            let b = c.binding.unwrap();
            if event.message.as_bytes()[0] != 0xff
                && b.channels & (1 << (event.message.as_bytes()[0] & 15)) == 0
            {
                c.cursor += 1;
                continue;
            }
            self.heap.push(Reverse((
                base + (event.frame - c.position) as u32,
                index as u16,
            )));
            break;
        }
        Ok(())
    }
    pub(super) fn quantum(
        &mut self,
        state: &StateProcessor,
        base: u32,
        events: &[InputEvent],
        sink: &mut impl FnMut(OutputEvent) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let q = state.quantum().ok_or(Error::InvalidState)?;
        let start = state
            .frame()
            .checked_sub(u64::from(q))
            .ok_or(Error::InvalidState)?;
        self.next_frame = Some(state.frame());
        let mut error = None;
        if let Err(e) = self.drain_recovery(base, sink) {
            error = Some(e);
        }
        self.owners.fill(0);
        for i in 0..self.contexts.len() {
            let Some(binding) = self.contexts[i].binding else {
                continue;
            };
            self.work.context_visits = self.work.context_visits.saturating_add(1);
            let record = state
                .loop_state(binding.loop_id)
                .map_err(|_| Error::InvalidState)?
                .render;
            if self.contexts[i].mode == Mode::Recording && record.mode != Mode::Recording {
                if let Err(e) = self.finish_recording(i) {
                    error.get_or_insert(e);
                    self.contexts[i].status = TakeStatus::Incomplete;
                }
            }
            if record.mode == Mode::Recording && record.begin_capture {
                if let Err(e) = self.start_recording(i, start) {
                    error.get_or_insert(e);
                    self.contexts[i].status = TakeStatus::Incomplete;
                }
            }
            if record.mode == Mode::Playing && record.position < record.content_length {
                let mask = &mut self.owners[usize::from(binding.destination)];
                if *mask & binding.channels != 0 {
                    error.get_or_insert(Error::Unsupported);
                }
                *mask |= binding.channels;
            }
        }
        for seed in &mut self.seeds {
            if let Some(id) = seed.take() {
                self.store.release_snapshot(id)?;
            }
        }
        for event in events
            .iter()
            .filter(|e| e.offset >= base && e.offset < base + q)
        {
            let input = usize::from(event.input);
            let before = self.inputs[input].steps();
            if let Err(e) = self.histories[input].observe(
                &mut self.store,
                &mut self.maps,
                &self.outputs,
                &mut self.inputs[input],
                start + u64::from(event.offset - base),
                event.message,
            ) {
                error.get_or_insert(e);
            }
            self.work.input_steps = self
                .work
                .input_steps
                .saturating_add(self.inputs[input].steps() - before);
            for c in &mut self.contexts {
                let Some(b) = c.binding else {
                    continue;
                };
                let Some(writer) = c.writer else {
                    continue;
                };
                if b.input != event.input
                    || c.status != TakeStatus::Recording
                    || (event.message.as_bytes()[0] != 0xff
                        && b.channels & (1 << (event.message.as_bytes()[0] & 15)) == 0)
                {
                    continue;
                }
                let frame = self.store.describe(writer)?.frames + u64::from(event.offset - base);
                if self.store.describe(writer)?.events >= c.record_limit {
                    c.status = TakeStatus::Incomplete;
                    self.store.mark_incomplete(writer)?;
                    error.get_or_insert(Error::Capacity);
                    continue;
                }
                if let Err(e) = self.store.append(writer, frame, event.message) {
                    c.status = TakeStatus::Incomplete;
                    error.get_or_insert(e);
                }
            }
        }
        for h in &mut self.histories {
            if let Err(e) = h.advance(&mut self.store, state.frame()) {
                error.get_or_insert(e);
            }
        }
        for c in &mut self.contexts {
            if c.status == TakeStatus::Recording {
                if let Some(writer) = c.writer {
                    let end = self
                        .store
                        .describe(writer)?
                        .frames
                        .checked_add(u64::from(q))
                        .ok_or(Error::Overflow)?;
                    if let Err(e) = self.store.advance(writer, end) {
                        c.status = TakeStatus::Incomplete;
                        error.get_or_insert(e);
                    }
                }
            }
        }
        self.heap.clear();
        let overlap = error == Some(Error::Unsupported);
        for i in 0..self.contexts.len() {
            let Some(b) = self.contexts[i].binding else {
                continue;
            };
            let record = state
                .loop_state(b.loop_id)
                .map_err(|_| Error::InvalidState)?
                .render;
            if !overlap && self.recovery[usize::from(b.destination)] == 32 {
                if let Err(e) = self.prepare_playback(i, record, base, sink) {
                    if !matches!(e, Error::Busy | Error::Incomplete | Error::Unsupported)
                        && !self.contexts[i].cleaning
                    {
                        self.fail_destination(b.destination);
                    }
                    error.get_or_insert(e);
                } else if record.mode == Mode::Playing && record.position < record.content_length {
                    self.push_head(i, base, q)?;
                }
            }
            self.contexts[i].mode =
                if record.mode == Mode::Playing && record.position >= record.content_length {
                    Mode::Stopped
                } else {
                    record.mode
                };
        }
        while let Some(Reverse((offset, index))) = self.heap.pop() {
            let i = usize::from(index);
            let c = &self.contexts[i];
            let b = c.binding.unwrap();
            if self.recovery[usize::from(b.destination)] != 32 {
                continue;
            }
            let event = self
                .store
                .read(c.active.unwrap(), c.cursor)?
                .ok_or(Error::InvalidState)?;
            if event.message.as_bytes()[0] == 0xff && b.channels != u16::MAX {
                self.fail_destination(b.destination);
                error.get_or_insert(Error::Unsupported);
                continue;
            }
            match self.emit(
                OutputEvent {
                    destination: b.destination,
                    offset,
                    message: event.message,
                },
                sink,
            ) {
                Ok(()) => {
                    self.update_activity(i, event.message)?;
                    self.contexts[i].cursor += 1;
                    self.push_head(i, base, q)?;
                }
                Err(e) => {
                    self.fail_destination(b.destination);
                    error.get_or_insert(e);
                }
            }
        }
        for c in &mut self.contexts {
            if c.active.is_some() && !c.cleaning {
                c.position = c.position.saturating_add(u64::from(q));
            }
        }
        self.store.maintenance(self.limits.maintenance_steps);
        self.transfer_events = 64;
        self.transfer_cells = 256;
        self.transfer_metadata = 64;
        self.publication_bindings = 128;
        match error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

pub(crate) struct SessionObserver<'a, F> {
    pub session: &'a mut MidiSession,
    pub events: &'a [InputEvent],
    pub sink: &'a mut F,
    pub error: Option<Error>,
}
impl<F: FnMut(OutputEvent) -> Result<(), Error>> crate::pipeline::Observer
    for SessionObserver<'_, F>
{
    fn quantum(&mut self, state: &StateProcessor, base: usize) {
        if let Err(e) = self
            .session
            .quantum(state, base as u32, self.events, self.sink)
        {
            self.error.get_or_insert(e);
        }
    }
}
