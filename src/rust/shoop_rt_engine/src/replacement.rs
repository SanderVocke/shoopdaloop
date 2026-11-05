//! A private, prepared replacement of the complete engine model.

use alloc::boxed::Box;
use alloc::vec;

use crate::{
    endpoint::ControlEndpoint,
    jobs::Job,
    model::{Controls, Model},
    queue::{Staged, HARD},
};
use shoop_rt_protocol::{BusControl, Outcome, Refusal, ReplacementEdit, Request};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum State {
    Empty,
    Receiving,
    Preparing,
    Prepared,
    Committing,
    Cancelling,
}
#[derive(Clone, Copy, Default)]
pub(crate) struct Initial {
    pub frames: u64,
    pub audio: Option<u64>,
    pub midi: Option<u64>,
}
pub(crate) struct Replacement {
    pub id: Option<u64>,
    pub state: State,
    last_id: u64,
    base_revision: u64,
    pub candidate: Model,
    pub initial: Box<[Option<Initial>]>,
}
impl Replacement {
    pub(crate) fn new(strips: usize, loops: usize, buses: usize, program_segments: usize) -> Self {
        Self {
            id: None,
            state: State::Empty,
            last_id: 0,
            base_revision: 0,
            candidate: Model::new(
                strips,
                loops,
                buses,
                0,
                Default::default(),
                program_segments,
            ),
            initial: vec![None; loops].into_boxed_slice(),
        }
    }
    pub(crate) fn clear(&mut self) {
        self.id = None;
        self.state = State::Empty;
        self.initial.fill(None);
    }
}

impl ControlEndpoint {
    pub(crate) fn replacement_request(
        &mut self,
        correlation: u64,
        request: Request,
    ) -> Result<Option<Outcome>, Refusal> {
        let id = match request {
            Request::ReplacementBegin { id } => {
                if id == 0 || id <= self.replacement.last_id {
                    return Err(Refusal::InvalidId);
                }
                if self.replacement.state != State::Empty
                    || self.job.is_some()
                    || self.family.id.is_some()
                {
                    return Err(Refusal::Busy);
                }
                let stage = &mut self.replacement;
                stage.last_id = id;
                stage.id = Some(id);
                stage.state = State::Receiving;
                stage.base_revision = self.model.revision;
                stage.candidate.copy_from(&self.model);
                stage.candidate.strips.fill(None);
                stage.candidate.loops.fill(None);
                stage.candidate.spans.fill(None);
                stage.candidate.buses.fill(None);
                stage.initial.fill(None);
                return Ok(Some(Outcome::Done));
            }
            Request::ReplacementEdit { id, .. }
            | Request::ReplacementPrepare { id }
            | Request::ReplacementCommit { id }
            | Request::ReplacementCancel { id } => id,
            _ => return Err(Refusal::Internal),
        };
        if self.replacement.id != Some(id) {
            return Err(Refusal::InvalidId);
        }
        match request {
            Request::ReplacementEdit { edit, .. } => {
                if self.replacement.state != State::Receiving {
                    return Err(Refusal::Busy);
                }
                core::mem::swap(&mut self.next, &mut self.replacement.candidate);
                let result = self.replacement_edit(edit);
                core::mem::swap(&mut self.next, &mut self.replacement.candidate);
                result.map(|()| Some(Outcome::Done))
            }
            Request::ReplacementPrepare { .. } => {
                if self.replacement.state != State::Receiving
                    || self.job.is_some()
                    || self.model.revision != self.replacement.base_revision
                {
                    return Err(Refusal::Busy);
                }
                self.validate_replacement()?;
                core::mem::swap(&mut self.next, &mut self.replacement.candidate);
                for slot in 0..self.model.strips.len() {
                    if self.model.strips[slot].is_some() || self.next.strips[slot].is_some() {
                        self.touched_strips[slot] = HARD;
                    }
                }
                for slot in 0..self.model.loops.len() {
                    if self.model.loops[slot].is_some() || self.next.loops[slot].is_some() {
                        self.touched_loops[slot] = HARD;
                    }
                }
                self.touched_buses.fill(HARD);
                self.replacement.state = State::Preparing;
                self.job = Some(Job::replacement(correlation));
                Ok(None)
            }
            Request::ReplacementCommit { .. } => {
                if self.replacement.state != State::Prepared
                    || self.model.revision != self.replacement.base_revision
                {
                    return Err(Refusal::Busy);
                }
                self.job
                    .as_mut()
                    .ok_or(Refusal::Busy)?
                    .commit_replacement(correlation)?;
                self.replacement.state = State::Committing;
                Ok(None)
            }
            Request::ReplacementCancel { .. } => {
                match self.replacement.state {
                    State::Receiving => {
                        self.replacement.clear();
                        return Ok(Some(Outcome::Done));
                    }
                    State::Preparing | State::Prepared => {
                        if let Some(previous) = self
                            .job
                            .as_mut()
                            .ok_or(Refusal::Busy)?
                            .cancel_replacement(correlation)?
                        {
                            self.answer(previous, Outcome::Refused(Refusal::Busy));
                        }
                        self.replacement.state = State::Cancelling;
                    }
                    _ => return Err(Refusal::Busy),
                }
                Ok(None)
            }
            _ => Err(Refusal::Internal),
        }
    }

    fn replacement_edit(&mut self, edit: ReplacementEdit) -> Result<(), Refusal> {
        let request = match edit {
            ReplacementEdit::Composite(d) => {
                let slot = d.loop_id.0.index as usize;
                let entry = self.next.find_loop(d.loop_id).ok_or(Refusal::InvalidId)?;
                if entry.program.is_some()
                    || d.previous.is_some()
                    || d.expected_version != 0
                    || d.version == 0
                    || d.unit_frames == 0
                    || d.unit_frames % u64::from(self.limits.quantum) != 0
                    || d.quantum != self.limits.quantum
                    || d.sample_rate != self.cells.driver().sample_rate
                    || d.unit_frames.checked_mul(d.iterations).is_none()
                {
                    return Err(Refusal::InvalidValue);
                }
                if d.entries > self.limits.composite_entries
                    || d.segments > self.limits.composite_segments
                {
                    return Err(Refusal::Capacity);
                }
                self.next.loops[slot].as_mut().unwrap().program = Some(d);
                return Ok(());
            }
            ReplacementEdit::CompositeSpan {
                loop_id,
                index,
                span,
            } => {
                let d = self
                    .next
                    .find_loop(loop_id)
                    .and_then(|l| l.program)
                    .ok_or(Refusal::InvalidId)?;
                if index as usize != self.next.program_spans(loop_id).count() || index >= d.segments
                {
                    return Err(Refusal::InvalidValue);
                }
                let child = self.next.find_loop(span.target).ok_or(Refusal::InvalidId)?;
                if span.target == loop_id
                    || span.start >= span.end
                    || span.end > d.iterations
                    || span.occurrence_start != span.start
                    || span.ordinal >= d.entries
                    || d.kind == shoop_rt_protocol::CompositeKind::Regular
                        && span.mode != shoop_rt_protocol::LoopMode::Playing
                    || child.program.is_some()
                        && span.mode == shoop_rt_protocol::LoopMode::Recording
                    || self.next.program_spans(loop_id).last().is_some_and(|old| {
                        old.target > span.target
                            || old.target == span.target
                                && (old.end > span.start
                                    || old.end == span.start && old.mode == span.mode)
                    })
                {
                    return Err(Refusal::InvalidValue);
                }
                let free = self
                    .next
                    .spans
                    .iter_mut()
                    .find(|s| s.is_none())
                    .ok_or(Refusal::Capacity)?;
                *free = Some(crate::model::OwnedSpan {
                    source: loop_id,
                    span,
                });
                return Ok(());
            }
            ReplacementEdit::RepeatSync { loop_id, enabled } => {
                let entry = self
                    .next
                    .loops
                    .get_mut(loop_id.0.index as usize)
                    .and_then(Option::as_mut)
                    .filter(|l| l.id == loop_id)
                    .ok_or(Refusal::InvalidId)?;
                entry.repeat_sync = enabled;
                return Ok(());
            }
            ReplacementEdit::Bus { bus, channels } => Request::CreateBus { bus, channels },
            ReplacementEdit::BusControl { bus, control } => {
                self.set_replacement_bus(bus, control)?;
                return Ok(());
            }
            ReplacementEdit::Strip {
                strip,
                shape,
                controls,
            } => {
                if !controls.input_gain.is_finite()
                    || controls.input_gain < 0.
                    || !controls.output_gain.is_finite()
                    || controls.output_gain < 0.
                    || !(-1.0..=1.0).contains(&controls.input_balance)
                    || !(-1.0..=1.0).contains(&controls.output_balance)
                {
                    return Err(Refusal::InvalidValue);
                }
                let change = match self.stage(Request::CreateStrip { strip, shape }, true) {
                    Staged::Change(change) => change,
                    Staged::Refused(reason) => return Err(reason),
                    _ => return Err(Refusal::Busy),
                };
                self.next.apply(change);
                self.next.strip_mut(strip).expect("created strip").controls = Controls {
                    input_gain: controls.input_gain,
                    input_balance: controls.input_balance,
                    monitoring: controls.monitoring,
                    automatic_monitoring: false,
                    output_gain: controls.output_gain,
                    output_balance: controls.output_balance,
                    output_mute: controls.output_mute,
                };
                return Ok(());
            }
            ReplacementEdit::Loop {
                loop_id,
                strip,
                sync,
                frames,
                audio,
                midi,
                gain,
                balance,
            } => {
                if frames > u32::MAX as u64
                    || !(0.0..=1.0).contains(&gain)
                    || !(-1.0..=1.0).contains(&balance)
                    || sync == Some(loop_id)
                {
                    return Err(Refusal::InvalidValue);
                }
                let change = match self.stage(Request::CreateLoop { loop_id, strip }, true) {
                    Staged::Change(change) => change,
                    Staged::Refused(reason) => return Err(reason),
                    _ => return Err(Refusal::Busy),
                };
                self.next.apply(change);
                let slot = loop_id.0.index as usize;
                let entry = self.next.loops[slot].as_mut().expect("created loop");
                entry.sync = sync;
                entry.gain = gain;
                entry.balance = balance;
                self.replacement.initial[slot] = Some(Initial {
                    frames,
                    audio,
                    midi,
                });
                return Ok(());
            }
            ReplacementEdit::Connect { port, host } => Request::Connect {
                port,
                host,
                connected: true,
            },
            ReplacementEdit::Route { port, bus } => Request::Route {
                port,
                bus,
                connected: true,
            },
        };
        match self.stage(request, true) {
            Staged::Change(change) => {
                self.next.apply(change);
                Ok(())
            }
            Staged::Done => Ok(()),
            Staged::Refused(reason) => Err(reason),
            Staged::Wait => Err(Refusal::Busy),
        }
    }

    fn set_replacement_bus(
        &mut self,
        bus: shoop_rt_protocol::BusId,
        control: BusControl,
    ) -> Result<(), Refusal> {
        let entry = self.next.bus_mut(bus).ok_or(Refusal::InvalidId)?;
        match control {
            BusControl::Gain(value) if !value.is_finite() || value < 0. => {
                return Err(Refusal::InvalidValue)
            }
            BusControl::Balance(value)
                if !(-1.0..=1.0).contains(&value) || (entry.channels != 2 && value != 0.) =>
            {
                return Err(Refusal::InvalidValue)
            }
            _ => {}
        }
        entry.apply(control);
        Ok(())
    }

    fn validate_replacement(&self) -> Result<(), Refusal> {
        let model = &self.replacement.candidate;
        let entries: u64 = model
            .loops
            .iter()
            .flatten()
            .filter_map(|l| l.program)
            .map(|p| u64::from(p.entries))
            .sum();
        if entries > u64::from(self.limits.composite_entries) {
            return Err(Refusal::Capacity);
        }
        for (slot, entry) in model.loops.iter().enumerate() {
            let Some(entry) = entry else { continue };
            if let Some(source) = entry.sync {
                if self.replacement.candidate.find_loop(source).is_none()
                    || !self.replacement.candidate.sync_allowed(entry.id, source)
                {
                    return Err(Refusal::InvalidId);
                }
            }
            let initial = self.replacement.initial[slot].ok_or(Refusal::InvalidValue)?;
            if let Some(d) = entry.program {
                if initial.frames != 0
                    || initial.audio.is_some()
                    || initial.midi.is_some()
                    || model.program_spans(entry.id).count() != d.segments as usize
                {
                    return Err(Refusal::InvalidValue);
                }
            }
            if initial.frames > 0 && (initial.audio.is_none() || initial.midi.is_none()) {
                return Err(Refusal::InvalidValue);
            }
            for (token, midi) in [(initial.audio, false), (initial.midi, true)] {
                let Some(token) = token else { continue };
                self.uploads.source(token, midi, initial.frames)?;
                for prior in self.replacement.initial[..slot].iter().flatten() {
                    if prior.audio == Some(token) || prior.midi == Some(token) {
                        return Err(Refusal::InvalidValue);
                    }
                }
                if initial.audio == initial.midi {
                    return Err(Refusal::InvalidValue);
                }
            }
        }
        Ok(())
    }
}
