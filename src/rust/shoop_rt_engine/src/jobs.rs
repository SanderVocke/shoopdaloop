//! Structural transactions over prepared slots.
//!
//! One transaction applies a batch of staged changes. It prepares everything off
//! the music first: node-pair edits for added or removed loops, a validated
//! timing image and a staged graph of the resulting model. The independent
//! preparations start together; topology records follow, one append per callback
//! at most. The transaction then schedules one activation at an exact frame.
//! Additions activate at once; removed paths first fade out over the prepared ramp
//! and their loops stop; rerouted paths cross-fade between the two connection
//! banks of their node. Every request of the batch is answered at the activation.
//! Afterwards new loops get their smoothing, their initial media is released and
//! the displaced owners are recycled into their prepared slots.

use crate::endpoint::{
    completion_refusal, execution_outcome, loop_id, ControlEndpoint, Purpose, RuntimeResult,
    PLANNED,
};
use crate::node::EngineOp;
use crate::topology::{TopologyLoop, RECORD_BYTES};
use rt_runtime::executor::Command;
use rt_runtime::{Frame, NodeId};
use shoop_loops::audio::Smoothing;
use shoop_loops::runtime::{GraphStageId, NodePairInitial, NodePairStageId, Operation, Reply};
use shoop_loops::topology::staging::StageId;
use shoop_loops::{Timing, Transition};
use shoop_rt_protocol::{LoopSlotId, Outcome, Refusal};

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
    Append,
    Validate,
    Activate,
    Activating,
    Query,
    Cleanup,
    Finishing,
    ClearStop,
    ClearApply,
    Smoothing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// The first `len` entries of the endpoint's batch.
    Batch(usize),
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
    topology_needed: bool,
    graph_needed: bool,
    appended: bool,
    node_pairs: Option<NodePairStageId>,
    topology: Option<StageId>,
    records: usize,
    sent: usize,
    ready: bool,
    graph: Option<GraphStageId>,
    faded: bool,
    activated: bool,
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
            topology_needed: false,
            graph_needed: false,
            appended: false,
            node_pairs: None,
            topology: None,
            records: 0,
            sent: 0,
            ready: false,
            graph: None,
            faded: false,
            activated: false,
        }
    }
    pub(crate) fn batch(len: usize) -> Self {
        Self::new(Kind::Batch(len))
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
                self.clear_touched();
            }
            if !progressed {
                return;
            }
        }
    }

    fn answer_job(&mut self, job: &mut Job, outcome: Outcome) {
        if let Kind::Batch(len) = job.kind {
            for index in 0..len {
                if let Some(correlation) = self.batch[index].0.take() {
                    self.answer(correlation, outcome);
                }
            }
        } else if let Some(correlation) = job.correlation.take() {
            self.answer(correlation, outcome);
        }
    }

    pub(crate) fn forget_job_answers(&mut self) {
        if let Some(job) = &mut self.job {
            job.correlation = None;
        }
        for entry in self.batch.iter_mut() {
            entry.0 = None;
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

    /// Collect the nodes that fade out before the activation into `fades`:
    /// removed strips' outputs and the faders of removed loops on remaining
    /// strips.
    fn collect_fades(&mut self) -> usize {
        let mut count = 0;
        for slot in 0..self.model.strips.len() {
            if let Some(strip) = self.model.strips[slot] {
                if self.next.strip(strip.id).is_none() {
                    self.fades[count] = self.pool.strip_output(slot);
                    count += 1;
                }
            }
        }
        for slot in 0..self.model.loops.len() {
            let Some(entry) = self.model.loops[slot] else {
                continue;
            };
            if self.next.find_loop(entry.id).is_none() && self.next.strip(entry.strip).is_some() {
                self.fades[count] = self.pool.fader(slot);
                count += 1;
            }
        }
        count
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
            if job.faded {
                job.faded = false;
                let at = self.at();
                for index in 0..self.collect_fades() {
                    let node = self.fades[index];
                    let _ = self.submit_op(at, node, EngineOp::Presence(true), Purpose::Silent);
                }
            }
            job.phase = Phase::Cleanup;
            return true;
        }
        match job.phase {
            Phase::Start => self.start_job(job),
            Phase::Append => self.append(job),
            Phase::Validate => {
                if job.topology_needed && !job.ready {
                    let id = job.topology.expect("staged topology");
                    self.submit_job_loop(
                        job,
                        Frame(0),
                        Operation::TopologyValidate { id },
                        Step::TopologyValidate,
                    );
                } else {
                    job.phase = Phase::Activate;
                }
                true
            }
            Phase::Activate => self.activate(job),
            Phase::Activating => self.activated(job),
            Phase::Query => self.retire_media(job),
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
        match job.kind {
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
                true
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
                true
            }
            Kind::Batch(_) => self.prepare_batch(job),
        }
    }

    /// Start the independent preparations of the staged model together.
    fn prepare_batch(&mut self, job: &mut Job) -> bool {
        if !self.room(3) {
            return false;
        }
        let (model, next) = (&self.model, &self.next);
        job.edits = (0..model.loops.len())
            .filter(|&slot| model.loops[slot].map(|l| l.id) != next.loops[slot].map(|l| l.id))
            .count();
        job.topology_needed = (0..model.loops.len()).any(|slot| {
            model.loops[slot].map(|l| (l.id, l.sync)) != next.loops[slot].map(|l| (l.id, l.sync))
        });
        job.graph_needed = job.edits > 0
            || model.master_outputs != next.master_outputs
            || (0..model.strips.len()).any(|slot| {
                let shape = |strip: Option<crate::model::Strip>| {
                    strip.map(|s| {
                        (
                            s.id,
                            s.shape,
                            s.inputs,
                            s.outputs,
                            s.midi_inputs,
                            s.midi_outputs,
                        )
                    })
                };
                shape(model.strips[slot]) != shape(next.strips[slot])
            });
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
        if job.topology_needed {
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
            let Some(mut payload) = self.work.take_payload(1) else {
                job.fail(Refusal::Internal);
                return true;
            };
            payload.copy_from_slice(&self.work.image[..RECORD_BYTES]);
            let command = self.loops.command_with_payload(
                Frame(0),
                Operation::TopologyBegin { slot: 0 },
                payload,
            );
            self.submit_job(job, command, Step::TopologyBegin, true);
        }
        if job.graph_needed {
            let Some(mut plan) = self.work.plans.iter_mut().find_map(Option::take) else {
                job.fail(Refusal::Internal);
                return true;
            };
            if let Err(error) = self.work.builder.build(
                &mut plan,
                &self.next,
                &self.pool,
                &self.shape,
                self.planner,
            ) {
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
        }
        job.phase = Phase::Append;
        true
    }

    /// Append node-pair edits and topology records once their stages exist.
    /// Topology appends are scheduled one callback apart, as the loop library
    /// accepts one per callback, and validation follows the last.
    fn append(&mut self, job: &mut Job) -> bool {
        if job.edits > 0 && !job.appended {
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
        }
        if job.topology_needed && job.sent < job.records {
            let Some(id) = job.topology else {
                job.fail(Refusal::Internal);
                return true;
            };
            let per_append = self.settings.records_per_append;
            let chunks = (job.records - job.sent).div_ceil(per_append);
            if !self.room(chunks + 1) {
                return false;
            }
            let first = self.at().0;
            let spacing = u64::from(self.settings.frames);
            let mut at = first;
            while job.sent < job.records {
                let chunk = (job.records - job.sent).min(per_append);
                let Some(mut payload) = self.work.take_payload(chunk) else {
                    break;
                };
                let start = RECORD_BYTES * (1 + job.sent);
                payload.copy_from_slice(&self.work.image[start..start + chunk * RECORD_BYTES]);
                let command = self.loops.command_with_payload(
                    Frame(at),
                    Operation::TopologyAppend {
                        id,
                        first_record: job.sent,
                    },
                    payload,
                );
                self.submit_job(job, command, Step::TopologyAppend, true);
                job.sent += chunk;
                at += spacing;
            }
            if job.sent < job.records {
                return true;
            }
            self.submit_job_loop(
                job,
                Frame(at.saturating_sub(spacing).max(first)),
                Operation::TopologyValidate { id },
                Step::TopologyValidate,
            );
        }
        job.phase = Phase::Validate;
        true
    }

    fn activate(&mut self, job: &mut Job) -> bool {
        let faded = self.collect_fades();
        let removed = |slot: usize, model: &crate::model::Model, next: &crate::model::Model| {
            model.loops[slot].is_some_and(|entry| next.find_loop(entry.id).is_none())
        };
        let stopping = (0..self.model.loops.len())
            .filter(|&slot| removed(slot, &self.model, &self.next))
            .count();
        let bound = faded
            + stopping * (PLANNED + 1)
            + 4 * self.next.strips.len()
            + self.next.loops.len()
            + 2;
        if !self.room(bound) {
            return false;
        }
        let ramp = u64::from(self.settings.ramp);
        let base = self.at().0.max(self.quiet_until);
        let mut activation = base;
        for slot in 0..self.next.strips.len() {
            let Some(strip) = self.next.strips[slot] else {
                continue;
            };
            if self.model.strip(strip.id).is_some() {
                continue;
            }
            let reset = EngineOp::Reset {
                inputs: strip.shape.audio_inputs,
                outputs: strip.shape.audio_outputs,
            };
            for node in [self.pool.strip_input(slot), self.pool.strip_output(slot)] {
                self.submit_job_op(job, Frame(base), node, reset);
            }
        }
        for slot in 0..self.next.loops.len() {
            let Some(entry) = self.next.loops[slot] else {
                continue;
            };
            if self.model.find_loop(entry.id).is_none() {
                let reset = EngineOp::Reset {
                    inputs: 0,
                    outputs: 0,
                };
                self.submit_job_op(job, Frame(base), self.pool.fader(slot), reset);
            }
        }
        if faded > 0 {
            job.faded = true;
            for index in 0..faded {
                let node = self.fades[index];
                self.submit_job_op(job, Frame(base), node, EngineOp::Presence(false));
            }
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
        for slot in 0..self.next.strips.len() {
            let Some(strip) = self.next.strips[slot] else {
                continue;
            };
            if strip.inputs.swapped {
                self.submit_job_op(
                    job,
                    Frame(activation),
                    self.pool.strip_input(slot),
                    EngineOp::Crossfade,
                );
            }
            if strip.outputs.swapped {
                self.submit_job_op(
                    job,
                    Frame(activation),
                    self.pool.strip_output(slot),
                    EngineOp::Crossfade,
                );
            }
        }
        if self.next.master_outputs.swapped {
            self.submit_job_op(
                job,
                Frame(activation),
                self.pool.master(),
                EngineOp::Crossfade,
            );
        }
        let graph_epoch = self.graph_epoch;
        let topology_epoch = self.topology_epoch;
        let operation = match (job.edits > 0, job.topology, job.graph, job.node_pairs) {
            (true, Some(topology), Some(graph), Some(node_pairs)) => {
                Operation::ConfigurationActivate {
                    node_pairs,
                    topology,
                    graph,
                    graph_epoch,
                    topology_epoch,
                }
            }
            (false, Some(id), Some(graph), _) => Operation::TopologyGraphActivate {
                id,
                graph,
                graph_epoch,
                topology_epoch,
            },
            (false, Some(id), None, _) => Operation::TopologyActivate {
                id,
                graph_epoch,
                topology_epoch,
            },
            (false, None, Some(id), _) => Operation::GraphActivate {
                id,
                graph_epoch,
                topology_epoch,
            },
            _ => {
                job.fail(Refusal::Internal);
                return true;
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
        job.faded = false;
        let revision = self.model.revision.max(self.next.revision) + 1;
        std::mem::swap(&mut self.model, &mut self.next);
        self.model.settle();
        self.model.revision = revision;
        let created = (0..self.model.loops.len())
            .filter(|&slot| {
                let after = self.model.loops[slot].map(|l| l.id);
                after.is_some() && after != self.next.loops[slot].map(|l| l.id)
            })
            .count();
        if !self.room(2 * created + 3) {
            std::mem::swap(&mut self.model, &mut self.next);
            return false;
        }
        for slot in 0..self.model.loops.len() {
            if self.model.loops[slot].map(|l| l.id) != self.next.loops[slot].map(|l| l.id) {
                self.planned[slot] = [None; PLANNED];
                self.cells.loops[slot].peaks.clear();
                self.dirty[slot] = true;
            }
        }
        self.answer_job(job, Outcome::Done);
        let at = self.at();
        for slot in 0..self.model.loops.len() {
            let Some(entry) = self.model.loops[slot] else {
                continue;
            };
            if self.next.loops[slot].map(|l| l.id) == Some(entry.id) {
                continue;
            }
            let target = loop_id(entry.id);
            let operation = Operation::Smoothing {
                target,
                smoothing: smoothing(self.settings.smoothing),
            };
            self.submit_job_loop(job, at, operation, Step::Prelude);
            self.submit_job_loop(job, at, Operation::State { target }, Step::Query);
        }
        self.recycle(job);
        job.phase = Phase::Query;
        true
    }

    /// Release the initial media the new loops reported.
    fn retire_media(&mut self, job: &mut Job) -> bool {
        if !self.room(self.retired.len()) {
            return false;
        }
        let at = self.at();
        while let Some(media) = self.retired.pop() {
            self.submit_job_loop(job, at, Operation::Retire { media }, Step::Retire);
        }
        job.phase = Phase::Finishing;
        true
    }

    /// Hand the stages back: recycled after an activation, cancelled otherwise.
    fn recycle(&mut self, job: &mut Job) {
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
    }

    fn cleanup(&mut self, job: &mut Job) -> bool {
        if !self.room(3) {
            return false;
        }
        self.recycle(job);
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
        let mut media = None;
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
                if !matches!(step, Step::Cleanup | Step::Query) {
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
                        }
                        | Reply::TopologyGraphActivated {
                            graph_epoch,
                            topology_epoch,
                            ..
                        },
                    ),
                ) => {
                    job.activated = true;
                    epochs = Some((graph_epoch, topology_epoch));
                }
                (Step::Query, Some(Reply::State(cut))) => media = cut.playback,
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
        if let Some(media) = media {
            let _ = self.retired.push(media);
        }
        if let Some(correlation) = answered {
            self.answer(correlation, Outcome::Done);
        }
    }
}
