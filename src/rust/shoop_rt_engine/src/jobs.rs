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

use alloc::boxed::Box;
use core::sync::atomic::Ordering;

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
    ProcessorPrepare,
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
    Configure,
    ContentAdopt,
    ContentSelect,
    ContentMetadata,
    FamilySelect,
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
    Configure,
    Prepared,
    ContentSelect,
    ContentFinish,
    FamilyFinish,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// The first `len` entries of the endpoint's batch.
    Batch(usize),
    Clear(LoopSlotId),
    Smoothing(u32),
    Replacement,
    Composite {
        target: LoopSlotId,
        conversion: bool,
    },
    Content {
        id: LoopSlotId,
        frames: u64,
        audio: Option<shoop_loops::runtime::NodePairSourceId>,
        auxiliary: Option<shoop_loops::runtime::NodePairSourceId>,
        midi: Option<shoop_loops::runtime::NodePairSourceId>,
        recorded: Option<shoop_rt_protocol::RecordedProcessorState>,
        family: Option<usize>,
    },
    Family {
        commit: bool,
    },
}

pub(crate) fn smoothing(frames: u32) -> Smoothing {
    match frames {
        0 => Smoothing::None,
        frames => Smoothing::LinearRestart { frames },
    }
}

pub(crate) struct Job {
    pub(crate) content_timing: Option<shoop_rt_protocol::TimingState>,
    pub correlation: Option<u64>,
    kind: Kind,
    phase: Phase,
    outstanding: usize,
    failure: Option<Refusal>,
    edits: usize,
    topology_needed: bool,
    configuration_needed: bool,
    graph_needed: bool,
    appended: bool,
    node_pairs: Option<NodePairStageId>,
    topology: Option<StageId>,
    records: usize,
    sent: usize,
    topology_callback: Option<u64>,
    ready: bool,
    graph: Option<GraphStageId>,
    faded: bool,
    activated: bool,
    applied_frame: Option<u64>,
    configured: bool,
    processors_prepared: bool,
    committed: bool,
    cancelled: bool,
    content_media: Option<shoop_loops::media::MediaId>,
    content_revision: Option<rt_runtime::Revision>,
    metadata_published: bool,
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
            configuration_needed: false,
            graph_needed: false,
            appended: false,
            node_pairs: None,
            topology: None,
            records: 0,
            sent: 0,
            topology_callback: None,
            ready: false,
            graph: None,
            faded: false,
            activated: false,
            applied_frame: None,
            configured: false,
            processors_prepared: false,
            committed: false,
            cancelled: false,
            content_timing: None,
            content_media: None,
            content_revision: None,
            metadata_published: false,
        }
    }
    pub(crate) fn batch(len: usize) -> Self {
        Self::new(Kind::Batch(len))
    }
    pub(crate) fn composite(target: LoopSlotId, conversion: bool) -> Self {
        Self::new(Kind::Composite { target, conversion })
    }
    pub(crate) fn cancel_composite(&mut self) {
        self.fail(Refusal::Busy);
    }
    pub(crate) fn replacement(correlation: u64) -> Self {
        let mut job = Self::new(Kind::Replacement);
        job.correlation = Some(correlation);
        job
    }
    fn replacing(&self) -> bool {
        self.kind == Kind::Replacement
    }
    pub(crate) fn idle(&self) -> bool {
        self.phase == Phase::Prepared && self.outstanding == 0
    }
    pub(crate) fn commit_replacement(&mut self, correlation: u64) -> Result<(), Refusal> {
        if !self.idle() || !self.replacing() || self.cancelled {
            return Err(Refusal::Busy);
        }
        self.committed = true;
        self.correlation = Some(correlation);
        self.phase = Phase::Activate;
        Ok(())
    }
    pub(crate) fn cancel_replacement(&mut self, correlation: u64) -> Result<Option<u64>, Refusal> {
        if self.committed || !self.replacing() || self.cancelled {
            return Err(Refusal::Busy);
        }
        self.cancelled = true;
        Ok(self.correlation.replace(correlation))
    }
    pub(crate) fn clear(id: LoopSlotId) -> Self {
        Self::new(Kind::Clear(id))
    }
    pub(crate) fn smoothing(frames: u32) -> Self {
        Self::new(Kind::Smoothing(frames))
    }
    pub(crate) fn content(
        id: LoopSlotId,
        frames: u64,
        audio: Option<shoop_loops::runtime::NodePairSourceId>,
        auxiliary: Option<shoop_loops::runtime::NodePairSourceId>,
        midi: Option<shoop_loops::runtime::NodePairSourceId>,
        recorded: Option<shoop_rt_protocol::RecordedProcessorState>,
    ) -> Self {
        Self::new(Kind::Content {
            id,
            frames,
            audio,
            auxiliary,
            midi,
            recorded,
            family: None,
        })
    }
    pub(crate) fn family_append(
        slot: usize,
        id: LoopSlotId,
        frames: u64,
        audio: Option<shoop_loops::runtime::NodePairSourceId>,
        auxiliary: Option<shoop_loops::runtime::NodePairSourceId>,
        midi: Option<shoop_loops::runtime::NodePairSourceId>,
        recorded: Option<shoop_rt_protocol::RecordedProcessorState>,
    ) -> Self {
        Self::new(Kind::Content {
            id,
            frames,
            audio,
            auxiliary,
            midi,
            recorded,
            family: Some(slot),
        })
    }
    pub(crate) fn family(commit: bool) -> Self {
        Self::new(Kind::Family { commit })
    }
    fn fail(&mut self, refusal: Refusal) {
        self.failure.get_or_insert(refusal);
    }
}

impl ControlEndpoint {
    fn configuration_nodes(&self) -> usize {
        self.next.buses.iter().flatten().count()
            + 2 * self.next.strips.iter().flatten().count()
            + self
                .next
                .loops
                .iter()
                .flatten()
                .filter(|l| l.program.is_none())
                .count()
    }
    fn prepare_node_configuration(&mut self, job: &mut Job) -> bool {
        if job.configured {
            if job.replacing() {
                self.replacement.state = crate::replacement::State::Prepared;
                job.phase = Phase::Prepared;
                self.answer_job(job, Outcome::Done);
            } else {
                job.phase = Phase::Activate;
            }
            return true;
        }
        if !self.room(self.configuration_nodes()) {
            return false;
        }
        let Ok(epoch) = self.graph_epoch.checked_next() else {
            job.fail(Refusal::Capacity);
            return true;
        };
        let base = crate::NodeConfiguration {
            recorded_state: None,
            processor: None,
            generation: 0,
            token: if job.replacing() {
                self.replacement.id.expect("live replacement")
            } else {
                u64::MAX
            },
            epoch,
            inputs: 0,
            outputs: 0,
            bank: 0,
            send_bank: 0,
            return_bank: 0,
            gain: 1.,
            balance: 0.,
            monitoring: false,
            muted: false,
        };
        job.configured = true;
        for slot in 0..self.next.strips.len() {
            let Some(strip) = self.next.strips[slot] else {
                continue;
            };
            if !job.replacing() && self.model.strip(strip.id).is_some() {
                continue;
            }
            let controls = strip.controls;
            self.submit_configuration(
                job,
                self.pool.strip_input(slot),
                crate::NodeConfiguration {
                    processor: strip.processor.map(|owner| owner.kind),
                    generation: strip.id.0.generation,
                    inputs: strip.shape.audio_inputs,
                    bank: strip.inputs.active as u8,
                    send_bank: strip.sends.active as u8,
                    return_bank: strip.returns.active as u8,
                    gain: controls.input_gain,
                    balance: controls.input_balance,
                    monitoring: controls.monitoring,
                    ..base
                },
            );
            self.submit_configuration(
                job,
                self.pool.strip_output(slot),
                crate::NodeConfiguration {
                    processor: strip.processor.map(|owner| owner.kind),
                    generation: strip.id.0.generation,
                    outputs: strip.shape.audio_outputs,
                    bank: strip.outputs.active as u8,
                    gain: controls.output_gain,
                    balance: controls.output_balance,
                    muted: controls.output_mute,
                    ..base
                },
            );
        }
        for slot in 0..self.next.loops.len() {
            if let Some(entry) = self.next.loops[slot].filter(|entry| entry.program.is_none()) {
                if !job.replacing() && self.model.find_loop(entry.id).is_some() {
                    continue;
                }
                let processor = self
                    .next
                    .strip(entry.strip.expect("primitive owner"))
                    .and_then(|strip| strip.processor)
                    .map(|owner| owner.kind);
                self.submit_configuration(
                    job,
                    self.pool.fader(slot),
                    crate::NodeConfiguration {
                        processor,
                        generation: entry.id.0.generation,
                        recorded_state: if job.replacing() {
                            self.replacement.initial[slot]
                                .and_then(|initial| initial.recorded_state)
                        } else {
                            None
                        },
                        ..base
                    },
                );
            }
        }
        for slot in 0..self.next.buses.len() {
            if !job.replacing() {
                break;
            }
            let Some(bus) = self.next.buses[slot] else {
                continue;
            };
            self.submit_configuration(
                job,
                self.pool.bus(slot),
                crate::NodeConfiguration {
                    generation: bus.id.0.generation,
                    outputs: bus.channels,
                    bank: bus.outputs.active as u8,
                    gain: bus.gain,
                    balance: bus.balance,
                    muted: bus.mute,
                    ..base
                },
            );
        }
        true
    }
    fn submit_configuration(
        &mut self,
        job: &mut Job,
        node: NodeId,
        configuration: crate::NodeConfiguration,
    ) {
        self.submit_job(
            job,
            Command::Schedule {
                at: Frame(0),
                action: rt_runtime::schedule::Action::Processor {
                    node,
                    operation: EngineOp::Configure(configuration),
                },
                payload: Box::new([]),
            },
            Step::Configure,
            true,
        );
    }

    pub(crate) fn advance_job(&mut self) {
        if self.job.is_none()
            && self.family.id.is_some()
            && self.family.cancel_requested
            && !self.family.committing
        {
            self.job = Some(Job::family(false));
        }
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
                if job.replacing() {
                    if job.cancelled {
                        self.answer_job(&mut job, Outcome::Done);
                    }
                    self.replacement.clear();
                }
                if matches!(job.kind, Kind::Composite { .. }) {
                    self.composite.clear();
                    self.model.revision += 1;
                }
                self.clear_touched();
                if self.family.id.is_some()
                    && self.family.cancel_requested
                    && !self.family.committing
                {
                    self.job = Some(Job::family(false));
                }
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
            if job.replacing() && !job.committed {
                job.cancelled = true;
            }
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
            Ok(sequence) => {
                if step == Step::Activation && matches!(job.kind, Kind::Composite { .. }) {
                    self.composite.activation = Some(sequence);
                }
                job.outstanding += 1;
            }
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
            if entry.program.is_none()
                && self.next.find_loop(entry.id).is_none()
                && entry.strip.and_then(|id| self.next.strip(id)).is_some()
            {
                self.fades[count] = self.pool.fader(slot);
                count += 1;
            }
        }
        for slot in 0..self.model.buses.len() {
            if self.model.buses[slot].is_some_and(|bus| self.next.bus(bus.id).is_none()) {
                self.fades[count] = self.pool.bus(slot);
                count += 1;
            }
        }
        count
    }

    /// Run the next phase of `job`; returns whether it can continue immediately.
    fn step_job(&mut self, job: &mut Job) -> bool {
        if let Kind::Family { commit } = job.kind {
            return self.step_family(job, commit);
        }
        if job.cancelled && !matches!(job.phase, Phase::Cleanup | Phase::Finishing) {
            job.phase = Phase::Cleanup;
            return true;
        }
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
            Phase::ContentSelect => {
                if let Kind::Content {
                    family: Some(slot), ..
                } = job.kind
                {
                    let entry = self.family.entries[slot]
                        .as_mut()
                        .expect("staged family entry");
                    entry.media = job.content_media.take();
                    entry.revision = job.content_revision;
                    entry.timing.content_revision =
                        job.content_revision.expect("adopted revision").0;
                    self.answer_job(
                        job,
                        Outcome::ContentPrepared {
                            revision: job.content_revision.expect("adopted revision").0,
                        },
                    );
                    job.phase = Phase::Finishing;
                    return true;
                }
                if !self.room(1) {
                    return false;
                }
                let Kind::Content { id, .. } = job.kind else {
                    unreachable!()
                };
                let media = job.content_media.expect("adopted content");
                self.submit_job_loop(
                    job,
                    Frame(0),
                    Operation::Select {
                        target: loop_id(id),
                        timing: Timing::At,
                        media,
                        preserve_position: false,
                    },
                    Step::ContentSelect,
                );
                job.phase = Phase::ContentFinish;
                true
            }
            Phase::ContentFinish => {
                if !self.room(1) {
                    return false;
                }
                if !job.metadata_published {
                    let Kind::Content { id, recorded, .. } = job.kind else {
                        unreachable!()
                    };
                    let operation = EngineOp::RecordedState {
                        generation: id.0.generation,
                        revision: job.content_revision.expect("adopted revision").0,
                        state: recorded,
                    };
                    self.submit_job(
                        job,
                        Command::Schedule {
                            at: Frame(0),
                            action: rt_runtime::schedule::Action::Processor {
                                node: self.pool.fader(id.0.index as usize),
                                operation,
                            },
                            payload: Box::new([]),
                        },
                        Step::ContentMetadata,
                        true,
                    );
                    job.metadata_published = true;
                    return true;
                }
                if let Some(media) = job.content_media.take() {
                    self.submit_job_loop(job, Frame(0), Operation::Retire { media }, Step::Retire);
                }
                if let (Kind::Content { id, .. }, Some(mut timing)) = (job.kind, job.content_timing)
                {
                    timing.content_revision = job.content_revision.expect("adopted revision").0;
                    self.publish_content_timing(id, timing);
                }
                self.answer_job(
                    job,
                    Outcome::ContentApplied {
                        revision: job.content_revision.expect("adopted revision").0,
                    },
                );
                job.phase = Phase::Finishing;
                true
            }
            Phase::Start => self.start_job(job),
            Phase::FamilyFinish => unreachable!("family job handled separately"),
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
                    job.phase = Phase::Configure;
                }
                true
            }
            Phase::Activate => self.activate(job),
            Phase::Configure => self.prepare_node_configuration(job),
            Phase::Prepared => false,
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
            Kind::Content {
                id,
                frames,
                audio,
                auxiliary,
                midi,
                ..
            } => {
                if !self.room(1) {
                    return false;
                }
                self.submit_job_loop(
                    job,
                    Frame(0),
                    Operation::Adopt {
                        auxiliary,
                        target: loop_id(id),
                        frames,
                        audio,
                        midi,
                    },
                    Step::ContentAdopt,
                );
                job.phase = Phase::ContentSelect;
                true
            }
            Kind::Smoothing(frames) => {
                let loops = self
                    .model
                    .loops
                    .iter()
                    .flatten()
                    .filter(|l| l.program.is_none())
                    .count();
                if !self.room(loops) {
                    return false;
                }
                let at = self.at();
                for slot in 0..self.model.loops.len() {
                    let Some(entry) = self.model.loops[slot].filter(|l| l.program.is_none()) else {
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
            Kind::Batch(_) | Kind::Replacement | Kind::Composite { .. } => self.prepare_batch(job),
            Kind::Family { .. } => unreachable!("family job handled separately"),
        }
    }

    fn step_family(&mut self, job: &mut Job, commit: bool) -> bool {
        if job.phase == Phase::Start {
            if commit {
                if !self.room(1) || self.family.payload.is_none() {
                    return false;
                }
                let payload = match self.family_payload() {
                    Ok(payload) => payload,
                    Err(refusal) => {
                        job.fail(refusal);
                        job.phase = Phase::FamilyFinish;
                        return true;
                    }
                };
                let operation = Operation::SelectFamily {
                    root: loop_id(self.family.root.expect("staged family root")),
                    topology_epoch: self.family.epoch,
                };
                let command = self
                    .loops
                    .command_with_payload(Frame(0), operation, payload);
                self.submit_job(job, command, Step::FamilySelect, true);
            }
            job.phase = Phase::FamilyFinish;
            return true;
        }
        if job.phase != Phase::FamilyFinish {
            job.fail(Refusal::Internal);
        }
        if commit && job.failure.is_none() && !job.metadata_published {
            while job.sent < self.family.entries.len() {
                let slot = job.sent;
                let Some(entry) = self.family.entries[slot].filter(|entry| entry.edit.replace)
                else {
                    job.sent += 1;
                    continue;
                };
                if !self.room(1) {
                    return false;
                }
                self.submit_job(
                    job,
                    Command::Schedule {
                        at: Frame(0),
                        action: rt_runtime::schedule::Action::Processor {
                            node: self.pool.fader(slot),
                            operation: EngineOp::RecordedState {
                                generation: entry.edit.loop_id.0.generation,
                                revision: entry.revision.expect("adopted family revision").0,
                                state: entry.edit.recorded_state,
                            },
                        },
                        payload: Box::new([]),
                    },
                    Step::ContentMetadata,
                    true,
                );
                job.sent += 1;
                return true;
            }
            job.metadata_published = true;
        }
        for slot in 0..self.family.entries.len() {
            if self.family.entries[slot].is_some_and(|entry| entry.media.is_some()) {
                if !self.room(1) {
                    return false;
                }
                let media = self.family.entries[slot]
                    .as_mut()
                    .unwrap()
                    .media
                    .take()
                    .unwrap();
                self.submit_job_loop(job, Frame(0), Operation::Retire { media }, Step::Retire);
                return true;
            }
        }
        if job.failure.is_none() && job.kind == (Kind::Family { commit: true }) {
            for slot in 0..self.family.entries.len() {
                if let Some(entry) = self.family.entries[slot] {
                    if entry.edit.replace {
                        self.publish_content_timing(entry.edit.loop_id, entry.timing);
                    }
                }
            }
        }
        self.answer_job(job, job.failure.map_or(Outcome::Done, Outcome::Refused));
        self.family.clear();
        job.phase = Phase::Finishing;
        true
    }

    /// Start the independent preparations of the staged model together.
    fn prepare_batch(&mut self, job: &mut Job) -> bool {
        if !job.processors_prepared {
            let count = self
                .next
                .strips
                .iter()
                .flatten()
                .filter(|strip| {
                    strip.processor.and_then(|owner| owner.slot).is_some()
                        && self.model.strip(strip.id).and_then(|old| old.processor)
                            != strip.processor
                })
                .count();
            let bus_count = self
                .next
                .buses
                .iter()
                .flatten()
                .filter(|bus| {
                    bus.processor.is_some()
                        && self.model.bus(bus.id).and_then(|old| old.processor) != bus.processor
                })
                .count();
            if !self.room((count + bus_count) * 4) {
                return false;
            }
            job.processors_prepared = true;
            let Ok(epoch) = self.graph_epoch.checked_next() else {
                job.fail(Refusal::Capacity);
                return true;
            };
            for index in 0..self.next.strips.len() {
                let Some(strip) = self.next.strips[index] else {
                    continue;
                };
                if self.model.strip(strip.id).and_then(|old| old.processor) == strip.processor {
                    continue;
                }
                let Some(owner) = strip.processor else {
                    continue;
                };
                let Some(slot) = owner.slot else {
                    continue;
                };
                let node = self.pool.processor(slot.index as usize);
                for operation in [
                    Some(EngineOp::ProcessorClaim {
                        generation: slot.generation,
                    }),
                    (owner.kind == shoop_rt_protocol::ProcessorKind::BuiltInFx).then_some(
                        EngineOp::ProcessorParameter {
                            generation: slot.generation,
                            parameter: shoop_rt_protocol::processors::WIDTH,
                            value: strip.shape.audio_inputs as f32,
                        },
                    ),
                    if job.replacing() {
                        self.replacement.processor_states[index].map(|control| {
                            EngineOp::ProcessorControl {
                                generation: slot.generation,
                                control,
                            }
                        })
                    } else {
                        None
                    },
                    Some(EngineOp::ProcessorBind {
                        generation: slot.generation,
                        strip_generation: strip.id.0.generation,
                        source: index as u32,
                        epoch,
                    }),
                ]
                .into_iter()
                .flatten()
                {
                    self.submit_job(
                        job,
                        Command::Schedule {
                            at: Frame(0),
                            action: rt_runtime::schedule::Action::Processor { node, operation },
                            payload: Box::new([]),
                        },
                        Step::ProcessorPrepare,
                        true,
                    );
                }
            }
            for index in 0..self.next.buses.len() {
                let Some(bus) = self.next.buses[index] else {
                    continue;
                };
                if self.model.bus(bus.id).and_then(|old| old.processor) == bus.processor {
                    continue;
                }
                let Some(slot) = bus.processor.and_then(|owner| owner.slot) else {
                    continue;
                };
                let node = self.pool.processor(slot.index as usize);
                for operation in [
                    Some(EngineOp::ProcessorClaim {
                        generation: slot.generation,
                    }),
                    Some(EngineOp::ProcessorParameter {
                        generation: slot.generation,
                        parameter: shoop_rt_protocol::processors::WIDTH,
                        value: bus.channels as f32,
                    }),
                    if job.replacing() {
                        self.replacement.bus_processor_states[index].map(|control| {
                            EngineOp::ProcessorControl {
                                generation: slot.generation,
                                control,
                            }
                        })
                    } else {
                        None
                    },
                    Some(EngineOp::ProcessorBindBus {
                        generation: slot.generation,
                        bus: bus.id,
                        epoch,
                    }),
                ]
                .into_iter()
                .flatten()
                {
                    self.submit_job(
                        job,
                        Command::Schedule {
                            at: Frame(0),
                            action: rt_runtime::schedule::Action::Processor { node, operation },
                            payload: Box::new([]),
                        },
                        Step::ProcessorPrepare,
                        true,
                    );
                }
            }
            if count + bus_count > 0 {
                return true;
            }
        }
        if !self.room(3 + self.configuration_nodes()) {
            return false;
        }
        for entry in self.next.strips.iter().flatten() {
            let active = self.model.strips[entry.id.0.index as usize]
                .map_or(0, |strip| strip.id.0.generation);
            self.cells.strips[entry.id.0.index as usize].configure_policy(
                entry.id.0.generation,
                active,
                entry.controls.automatic_monitoring,
            );
        }
        for entry in self
            .next
            .loops
            .iter()
            .flatten()
            .filter(|entry| entry.program.is_none())
        {
            if let Some(strip) = entry.strip {
                let active = self.model.loops[entry.id.0.index as usize]
                    .map_or(0, |entry| entry.id.0.generation);
                self.cells.loops[entry.id.0.index as usize].configure_owner(
                    entry.id.0.generation,
                    strip.0.index,
                    active,
                );
            }
        }
        let (model, next) = (&self.model, &self.next);
        job.edits = (0..model.loops.len())
            .filter(|&slot| {
                model.loops[slot]
                    .filter(|l| l.program.is_none())
                    .map(|l| l.id)
                    != next.loops[slot]
                        .filter(|l| l.program.is_none())
                        .map(|l| l.id)
            })
            .count();
        job.configuration_needed = (0..model.loops.len())
            .any(|slot| model.loops[slot].map(|l| l.id) != next.loops[slot].map(|l| l.id));
        job.topology_needed = (0..model.loops.len()).any(|slot| {
            model.loops[slot].map(|l| (l.id, l.sync, l.program))
                != next.loops[slot].map(|l| (l.id, l.sync, l.program))
        });
        job.graph_needed = job.configuration_needed
            || model.global_midi != next.global_midi
            || model.buses != next.buses
            || (0..model.strips.len()).any(|slot| {
                let shape = |strip: Option<crate::model::Strip>| {
                    strip.map(|s| {
                        (
                            s.id,
                            s.shape,
                            s.processor,
                            s.inputs,
                            s.outputs,
                            s.midi_inputs,
                            s.midi_outputs,
                            s.sends,
                            s.returns,
                            s.midi_send,
                        )
                    })
                };
                shape(model.strips[slot]) != shape(next.strips[slot])
            });
        if job.replacing() {
            job.topology_needed = true;
            job.graph_needed = true;
        }
        if job.configuration_needed || job.replacing() {
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
                        program: entry.program,
                        length: entry.seed_frames,
                    })
                },
                &next.spans,
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
            if let Some(control) = &self.cells.profiling {
                control.reset();
            }
            if let Err(error) =
                self.work
                    .builder
                    .build(&mut plan, &self.next, &self.pool, &self.shape)
            {
                self.work.return_plan(plan);
                job.fail(if error == rt_runtime::Error::Capacity {
                    Refusal::Capacity
                } else {
                    Refusal::Internal
                });
                return true;
            }
            self.profile_layout.stage(plan.descriptor(), &self.next);
            let epoch = self.graph_epoch;
            self.submit_job(
                job,
                Command::stage_graph(0, epoch, plan),
                Step::StageGraph,
                false,
            );
        }
        if !self.prepare_node_configuration(job) {
            return false;
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
                let before = self.model.loops[slot]
                    .filter(|l| l.program.is_none())
                    .map(|l| l.id);
                let after = self.next.loops[slot]
                    .filter(|l| l.program.is_none())
                    .map(|l| l.id);
                if before == after {
                    continue;
                }
                let initial = match after {
                    None => None,
                    Some(created) if job.replacing() => {
                        let data =
                            self.replacement.initial[slot].expect("validated candidate loop");
                        let audio = data
                            .audio
                            .map(|id| self.uploads.source(id, false, data.frames))
                            .transpose();
                        let midi = data
                            .midi
                            .map(|id| self.uploads.source(id, true, data.frames))
                            .transpose();
                        let auxiliary = data
                            .dry_audio
                            .map(|id| self.uploads.source(id, false, data.frames))
                            .transpose();
                        let (Ok(audio), Ok(auxiliary), Ok(midi)) = (audio, auxiliary, midi) else {
                            job.fail(Refusal::InvalidValue);
                            return true;
                        };
                        let entry = self.next.loops[slot].expect("candidate loop");
                        Some(NodePairInitial {
                            history_source: entry.strip.expect("primitive owner").0.index,
                            audio_widths: self
                                .next
                                .strip(entry.strip.expect("primitive owner"))
                                .map(|strip| {
                                    [
                                        if strip.processor.is_some() {
                                            strip.shape.audio_outputs
                                        } else {
                                            strip.shape.audio_inputs
                                        },
                                        if strip.processor.is_some() {
                                            strip.shape.audio_inputs
                                        } else {
                                            0
                                        },
                                    ]
                                }),
                            dry_wet: self
                                .next
                                .strip(entry.strip.expect("primitive owner"))
                                .is_some_and(|strip| strip.processor.is_some()),
                            auxiliary,
                            target: loop_id(created),
                            frames: data.frames,
                            audio,
                            midi,
                            gain: entry.gain,
                            balance: entry.balance,
                            preserve_input: false,
                        })
                    }
                    Some(created) => {
                        let entry = self.next.loops[slot].expect("new loop");
                        Some(NodePairInitial {
                            history_source: entry.strip.expect("primitive owner").0.index,
                            audio_widths: self
                                .next
                                .strip(entry.strip.expect("primitive owner"))
                                .map(|strip| {
                                    [
                                        if strip.processor.is_some() {
                                            strip.shape.audio_outputs
                                        } else {
                                            strip.shape.audio_inputs
                                        },
                                        if strip.processor.is_some() {
                                            strip.shape.audio_inputs
                                        } else {
                                            0
                                        },
                                    ]
                                }),
                            dry_wet: self
                                .next
                                .strip(entry.strip.expect("primitive owner"))
                                .is_some_and(|strip| strip.processor.is_some()),
                            auxiliary: None,
                            target: loop_id(created),
                            frames: 0,
                            audio: None,
                            midi: None,
                            gain: 1.,
                            balance: 0.,
                            preserve_input: false,
                        })
                    }
                };
                self.submit_job_loop(
                    job,
                    Frame(0),
                    Operation::NodePairAppend {
                        id,
                        previous: before.map(loop_id),
                        initial,
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
            if !self.room(2)
                || job.topology_callback == Some(self.cells.callbacks.load(Ordering::Relaxed))
            {
                return false;
            }
            let chunk = (job.records - job.sent).min(per_append);
            let Some(mut payload) = self.work.take_payload(chunk) else {
                return false;
            };
            let start = RECORD_BYTES * (1 + job.sent);
            payload.copy_from_slice(&self.work.image[start..start + chunk * RECORD_BYTES]);
            let command = self.loops.command_with_payload(
                self.at(),
                Operation::TopologyAppend {
                    id,
                    first_record: job.sent,
                },
                payload,
            );
            self.submit_job(job, command, Step::TopologyAppend, true);
            job.sent += chunk;
            if job.sent == job.records {
                self.submit_job_loop(
                    job,
                    self.at(),
                    Operation::TopologyValidate { id },
                    Step::TopologyValidate,
                );
                job.phase = Phase::Validate;
            }
            return true;
        }
        job.phase = Phase::Validate;
        true
    }

    fn activate(&mut self, job: &mut Job) -> bool {
        if let Kind::Composite {
            target,
            conversion: false,
        } = job.kind
        {
            if !self.room(1) {
                return false;
            }
            self.submit_job_loop(
                job,
                self.at(),
                Operation::ProgramActivate {
                    id: job.topology.expect("prepared composite topology"),
                    target: loop_id(target),
                    graph_epoch: self.graph_epoch,
                    topology_epoch: self.topology_epoch,
                },
                Step::Activation,
            );
            job.phase = Phase::Activating;
            return true;
        }
        if let Kind::Composite {
            conversion: true, ..
        } = job.kind
        {
            if !self.room(1) {
                return false;
            }
            let previous = self
                .composite
                .description
                .and_then(|d| d.previous)
                .expect("conversion source");
            self.submit_job_loop(
                job,
                self.at(),
                Operation::ConvertEmpty {
                    previous: loop_id(previous),
                    node_pairs: job.node_pairs.expect("prepared node pairs"),
                    topology: job.topology.expect("prepared topology"),
                    graph: job.graph.expect("prepared graph"),
                    graph_epoch: self.graph_epoch,
                    topology_epoch: self.topology_epoch,
                },
                Step::Activation,
            );
            job.phase = Phase::Activating;
            return true;
        }
        if job.replacing() {
            let repeats = self
                .next
                .loops
                .iter()
                .flatten()
                .filter(|l| l.repeat_sync)
                .count();
            if !self.room(repeats + 1) {
                return false;
            }
            self.submit_job_loop(
                job,
                Frame(0),
                Operation::ConfigurationReplace {
                    node_pairs: job.node_pairs.expect("prepared node pairs"),
                    topology: job.topology.expect("prepared topology"),
                    graph: job.graph.expect("prepared graph"),
                    graph_epoch: self.graph_epoch,
                    topology_epoch: self.topology_epoch,
                },
                Step::Activation,
            );
            for slot in 0..self.next.loops.len() {
                if let Some(entry) = self.next.loops[slot].filter(|l| l.repeat_sync) {
                    self.submit_job_loop(
                        job,
                        Frame(0),
                        Operation::RepeatSync {
                            target: loop_id(entry.id),
                            enabled: true,
                        },
                        Step::Prelude,
                    );
                }
            }
            job.phase = Phase::Activating;
            return true;
        }
        let faded = self.collect_fades();
        let removed = |slot: usize, model: &crate::model::Model, next: &crate::model::Model| {
            model.loops[slot].is_some_and(|entry| next.find_loop(entry.id).is_none())
        };
        let stopping = (0..self.model.loops.len())
            .filter(|&slot| removed(slot, &self.model, &self.next))
            .count();
        let bound = faded
            + stopping * (PLANNED + 1)
            + 6 * self.next.strips.len()
            + self.next.loops.len()
            + 2 * self.next.buses.len()
            + 2;
        if !self.room(bound) {
            return false;
        }
        let timing = (|| {
            let quantum = rt_runtime::Quantum::new(self.limits.quantum as usize)?;
            let base = quantum.ceil(Frame(self.at().0.max(self.quiet_until)))?;
            let stop = quantum.ceil(base.checked_add(self.settings.ramp as usize)?)?;
            let activation = if faded == 0 && stopping == 0 {
                base
            } else if stopping == 0 {
                stop
            } else {
                stop.checked_add(quantum.frames())?
            };
            let quiet = activation.checked_add(self.settings.ramp as usize)?;
            Ok::<_, rt_runtime::Error>((base.0, stop.0, activation.0, quiet.0))
        })();
        let Ok((base, stop, activation, quiet)) = timing else {
            job.fail(Refusal::InvalidValue);
            return true;
        };
        if faded > 0 {
            job.faded = true;
            for index in 0..faded {
                let node = self.fades[index];
                self.submit_job_op(job, Frame(base), node, EngineOp::Presence(false));
            }
            for slot in 0..self.model.loops.len() {
                if !removed(slot, &self.model, &self.next) {
                    continue;
                }
                let id = self.model.loops[slot].expect("removed loop").id;
                self.cancel_planned(slot, None, Frame(base));
                self.submit_job_loop(
                    job,
                    Frame(stop),
                    Operation::Transition {
                        target: loop_id(id),
                        timing: Timing::At,
                        transition: Transition::Stop,
                    },
                    Step::Prelude,
                );
            }
        }
        for slot in 0..self.model.loops.len() {
            if let Some(entry) = self.model.loops[slot]
                .filter(|entry| entry.program.is_some() && removed(slot, &self.model, &self.next))
            {
                self.cancel_planned(slot, None, Frame(base));
                self.submit_job_loop(
                    job,
                    Frame(stop),
                    Operation::Transition {
                        target: loop_id(entry.id),
                        timing: Timing::At,
                        transition: Transition::Stop,
                    },
                    Step::Prelude,
                );
            }
        }
        for slot in 0..self.next.strips.len() {
            let Some(strip) = self.next.strips[slot] else {
                continue;
            };
            for (changed, operation) in [
                (strip.sends.swapped, EngineOp::CrossfadeSend),
                (strip.returns.swapped, EngineOp::CrossfadeReturn),
            ] {
                if changed {
                    self.submit_job_op(
                        job,
                        Frame(activation),
                        self.pool.strip_input(slot),
                        operation,
                    );
                }
            }
            if strip.inputs.swapped {
                self.submit_job_op(
                    job,
                    Frame(activation),
                    self.pool.strip_input(slot),
                    EngineOp::Crossfade,
                );
            }
            if job.graph.is_some() && self.model.strip(strip.id).is_some() {
                let Ok(epoch) = self.graph_epoch.checked_next() else {
                    job.fail(Refusal::Capacity);
                    return true;
                };
                self.submit_job_op(
                    job,
                    Frame(activation),
                    self.pool.strip_input(slot),
                    EngineOp::InputSource {
                        epoch,
                        changed: self.model.strips[slot]
                            .is_none_or(|old| old.midi_inputs != strip.midi_inputs),
                    },
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
        for slot in 0..self.next.buses.len() {
            let Some(bus) = self.next.buses[slot] else {
                continue;
            };
            if self.model.bus(bus.id).is_none() {
                self.submit_job_op(
                    job,
                    Frame(base),
                    self.pool.bus_input(slot),
                    EngineOp::Reset {
                        inputs: 0,
                        outputs: bus.channels,
                        generation: bus.id.0.generation,
                    },
                );
                self.submit_job_op(
                    job,
                    Frame(base),
                    self.pool.bus(slot),
                    EngineOp::Reset {
                        inputs: 0,
                        outputs: bus.channels,
                        generation: bus.id.0.generation,
                    },
                );
            }
            if bus.outputs.swapped {
                self.submit_job_op(
                    job,
                    Frame(activation),
                    self.pool.bus(slot),
                    EngineOp::Crossfade,
                );
            }
        }
        let graph_epoch = self.graph_epoch;
        let topology_epoch = self.topology_epoch;
        let operation = match (
            job.configuration_needed,
            job.topology,
            job.graph,
            job.node_pairs,
        ) {
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
        self.quiet_until = quiet;
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
        core::mem::swap(&mut self.model, &mut self.next);
        self.model.settle();
        self.model.revision = revision;
        let created = (0..self.model.loops.len())
            .filter(|&slot| {
                let after = self.model.loops[slot].map(|l| l.id);
                after.is_some() && after != self.next.loops[slot].map(|l| l.id)
            })
            .count();
        let controls: usize = self
            .model
            .loops
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| {
                entry.filter(|entry| {
                    entry.program.is_none()
                        && self.next.loops[slot].map(|old| old.id) != Some(entry.id)
                })
            })
            .map(|entry| usize::from(entry.gain != 1.0) + usize::from(entry.balance != 0.0))
            .sum();
        if !self.room(3 * created + controls + self.next.strips.len() + 3) {
            core::mem::swap(&mut self.model, &mut self.next);
            return false;
        }
        self.snapshots.confirmed = (self.graph_epoch.0, self.topology_epoch.0);
        for slot in 0..self.model.loops.len() {
            if self.model.loops[slot].map(|l| l.id) != self.next.loops[slot].map(|l| l.id) {
                self.planned[slot] = [None; PLANNED];
                self.cells.loops[slot].peaks.clear();
                self.dirty[slot] = true;
            }
        }
        let outcome = if let Kind::Composite { target, .. } = job.kind {
            match (
                self.model.find_loop(target).and_then(|entry| entry.program),
                job.applied_frame,
            ) {
                (Some(program), Some(frame)) => Outcome::CompositeApplied {
                    version: program.version,
                    frame,
                },
                _ => Outcome::Refused(Refusal::Internal),
            }
        } else {
            Outcome::Done
        };
        self.answer_job(job, outcome);
        let at = self.at();
        for slot in 0..self.next.strips.len() {
            let Some(old) = self.next.strips[slot] else {
                continue;
            };
            if self.model.strip(old.id).and_then(|strip| strip.processor) == old.processor {
                continue;
            }
            if let Some(owner) = old.processor.and_then(|owner| owner.slot) {
                self.submit_job_op(
                    job,
                    at,
                    self.pool.processor(owner.index as usize),
                    EngineOp::ProcessorRetire {
                        generation: owner.generation,
                    },
                );
            }
        }
        for index in 0..self.next.buses.len() {
            let Some(bus) = self.next.buses[index] else {
                continue;
            };
            if self.model.bus(bus.id).and_then(|old| old.processor) == bus.processor {
                continue;
            }
            if let Some(owner) = bus.processor.and_then(|owner| owner.slot) {
                self.submit_job_op(
                    job,
                    at,
                    self.pool.processor(owner.index as usize),
                    EngineOp::ProcessorRetire {
                        generation: owner.generation,
                    },
                );
            }
        }
        for slot in 0..self.model.loops.len() {
            let Some(entry) = self.model.loops[slot] else {
                continue;
            };
            if self.next.loops[slot].map(|l| l.id) == Some(entry.id) {
                continue;
            }
            if entry.repeat_sync && !job.replacing() {
                self.submit_job_loop(
                    job,
                    at,
                    Operation::RepeatSync {
                        target: loop_id(entry.id),
                        enabled: true,
                    },
                    Step::Prelude,
                );
            }
            if entry.program.is_some() {
                continue;
            }
            let target = loop_id(entry.id);
            if entry.gain != 1.0 {
                self.submit_job_loop(
                    job,
                    at,
                    Operation::Gain {
                        target,
                        value: entry.gain,
                    },
                    Step::Prelude,
                );
            }
            if entry.balance != 0.0 {
                self.submit_job_loop(
                    job,
                    at,
                    Operation::Balance {
                        target,
                        value: entry.balance,
                    },
                    Step::Prelude,
                );
            }
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
        if let Some(media) = job.content_media {
            if !self.room(1) {
                return false;
            }
            job.content_media = None;
            self.submit_job_loop(job, Frame(0), Operation::Retire { media }, Step::Retire);
            return true;
        }
        if job.processors_prepared && !job.activated {
            if !self.room(self.next.strips.len() + self.next.buses.len()) {
                return false;
            }
            for index in 0..self.next.strips.len() {
                let Some(strip) = self.next.strips[index] else {
                    continue;
                };
                if self.model.strip(strip.id).and_then(|old| old.processor) == strip.processor {
                    continue;
                }
                if let Some(owner) = strip.processor.and_then(|owner| owner.slot) {
                    if self.processor_cells[owner.index as usize]
                        .read()
                        .is_some_and(|state| {
                            state.generation == owner.generation
                                && state.phase != crate::ProcessorPhase::Free
                                && state.phase != crate::ProcessorPhase::Retiring
                        })
                    {
                        self.submit_job_op(
                            job,
                            Frame(0),
                            self.pool.processor(owner.index as usize),
                            EngineOp::ProcessorRetire {
                                generation: owner.generation,
                            },
                        );
                    }
                }
            }
            for index in 0..self.next.buses.len() {
                let Some(bus) = self.next.buses[index] else {
                    continue;
                };
                if self.model.bus(bus.id).and_then(|old| old.processor) == bus.processor {
                    continue;
                }
                if let Some(owner) = bus.processor.and_then(|owner| owner.slot) {
                    if self.processor_cells[owner.index as usize]
                        .read()
                        .is_some_and(|state| {
                            state.generation == owner.generation
                                && !matches!(
                                    state.phase,
                                    crate::ProcessorPhase::Free | crate::ProcessorPhase::Retiring
                                )
                        })
                    {
                        self.submit_job_op(
                            job,
                            Frame(0),
                            self.pool.processor(owner.index as usize),
                            EngineOp::ProcessorRetire {
                                generation: owner.generation,
                            },
                        );
                    }
                }
            }
            job.processors_prepared = false;
            return true;
        }
        if job.configured && !job.activated {
            let count = self.configuration_nodes();
            if !self.room(count + 3) {
                return false;
            }
            let token = if job.replacing() {
                self.replacement.id.expect("live replacement")
            } else {
                u64::MAX
            };
            for slot in 0..self.next.strips.len() {
                if self.next.strips[slot]
                    .is_some_and(|strip| job.replacing() || self.model.strip(strip.id).is_none())
                {
                    for node in [self.pool.strip_input(slot), self.pool.strip_output(slot)] {
                        self.submit_job_op(
                            job,
                            Frame(0),
                            node,
                            EngineOp::DiscardConfiguration { token },
                        );
                    }
                }
            }
            for slot in 0..self.next.loops.len() {
                if self.next.loops[slot].is_some_and(|entry| {
                    job.replacing() || self.model.find_loop(entry.id).is_none()
                }) {
                    self.submit_job_op(
                        job,
                        Frame(0),
                        self.pool.fader(slot),
                        EngineOp::DiscardConfiguration { token },
                    );
                }
            }
            for slot in 0..self.next.buses.len() {
                if job.replacing() && self.next.buses[slot].is_some() {
                    self.submit_job_op(
                        job,
                        Frame(0),
                        self.pool.bus(slot),
                        EngineOp::DiscardConfiguration { token },
                    );
                }
            }
            job.configured = false;
        }
        if !self.room(3) {
            return false;
        }
        self.recycle(job);
        job.failure = None;
        job.phase = Phase::Finishing;
        true
    }

    pub(crate) fn job_result(&mut self, step: Step, result: RuntimeResult) {
        if step == Step::ContentAdopt {
            if let RuntimeResult::Execution(execution) = &result {
                if let Some(Reply::Adopted { .. }) = execution.operation_result {
                    if let rt_runtime::schedule::Action::Planner {
                        operation:
                            Operation::Adopt {
                                audio,
                                auxiliary,
                                midi,
                                ..
                            },
                        ..
                    } = execution.action
                    {
                        for source in [audio, auxiliary, midi].into_iter().flatten() {
                            self.uploads.consumed(source);
                        }
                    }
                }
            }
        }
        if step == Step::NodePairAppend {
            if let RuntimeResult::Execution(execution) = &result {
                if execution_outcome(execution) == Outcome::Done {
                    if let rt_runtime::schedule::Action::Planner {
                        operation:
                            Operation::NodePairAppend {
                                initial: Some(initial),
                                ..
                            },
                        ..
                    } = execution.action
                    {
                        for source in [initial.audio, initial.auxiliary, initial.midi]
                            .into_iter()
                            .flatten()
                        {
                            self.uploads.consumed(source);
                        }
                    }
                }
            }
        }
        let applied_frame = match &result {
            RuntimeResult::Execution(execution) if step == Step::Activation => {
                Some(execution.actual_frame.0)
            }
            _ => None,
        };
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
                if step == Step::Activation
                    && refusal.is_none()
                    && self.composite.description.is_some()
                {
                    self.composite.admitted = true;
                    self.model.revision += 1;
                    let slot = self.composite.description.unwrap().loop_id.0.index as usize;
                    self.dirty[slot] = true;
                }
                (refusal, None, refusal.is_some() || !schedule, graph)
            }
            RuntimeResult::Execution(execution) => {
                let refusal = match (step, execution_outcome(&execution)) {
                    (Step::Prelude | Step::Cleanup | Step::Retire | Step::ClearStop, _) => None,
                    (_, Outcome::Refused(refusal)) => Some(refusal),
                    (_, Outcome::Done) => None,
                    (
                        _,
                        Outcome::CompositeApplied { .. }
                        | Outcome::TimingApplied(_)
                        | Outcome::ContentApplied { .. }
                        | Outcome::ContentPrepared { .. }
                        | Outcome::Input(_)
                        | Outcome::InputCapacity(_),
                    ) => Some(Refusal::Internal),
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
                (Step::ContentAdopt, Some(Reply::Adopted { media, revision })) => {
                    job.content_media = Some(media);
                    job.content_revision = Some(revision);
                }
                (Step::NodePairBegin, Some(Reply::NodePairStatus { id, .. })) => {
                    job.node_pairs = Some(id)
                }
                (Step::TopologyBegin, Some(Reply::TopologyStatus { id, .. })) => {
                    job.topology = Some(id)
                }
                (Step::TopologyAppend, Some(Reply::TopologyStatus { .. })) => {
                    job.topology_callback = Some(self.cells.callbacks.load(Ordering::Relaxed));
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
                        }
                        | Reply::ProgramChanged {
                            graph_epoch,
                            topology_epoch,
                            ..
                        }
                        | Reply::ProgramGraphChanged {
                            graph_epoch,
                            topology_epoch,
                            ..
                        },
                    ),
                ) => {
                    job.activated = true;
                    job.applied_frame = applied_frame;
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
            if graph_epoch != self.graph_epoch {
                self.profile_layout.activate();
            }
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
