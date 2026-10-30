//! Structural transactions over prepared slots.
//!
//! A transaction prepares everything off the music first: node-pair edits for
//! added or removed loops, a validated timing image and a staged graph. It then
//! schedules the activation at an exact frame. Removed paths first fade out over
//! the prepared ramp and their loops stop; rerouted paths cross-fade between the
//! two connection banks of a strip. Afterwards the displaced owners are
//! recycled into their prepared slots.

use crate::endpoint::{
    completion_refusal, execution_outcome, loop_id, ControlEndpoint, Purpose, RuntimeResult,
    PLANNED,
};
use crate::model::Change;
use crate::node::EngineOp;
use crate::topology::{TopologyLoop, RECORD_BYTES};
use rt_runtime::executor::Command;
use rt_runtime::{Frame, NodeId};
use shoop_loops::audio::Smoothing;
use shoop_loops::runtime::{GraphStageId, NodePairInitial, NodePairStageId, Operation, Reply};
use shoop_loops::topology::staging::StageId;
use shoop_loops::{media::MediaId, LoopId, Timing, Transition};
use shoop_rt_protocol::{LoopSlotId, Outcome, PortKind, Refusal};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    NodePairBegin,
    NodePairAppend,
    TopologyBegin,
    TopologyAppend,
    TopologyValidate,
    StageGraph,
    Prelude,
    Activation,
    Query,
    Retire,
    Cleanup,
    ClearStop,
    ClearApply,
    Smoothing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Start,
    NodePairs,
    Topology,
    Graph,
    Activate,
    Activating,
    Query,
    Retire,
    Cleanup,
    Finishing,
    ClearStop,
    ClearApply,
    Smoothing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Change(Change),
    Clear(LoopSlotId),
    Smoothing(u32),
}

pub(crate) fn smoothing(frames: u32) -> Smoothing {
    match frames {
        0 => Smoothing::None,
        frames => Smoothing::LinearRestart { frames },
    }
}

pub(crate) struct Job {
    pub correlation: Option<u64>,
    kind: Kind,
    phase: Phase,
    outstanding: usize,
    failure: Option<Refusal>,
    edits: usize,
    appended: bool,
    node_pairs: Option<NodePairStageId>,
    topology: Option<StageId>,
    records: usize,
    sent: usize,
    chunk: usize,
    ready: bool,
    graph: Option<GraphStageId>,
    faded: Option<NodeId>,
    activated: bool,
    created: Option<LoopId>,
    media: Option<MediaId>,
}

impl Job {
    fn new(kind: Kind) -> Self {
        Self {
            correlation: None,
            kind,
            phase: Phase::Start,
            outstanding: 0,
            failure: None,
            edits: 0,
            appended: false,
            node_pairs: None,
            topology: None,
            records: 0,
            sent: 0,
            chunk: 0,
            ready: false,
            graph: None,
            faded: None,
            activated: false,
            created: None,
            media: None,
        }
    }
    pub(crate) fn change(change: Change) -> Self {
        Self::new(Kind::Change(change))
    }
    pub(crate) fn clear(id: LoopSlotId) -> Self {
        Self::new(Kind::Clear(id))
    }
    pub(crate) fn smoothing(frames: u32) -> Self {
        Self::new(Kind::Smoothing(frames))
    }
    fn fail(&mut self, refusal: Refusal) {
        self.failure.get_or_insert(refusal);
    }
    fn change_ref(&self) -> Option<Change> {
        match self.kind {
            Kind::Change(change) => Some(change),
            Kind::Clear(_) | Kind::Smoothing(_) => None,
        }
    }
}

impl ControlEndpoint {
    pub(crate) fn advance_job(&mut self) {
        for _ in 0..16 {
            let Some(mut job) = self.job.take() else {
                return;
            };
            if job.outstanding > 0 {
                self.job = Some(job);
                return;
            }
            let progressed = self.step_job(&mut job);
            if job.phase != Phase::Finishing || job.outstanding > 0 {
                self.job = Some(job);
            } else {
                self.job = None;
            }
            if !progressed {
                return;
            }
        }
    }

    fn answer_job(&mut self, job: &mut Job, outcome: Outcome) {
        if let Some(correlation) = job.correlation.take() {
            self.answer(correlation, outcome);
        }
    }

    fn submit_job(
        &mut self,
        job: &mut Job,
        command: crate::EngineCommand,
        step: Step,
        expects: bool,
    ) {
        match self.submit(command, Purpose::Job(step), expects) {
            Ok(_) => job.outstanding += 1,
            Err(refusal) => job.fail(refusal),
        }
    }
    fn submit_job_loop(&mut self, job: &mut Job, at: Frame, operation: Operation, step: Step) {
        let command = self.loops.command(at, operation);
        self.submit_job(job, command, step, true);
    }
    fn submit_job_op(&mut self, job: &mut Job, at: Frame, node: NodeId, operation: EngineOp) {
        self.submit_job(
            job,
            Command::Schedule {
                at,
                action: rt_runtime::schedule::Action::Processor { node, operation },
                payload: Box::new([]),
            },
            Step::Prelude,
            true,
        );
    }

    /// Run the next phase of `job`; returns whether it can continue immediately.
    fn step_job(&mut self, job: &mut Job) -> bool {
        if job.failure.is_some()
            && !matches!(
                job.phase,
                Phase::Cleanup | Phase::Finishing | Phase::ClearApply
            )
        {
            let refusal = job.failure.expect("failure");
            self.answer_job(job, Outcome::Refused(refusal));
            if let Some(node) = job.faded.take() {
                let at = self.at();
                let _ = self.submit_op(at, node, EngineOp::Presence(true), Purpose::Silent);
            }
            job.phase = Phase::Cleanup;
            return true;
        }
        match job.phase {
            Phase::Start => self.start_job(job),
            Phase::NodePairs => self.node_pairs(job),
            Phase::Topology => self.topology(job),
            Phase::Graph => self.graph(job),
            Phase::Activate => self.activate(job),
            Phase::Activating => self.activated(job),
            Phase::Query => {
                let media = job.media.take();
                job.phase = Phase::Retire;
                if let Some(media) = media {
                    let at = self.at();
                    self.submit_job_loop(job, at, Operation::Retire { media }, Step::Retire);
                }
                true
            }
            Phase::Retire => {
                job.phase = Phase::Cleanup;
                true
            }
            Phase::Cleanup => self.cleanup(job),
            Phase::Finishing => false,
            Phase::ClearStop => {
                let Kind::Clear(id) = job.kind else {
                    unreachable!("clear job")
                };
                let at = self.at();
                job.phase = Phase::ClearApply;
                self.submit_job_loop(
                    job,
                    at,
                    Operation::Clear {
                        target: loop_id(id),
                    },
                    Step::ClearApply,
                );
                if let Some(refusal) = job.failure {
                    self.answer_job(job, Outcome::Refused(refusal));
                    job.phase = Phase::Finishing;
                }
                true
            }
            Phase::Smoothing => {
                if let Kind::Smoothing(frames) = job.kind {
                    self.settings.smoothing = frames;
                }
                self.answer_job(job, Outcome::Done);
                job.phase = Phase::Finishing;
                true
            }
            Phase::ClearApply => {
                if let Some(refusal) = job.failure {
                    self.answer_job(job, Outcome::Refused(refusal));
                }
                job.phase = Phase::Finishing;
                true
            }
        }
    }

    fn start_job(&mut self, job: &mut Job) -> bool {
        let change = match job.kind {
            Kind::Smoothing(frames) => {
                let loops = self.model.loops.iter().flatten().count();
                if !self.room(loops) {
                    return false;
                }
                let at = self.at();
                for slot in 0..self.model.loops.len() {
                    let Some(entry) = self.model.loops[slot] else {
                        continue;
                    };
                    let operation = Operation::Smoothing {
                        target: loop_id(entry.id),
                        smoothing: smoothing(frames),
                    };
                    self.submit_job_loop(job, at, operation, Step::Smoothing);
                }
                job.phase = Phase::Smoothing;
                return true;
            }
            Kind::Clear(id) => {
                if !self.room(PLANNED + 1) {
                    return false;
                }
                let at = self.at();
                self.cancel_planned(id.0.index as usize, None, at);
                job.phase = Phase::ClearStop;
                self.submit_job_loop(
                    job,
                    at,
                    Operation::Transition {
                        target: loop_id(id),
                        timing: Timing::At,
                        transition: Transition::Stop,
                    },
                    Step::ClearStop,
                );
                return true;
            }
            Kind::Change(change) => change,
        };
        self.next.copy_from(&self.model);
        self.next.apply(change);
        job.edits = (0..self.model.loops.len())
            .filter(|&slot| {
                self.model.loops[slot].map(|l| l.id) != self.next.loops[slot].map(|l| l.id)
            })
            .count();
        job.phase = Phase::NodePairs;
        if job.edits > 0 {
            self.submit_job_loop(
                job,
                Frame(0),
                Operation::NodePairBegin {
                    slot: 0,
                    changes: job.edits,
                },
                Step::NodePairBegin,
            );
        }
        true
    }

    fn node_pairs(&mut self, job: &mut Job) -> bool {
        if job.edits == 0 || job.appended {
            job.phase = Phase::Topology;
            return true;
        }
        let Some(id) = job.node_pairs else {
            job.fail(Refusal::Internal);
            return true;
        };
        if !self.room(job.edits) {
            return false;
        }
        job.appended = true;
        for slot in 0..self.model.loops.len() {
            let before = self.model.loops[slot].map(|l| l.id);
            let after = self.next.loops[slot].map(|l| l.id);
            if before == after {
                continue;
            }
            if let Some(created) = after {
                job.created = Some(loop_id(created));
            }
            self.submit_job_loop(
                job,
                Frame(0),
                Operation::NodePairAppend {
                    id,
                    previous: before.map(loop_id),
                    initial: after.map(|created| NodePairInitial {
                        target: loop_id(created),
                        frames: 0,
                        audio: None,
                        midi: None,
                        gain: 1.,
                        preserve_input: false,
                    }),
                },
                Step::NodePairAppend,
            );
        }
        true
    }

    fn topology_needed(&self, job: &Job) -> bool {
        job.edits > 0 || matches!(job.change_ref(), Some(Change::Sync { .. }))
    }

    fn topology(&mut self, job: &mut Job) -> bool {
        if !self.topology_needed(job) || job.ready {
            job.phase = Phase::Graph;
            return true;
        }
        let at = Frame(0);
        let Some(id) = job.topology else {
            let next = &self.next;
            let records = self.work.encoder.encode(
                |slot| {
                    next.loops[slot].map(|entry| TopologyLoop {
                        id: loop_id(entry.id),
                        sync: loop_id(entry.sync.unwrap_or(entry.id)),
                    })
                },
                &mut self.work.image,
            );
            match records {
                Ok(records) => job.records = records,
                Err(_) => {
                    job.fail(Refusal::InvalidValue);
                    return true;
                }
            }
            let Some(mut payload) = self.work.payloads[0].take() else {
                return false;
            };
            payload.copy_from_slice(&self.work.image[..RECORD_BYTES]);
            let command =
                self.loops
                    .command_with_payload(at, Operation::TopologyBegin { slot: 0 }, payload);
            self.submit_job(job, command, Step::TopologyBegin, true);
            return true;
        };
        if job.sent < job.records {
            let chunk = (job.records - job.sent).min(self.settings.records_per_append);
            let Some(mut payload) = self.work.payloads[chunk - 1].take() else {
                return false;
            };
            let start = RECORD_BYTES * (1 + job.sent);
            payload.copy_from_slice(&self.work.image[start..start + chunk * RECORD_BYTES]);
            job.chunk = chunk;
            let command = self.loops.command_with_payload(
                at,
                Operation::TopologyAppend {
                    id,
                    first_record: job.sent,
                },
                payload,
            );
            self.submit_job(job, command, Step::TopologyAppend, true);
            return true;
        }
        self.submit_job_loop(
            job,
            at,
            Operation::TopologyValidate { id },
            Step::TopologyValidate,
        );
        true
    }

    fn graph(&mut self, job: &mut Job) -> bool {
        if matches!(job.change_ref(), Some(Change::Sync { .. })) || job.graph.is_some() {
            job.phase = Phase::Activate;
            return true;
        }
        let Some(mut plan) = self.work.plans.iter_mut().find_map(Option::take) else {
            job.fail(Refusal::Internal);
            return true;
        };
        if let Err(error) =
            self.work
                .builder
                .build(&mut plan, &self.next, &self.pool, &self.shape, self.planner)
        {
            self.work.return_plan(plan);
            job.fail(if error == rt_runtime::Error::Capacity {
                Refusal::Capacity
            } else {
                Refusal::Internal
            });
            return true;
        }
        let epoch = self.graph_epoch;
        self.submit_job(
            job,
            Command::stage_graph(0, epoch, plan),
            Step::StageGraph,
            false,
        );
        true
    }

    fn activate(&mut self, job: &mut Job) -> bool {
        let change = job.change_ref().expect("structural job");
        let removed = |slot: usize, model: &crate::model::Model, next: &crate::model::Model| {
            model.loops[slot].is_some() && next.loops[slot].is_none()
        };
        let stopping = (0..self.model.loops.len())
            .filter(|&slot| removed(slot, &self.model, &self.next))
            .count();
        if !self.room(2 + stopping * (PLANNED + 1) + 1) {
            return false;
        }
        let ramp = u64::from(self.settings.ramp);
        let base = self.at().0.max(self.quiet_until);
        let mut activation = base;
        let mut crossfade = None;
        match change {
            Change::RemoveStrip { strip } => {
                job.faded = Some(self.pool.strip_output(strip.0.index as usize));
            }
            Change::RemoveLoop { loop_id } => {
                job.faded = Some(self.pool.fader(loop_id.0.index as usize));
            }
            Change::CreateStrip { strip, shape } => {
                let slot = strip.0.index as usize;
                let reset = EngineOp::Reset {
                    inputs: shape.audio_inputs,
                    outputs: shape.audio_outputs,
                };
                for node in [self.pool.strip_input(slot), self.pool.strip_output(slot)] {
                    self.submit_job_op(job, Frame(base), node, reset);
                }
            }
            Change::CreateLoop { loop_id, .. } => {
                let node = self.pool.fader(loop_id.0.index as usize);
                let reset = EngineOp::Reset {
                    inputs: 0,
                    outputs: 0,
                };
                self.submit_job_op(job, Frame(base), node, reset);
            }
            Change::Connect { port, .. } => {
                let slot = port.strip.0.index as usize;
                crossfade = match port.kind {
                    PortKind::AudioIn => Some(self.pool.strip_input(slot)),
                    PortKind::AudioOut => Some(self.pool.strip_output(slot)),
                    PortKind::MidiIn | PortKind::MidiOut => None,
                };
            }
            Change::Route { strip, .. } => {
                crossfade = Some(self.pool.strip_output(strip.0.index as usize));
            }
            Change::ConnectMaster { .. } => crossfade = Some(self.pool.master()),
            Change::Sync { .. } => {}
        }
        if let Some(node) = job.faded {
            self.submit_job_op(job, Frame(base), node, EngineOp::Presence(false));
            activation = base + ramp;
            for slot in 0..self.model.loops.len() {
                if !removed(slot, &self.model, &self.next) {
                    continue;
                }
                let id = self.model.loops[slot].expect("removed loop").id;
                self.cancel_planned(slot, None, Frame(base));
                self.submit_job_loop(
                    job,
                    Frame(base + ramp),
                    Operation::Transition {
                        target: loop_id(id),
                        timing: Timing::At,
                        transition: Transition::Stop,
                    },
                    Step::Prelude,
                );
                activation = base + ramp + 1;
            }
        }
        if let Some(node) = crossfade {
            self.submit_job_op(job, Frame(activation), node, EngineOp::Crossfade);
        }
        let graph_epoch = self.graph_epoch;
        let topology_epoch = self.topology_epoch;
        let operation = if job.edits > 0 {
            match (job.node_pairs, job.topology, job.graph) {
                (Some(node_pairs), Some(topology), Some(graph)) => {
                    Operation::ConfigurationActivate {
                        node_pairs,
                        topology,
                        graph,
                        graph_epoch,
                        topology_epoch,
                    }
                }
                _ => {
                    job.fail(Refusal::Internal);
                    return true;
                }
            }
        } else if let Change::Sync { .. } = change {
            let Some(id) = job.topology else {
                job.fail(Refusal::Internal);
                return true;
            };
            Operation::TopologyActivate {
                id,
                graph_epoch,
                topology_epoch,
            }
        } else {
            let Some(id) = job.graph else {
                job.fail(Refusal::Internal);
                return true;
            };
            Operation::GraphActivate {
                id,
                graph_epoch,
                topology_epoch,
            }
        };
        if job.failure.is_some() {
            return true;
        }
        self.quiet_until = activation + ramp;
        self.submit_job_loop(job, Frame(activation), operation, Step::Activation);
        job.phase = Phase::Activating;
        true
    }

    fn activated(&mut self, job: &mut Job) -> bool {
        if !job.activated {
            job.fail(Refusal::Busy);
            return true;
        }
        job.faded = None;
        std::mem::swap(&mut self.model, &mut self.next);
        self.model.revision += 1;
        for slot in 0..self.model.loops.len() {
            if self.model.loops[slot].map(|l| l.id) != self.next.loops[slot].map(|l| l.id) {
                self.planned[slot] = [None; PLANNED];
                self.cells.loops[slot].peaks.clear();
                self.dirty[slot] = true;
            }
        }
        self.answer_job(job, Outcome::Done);
        if let Some(created) = job.created {
            job.phase = Phase::Query;
            let at = self.at();
            let operation = Operation::Smoothing {
                target: created,
                smoothing: smoothing(self.settings.smoothing),
            };
            self.submit_job_loop(job, at, operation, Step::Prelude);
            self.submit_job_loop(job, at, Operation::State { target: created }, Step::Query);
        } else {
            job.phase = Phase::Cleanup;
        }
        true
    }

    fn cleanup(&mut self, job: &mut Job) -> bool {
        if !self.room(3) {
            return false;
        }
        let at = Frame(0);
        let success = job.activated;
        if let Some(id) = job.node_pairs.take() {
            let operation = if success {
                Operation::NodePairRecycle { id }
            } else {
                Operation::NodePairCancel { id }
            };
            self.submit_job_loop(job, at, operation, Step::Cleanup);
        }
        if let Some(id) = job.topology.take() {
            let operation = if success {
                Operation::TopologyRecycle { id }
            } else {
                Operation::TopologyCancel { id }
            };
            self.submit_job_loop(job, at, operation, Step::Cleanup);
        }
        if let Some(id) = job.graph.take() {
            self.submit_job(job, Command::ReleaseGraph { id }, Step::Cleanup, false);
        }
        job.failure = None;
        job.phase = Phase::Finishing;
        true
    }

    pub(crate) fn job_result(&mut self, step: Step, result: RuntimeResult) {
        let (refusal, reply, done, graph) = match result {
            RuntimeResult::Completion(outcome, command, retired) => {
                let graph = match &command {
                    Command::StageGraph { id, .. } => *id,
                    _ => None,
                };
                let schedule = matches!(command, Command::Schedule { .. });
                self.reclaim(command);
                self.retire(retired);
                let refusal = completion_refusal(outcome);
                (refusal, None, refusal.is_some() || !schedule, graph)
            }
            RuntimeResult::Execution(execution) => {
                let refusal = match (step, execution_outcome(&execution)) {
                    (Step::Prelude | Step::Cleanup | Step::Retire | Step::ClearStop, _) => None,
                    (_, Outcome::Refused(refusal)) => Some(refusal),
                    (_, Outcome::Done) => None,
                };
                (refusal, execution.operation_result, true, None)
            }
        };
        let mut epochs = None;
        let mut answered = None;
        {
            let Some(job) = self.job.as_mut() else {
                return;
            };
            if let Some(graph) = graph {
                job.graph = Some(graph);
            }
            if done {
                job.outstanding = job.outstanding.saturating_sub(1);
            }
            if let Some(refusal) = refusal {
                if !matches!(step, Step::Cleanup) {
                    job.fail(refusal);
                }
                return;
            }
            match (step, reply) {
                (Step::NodePairBegin, Some(Reply::NodePairStatus { id, .. })) => {
                    job.node_pairs = Some(id)
                }
                (Step::TopologyBegin, Some(Reply::TopologyStatus { id, .. })) => {
                    job.topology = Some(id)
                }
                (Step::TopologyAppend, Some(Reply::TopologyStatus { .. })) => {
                    job.sent += job.chunk;
                    job.chunk = 0;
                }
                (Step::TopologyValidate, Some(Reply::TopologyProgress { progress, .. })) => {
                    job.ready = progress.ready;
                }
                (
                    Step::Activation,
                    Some(
                        Reply::ConfigurationActivated {
                            graph_epoch,
                            topology_epoch,
                            ..
                        }
                        | Reply::GraphActivated {
                            graph_epoch,
                            topology_epoch,
                            ..
                        }
                        | Reply::TopologyActivated {
                            graph_epoch,
                            topology_epoch,
                            ..
                        },
                    ),
                ) => {
                    job.activated = true;
                    epochs = Some((graph_epoch, topology_epoch));
                }
                (Step::Query, Some(Reply::State(cut))) => job.media = cut.playback,
                (Step::ClearApply, Some(Reply::Transition(_))) => {
                    answered = job.correlation.take();
                }
                _ => {}
            }
        }
        if let Some((graph_epoch, topology_epoch)) = epochs {
            self.graph_epoch = graph_epoch;
            self.topology_epoch = topology_epoch;
        }
        if let Some(correlation) = answered {
            self.answer(correlation, Outcome::Done);
        }
    }
}
