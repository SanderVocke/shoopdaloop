//! Control-side prepared plans, logical edit receipts and confirmed timing identities.
pub(crate) mod control;

use crate::{
    mirror::{Loop, LoopView},
    requests::Purpose,
    EngineClock, RtBackend,
};
use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::ToString,
    vec::Vec,
};
use anyhow::{anyhow, bail, Result};
use shoop_backend::{
    BackendActiveCompositeChild, BackendCompositeConfig, BackendCompositeId, BackendCompositeKind,
    BackendCompositeLimits, BackendCompositeOutcome, BackendCompositePending,
    BackendCompositeReceipt, BackendCompositeState, BackendCompositeTarget, BackendLoopId,
    BackendLoopMode, BackendMutationCompletion, BackendPreparedCompositePlan, BackendTrackTopology,
};
use shoop_rt_protocol::{
    CompositeCommand, CompositeDescription, CompositeKind, CompositeSpan, LoopMode, LoopSlotId,
    Outcome, Refusal, Request, Telemetry,
};
use shoop_rt_transport::carrier::Carrier;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Queued,
    Begin,
    Spans,
    Commit,
    Cancelling,
    Confirming,
    Finished,
}

pub(crate) struct Job {
    pub sequence: u64,
    pub id: BackendLoopId,
    pub version: u64,
    pub plan: BackendPreparedCompositePlan,
    phase: Phase,
    pub description: Option<CompositeDescription>,
    spans: Vec<CompositeSpan>,
    sent: usize,
    outstanding: usize,
    retry_after: Option<u64>,
    cancel: Option<BackendCompositeOutcome>,
    pub cancel_waiters: Vec<u64>,
    failure: Option<BackendCompositeOutcome>,
    applied: Option<u64>,
    telemetry: Option<Telemetry>,
    yielding: bool,
    cancel_sent: bool,
}

#[derive(Default)]
pub(crate) struct Composites {
    pub jobs: VecDeque<Job>,
    pub controls: BTreeMap<u64, control::Control>,
    pub next_stage: u64,
    pub next_version: u64,
    pub receipts: Vec<BackendCompositeReceipt>,
    pub removed: Vec<BackendCompositeId>,
    pub applied_frames: BTreeMap<BackendLoopId, u64>,
}

fn primitive(id: BackendCompositeId) -> BackendLoopId {
    BackendLoopId::from_raw(id.raw())
}
fn composite(id: BackendLoopId) -> BackendCompositeId {
    BackendCompositeId::from_raw(id.raw())
}
fn target_id(target: BackendCompositeTarget) -> BackendLoopId {
    match target {
        BackendCompositeTarget::Loop(id) => id,
        BackendCompositeTarget::Composite(id) => primitive(id),
    }
}
pub(crate) fn wire_mode(mode: BackendLoopMode) -> Result<LoopMode> {
    Ok(match mode {
        BackendLoopMode::Stopped => LoopMode::Stopped,
        BackendLoopMode::Playing => LoopMode::Playing,
        BackendLoopMode::Recording => LoopMode::Recording,
        _ => bail!("composite event mode is unavailable on this engine"),
    })
}
fn app_mode(mode: LoopMode) -> BackendLoopMode {
    match mode {
        LoopMode::Stopped => BackendLoopMode::Stopped,
        LoopMode::Playing => BackendLoopMode::Playing,
        LoopMode::Recording => BackendLoopMode::Recording,
        LoopMode::Replacing => BackendLoopMode::Replacing,
        LoopMode::PlayingDryThroughWet => BackendLoopMode::PlayingDryThroughWet,
        LoopMode::RecordingDryIntoWet => BackendLoopMode::RecordingDryIntoWet,
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn composite_limits_core(&self) -> Option<BackendCompositeLimits> {
        self.limits.map(|l| BackendCompositeLimits {
            entries: l.composite_entries,
            segments: l.composite_segments,
            relations: l.timing_relations,
            quantum: l.quantum,
        })
    }

    /// Creates a timing slot for negotiated prepared-plan clients.
    pub fn create_prepared_composite(&mut self) -> Result<BackendCompositeId> {
        self.ready()?;
        let slot = LoopSlotId(
            self.mirror
                .loop_slots
                .reserve()
                .ok_or_else(|| anyhow!("prepared loop capacity exhausted"))?,
        );
        let id = self.mirror.next_loop_id();
        let description = CompositeDescription {
            id: 0,
            loop_id: slot,
            sync: None,
            previous: None,
            expected_version: 0,
            version: 0,
            kind: CompositeKind::Regular,
            unit_frames: u64::from(self.limits().quantum),
            iterations: 0,
            entries: 0,
            segments: 0,
            sample_rate: self.driver.map_or(0, |d| d.sample_rate),
            quantum: self.limits().quantum,
        };
        self.mirror.loops.insert(
            id,
            Loop {
                recorded: None,
                timing: Default::default(),
                slot,
                program: Some(description),
                program_spans: Vec::new(),
                composite: None,
                repeat_sync: false,
                track: None,
                confirmed: false,
                sync: None,
                gain: 1.0,
                balance: 0.0,
                view: LoopView::default(),
                meter: Default::default(),
                content: 0,
                rt_content_revision: 0,
            },
        );
        self.mirror.slot_loops.insert(slot, id);
        self.requests.push(
            Request::CreateComposite {
                loop_id: slot,
                strip: None,
            },
            Purpose::CreateLoop(id),
        );
        Ok(composite(id))
    }

    pub(crate) fn validate_prepared(
        &self,
        id: BackendLoopId,
        plan: &BackendPreparedCompositePlan,
    ) -> Result<()> {
        let entry = self
            .mirror
            .live_loop(id)
            .ok_or_else(|| anyhow!("unknown composite target"))?;
        for target in
            core::iter::once(id).chain(plan.spans.iter().map(|span| target_id(span.target)))
        {
            if self
                .mirror
                .loops
                .get(&target)
                .and_then(|entry| entry.track)
                .is_some_and(|owner| {
                    !matches!(
                        self.mirror.tracks[&owner].topology,
                        BackendTrackTopology::Direct { .. }
                    )
                })
            {
                bail!("composites currently require Direct tracks");
            }
        }
        if entry.program.is_none()
            && (!entry.confirmed
                || entry.view.mode != LoopMode::Stopped
                || entry.view.length != 0
                || entry.view.next_mode.is_some())
        {
            bail!("only an empty stopped primitive can become a composite");
        }
        let limits = self.limits();
        if plan.unit_frames == 0
            || plan.unit_frames % u64::from(limits.quantum) != 0
            || plan.unit_frames.checked_mul(plan.iterations).is_none()
            || plan.entries > limits.composite_entries
            || plan.spans.len() > limits.composite_segments as usize
            || (plan.iterations == 0) != plan.spans.is_empty()
            || plan.spans.iter().map(|span| span.end).max().unwrap_or(0) != plan.iterations
            || plan.spans.is_empty()
                && (plan.entries != 0 || plan.kind != BackendCompositeKind::Regular)
        {
            bail!("invalid or oversized prepared composite timing");
        }
        let source = plan
            .sync_source
            .map(|source| {
                self.mirror
                    .live_loop(source)
                    .ok_or_else(|| anyhow!("unknown synchronization source"))
            })
            .transpose()?;
        if plan.sync_source == Some(id) {
            bail!("composite cannot synchronize to itself");
        }
        let mut previous: BTreeMap<LoopSlotId, (u64, LoopMode)> = BTreeMap::new();
        let mut edges: BTreeMap<BackendLoopId, BTreeSet<BackendLoopId>> = BTreeMap::new();
        let mut budgets: BTreeMap<BackendLoopId, (u32, usize)> = self
            .mirror
            .loops
            .iter()
            .filter_map(|(key, e)| {
                e.program
                    .map(|p| (*key, (p.entries, e.program_spans.len())))
            })
            .collect();
        for (&parent, e) in &self.mirror.loops {
            if parent == id {
                continue;
            }
            if let Some(sync) = e.sync {
                edges.entry(sync).or_default().insert(parent);
            }
            for span in &e.program_spans {
                if let Some(child) = self.mirror.slot_loops.get(&span.target) {
                    edges.entry(parent).or_default().insert(*child);
                }
            }
        }
        // Reserve every accepted candidate's positive growth until its terminal result.
        for job in &self.composites.jobs {
            let budget = budgets.entry(job.id).or_default();
            budget.0 = budget.0.max(job.plan.entries);
            budget.1 = budget.1.max(job.plan.spans.len());
            if job.id != id {
                if let Some(sync) = job.plan.sync_source {
                    edges.entry(sync).or_default().insert(job.id);
                }
                for span in &job.plan.spans {
                    edges
                        .entry(job.id)
                        .or_default()
                        .insert(target_id(span.target));
                }
            }
        }
        let budget = budgets.entry(id).or_default();
        budget.0 = budget.0.max(plan.entries);
        budget.1 = budget.1.max(plan.spans.len());
        if budgets.values().map(|b| u64::from(b.0)).sum::<u64>()
            > u64::from(limits.composite_entries)
            || budgets.values().map(|b| b.1).sum::<usize>() > limits.composite_segments as usize
        {
            bail!("aggregate composite capacity exhausted");
        }
        if source.is_some() {
            edges
                .entry(plan.sync_source.unwrap())
                .or_default()
                .insert(id);
        }
        for span in &plan.spans {
            let child_id = target_id(span.target);
            let child = self
                .mirror
                .live_loop(child_id)
                .ok_or_else(|| anyhow!("unknown composite child"))?;
            let is_composite = child.program.is_some()
                || self.composites.jobs.iter().any(|job| job.id == child_id);
            if matches!(span.target, BackendCompositeTarget::Composite(_)) != is_composite
                || child_id == id
                || span.start >= span.end
                || span.end > plan.iterations
                || span.occurrence_start != span.start
                || span.ordinal >= plan.entries
            {
                bail!("invalid composite occurrence");
            }
            let mode = wire_mode(span.mode)?;
            if plan.kind == BackendCompositeKind::Regular && mode != LoopMode::Playing
                || is_composite && mode == LoopMode::Recording
            {
                bail!("invalid composite child mode");
            }
            if let Some((end, old_mode)) = previous.insert(child.slot, (span.end, mode)) {
                if end > span.start || end == span.start && mode == old_mode {
                    bail!("prepared spans must be normalized and ordered per target");
                }
            }
            edges.entry(id).or_default().insert(child_id);
        }
        let relation_count: usize = edges.values().map(BTreeSet::len).sum();
        if relation_count > limits.timing_relations as usize {
            bail!("prepared timing relation capacity exhausted");
        }
        fn visit(
            id: BackendLoopId,
            edges: &BTreeMap<BackendLoopId, BTreeSet<BackendLoopId>>,
            visiting: &mut BTreeSet<BackendLoopId>,
            visited: &mut BTreeSet<BackendLoopId>,
        ) -> bool {
            if visited.contains(&id) {
                return true;
            }
            if !visiting.insert(id) {
                return false;
            }
            if edges.get(&id).is_some_and(|children| {
                children
                    .iter()
                    .any(|child| !visit(*child, edges, visiting, visited))
            }) {
                return false;
            }
            visiting.remove(&id);
            visited.insert(id);
            true
        }
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for node in edges.keys() {
            if !visit(*node, &edges, &mut visiting, &mut visited) {
                bail!("composite action/synchronization cycle");
            }
        }
        if self.composites.next_version == u64::MAX || self.composites.next_stage == u64::MAX {
            bail!("composite version space exhausted");
        }
        Ok(())
    }

    pub(crate) fn queue_composite(
        &mut self,
        id: BackendLoopId,
        plan: &BackendPreparedCompositePlan,
    ) -> Result<u64> {
        self.ready()?;
        self.validate_prepared(id, plan)?;
        self.composites.next_version += 1;
        let version = self.composites.next_version;
        for job in &mut self.composites.jobs {
            if job.id == id {
                job.cancel
                    .get_or_insert(BackendCompositeOutcome::Superseded);
            }
        }
        let sequence = self.requests.reserve_mutation();
        tracing::debug!(
            sequence,
            version,
            composite = id.raw(),
            entries = plan.entries,
            segments = plan.spans.len(),
            "backend.rt.composite_admitted"
        );
        self.composites.jobs.push_back(Job {
            sequence,
            id,
            version,
            plan: plan.clone(),
            phase: Phase::Queued,
            description: None,
            spans: Vec::new(),
            sent: 0,
            outstanding: 0,
            retry_after: None,
            cancel: None,
            cancel_waiters: Vec::new(),
            failure: None,
            applied: None,
            telemetry: None,
            yielding: false,
            cancel_sent: false,
        });
        Ok(version)
    }

    pub(crate) fn convert_composite(&mut self, id: BackendLoopId) -> Result<BackendCompositeId> {
        let plan = BackendPreparedCompositePlan {
            kind: BackendCompositeKind::Regular,
            sync_source: self.mirror.live_loop(id).and_then(|e| e.sync),
            unit_frames: u64::from(self.limits().quantum),
            iterations: 0,
            entries: 0,
            spans: Vec::new(),
        };
        self.queue_composite(id, &plan)?;
        Ok(composite(id))
    }

    pub(crate) fn legacy_composite_plan(
        &self,
        id: BackendCompositeId,
        config: &BackendCompositeConfig,
    ) -> Result<BackendPreparedCompositePlan> {
        let id = primitive(id);
        let entry = self
            .mirror
            .live_loop(id)
            .ok_or_else(|| anyhow!("unknown composite"))?;
        let unit = self
            .composites
            .jobs
            .iter()
            .rev()
            .find(|job| job.id == id && job.cancel.is_none())
            .map(|job| job.plan.unit_frames)
            .or_else(|| {
                entry
                    .program
                    .filter(|p| p.version != 0)
                    .map(|p| p.unit_frames)
            })
            .or_else(|| {
                self.mirror
                    .live_loop(config.sync_source)
                    .map(|entry| entry.view.length)
                    .filter(|frames| *frames > 0)
            })
            .unwrap_or(u64::from(self.limits().quantum));
        let mut entries = Vec::new();
        for timeline in &config.timelines {
            let mut base = 0u64;
            for group in timeline {
                let mut end = base;
                for event in group {
                    let child_id = target_id(event.target);
                    let child = self
                        .mirror
                        .live_loop(child_id)
                        .ok_or_else(|| anyhow!("unknown composite child"))?;
                    let delay = u64::try_from(event.delay)
                        .map_err(|_| anyhow!("negative composite delay"))?;
                    let duration = event
                        .n_cycles
                        .map(u64::try_from)
                        .transpose()
                        .map_err(|_| anyhow!("negative composite duration"))?;
                    let mode = match config.kind {
                        BackendCompositeKind::Regular => {
                            if event.mode.is_some() {
                                bail!("regular composite events inherit playing mode");
                            }
                            None
                        }
                        BackendCompositeKind::Script => Some(
                            match wire_mode(
                                event
                                    .mode
                                    .ok_or_else(|| anyhow!("script event requires a mode"))?,
                            )? {
                                LoopMode::Replacing => {
                                    bail!("composite replacing events are not supported")
                                }
                                LoopMode::Stopped => shoop_loop_plan::Mode::Stopped,
                                LoopMode::Playing => shoop_loop_plan::Mode::Playing,
                                LoopMode::Recording => shoop_loop_plan::Mode::Recording,
                                LoopMode::PlayingDryThroughWet => {
                                    shoop_loop_plan::Mode::PlayingDryThroughWet
                                }
                                LoopMode::RecordingDryIntoWet => {
                                    shoop_loop_plan::Mode::RecordingDryIntoWet
                                }
                            },
                        ),
                    };
                    let start = base
                        .checked_add(delay)
                        .ok_or_else(|| anyhow!("composite position overflow"))?;
                    let length =
                        duration.unwrap_or_else(|| child.view.length.div_ceil(unit).max(1));
                    end = end.max(
                        start
                            .checked_add(length)
                            .ok_or_else(|| anyhow!("composite duration overflow"))?,
                    );
                    entries.push(shoop_loop_plan::Entry {
                        target: child_id,
                        delay: start,
                        duration,
                        repetitions: 1,
                        mode,
                    });
                }
                base = end;
            }
        }
        let limits = shoop_loop_plan::Limits {
            program_entries: self.limits().composite_entries as usize,
            program_segments: self.limits().composite_segments as usize,
        };
        let count = u32::try_from(entries.len())?;
        let description = shoop_loop_plan::Description {
            source: id,
            nominal_unit_frames: unit,
            sections: alloc::vec![shoop_loop_plan::Section { entries }],
        };
        let compiled =
            shoop_loop_plan::compile::<_, shoop_loop_plan::Error>(&description, limits, |target| {
                self.mirror
                    .live_loop(target)
                    .map(|e| e.view.length)
                    .ok_or(shoop_loop_plan::Error::MissingTarget)
            })
            .map_err(|e| anyhow!("invalid composite program: {e:?}"))?;
        let spans = compiled
            .targets()
            .iter()
            .flat_map(|target| {
                target
                    .segments
                    .iter()
                    .map(|span| shoop_backend::BackendCompositeSpan {
                        target: if self.mirror.loops[&target.target].program.is_some() {
                            BackendCompositeTarget::Composite(composite(target.target))
                        } else {
                            BackendCompositeTarget::Loop(target.target)
                        },
                        start: span.start,
                        end: span.end,
                        occurrence_start: span.occurrence_start,
                        ordinal: span.ordinal,
                        mode: match span.mode {
                            shoop_loop_plan::Mode::Replacing => BackendLoopMode::Replacing,
                            shoop_loop_plan::Mode::Stopped => BackendLoopMode::Stopped,
                            shoop_loop_plan::Mode::Playing => BackendLoopMode::Playing,
                            shoop_loop_plan::Mode::Recording => BackendLoopMode::Recording,
                            shoop_loop_plan::Mode::PlayingDryThroughWet => {
                                BackendLoopMode::PlayingDryThroughWet
                            }
                            shoop_loop_plan::Mode::RecordingDryIntoWet => {
                                BackendLoopMode::RecordingDryIntoWet
                            }
                        },
                    })
            })
            .collect();
        Ok(BackendPreparedCompositePlan {
            kind: if count == 0 {
                BackendCompositeKind::Regular
            } else {
                config.kind
            },
            sync_source: Some(config.sync_source),
            unit_frames: unit,
            iterations: compiled.iterations,
            entries: count,
            spans,
        })
    }

    pub(crate) fn cancel_composite(&mut self, id: BackendCompositeId, version: u64) -> Result<()> {
        self.ready()?;
        let job = self
            .composites
            .jobs
            .iter_mut()
            .find(|job| job.id == primitive(id) && job.version == version)
            .ok_or_else(|| anyhow!("unknown pending composite version"))?;
        let sequence = self.requests.reserve_mutation();
        job.cancel.get_or_insert(BackendCompositeOutcome::Cancelled);
        job.cancel_waiters.push(sequence);
        Ok(())
    }

    pub(crate) fn advance_composites(&mut self) {
        if !self.is_ready() {
            return;
        }
        self.advance_composite_controls();
        let Some(mut job) = self.composites.jobs.pop_front() else {
            return;
        };
        if self
            .input
            .as_ref()
            .is_some_and(|input| input.sequence < job.sequence)
        {
            self.composites.jobs.push_front(job);
            return;
        }
        let mut terminal = None;
        if job.phase == Phase::Finished && job.outstanding == 0 {
            terminal = job.failure.take();
        } else if job.phase == Phase::Confirming
            && job.applied.is_some()
            && job.telemetry.is_some()
            && job.outstanding == 0
        {
            terminal = Some(self.install_composite(&mut job));
        } else if job.outstanding == 0
            && job
                .retry_after
                .is_none_or(|callback| self.status.callbacks > callback)
        {
            job.retry_after = None;
            if job.cancel.is_some() && job.description.is_none() {
                terminal = job.cancel.take();
            } else if (job.cancel.is_some() || job.failure.is_some())
                && job.phase != Phase::Confirming
            {
                let d = job.description.unwrap();
                job.phase = Phase::Cancelling;
                self.composite_send(
                    &mut job,
                    Request::Composite(CompositeCommand::Cancel { id: d.id }),
                );
            } else {
                match job.phase {
                    Phase::Queued => match self.prepare_composite_job(&mut job) {
                        Ok(true) => {}
                        Ok(false) => {}
                        Err(error) => {
                            terminal = Some(BackendCompositeOutcome::Refused(error.to_string()))
                        }
                    },
                    Phase::Begin => {
                        let d = job.description.unwrap();
                        self.composite_send(
                            &mut job,
                            Request::Composite(CompositeCommand::Begin(d)),
                        );
                    }
                    Phase::Spans => {
                        if job.sent == job.spans.len() {
                            job.phase = Phase::Commit;
                        } else {
                            for _ in 0..32 {
                                if job.sent == job.spans.len() {
                                    break;
                                }
                                let index = job.sent;
                                let span = job.spans[index];
                                let id = job.description.unwrap().id;
                                self.composite_send(
                                    &mut job,
                                    Request::Composite(CompositeCommand::Span {
                                        id,
                                        index: index as u32,
                                        span,
                                    }),
                                );
                                job.sent += 1;
                            }
                        }
                    }
                    Phase::Commit => {
                        let id = job.description.unwrap().id;
                        self.composite_send(
                            &mut job,
                            Request::Composite(CompositeCommand::Commit { id }),
                        );
                    }
                    Phase::Cancelling | Phase::Confirming | Phase::Finished => {}
                }
            }
        }
        let structural_waiting = self
            .requests
            .outgoing
            .iter()
            .chain(self.requests.in_flight.values())
            .any(|pending| {
                !matches!(pending.purpose, Purpose::Composite(_))
                    && matches!(
                        pending.request,
                        Request::ResetComposite { .. }
                            | Request::CreateStrip { .. }
                            | Request::RemoveStrip { .. }
                            | Request::CreateLoop { .. }
                            | Request::CreateComposite { .. }
                            | Request::RemoveLoop { .. }
                            | Request::LoopSync { .. }
                            | Request::CreateBus { .. }
                            | shoop_rt_protocol::Request::CreateProcessedBus { .. }
                            | shoop_rt_protocol::Request::SetBusProcessor { .. }
                            | shoop_rt_protocol::Request::BusProcessorControl { .. }
                            | Request::RemoveBus { .. }
                            | Request::Route { .. }
                    )
            });
        if job.phase == Phase::Commit
            && job.outstanding == 1
            && !job.cancel_sent
            && (job.cancel.is_some() || structural_waiting)
        {
            job.yielding = job.cancel.is_none();
            job.cancel_sent = true;
            let id = job.description.unwrap().id;
            self.composite_send(
                &mut job,
                Request::Composite(CompositeCommand::Cancel { id }),
            );
        }
        if let Some(outcome) = terminal {
            self.finish_composite(job, outcome);
        } else {
            self.composites.jobs.push_front(job);
        }
    }

    fn prepare_composite_job(&mut self, job: &mut Job) -> Result<bool> {
        let entry = self
            .mirror
            .live_loop(job.id)
            .ok_or_else(|| anyhow!("composite was removed"))?;
        if !entry.confirmed {
            return Ok(false);
        }
        let old = entry.slot;
        let previous = entry.program.is_none().then_some(old);
        let expected_version = entry.program.map_or(0, |p| p.version);
        for span in &job.plan.spans {
            let child = self
                .mirror
                .live_loop(target_id(span.target))
                .ok_or_else(|| anyhow!("composite child was removed"))?;
            if !child.confirmed {
                return Ok(false);
            }
            if matches!(span.target, BackendCompositeTarget::Composite(_))
                != child.program.is_some()
            {
                bail!("composite child kind changed before staging");
            }
        }
        let slot = if previous.is_some() {
            LoopSlotId(
                self.mirror
                    .loop_slots
                    .bump_generation(old.0)
                    .ok_or_else(|| anyhow!("loop generation space exhausted"))?,
            )
        } else {
            old
        };
        job.spans = job
            .plan
            .spans
            .iter()
            .map(|span| {
                Ok(CompositeSpan {
                    target: self
                        .mirror
                        .live_loop(target_id(span.target))
                        .ok_or_else(|| anyhow!("composite child was removed"))?
                        .slot,
                    start: span.start,
                    end: span.end,
                    occurrence_start: span.occurrence_start,
                    ordinal: span.ordinal,
                    mode: wire_mode(span.mode)?,
                })
            })
            .collect::<Result<_>>()?;
        job.spans.sort_by_key(|span| (span.target, span.start));
        self.composites.next_stage = self
            .composites
            .next_stage
            .checked_add(1)
            .ok_or_else(|| anyhow!("composite stage space exhausted"))?;
        job.description = Some(CompositeDescription {
            id: self.composites.next_stage,
            loop_id: slot,
            sync: job
                .plan
                .sync_source
                .map(|id| {
                    self.mirror
                        .live_loop(id)
                        .map(|entry| entry.slot)
                        .ok_or_else(|| anyhow!("sync source was removed"))
                })
                .transpose()?,
            previous,
            expected_version,
            version: job.version,
            kind: match job.plan.kind {
                BackendCompositeKind::Regular => CompositeKind::Regular,
                BackendCompositeKind::Script => CompositeKind::Explicit,
            },
            unit_frames: job.plan.unit_frames,
            iterations: job.plan.iterations,
            entries: job.plan.entries,
            segments: u32::try_from(job.spans.len())?,
            sample_rate: self.driver.map_or(0, |d| d.sample_rate),
            quantum: self.limits().quantum,
        });
        job.phase = Phase::Begin;
        Ok(true)
    }

    fn composite_send(&mut self, job: &mut Job, request: Request) {
        self.requests
            .push_internal(request, Purpose::Composite(job.sequence));
        job.outstanding += 1;
    }

    pub(crate) fn composite_answer(&mut self, sequence: u64, request: Request, outcome: Outcome) {
        let Some(mut job) = self.composites.jobs.pop_front() else {
            return;
        };
        if job.sequence != sequence {
            self.composites.jobs.push_front(job);
            return;
        }
        job.outstanding = job.outstanding.saturating_sub(1);
        let mut terminal = None;
        match (request, outcome) {
            (Request::LoopSync { .. }, Outcome::Done) => {
                if let Some(entry) = self.mirror.loops.get_mut(&job.id) {
                    entry.sync = job.plan.sync_source;
                }
                job.phase = Phase::Begin;
            }
            (Request::Composite(CompositeCommand::Begin(_)), Outcome::Done) => {
                job.phase = Phase::Spans
            }
            (Request::Composite(CompositeCommand::Span { .. }), Outcome::Done) => {}
            (
                Request::Composite(CompositeCommand::Commit { .. }),
                Outcome::CompositeApplied { version, frame },
            ) if version == job.version => {
                job.applied = Some(frame);
                job.phase = Phase::Confirming;
            }
            (
                Request::Composite(CompositeCommand::Cancel { .. }),
                Outcome::Done | Outcome::Refused(Refusal::InvalidId),
            ) => {
                if job.phase == Phase::Cancelling {
                    terminal = job.failure.take().or_else(|| job.cancel.take());
                }
            }
            (
                Request::Composite(CompositeCommand::Cancel { .. }),
                Outcome::Refused(Refusal::Busy),
            ) => {}
            (Request::Composite(CompositeCommand::Commit { .. }), Outcome::Refused(_))
                if job.yielding =>
            {
                job.phase = Phase::Finished;
                job.failure = Some(BackendCompositeOutcome::Cancelled);
            }
            (
                Request::Composite(CompositeCommand::Begin(_) | CompositeCommand::Commit { .. }),
                Outcome::Refused(Refusal::Busy | Refusal::Overloaded),
            ) if job.cancel.is_none() => {
                job.retry_after = Some(self.status.callbacks);
            }
            (Request::Composite(CompositeCommand::Commit { .. }), Outcome::Refused(_))
                if job.cancel.is_some() =>
            {
                terminal = job.cancel.take()
            }
            (Request::Composite(CompositeCommand::Begin(_)), Outcome::Refused(_))
                if job.cancel.is_some() =>
            {
                terminal = job.cancel.take();
            }
            (Request::Composite(CompositeCommand::Begin(_)), Outcome::Refused(reason)) => {
                terminal = Some(BackendCompositeOutcome::Refused(
                    crate::incoming::refusal_text(reason).to_string(),
                ))
            }
            (_, Outcome::Refused(reason)) => {
                job.failure = Some(BackendCompositeOutcome::Refused(
                    crate::incoming::refusal_text(reason).to_string(),
                ));
            }
            _ => {
                job.failure = Some(BackendCompositeOutcome::Refused(
                    "unexpected composite receipt".to_string(),
                ));
            }
        }
        if job.yielding
            && job.phase == Phase::Finished
            && job.outstanding == 0
            && job.cancel.is_none()
        {
            job.phase = Phase::Queued;
            job.description = None;
            job.spans.clear();
            job.sent = 0;
            job.failure = None;
            job.cancel_sent = false;
            job.yielding = false;
            job.retry_after = Some(self.status.callbacks);
            terminal = None;
        }
        if job.phase == Phase::Confirming && job.telemetry.is_some() && job.outstanding == 0 {
            terminal = Some(self.install_composite(&mut job));
        }
        if let Some(outcome) = terminal {
            if job.outstanding == 0 {
                self.finish_composite(job, outcome);
            } else {
                job.phase = Phase::Finished;
                job.failure = Some(outcome);
                self.composites.jobs.push_front(job);
            }
        } else {
            self.composites.jobs.push_front(job);
        }
    }

    pub(crate) fn composite_telemetry(&mut self, record: Telemetry) {
        let Telemetry::Composite {
            loop_id,
            active_version,
            ..
        } = record
        else {
            return;
        };
        if let Some(job) = self.composites.jobs.front_mut().filter(|job| {
            job.version == active_version && job.description.is_some_and(|d| d.loop_id == loop_id)
        }) {
            job.telemetry = Some(record);
        }
        if let Some(entry) = self
            .mirror
            .slot_loops
            .get(&loop_id)
            .and_then(|id| self.mirror.loops.get_mut(id))
            .filter(|e| e.program.is_some_and(|p| p.version == active_version))
        {
            Self::apply_composite_telemetry(entry, record);
        }
    }

    fn apply_composite_telemetry(entry: &mut Loop, record: Telemetry) {
        if let Telemetry::Composite {
            mode,
            position,
            length,
            ..
        } = record
        {
            entry.composite = Some(record);
            entry.view.mode = mode;
            entry.view.position = position;
            entry.view.length = length;
        }
    }

    fn install_composite(&mut self, job: &mut Job) -> BackendCompositeOutcome {
        let description = job.description.unwrap();
        let frame = job.applied.unwrap();
        let Some(entry) = self.mirror.loops.get_mut(&job.id) else {
            return BackendCompositeOutcome::Refused(
                "composite disappeared before confirmation".to_string(),
            );
        };
        let old = entry.slot;
        entry.slot = description.loop_id;
        if old != entry.slot {
            entry.timing = Default::default();
            entry.rt_content_revision = 0;
            entry.recorded = None;
        }
        entry.program = Some(description);
        entry.program_spans = job.spans.clone();
        entry.sync = job.plan.sync_source;
        Self::apply_composite_telemetry(entry, job.telemetry.unwrap());
        self.mirror.slot_loops.remove(&old);
        self.mirror.slot_loops.insert(description.loop_id, job.id);
        self.composites.applied_frames.insert(job.id, frame);
        self.reads.forget(job.id);
        self.canonical_content.remove(&job.id);
        self.canonical_imports.remove(&job.id);
        self.prepared_content.remove(&job.id);
        BackendCompositeOutcome::Applied { frame }
    }

    fn finish_composite(&mut self, job: Job, outcome: BackendCompositeOutcome) {
        tracing::debug!(
            sequence = job.sequence,
            version = job.version,
            composite = job.id.raw(),
            ?outcome,
            "backend.rt.composite_terminal"
        );
        let result = match &outcome {
            BackendCompositeOutcome::Applied { .. } => Ok(()),
            other => Err(alloc::format!(
                "composite version {}: {other:?}",
                job.version
            )),
        };
        self.mutation_completions.push(BackendMutationCompletion {
            sequence: job.sequence,
            result,
        });
        let cancel_result = match &outcome {
            BackendCompositeOutcome::Applied { .. } => {
                Err("the composite version applied before cancellation".to_string())
            }
            BackendCompositeOutcome::ConnectionLost => Err(
                "the composite cancellation outcome is unknown after connection loss".to_string(),
            ),
            _ => Ok(()),
        };
        for sequence in &job.cancel_waiters {
            self.mutation_completions.push(BackendMutationCompletion {
                sequence: *sequence,
                result: cancel_result.clone(),
            });
        }
        self.composites.receipts.push(BackendCompositeReceipt {
            sequence: job.sequence,
            composite: composite(job.id),
            version: job.version,
            outcome,
        });
    }

    pub(crate) fn disconnect_composites(&mut self) {
        self.disconnect_composite_controls();
        while let Some(job) = self.composites.jobs.pop_front() {
            self.finish_composite(job, BackendCompositeOutcome::ConnectionLost);
        }
    }

    fn composite_pending_stage(&self, job: &Job) -> BackendCompositePending {
        if job.cancel.is_some() {
            return BackendCompositePending::Cancelling;
        }
        match job.phase {
            Phase::Queued => BackendCompositePending::Queued,
            Phase::Confirming => BackendCompositePending::Confirming,
            Phase::Commit if self.mirror.loops.get(&job.id).is_some_and(|entry| matches!(entry.composite, Some(Telemetry::Composite { pending_version: Some(version), pending_admitted: true, .. }) if version == job.version)) => BackendCompositePending::AwaitingRestart,
            _ => BackendCompositePending::Staging,
        }
    }

    pub(crate) fn composite_operations(
        &self,
    ) -> Vec<shoop_backend::BackendCompositeOperationState> {
        self.composites
            .jobs
            .iter()
            .map(|job| shoop_backend::BackendCompositeOperationState {
                composite: composite(job.id),
                version: job.version,
                stage: self.composite_pending_stage(job),
            })
            .collect()
    }

    pub(crate) fn composite_states(&self) -> BTreeMap<BackendCompositeId, BackendCompositeState> {
        self.mirror
            .loops
            .iter()
            .filter_map(|(&id, entry)| {
                let program = entry.program.filter(|_| entry.confirmed)?;
                let (iteration, passes, waiting_repeat) = match entry.composite {
                    Some(Telemetry::Composite {
                        iteration,
                        passes,
                        waiting_repeat,
                        ..
                    }) => (iteration, passes, waiting_repeat),
                    _ => (0, 0, false),
                };
                let pending = self
                    .composites
                    .jobs
                    .iter()
                    .rev()
                    .find(|job| job.id == id && job.cancel.is_none());
                let active_children = if entry.view.mode == LoopMode::Playing && !waiting_repeat {
                    entry
                        .program_spans
                        .iter()
                        .filter(|s| s.start <= iteration && iteration < s.end)
                        .filter_map(|s| {
                            self.mirror.slot_loops.get(&s.target).map(|child| {
                                BackendActiveCompositeChild {
                                    target: if self.mirror.loops[child].program.is_some() {
                                        BackendCompositeTarget::Composite(composite(*child))
                                    } else {
                                        BackendCompositeTarget::Loop(*child)
                                    },
                                    mode: app_mode(s.mode),
                                    cycle_offset: u32::try_from(iteration - s.occurrence_start)
                                        .unwrap_or(u32::MAX),
                                }
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                Some((
                    composite(id),
                    BackendCompositeState {
                        observed_frame: match entry.composite {
                            Some(Telemetry::Composite { frame, .. }) => frame,
                            _ => 0,
                        },
                        sync_source: entry.sync,
                        repeat_sync: entry.repeat_sync,
                        prepared_unit_frames: program.unit_frames,
                        waiting_repeat,
                        pending_stage: pending.map(|job| self.composite_pending_stage(job)),
                        active_applied_frame: self.composites.applied_frames.get(&id).copied(),
                        mode: app_mode(entry.view.mode),
                        next_mode: entry.view.next_mode.map(app_mode),
                        next_transition_delay: entry.view.next_delay,
                        iteration: u32::try_from(iteration).unwrap_or(u32::MAX),
                        cycle_count: passes,
                        length: entry.view.length,
                        position: entry.view.position,
                        active_plan_version: program.version,
                        pending_plan_version: pending.map(|job| job.version),
                        active_children,
                    },
                ))
            })
            .collect()
    }

    pub(crate) fn transition_composite(
        &mut self,
        id: BackendCompositeId,
        mode: BackendLoopMode,
        delay: Option<u32>,
        align: Option<i64>,
    ) -> Result<()> {
        self.ready()?;
        let id = primitive(id);
        let entry = self
            .mirror
            .live_loop(id)
            .filter(|entry| entry.program.is_some() || self.pending_conversion(id))
            .ok_or_else(|| anyhow!("unknown composite"))?;
        let mode = wire_mode(mode)?;
        if mode == LoopMode::Recording {
            bail!("a composite's outer mode is Play or Stop");
        }
        if let Some(iteration) = align {
            let iteration =
                u64::try_from(iteration).map_err(|_| anyhow!("negative composite iteration"))?;
            let iterations = entry
                .program
                .map(|program| program.iterations)
                .or_else(|| {
                    self.composites
                        .jobs
                        .iter()
                        .rev()
                        .find(|job| job.id == id)
                        .map(|job| job.plan.iterations)
                })
                .ok_or_else(|| anyhow!("composite has no accepted plan"))?;
            if iteration >= iterations && !(iteration == 0 && iterations == 0) {
                bail!("composite iteration is outside the accepted arrangement");
            }
        }
        if self.pending_kind_change(id) {
            let iteration = align
                .map(u64::try_from)
                .transpose()
                .map_err(|_| anyhow!("negative composite iteration"))?;
            if iteration.is_some() && (delay.is_some() || mode != LoopMode::Playing) {
                bail!("iteration alignment requires immediate Play");
            }
            self.defer_composite_control(
                id,
                control::Action::Transport {
                    mode,
                    delay,
                    iteration,
                },
                true,
            );
            return Ok(());
        }
        let request = if let Some(iteration) = align {
            if delay.is_some() || mode != LoopMode::Playing {
                bail!("iteration alignment requires immediate Play");
            }
            let iteration =
                u64::try_from(iteration).map_err(|_| anyhow!("negative composite iteration"))?;
            Request::LoopIteration {
                loop_id: entry.slot,
                iteration,
                retrigger: false,
                seek: false,
            }
        } else {
            Request::LoopTransition {
                loop_id: entry.slot,
                mode,
                delay,
            }
        };
        self.requests.push(request, Purpose::Transition(id));
        Ok(())
    }

    pub(crate) fn repeat_sync_core(&mut self, id: BackendLoopId, enabled: bool) -> Result<()> {
        self.ready()?;
        let slot = self
            .mirror
            .live_loop(id)
            .ok_or_else(|| anyhow!("unknown loop"))?
            .slot;
        if self.pending_kind_change(id) {
            self.defer_composite_control(id, control::Action::Repeat(enabled), false);
            return Ok(());
        }
        self.requests.push(
            Request::LoopRepeatSync {
                loop_id: slot,
                enabled,
            },
            Purpose::RepeatSync(id, enabled),
        );
        Ok(())
    }

    pub(crate) fn reset_composite(&mut self, id: BackendCompositeId) -> Result<BackendLoopId> {
        self.ready()?;
        let id = primitive(id);
        let entry = self
            .mirror
            .live_loop(id)
            .filter(|entry| entry.program.is_some() && entry.track.is_some())
            .ok_or_else(|| {
                anyhow!("only a track-owned composite can return to an empty primitive")
            })?;
        let previous = entry.slot;
        if self.mirror.loops.values().any(|entry| {
            entry
                .program_spans
                .iter()
                .any(|span| span.target == previous)
        }) {
            bail!("remove dependent composite events before clearing this slot");
        }
        let slot = LoopSlotId(
            self.mirror
                .loop_slots
                .bump_generation(previous.0)
                .ok_or_else(|| anyhow!("loop generation space exhausted"))?,
        );
        for job in &mut self.composites.jobs {
            if job.id == id && job.cancel.is_none() {
                job.cancel = Some(BackendCompositeOutcome::Cancelled);
            }
        }
        self.requests.push(
            Request::ResetComposite {
                previous,
                loop_id: slot,
            },
            Purpose::ResetComposite(id, slot),
        );
        Ok(id)
    }

    pub(crate) fn remove_composite(&mut self, id: BackendCompositeId) -> Result<Option<u64>> {
        self.ready()?;
        let id = primitive(id);
        let entry = self
            .mirror
            .live_loop(id)
            .filter(|e| e.program.is_some())
            .ok_or_else(|| anyhow!("unknown composite"))?;
        if self
            .composites
            .jobs
            .iter()
            .any(|job| job.id == id || job.plan.spans.iter().any(|s| target_id(s.target) == id))
        {
            bail!("cancel pending composite work before removal");
        }
        let slot = entry.slot;
        let sequence = self.requests.push(
            Request::RemoveLoop { loop_id: slot },
            Purpose::RemoveComposite(id),
        );
        Ok(Some(sequence))
    }
}
