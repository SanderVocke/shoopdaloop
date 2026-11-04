//! Prepared program admission. This storage is also used on worklet control
//! delivery paths, so staging never allocates or destroys an owning plan.

use crate::endpoint::{loop_id, ControlEndpoint, Purpose};
use crate::jobs::Job;
use alloc::{boxed::Box, vec};
use core::sync::atomic::Ordering;
use shoop_loops::runtime::Operation;
use shoop_loops::{Mode, Sequence};
use shoop_rt_protocol::{
    CompositeCommand, CompositeDescription, CompositeKind, CompositeSpan, LoopMode, LoopSlotId,
    Outcome, Refusal, SlotId,
};

pub(crate) const EMPTY_SPAN: CompositeSpan = CompositeSpan {
    target: LoopSlotId(SlotId::new(0, 0)),
    start: 0,
    end: 0,
    occurrence_start: 0,
    ordinal: 0,
    mode: LoopMode::Stopped,
};

pub(crate) struct Stage {
    pub description: Option<CompositeDescription>,
    pub spans: Box<[CompositeSpan]>,
    pub received: usize,
    pub committed: bool,
    pub activation: Option<Sequence>,
    pub admitted: bool,
    pub(crate) last_id: u64,
}
impl Stage {
    pub(crate) fn new(segments: usize) -> Self {
        Self {
            description: None,
            spans: vec![EMPTY_SPAN; segments].into_boxed_slice(),
            received: 0,
            committed: false,
            activation: None,
            admitted: false,
            last_id: 0,
        }
    }
    pub(crate) fn clear(&mut self) {
        self.description = None;
        self.received = 0;
        self.committed = false;
        self.activation = None;
        self.admitted = false;
    }
}

impl ControlEndpoint {
    fn validate_composite(&self, d: CompositeDescription) -> Result<(), Refusal> {
        let slot = d.loop_id.0.index as usize;
        let previous = self
            .model
            .loops
            .get(slot)
            .copied()
            .flatten()
            .ok_or(Refusal::InvalidId)?;
        if let Some(old) = d.previous {
            if old != previous.id
                || previous.program.is_some()
                || d.expected_version != 0
                || old.0.index != d.loop_id.0.index
                || d.loop_id.0.generation <= self.model.loop_generations[slot]
                || self.observed_mode(old) != Mode::Stopped
                || self.cells.loops[slot].length.load(Ordering::Relaxed) != 0
                || self
                    .model
                    .spans
                    .iter()
                    .flatten()
                    .any(|s| s.span.target == old)
                || self.planned[slot].iter().any(Option::is_some)
            {
                return Err(Refusal::InvalidValue);
            }
        } else if previous.id != d.loop_id
            || previous.program.map(|p| p.version) != Some(d.expected_version)
        {
            return Err(Refusal::InvalidId);
        }
        if d.sync.is_some_and(|id| {
            id == d.loop_id || d.previous == Some(id) || self.model.find_loop(id).is_none()
        }) {
            return Err(Refusal::InvalidId);
        }
        if d.id == 0
            || d.version <= d.expected_version
            || d.unit_frames == 0
            || d.quantum != self.limits.quantum
            || d.sample_rate != self.cells.driver().sample_rate
            || d.unit_frames % u64::from(self.limits.quantum) != 0
            || d.unit_frames.checked_mul(d.iterations).is_none()
            || (d.segments == 0) != (d.iterations == 0)
            || d.segments == 0 && (d.entries != 0 || d.kind != CompositeKind::Regular)
        {
            return Err(Refusal::InvalidValue);
        }
        let entries: u64 = self
            .model
            .loops
            .iter()
            .flatten()
            .filter_map(|l| l.program)
            .filter(|p| p.loop_id.0.index != d.loop_id.0.index)
            .map(|p| u64::from(p.entries))
            .sum();
        let segments = self
            .model
            .spans
            .iter()
            .flatten()
            .filter(|s| s.source.0.index != d.loop_id.0.index)
            .count();
        if entries + u64::from(d.entries) > u64::from(self.limits.composite_entries)
            || segments + d.segments as usize > self.limits.composite_segments as usize
        {
            return Err(Refusal::Capacity);
        }
        Ok(())
    }

    pub(crate) fn composite_request(
        &mut self,
        correlation: u64,
        command: CompositeCommand,
    ) -> Result<Option<Outcome>, Refusal> {
        match command {
            CompositeCommand::Begin(d) => {
                if self.composite.description.is_some() {
                    return Err(Refusal::Busy);
                }
                if d.id <= self.composite.last_id {
                    return Err(Refusal::InvalidId);
                }
                self.validate_composite(d)?;
                self.composite.description = Some(d);
                self.composite.received = 0;
                self.composite.last_id = d.id;
                self.model.revision += 1;
                Ok(Some(Outcome::Done))
            }
            CompositeCommand::Span { id, index, span } => {
                let d = self
                    .composite
                    .description
                    .filter(|d| d.id == id)
                    .ok_or(Refusal::InvalidId)?;
                if self.composite.committed {
                    return Err(Refusal::Busy);
                }
                if index as usize != self.composite.received || index >= d.segments {
                    return Err(Refusal::InvalidValue);
                }
                let child = self
                    .model
                    .find_loop(span.target)
                    .ok_or(Refusal::InvalidId)?;
                if span.target == d.loop_id
                    || d.previous == Some(span.target)
                    || span.start >= span.end
                    || span.end > d.iterations
                    || span.occurrence_start != span.start
                    || span.ordinal >= d.entries
                    || d.kind == CompositeKind::Regular && span.mode != LoopMode::Playing
                    || child.program.is_some() && span.mode == LoopMode::Recording
                    || self.composite.received.checked_sub(1).is_some_and(|i| {
                        let old = self.composite.spans[i];
                        old.target > span.target
                            || old.target == span.target
                                && (old.end > span.start
                                    || old.end == span.start && old.mode == span.mode)
                    })
                {
                    return Err(Refusal::InvalidValue);
                }
                self.composite.spans[index as usize] = span;
                self.composite.received += 1;
                self.model.revision += 1;
                Ok(Some(Outcome::Done))
            }
            CompositeCommand::Commit { id } => {
                let d = self
                    .composite
                    .description
                    .filter(|d| d.id == id)
                    .ok_or(Refusal::InvalidId)?;
                if self.composite.committed || self.job.is_some() {
                    return Err(Refusal::Busy);
                }
                self.validate_composite(d)?;
                if self.composite.received != d.segments as usize {
                    return Err(Refusal::InvalidValue);
                }
                for span in &self.composite.spans[..self.composite.received] {
                    self.model
                        .find_loop(span.target)
                        .ok_or(Refusal::InvalidId)?;
                }
                self.next.copy_from(&self.model);
                if let Some(old) = d.previous {
                    let entry = self.next.loops[d.loop_id.0.index as usize]
                        .as_mut()
                        .unwrap();
                    entry.id = d.loop_id;
                    if entry.sync == Some(old) {
                        entry.sync = None;
                    }
                    self.next.loop_generations[d.loop_id.0.index as usize] = d.loop_id.0.generation;
                    for entry in self.next.loops.iter_mut().flatten() {
                        if entry.sync == Some(old) {
                            entry.sync = Some(d.loop_id);
                        }
                    }
                }
                self.next.loops[d.loop_id.0.index as usize]
                    .as_mut()
                    .unwrap()
                    .sync = d.sync;
                self.next
                    .install_program(d, &self.composite.spans[..self.composite.received]);
                self.touched_loops[d.loop_id.0.index as usize] = if d.previous.is_some() {
                    crate::queue::HARD
                } else {
                    crate::queue::SOFT
                };
                self.composite.committed = true;
                self.model.revision += 1;
                let mut job = Job::composite(d.loop_id, d.previous.is_some());
                job.correlation = Some(correlation);
                self.job = Some(job);
                self.advance_job();
                Ok(None)
            }
            CompositeCommand::Cancel { id } => {
                let d = self
                    .composite
                    .description
                    .filter(|d| d.id == id)
                    .ok_or(Refusal::InvalidId)?;
                if let Some(sequence) = self.composite.activation {
                    if d.previous.is_some() {
                        return Err(Refusal::Busy);
                    }
                    self.submit_loop(
                        self.at(),
                        Operation::Cancel { sequence },
                        Purpose::Reply(Some(correlation)),
                    )?;
                    Ok(None)
                } else if self.composite.committed {
                    self.job.as_mut().ok_or(Refusal::Busy)?.cancel_composite();
                    Ok(Some(Outcome::Done))
                } else {
                    self.composite.clear();
                    self.model.revision += 1;
                    Ok(Some(Outcome::Done))
                }
            }
        }
    }
    pub(crate) fn composite_iteration(
        &mut self,
        correlation: u64,
        id: LoopSlotId,
        iteration: u64,
        retrigger: bool,
        seek: bool,
    ) -> Result<Option<Outcome>, Refusal> {
        let program = self
            .model
            .find_loop(id)
            .and_then(|l| l.program)
            .ok_or(Refusal::InvalidId)?;
        if iteration >= program.iterations && (iteration != 0 || program.iterations != 0 || seek) {
            return Err(Refusal::InvalidValue);
        }
        if !self.room(crate::endpoint::PLANNED + 1) {
            return Err(Refusal::Overloaded);
        }
        let at = self.at();
        self.cancel_planned(id.0.index as usize, None, at);
        let transition = if seek {
            shoop_loops::Transition::SeekIteration {
                iteration,
                retrigger,
            }
        } else {
            shoop_loops::Transition::PlayIteration {
                iteration,
                retrigger,
            }
        };
        self.submit_loop(
            at,
            Operation::Transition {
                target: loop_id(id),
                timing: shoop_loops::Timing::At,
                transition,
            },
            Purpose::Transition {
                correlation: Some(correlation),
                delayed: false,
                slot: id.0.index as usize,
            },
        )?;
        Ok(None)
    }
    pub(crate) fn repeat_sync(
        &mut self,
        correlation: u64,
        id: LoopSlotId,
        enabled: bool,
    ) -> Result<Option<Outcome>, Refusal> {
        self.model.find_loop(id).ok_or(Refusal::InvalidId)?;
        self.submit_loop(
            self.at(),
            Operation::RepeatSync {
                target: loop_id(id),
                enabled,
            },
            Purpose::Reply(Some(correlation)),
        )?;
        self.model.loops[id.0.index as usize]
            .as_mut()
            .unwrap()
            .repeat_sync = enabled;
        if self.job.is_some() {
            if let Some(entry) = self.next.loops[id.0.index as usize]
                .as_mut()
                .filter(|l| l.id == id)
            {
                entry.repeat_sync = enabled;
            }
        }
        self.model.revision += 1;
        Ok(None)
    }
}
