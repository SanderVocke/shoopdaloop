use super::{
    Cursor, Emission, EmissionKind, Error, Event, Interpreter, MidiState, Take, TakeStatus, Work,
};
use crate::{
    pipeline::PipelineError,
    records::{Identity, Mode},
    state::StateProcessor,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessError {
    Midi(Error),
    Pipeline(PipelineError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Report {
    pub error: Option<Error>,
    pub take: TakeStatus,
    pub work: Work,
}

pub struct Engine {
    binding: Identity,
    take: Take,
    input: Interpreter,
    output: Interpreter,
    cursor: Option<Cursor>,
    mode: Mode,
    next_frame: Option<u64>,
    next_position: u64,
    window_events: u32,
    recovery: Option<Emission>,
    cleanup: Option<Emission>,
    desynchronized: bool,
    work: Work,
}
impl Engine {
    pub fn prepare(
        binding: Identity,
        record_events: u32,
        window_events: u32,
    ) -> Result<Self, Error> {
        if binding.generation == 0 || window_events == 0 {
            return Err(Error::InvalidState);
        }
        Ok(Self {
            binding,
            take: Take::prepare(record_events)?,
            input: Interpreter::default(),
            output: Interpreter::default(),
            cursor: None,
            mode: Mode::Stopped,
            next_frame: None,
            next_position: 0,
            window_events,
            recovery: None,
            cleanup: None,
            desynchronized: false,
            work: Work::default(),
        })
    }
    pub fn take(&self) -> &Take {
        &self.take
    }
    pub fn input_state(&self) -> &MidiState {
        self.input.state().unwrap()
    }
    pub fn output_state(&self) -> Result<&MidiState, Error> {
        if self.recovery.is_some() || self.cleanup.is_some() || self.desynchronized {
            Err(Error::Busy)
        } else {
            self.output.state()
        }
    }
    pub fn work(&self) -> Work {
        self.work
    }
    pub fn recovery_pending(&self) -> bool {
        self.recovery.is_some()
    }
    pub fn cleanup_pending(&self) -> bool {
        self.cleanup.is_some()
    }
    pub fn prepared_bytes(&self) -> usize {
        size_of::<Self>() + self.take.prepared_bytes() - size_of::<Take>()
    }
    pub(crate) fn validate(
        &self,
        state: &StateProcessor,
        frames: usize,
        events: &[Event],
    ) -> Result<(), Error> {
        let frames = u32::try_from(frames).map_err(|_| Error::Overflow)?;
        Event::validate_window(events, frames)?;
        if self.next_frame.is_some_and(|next| next != state.frame()) {
            return Err(Error::InvalidState);
        }
        let slot = state
            .loop_state(self.binding)
            .map_err(|_| Error::InvalidState)?;
        if self.next_frame.is_none() && slot.state.mode != crate::transitions::Mode::Stopped {
            return Err(Error::Unsupported);
        }
        for plan in slot.planned.iter().flatten() {
            match plan.operation {
                crate::transitions::Operation::Play { position, .. } if position != 0 => {
                    return Err(Error::Unsupported)
                }
                crate::transitions::Operation::Seek { .. } => return Err(Error::Unsupported),
                _ => {}
            }
        }
        let quantum = state.quantum().ok_or(Error::InvalidState)?;
        let mut window = 0;
        let mut count = 0u32;
        for event in events {
            let current = event.offset / quantum;
            if current != window {
                window = current;
                count = 0;
            }
            count = count.checked_add(1).ok_or(Error::Overflow)?;
            if count > self.window_events {
                return Err(Error::Capacity);
            }
        }
        Ok(())
    }
    pub(crate) fn report(&self, error: Option<Error>) -> Report {
        Report {
            error,
            take: self.take.status(),
            work: self.work,
        }
    }
    pub(crate) fn fault(&mut self) {
        self.cleanup = None;
        if self.recovery.is_none() {
            self.recovery = Some(Emission::new(EmissionKind::Recovery));
        }
        self.desynchronized = true;
    }
    pub fn drain_recovery(
        &mut self,
        offset: u32,
        sink: &mut impl FnMut(Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if let Some(recovery) = &mut self.recovery {
            while !recovery.complete() {
                let before = self.output.steps();
                let result = recovery.step(self.take.initial(), &mut self.work, &mut |message| {
                    sink(Event { offset, message })?;
                    self.output.apply(message)
                });
                self.work.update_steps = self
                    .work
                    .update_steps
                    .saturating_add(self.output.steps() - before);
                result?;
            }
            self.recovery = None;
        }
        Ok(())
    }
    fn emit_boundary(
        &mut self,
        kind: EmissionKind,
        offset: u32,
        sink: &mut impl FnMut(Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut emission = Emission::new(kind);
        let source = if kind == EmissionKind::Restore {
            *self.take.initial()
        } else {
            *self.output.state()?
        };
        while !emission.complete() {
            let before = self.output.steps();
            let result = emission.step(&source, &mut self.work, &mut |message| {
                sink(Event { offset, message })?;
                self.output.apply(message)
            });
            self.work.update_steps = self
                .work
                .update_steps
                .saturating_add(self.output.steps() - before);
            result?;
        }
        self.output = Interpreter::new(if kind == EmissionKind::Restore {
            *self.take.initial()
        } else {
            MidiState::default()
        });
        Ok(())
    }
    fn cleanup_output(
        &mut self,
        offset: u32,
        sink: &mut impl FnMut(Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let emission = self
            .cleanup
            .get_or_insert_with(|| Emission::new(EmissionKind::Cleanup));
        while !emission.complete() {
            emission.step(self.output.state()?, &mut self.work, &mut |message| {
                sink(Event { offset, message })
            })?;
        }
        self.cleanup = None;
        self.output = Interpreter::default();
        Ok(())
    }
    pub(crate) fn quantum(
        &mut self,
        state: &StateProcessor,
        base: usize,
        events: &[Event],
        sink: &mut impl FnMut(Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let q = state.quantum().ok_or(Error::InvalidState)?;
        let slot = state
            .loop_state(self.binding)
            .map_err(|_| Error::InvalidState)?;
        let record = slot.render;
        let offset = u32::try_from(base).map_err(|_| Error::Overflow)?;
        let end = offset.checked_add(q).ok_or(Error::Overflow)?;
        let start_index = events.partition_point(|e| e.offset < offset);
        let end_index = events.partition_point(|e| e.offset < end);
        let input = &events[start_index..end_index];
        self.next_frame = Some(state.frame());

        let mut result = Ok(());
        if self.mode == Mode::Recording && record.mode != Mode::Recording {
            result = self.take.finish();
        }
        if record.mode == Mode::Recording && record.begin_capture {
            if let Some(cursor) = self.cursor.take() {
                self.take.end_playback(cursor)?;
            }
            self.take.begin(self.input.state()?)?;
        }
        if record.mode == Mode::Recording {
            let capture = self.capture_window(q, offset, input);
            if result.is_ok() {
                result = capture;
            }
        }
        for event in input {
            let before = self.input.steps();
            self.input.apply(event.message)?;
            self.work.update_steps = self
                .work
                .update_steps
                .saturating_add(self.input.steps() - before);
        }

        let output_result = self.play_window(record, q, offset, sink);
        if let Err(error) = output_result {
            if self.cleanup.is_none()
                && error != Error::Incomplete
                && error != Error::Unsupported
                && error != Error::Busy
            {
                self.fault();
            }
            if result.is_ok() {
                result = Err(error);
            }
        }
        self.mode = record.mode;
        result
    }
    fn capture_window(&mut self, frames: u32, base: u32, input: &[Event]) -> Result<(), Error> {
        self.take.append_window_at(frames, base, input)
    }
    fn play_window(
        &mut self,
        record: crate::records::LoopRecord,
        q: u32,
        base: u32,
        sink: &mut impl FnMut(Event) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.drain_recovery(base, sink)?;
        let boundary = record.mode == Mode::Playing && record.position == 0;
        if self.cleanup.is_some()
            || ((self.mode == Mode::Playing || self.cursor.is_some())
                && (record.mode != Mode::Playing || boundary))
        {
            self.desynchronized = true;
            self.cleanup_output(base, sink)?;
            if let Some(cursor) = self.cursor.take() {
                self.take.end_playback(cursor)?;
            }
        }
        if self.desynchronized && record.mode == Mode::Playing && !boundary {
            return Err(Error::Busy);
        }
        if record.mode != Mode::Playing {
            self.desynchronized = false;
            return Ok(());
        }
        if record.content_length != self.take.frames() || self.take.status() != TakeStatus::Complete
        {
            return Err(Error::Incomplete);
        }
        if self.cursor.is_none() || boundary {
            if !boundary {
                return Err(Error::Unsupported);
            }
            self.cursor = Some(self.take.start_playback()?);
            self.emit_boundary(EmissionKind::Restore, base, sink)?;
            self.desynchronized = false;
        } else if record.position != self.next_position {
            return Err(Error::Unsupported);
        }
        let end = record
            .position
            .checked_add(u64::from(q))
            .ok_or(Error::Overflow)?;
        if end > self.take.frames() {
            return Err(Error::InvalidState);
        }
        let cursor = self.cursor.as_mut().unwrap();
        while let Some(event) = cursor.peek(&self.take)? {
            self.work.record_reads = self.work.record_reads.saturating_add(1);
            if event.frame >= end {
                break;
            }
            if event.frame < record.position {
                return Err(Error::InvalidState);
            }
            let offset = base
                .checked_add((event.frame - record.position) as u32)
                .ok_or(Error::Overflow)?;
            sink(Event {
                offset,
                message: event.message,
            })?;
            let before = self.output.steps();
            self.output.apply(event.message)?;
            self.work.update_steps = self
                .work
                .update_steps
                .saturating_add(self.output.steps() - before);
            self.work.accepted = self.work.accepted.saturating_add(1);
            cursor.acknowledge(&self.take)?;
        }
        self.next_position = end;
        Ok(())
    }
}

pub(crate) struct MidiObserver<'a, F> {
    pub engine: &'a mut Engine,
    pub events: &'a [Event],
    pub sink: &'a mut F,
    pub error: Option<Error>,
}
impl<F: FnMut(Event) -> Result<(), Error>> crate::pipeline::Observer for MidiObserver<'_, F> {
    fn quantum(&mut self, state: &StateProcessor, base: usize) {
        if let Err(error) = self.engine.quantum(state, base, self.events, self.sink) {
            if self.error.is_none() {
                self.error = Some(error);
            }
        }
    }
}
