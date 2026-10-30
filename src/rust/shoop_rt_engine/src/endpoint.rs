//! The engine's control endpoint: protocol messages to runtime commands,
//! results, full-state resynchronization, telemetry and bulk reads.
//!
//! Requests are handled in arrival order. Controls, transitions and loop
//! parameters are forwarded as soon as they reach the head of the queue;
//! structural requests run as one transaction at a time and hold back later
//! requests until their activation is confirmed, so every request is validated
//! against the confirmed state. All storage is prepared; delivering and polling
//! do not allocate.

use crate::bulk::Bulk;
use crate::cells::{mode_from_code, Cells};
use crate::graph::{Builder, Shape};
use crate::jobs::{Job, Step};
use crate::model::{Change, Model};
use crate::node::{EngineNode, EngineOp};
use crate::pool::Pool;
use crate::resync::Resync;
use crate::ring::Ring;
use crate::topology::Encoder;
use crate::{EngineCommand, EngineControl, EngineExecution};
use rt_runtime::executor::Command;
use rt_runtime::graph::PreparedPlan;
use rt_runtime::planner::PlannerSpec;
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Lane, Outcome as RuntimeOutcome};
use rt_runtime::{Epoch, Frame, NodeId, Sequence};
use shoop_loops::runtime::{Failure, LoopClient, Operation, Reply};
use shoop_loops::{LoopId, Mode, Timing, Transition};
use shoop_rt_protocol::carrier::{Endpoint, Outbox};
use shoop_rt_protocol::{
    BusControl, BusId, Capabilities, ClientMessage, Direction, DisconnectReason, EngineMessage,
    HostPort, HostPortId, HostPortKind, Lane as ProtocolLane, Limits, LoopMode, LoopSlotId,
    Outcome, PortKind, Refusal, Request, StripControl, StripId, Telemetry, PROTOCOL_VERSION,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Planned delayed transitions retained per loop.
pub(crate) const PLANNED: usize = 8;

pub(crate) struct Settings {
    pub ramp: u32,
    pub lead: u32,
    pub smoothing: u32,
    pub smoothing_limit: u32,
    pub telemetry_interval: u32,
    pub channels: usize,
    pub master_channels: usize,
    /// Audio inputs, audio outputs, MIDI inputs, MIDI outputs.
    pub host: [usize; 4],
    pub records_per_append: usize,
    /// The largest callback, which separates scheduled topology appends.
    pub frames: u32,
    pub command_slots: usize,
    pub max_reliable: u16,
    pub max_bulk: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Planned {
    pub sequence: Sequence,
    pub mode: LoopMode,
    pub delay: u32,
    pub source: usize,
    pub baseline: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Purpose {
    /// Answer `correlation` with the execution's outcome.
    Reply(Option<u64>),
    /// A loop transition: answered when executed, or once scheduled if delayed.
    Transition {
        correlation: Option<u64>,
        delayed: bool,
        slot: usize,
    },
    Silent,
    Job(Step),
    Bulk,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pending {
    pub sequence: Sequence,
    pub purpose: Purpose,
    pub completed: bool,
    pub expects: bool,
    pub executed: bool,
}

/// A runtime result, routed to its submitter.
pub(crate) enum RuntimeResult {
    Completion(
        RuntimeOutcome,
        EngineCommand,
        Option<rt_runtime::executor::Retired<EngineNode>>,
    ),
    Execution(EngineExecution),
}

pub(crate) struct Work {
    pub builder: Builder,
    pub encoder: Encoder,
    pub image: Box<[u8]>,
    /// Topology payloads of one to `records_per_append` records, by length.
    pub payloads: Box<[Option<Box<[u8]>>]>,
    /// Further payloads of `records_per_append` records, so that a whole image
    /// can be appended at once.
    pub full: Vec<Box<[u8]>>,
    pub plans: [Option<PreparedPlan>; 2],
}

impl Work {
    pub(crate) fn take_payload(&mut self, records: usize) -> Option<Box<[u8]>> {
        let slot = self.payloads.get_mut(records.checked_sub(1)?)?;
        slot.take().or_else(|| {
            (records == self.payloads.len())
                .then(|| self.full.pop())
                .flatten()
        })
    }
    pub(crate) fn return_payload(&mut self, payload: Box<[u8]>) {
        let index = payload.len() / crate::topology::RECORD_BYTES;
        let Some(slot) = index.checked_sub(1).and_then(|i| self.payloads.get_mut(i)) else {
            return;
        };
        if slot.is_none() {
            *slot = Some(payload);
        } else if self.full.len() < self.full.capacity() {
            self.full.push(payload);
        }
    }
    pub(crate) fn return_plan(&mut self, plan: PreparedPlan) {
        if let Some(slot) = self.plans.iter_mut().find(|p| p.is_none()) {
            *slot = Some(plan);
        }
    }
}

pub struct ControlEndpoint {
    pub(crate) control: EngineControl,
    pub(crate) loops: LoopClient,
    pub(crate) pool: Pool,
    pub(crate) shape: Shape,
    pub(crate) planner: PlannerSpec,
    pub(crate) cells: Arc<Cells>,
    pub(crate) settings: Settings,
    pub(crate) host_ports: Box<[HostPort]>,
    pub(crate) model: Model,
    pub(crate) next: Model,
    pub(crate) limits: Limits,
    pub(crate) connected: bool,
    pub(crate) queue: Ring<(u64, Request)>,
    pub(crate) unanswered: usize,
    pub(crate) results: Ring<(u64, Outcome)>,
    pub(crate) pending: Box<[Option<Pending>]>,
    pub(crate) planned: Box<[[Option<Planned>; PLANNED]]>,
    pub(crate) job: Option<Job>,
    /// The staged transaction's requests and changes, in order.
    pub(crate) batch: Box<[(Option<u64>, Change)]>,
    pub(crate) batch_len: usize,
    /// How far the pending transaction affects each strip, loop and the
    /// Master (see `queue`).
    pub(crate) touched_strips: Box<[u8]>,
    pub(crate) touched_loops: Box<[u8]>,
    pub(crate) touched_master: u8,
    /// Targets of requests that wait in the current pass over the queue.
    pub(crate) blocked_strips: Box<[bool]>,
    pub(crate) blocked_loops: Box<[bool]>,
    pub(crate) blocked_master: bool,
    pub(crate) blocked_all: bool,
    /// Initial media of new loops, to release.
    pub(crate) retired: Ring<shoop_loops::media::MediaId>,
    /// Nodes that fade out before an activation.
    pub(crate) fades: Box<[NodeId]>,
    pub(crate) work: Work,
    pub(crate) graph_epoch: Epoch,
    pub(crate) topology_epoch: Epoch,
    pub(crate) quiet_until: u64,
    pub(crate) resync: Option<Resync>,
    pub(crate) bulk: Bulk,
    pub(crate) telemetry_frame: Option<u64>,
    pub(crate) dirty: Box<[bool]>,
    pub(crate) limits_changed: bool,
    pub(crate) protocol_error: bool,
}

fn runtime_refusal(error: rt_runtime::Error) -> Refusal {
    use rt_runtime::Error;
    match error {
        Error::Capacity | Error::Exhausted => Refusal::Capacity,
        Error::InvalidId | Error::StaleEpoch | Error::StaleRevision => Refusal::InvalidId,
        Error::InvalidParameter | Error::InvalidEvent | Error::InvalidShape => {
            Refusal::InvalidValue
        }
        Error::Busy | Error::InvalidState => Refusal::Busy,
        Error::Full | Error::Budget => Refusal::Overloaded,
        Error::Disconnected => Refusal::NotConnected,
        _ => Refusal::Internal,
    }
}
fn timeline_refusal(error: shoop_loops::Error) -> Refusal {
    use shoop_loops::Error;
    match error {
        Error::Capacity | Error::Exhausted => Refusal::Capacity,
        Error::InvalidId | Error::DuplicateId => Refusal::InvalidId,
        Error::EmptyContent | Error::MissingSync | Error::InvalidFrame => Refusal::InvalidValue,
        Error::InvalidState => Refusal::Busy,
        _ => Refusal::Internal,
    }
}
pub(crate) fn failure_refusal(failure: Failure) -> Refusal {
    match failure {
        Failure::Runtime(error) => runtime_refusal(error),
        Failure::Timeline(error) => timeline_refusal(error),
    }
}
/// The domain outcome of an execution.
pub(crate) fn execution_outcome(execution: &EngineExecution) -> Outcome {
    if let Err(error) = execution.result {
        return Outcome::Refused(runtime_refusal(error));
    }
    match execution.operation_result {
        Some(Reply::Failed(failure)) => Outcome::Refused(failure_refusal(failure)),
        Some(Reply::Transition(completion)) => match completion.outcome {
            shoop_loops::Outcome::Applied { .. } | shoop_loops::Outcome::Superseded { .. } => {
                Outcome::Done
            }
            shoop_loops::Outcome::Cancelled { .. } => Outcome::Refused(Refusal::Busy),
            shoop_loops::Outcome::Failed { reason, .. } => {
                Outcome::Refused(timeline_refusal(reason))
            }
        },
        _ => Outcome::Done,
    }
}
pub(crate) fn completion_refusal(outcome: RuntimeOutcome) -> Option<Refusal> {
    match outcome {
        RuntimeOutcome::Rejected(error) => Some(runtime_refusal(error)),
        RuntimeOutcome::Cancelled => Some(Refusal::Busy),
        _ => None,
    }
}

fn loop_transition(mode: LoopMode) -> Transition {
    match mode {
        LoopMode::Stopped => Transition::Stop,
        LoopMode::Playing => Transition::Play {
            position: 0,
            retrigger: false,
        },
        LoopMode::Recording => Transition::Record {
            monitor_previous: false,
        },
    }
}
pub(crate) fn protocol_mode(mode: Mode) -> LoopMode {
    match mode {
        Mode::Stopped => LoopMode::Stopped,
        Mode::Playing => LoopMode::Playing,
        Mode::Recording => LoopMode::Recording,
    }
}
pub(crate) fn loop_id(id: LoopSlotId) -> LoopId {
    LoopId::from_parts(id.0.index, id.0.generation).expect("validated loop identity")
}
fn finite_balance(value: f32) -> bool {
    (-1.0..=1.0).contains(&value)
}

impl ControlEndpoint {
    pub(crate) fn next_frame(&self) -> Frame {
        Frame(self.cells.next_frame.load(Ordering::Relaxed))
    }
    /// The earliest frame a newly submitted command can be scheduled at.
    pub(crate) fn at(&self) -> Frame {
        Frame(self.next_frame().0 + u64::from(self.settings.lead))
    }

    /// No request, transaction or bulk read is in progress and every ramp of
    /// the last structural change has finished.
    pub fn is_idle(&self) -> bool {
        self.queue.len() == 0
            && self.job.is_none()
            && self.pending.iter().all(Option::is_none)
            && self.bulk.idle()
            && self.next_frame().0 >= self.quiet_until
    }

    /// Change the client's credits within the prepared maximum; a connected client
    /// receives the new limits.
    pub fn set_credits(&mut self, reliable: u16, bulk: u16) -> Result<(), Refusal> {
        if reliable == 0
            || bulk == 0
            || reliable > self.settings.max_reliable
            || bulk > self.settings.max_bulk
        {
            return Err(Refusal::InvalidValue);
        }
        self.limits.reliable_credits = reliable;
        self.limits.bulk_credits = bulk;
        self.limits_changed = self.connected;
        Ok(())
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    pub(crate) fn capabilities() -> Capabilities {
        Capabilities::MIDI_INJECTION.union(Capabilities::BULK_MIDI)
    }

    fn free_pending(&self) -> usize {
        self.pending.iter().filter(|p| p.is_none()).count()
    }
    /// Room for `count` more runtime commands.
    pub(crate) fn room(&self, count: usize) -> bool {
        self.free_pending() >= count
            && self.control.outstanding() + count <= self.settings.command_slots
    }

    /// Submit a command whose result goes to `purpose`. Owned parts of a refused
    /// command return to their pools.
    pub(crate) fn submit(
        &mut self,
        command: EngineCommand,
        purpose: Purpose,
        expects: bool,
    ) -> Result<Sequence, Refusal> {
        let Some(slot) = self.pending.iter().position(Option::is_none) else {
            self.reclaim(command);
            return Err(Refusal::Overloaded);
        };
        match self.control.submit_prepared(Lane::Normal, command) {
            Ok(ticket) => {
                self.pending[slot] = Some(Pending {
                    sequence: ticket.sequence(),
                    purpose,
                    completed: false,
                    expects,
                    executed: false,
                });
                Ok(ticket.sequence())
            }
            Err(error) => {
                let refusal = runtime_refusal(error.error);
                self.reclaim(error.value);
                Err(refusal)
            }
        }
    }
    pub(crate) fn submit_loop(
        &mut self,
        at: Frame,
        operation: Operation,
        purpose: Purpose,
    ) -> Result<Sequence, Refusal> {
        let command = self.loops.command(at, operation);
        self.submit(command, purpose, true)
    }
    pub(crate) fn submit_op(
        &mut self,
        at: Frame,
        node: NodeId,
        operation: EngineOp,
        purpose: Purpose,
    ) -> Result<(), Refusal> {
        self.submit(
            Command::Schedule {
                at,
                action: Action::Processor { node, operation },
                payload: Box::new([]),
            },
            purpose,
            true,
        )
        .map(|_| ())
    }
    pub(crate) fn reclaim(&mut self, command: EngineCommand) {
        match command {
            Command::Schedule { payload, .. } => {
                if !payload.is_empty() {
                    self.work.return_payload(payload);
                }
            }
            Command::StageGraph {
                plan: Some(plan), ..
            }
            | Command::Commit {
                plan: Some(plan), ..
            } => self.work.return_plan(plan),
            _ => {}
        }
    }

    pub(crate) fn answer(&mut self, correlation: u64, outcome: Outcome) {
        self.unanswered = self.unanswered.saturating_sub(1);
        if self.results.push((correlation, outcome)).is_err() {
            self.protocol_error = true;
        }
    }

    fn reset_connection(&mut self) {
        self.connected = false;
        self.queue.clear();
        self.results.clear();
        self.unanswered = 0;
        self.resync = None;
        self.limits_changed = false;
        self.protocol_error = false;
        for pending in self.pending.iter_mut().flatten() {
            match &mut pending.purpose {
                Purpose::Reply(correlation) | Purpose::Transition { correlation, .. } => {
                    *correlation = None
                }
                _ => {}
            }
        }
        self.forget_job_answers();
        self.bulk.disconnect();
    }

    fn drain_runtime(&mut self) {
        for _ in 0..self.pending.len() {
            let Some(completion) = self.control.try_completion() else {
                break;
            };
            self.route(
                completion.sequence,
                RuntimeResult::Completion(
                    completion.outcome,
                    completion.request,
                    completion.retired,
                ),
            );
        }
        for _ in 0..self.pending.len() {
            let Some(execution) = self.control.try_execution() else {
                break;
            };
            self.route(execution.sequence, RuntimeResult::Execution(execution));
        }
    }

    fn route(&mut self, sequence: Sequence, result: RuntimeResult) {
        let Some(index) = self
            .pending
            .iter()
            .position(|p| p.is_some_and(|p| p.sequence == sequence))
        else {
            if let RuntimeResult::Completion(_, command, retired) = result {
                self.reclaim(command);
                self.retire(retired);
            }
            return;
        };
        let mut pending = self.pending[index].expect("found pending");
        match &result {
            RuntimeResult::Completion(outcome, ..) => {
                pending.completed = true;
                if completion_refusal(*outcome).is_some() {
                    pending.expects = false;
                }
            }
            RuntimeResult::Execution(_) => pending.executed = true,
        }
        let finished = pending.completed && (pending.executed || !pending.expects);
        self.pending[index] = if finished { None } else { Some(pending) };
        match pending.purpose {
            Purpose::Reply(correlation) => self.reply_result(correlation, result),
            Purpose::Transition {
                correlation,
                delayed,
                slot,
            } => self.transition_result(
                index,
                finished,
                correlation,
                delayed,
                slot,
                sequence,
                result,
            ),
            Purpose::Silent => {
                if let RuntimeResult::Completion(_, command, retired) = result {
                    self.reclaim(command);
                    self.retire(retired);
                }
            }
            Purpose::Job(step) => self.job_result(step, result),
            Purpose::Bulk => self.bulk_result(result),
        }
    }

    pub(crate) fn retire(&mut self, retired: Option<rt_runtime::executor::Retired<EngineNode>>) {
        if let Some(rt_runtime::executor::Retired::Plan(plan)) = retired {
            self.work.return_plan(plan);
        }
    }

    fn reply_result(&mut self, correlation: Option<u64>, result: RuntimeResult) {
        let outcome = match result {
            RuntimeResult::Completion(outcome, command, retired) => {
                self.reclaim(command);
                self.retire(retired);
                completion_refusal(outcome).map(Outcome::Refused)
            }
            RuntimeResult::Execution(execution) => Some(execution_outcome(&execution)),
        };
        if let (Some(correlation), Some(outcome)) = (correlation, outcome) {
            self.answer(correlation, outcome);
        }
    }

    fn transition_result(
        &mut self,
        index: usize,
        finished: bool,
        correlation: Option<u64>,
        delayed: bool,
        slot: usize,
        sequence: Sequence,
        result: RuntimeResult,
    ) {
        let (outcome, terminal) = match result {
            RuntimeResult::Completion(outcome, command, _) => {
                self.reclaim(command);
                match completion_refusal(outcome) {
                    Some(refusal) => (Some(Outcome::Refused(refusal)), true),
                    None if delayed => (Some(Outcome::Done), false),
                    None => (None, false),
                }
            }
            RuntimeResult::Execution(execution) => (Some(execution_outcome(&execution)), true),
        };
        if terminal || finished {
            self.forget_planned(slot, sequence);
        }
        if let (Some(correlation), Some(outcome)) = (correlation, outcome) {
            self.answer(correlation, outcome);
            if let Some(Some(pending)) = self.pending.get_mut(index) {
                if let Purpose::Transition { correlation, .. } = &mut pending.purpose {
                    *correlation = None;
                }
            }
        }
    }

    pub(crate) fn forget_planned(&mut self, slot: usize, sequence: Sequence) {
        if let Some(entries) = self.planned.get_mut(slot) {
            for entry in entries.iter_mut() {
                if entry.is_some_and(|e| e.sequence == sequence) {
                    *entry = None;
                    self.dirty[slot] = true;
                }
            }
        }
    }

    fn pulses(&self, slot: usize) -> u64 {
        self.cells.loops[slot].pulses.load(Ordering::Relaxed)
    }
    pub(crate) fn remaining(&self, entry: &Planned) -> u32 {
        let passed = self.pulses(entry.source).saturating_sub(entry.baseline);
        entry
            .delay
            .saturating_sub(passed.min(u64::from(u32::MAX)) as u32)
    }
    pub(crate) fn observed_mode(&self, id: LoopSlotId) -> Mode {
        let cells = &self.cells.loops[id.0.index as usize];
        if cells.generation.load(Ordering::Relaxed) == id.0.generation {
            mode_from_code(cells.mode.load(Ordering::Relaxed))
        } else {
            Mode::Stopped
        }
    }

    /// Cancel the planned transitions of `slot` whose remaining delay is at least
    /// `from` (all of them for `None`).
    pub(crate) fn cancel_planned(&mut self, slot: usize, from: Option<u32>, at: Frame) {
        for index in 0..PLANNED {
            let Some(entry) = self.planned[slot][index] else {
                continue;
            };
            if from.is_some_and(|delay| self.remaining(&entry) < delay) {
                continue;
            }
            self.planned[slot][index] = None;
            self.dirty[slot] = true;
            let _ = self.submit_loop(
                at,
                Operation::Cancel {
                    sequence: entry.sequence,
                },
                Purpose::Silent,
            );
        }
    }

    fn transition(
        &mut self,
        correlation: u64,
        id: LoopSlotId,
        mode: LoopMode,
        delay: Option<u32>,
    ) -> Result<(), Refusal> {
        let entry = *self.model.find_loop(id).ok_or(Refusal::InvalidId)?;
        let slot = id.0.index as usize;
        if !self.room(PLANNED + 1) {
            return Err(Refusal::Overloaded);
        }
        let at = self.at();
        let target = loop_id(id);
        let immediate =
            delay.is_none() || (entry.sync.is_none() && self.observed_mode(id) != Mode::Playing);
        if immediate {
            self.cancel_planned(slot, None, at);
            self.submit_loop(
                at,
                Operation::Transition {
                    target,
                    timing: Timing::At,
                    transition: loop_transition(mode),
                },
                Purpose::Transition {
                    correlation: Some(correlation),
                    delayed: false,
                    slot,
                },
            )?;
            return Ok(());
        }
        let delay = delay.expect("delayed transition");
        let source = entry.sync.unwrap_or(id);
        let source_slot = source.0.index as usize;
        self.cancel_planned(slot, Some(delay), at);
        let free = self.planned[slot]
            .iter()
            .position(Option::is_none)
            .ok_or(Refusal::Capacity)?;
        let baseline = self.pulses(source_slot);
        let sequence = self.submit_loop(
            at,
            Operation::Transition {
                target,
                timing: Timing::AfterTicks {
                    source: loop_id(source),
                    skip: u64::from(delay),
                },
                transition: loop_transition(mode),
            },
            Purpose::Transition {
                correlation: Some(correlation),
                delayed: true,
                slot,
            },
        )?;
        self.planned[slot][free] = Some(Planned {
            sequence,
            mode,
            delay,
            source: source_slot,
            baseline,
        });
        self.dirty[slot] = true;
        Ok(())
    }

    pub(crate) fn host_index(&self, host: HostPortId, kind: PortKind) -> Option<usize> {
        let port = self.host_ports.get(host.0 as usize)?;
        let expected = match kind {
            PortKind::AudioIn => (HostPortKind::Audio, Direction::Input),
            PortKind::AudioOut => (HostPortKind::Audio, Direction::Output),
            PortKind::MidiIn => (HostPortKind::Midi, Direction::Input),
            PortKind::MidiOut => (HostPortKind::Midi, Direction::Output),
        };
        if (port.kind, port.direction) != expected {
            return None;
        }
        let first = self
            .host_ports
            .iter()
            .position(|p| (p.kind, p.direction) == expected)?;
        Some(host.0 as usize - first)
    }

    /// Handle a request that is not structural. `Err` keeps it queued.
    pub(crate) fn start(&mut self, correlation: u64, request: Request) -> Result<(), ()> {
        let result = match request {
            Request::StripControl { strip, control } => {
                self.strip_control(correlation, strip, control)
            }
            Request::LoopTransition {
                loop_id,
                mode,
                delay,
            } => self
                .transition(correlation, loop_id, mode, delay)
                .map(|()| None),
            Request::LoopClear { loop_id } => match self.model.find_loop(loop_id) {
                None => Err(Refusal::InvalidId),
                Some(_) => return self.begin_job(correlation, Job::clear(loop_id)),
            },
            Request::LoopGain { loop_id, gain } => {
                if self.model.find_loop(loop_id).is_none() {
                    Err(Refusal::InvalidId)
                } else if !gain.is_finite() {
                    Err(Refusal::InvalidValue)
                } else {
                    self.loop_parameter(
                        correlation,
                        loop_id,
                        Operation::Gain {
                            target: self::loop_id(loop_id),
                            value: gain,
                        },
                    )
                }
            }
            Request::LoopBalance { loop_id, balance } => {
                if self.model.find_loop(loop_id).is_none() {
                    Err(Refusal::InvalidId)
                } else if !finite_balance(balance) {
                    Err(Refusal::InvalidValue)
                } else {
                    self.loop_parameter(
                        correlation,
                        loop_id,
                        Operation::Balance {
                            target: self::loop_id(loop_id),
                            value: balance,
                        },
                    )
                }
            }
            Request::BusControl {
                bus: BusId::Master,
                control,
            } => self.bus_control(correlation, control),
            Request::InjectMidi { strip, message } => match self.model.strip(strip) {
                None => Err(Refusal::InvalidId),
                Some(entry) if !entry.shape.midi_input => Err(Refusal::Unsupported),
                Some(_) => {
                    let node = self.pool.strip_input(strip.0.index as usize);
                    let at = self.at();
                    self.submit_op(
                        at,
                        node,
                        EngineOp::Inject(message),
                        Purpose::Reply(Some(correlation)),
                    )
                    .map(|()| None)
                }
            },
            Request::CreateStrip { .. }
            | Request::RemoveStrip { .. }
            | Request::CreateLoop { .. }
            | Request::RemoveLoop { .. }
            | Request::LoopSync { .. }
            | Request::Connect { .. }
            | Request::Route { .. } => Err(Refusal::Internal),
            Request::SetSmoothing { frames } => {
                if frames > self.settings.smoothing_limit {
                    Err(Refusal::InvalidValue)
                } else if frames == self.settings.smoothing {
                    Ok(Some(Outcome::Done))
                } else {
                    return self.begin_job(correlation, Job::smoothing(frames));
                }
            }
        };
        match result {
            Ok(Some(outcome)) => self.answer(correlation, outcome),
            Ok(None) => {}
            Err(Refusal::Overloaded) => return Err(()),
            Err(refusal) => self.answer(correlation, Outcome::Refused(refusal)),
        }
        Ok(())
    }

    fn begin_job(&mut self, correlation: u64, mut job: Job) -> Result<(), ()> {
        job.correlation = Some(correlation);
        self.job = Some(job);
        self.advance_job();
        Ok(())
    }

    fn loop_parameter(
        &mut self,
        correlation: u64,
        id: LoopSlotId,
        operation: Operation,
    ) -> Result<Option<Outcome>, Refusal> {
        let at = self.at();
        self.submit_loop(at, operation, Purpose::Reply(Some(correlation)))?;
        let staged = self.job.is_some();
        for model in [&mut self.model, &mut self.next] {
            if let Some(Some(entry)) = model.loops.get_mut(id.0.index as usize) {
                match operation {
                    Operation::Gain { value, .. } => entry.gain = value,
                    Operation::Balance { value, .. } => entry.balance = value,
                    _ => {}
                }
            }
            if !staged {
                break;
            }
        }
        Ok(None)
    }

    fn strip_control(
        &mut self,
        correlation: u64,
        strip: StripId,
        control: StripControl,
    ) -> Result<Option<Outcome>, Refusal> {
        let slot = strip.0.index as usize;
        self.model.strip(strip).ok_or(Refusal::InvalidId)?;
        let (node, operation) = match control {
            StripControl::InputGain(value) if value.is_finite() => {
                (self.pool.strip_input(slot), EngineOp::Gain(value))
            }
            StripControl::InputBalance(value) if finite_balance(value) => {
                (self.pool.strip_input(slot), EngineOp::Balance(value))
            }
            StripControl::Monitoring(on) => (self.pool.strip_input(slot), EngineOp::Monitor(on)),
            StripControl::OutputGain(value) if value.is_finite() => {
                (self.pool.strip_output(slot), EngineOp::Gain(value))
            }
            StripControl::OutputBalance(value) if finite_balance(value) => {
                (self.pool.strip_output(slot), EngineOp::Balance(value))
            }
            StripControl::OutputMute(muted) => {
                (self.pool.strip_output(slot), EngineOp::Mute(muted))
            }
            _ => return Err(Refusal::InvalidValue),
        };
        let at = self.at();
        self.submit_op(at, node, operation, Purpose::Reply(Some(correlation)))?;
        self.model
            .strip_mut(strip)
            .expect("validated strip")
            .controls
            .apply(control);
        self.model.revision += 1;
        if self.job.is_some() {
            if let Some(entry) = self.next.strip_mut(strip) {
                entry.controls.apply(control);
            }
        }
        Ok(None)
    }

    fn bus_control(
        &mut self,
        correlation: u64,
        control: BusControl,
    ) -> Result<Option<Outcome>, Refusal> {
        let operation = match control {
            BusControl::Gain(value) if value.is_finite() => EngineOp::Gain(value),
            BusControl::Balance(value) if finite_balance(value) => EngineOp::Balance(value),
            BusControl::Mute(muted) => EngineOp::Mute(muted),
            _ => return Err(Refusal::InvalidValue),
        };
        let at = self.at();
        let master = self.pool.master();
        self.submit_op(at, master, operation, Purpose::Reply(Some(correlation)))?;
        self.model.master.apply(control);
        self.model.revision += 1;
        if self.job.is_some() {
            self.next.master.apply(control);
        }
        Ok(None)
    }

    fn flush(&mut self, out: &mut Outbox) {
        if self.protocol_error {
            if out
                .push(EngineMessage::Disconnected(DisconnectReason::ProtocolError))
                .is_ok()
            {
                self.reset_connection();
            }
            return;
        }
        while let Some((correlation, outcome)) = self.results.front() {
            if out.available(ProtocolLane::Reliable) == 0 {
                return;
            }
            let _ = out.push(EngineMessage::Result {
                correlation,
                outcome,
            });
            self.results.pop();
        }
        if self.limits_changed && out.available(ProtocolLane::Reliable) > 0 {
            let _ = out.push(EngineMessage::LimitsChanged(self.limits));
            self.limits_changed = false;
        }
        self.stream_resync(out);
        self.flush_bulk(out);
    }

    fn telemetry(&mut self, out: &mut Outbox) {
        if !self.connected {
            return;
        }
        let frame = self.next_frame().0;
        let interval = u64::from(self.settings.telemetry_interval.max(1));
        let periodic = self
            .telemetry_frame
            .is_none_or(|last| frame >= last.saturating_add(interval));
        if periodic {
            self.telemetry_frame = Some(frame);
            let peaks = self.cells.host_peaks.take(2);
            let _ = out.push(EngineMessage::Telemetry(Telemetry::Engine {
                frame,
                callbacks: self.cells.callbacks.load(Ordering::Relaxed),
                xruns: self.cells.xruns(),
                load: self.cells.load(),
                input_peak: peaks.as_slice()[0],
                output_peak: peaks.as_slice()[1],
                driver: self.cells.driver(),
            }));
            for slot in 0..self.model.strips.len() {
                let Some(strip) = self.model.strips[slot] else {
                    continue;
                };
                let cells = &self.cells.strips[slot];
                let _ = out.push(EngineMessage::Telemetry(Telemetry::Strip {
                    strip: strip.id,
                    input_peaks: cells.input.take(strip.shape.audio_inputs as usize),
                    output_peaks: cells.output.take(strip.shape.audio_outputs as usize),
                    midi_in: cells.midi_in.swap(false, Ordering::Relaxed),
                    midi_out: cells.midi_out.swap(false, Ordering::Relaxed),
                }));
            }
            let _ = out.push(EngineMessage::Telemetry(Telemetry::Bus {
                bus: BusId::Master,
                peaks: self.cells.master.take(self.settings.master_channels),
            }));
        }
        for slot in 0..self.model.loops.len() {
            if !(periodic || self.dirty[slot]) {
                continue;
            }
            self.dirty[slot] = false;
            if let Some(record) = self.loop_telemetry(slot) {
                let _ = out.push(EngineMessage::Telemetry(record));
            }
        }
    }

    fn loop_telemetry(&self, slot: usize) -> Option<Telemetry> {
        let entry = self.model.loops[slot]?;
        let cells = &self.cells.loops[slot];
        let current = cells.generation.load(Ordering::Relaxed) == entry.id.0.generation;
        let next = self.planned[slot]
            .iter()
            .flatten()
            .map(|planned| (self.remaining(planned), planned.mode))
            .min_by_key(|(remaining, _)| *remaining);
        let channels = self
            .model
            .strip(entry.strip)
            .map_or(0, |strip| strip.shape.audio_outputs as usize);
        Some(Telemetry::Loop {
            loop_id: entry.id,
            mode: protocol_mode(self.observed_mode(entry.id)),
            next_mode: next.map(|(_, mode)| mode),
            next_delay: next.map(|(remaining, _)| remaining),
            length: if current {
                cells.length.load(Ordering::Relaxed)
            } else {
                0
            },
            position: if current {
                cells.position.load(Ordering::Relaxed)
            } else {
                0
            },
            peaks: cells.peaks.take(channels),
        })
    }
}

impl Endpoint for ControlEndpoint {
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        match message {
            ClientMessage::Hello { version } => {
                if self.cells.faulted.load(Ordering::Relaxed) {
                    let _ = out.push(EngineMessage::Disconnected(DisconnectReason::Fault));
                    return;
                }
                if version != PROTOCOL_VERSION {
                    let _ = out.push(EngineMessage::Rejected {
                        version: PROTOCOL_VERSION,
                    });
                    return;
                }
                self.reset_connection();
                self.connected = true;
                self.telemetry_frame = None;
                self.dirty.fill(true);
                let _ = out.push(EngineMessage::Welcome {
                    version: PROTOCOL_VERSION,
                    capabilities: Self::capabilities(),
                    limits: self.limits,
                    driver: self.cells.driver(),
                });
                return;
            }
            ClientMessage::Request {
                correlation,
                request,
            } => {
                if !self.connected {
                    let _ = out.push(EngineMessage::Result {
                        correlation,
                        outcome: Outcome::Refused(Refusal::NotConnected),
                    });
                    return;
                }
                if self.unanswered >= self.limits.reliable_credits as usize || self.queue.is_full()
                {
                    if self
                        .results
                        .push((correlation, Outcome::Refused(Refusal::Overloaded)))
                        .is_err()
                    {
                        self.protocol_error = true;
                    }
                } else {
                    self.unanswered += 1;
                    let _ = self.queue.push((correlation, request));
                    self.process_queue(false);
                }
            }
            ClientMessage::Resync => {
                if self.connected {
                    self.resync = Some(Resync::default());
                }
            }
            ClientMessage::Bulk(read) => self.deliver_bulk(read, out),
            ClientMessage::Goodbye => {
                if self.connected {
                    self.reset_connection();
                    let _ = out.push(EngineMessage::Disconnected(DisconnectReason::Shutdown));
                }
                return;
            }
        }
        self.flush(out);
    }

    fn poll(&mut self, out: &mut Outbox) {
        if self.connected && self.cells.faulted.load(Ordering::Relaxed) {
            if out
                .push(EngineMessage::Disconnected(DisconnectReason::Fault))
                .is_ok()
            {
                self.reset_connection();
            }
            return;
        }
        self.drain_runtime();
        self.advance_job();
        self.process_queue(true);
        self.advance_bulk();
        self.flush(out);
        self.telemetry(out);
    }
}

impl Drop for ControlEndpoint {
    fn drop(&mut self) {
        self.cells.endpoint_alive.store(false, Ordering::Relaxed);
    }
}
