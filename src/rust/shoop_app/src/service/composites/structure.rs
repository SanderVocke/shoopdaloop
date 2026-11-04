//! Receipt-driven lifecycle cleanup for referenced stable loop slots.
use super::*;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Detach,
    Mutate,
    Restore,
    Rollback,
    Done,
}

pub(in crate::service) struct Job {
    pub operation: Option<OperationId>,
    pub stopped: Vec<LoopId>,
    targets: BTreeSet<LoopId>,
    track: Option<TrackId>,
    original: BTreeMap<LoopId, Versioned>,
    final_plans: BTreeMap<LoopId, Versioned>,
    phase: Phase,
    first: u64,
    last: u64,
    completed: BTreeSet<u64>,
    error: Option<CommandError>,
}
impl Job {
    pub fn done(&self) -> bool {
        self.phase == Phase::Done
    }
    pub fn working(&self, id: LoopId) -> Option<Document> {
        (!self.done())
            .then(|| self.final_plans.get(&id).map(|plan| plan.document.clone()))
            .flatten()
    }
    fn settled(&self) -> bool {
        self.completed.len() as u64
            == self
                .last
                .checked_sub(self.first)
                .map_or(0, |count| count + 1)
    }
}

impl Service {
    pub(in crate::service) fn composite_structure_busy(&self, command: Option<&Command>) -> bool {
        if self.composite_structure.is_none() {
            return false;
        }
        !matches!(
            command,
            Some(
                Command::SelectLoops { .. }
                    | Command::SetTarget(_)
                    | Command::Control(
                        ControlOperation::Transition {
                            mode: LoopMode::Stopped,
                            ..
                        } | ControlOperation::Trigger {
                            mode: LoopMode::Stopped,
                            ..
                        }
                    )
                    | Command::Global(GlobalControlAction::StopAll)
            )
        )
    }
    pub(in crate::service) fn clear_composite_slots(
        &mut self,
        loops: Vec<LoopId>,
    ) -> Result<Effect, CommandError> {
        let targets = loops.into_iter().collect::<BTreeSet<_>>();
        for id in &targets {
            self.loop_entry(*id)?;
        }
        if targets.iter().all(|id| !self.composites.contains_key(id)) {
            for id in targets {
                self.backend.clear_loop(self.loops[&id].backend)?;
            }
            return Ok(Effect::None);
        }
        self.start_composite_structure(targets, None)?;
        Ok(Effect::None)
    }
    pub(in crate::service) fn remove_track_with_composites(
        &mut self,
        track: TrackId,
    ) -> Result<Effect, CommandError> {
        let targets = self.tracks[&track].loops.iter().copied().collect();
        self.start_composite_structure(targets, Some(track))?;
        self.tracks.get_mut(&track).unwrap().view.structural_state = StructuralState::Removing;
        Ok(Effect::TrackRemoved(track))
    }
    fn start_composite_structure(
        &mut self,
        targets: BTreeSet<LoopId>,
        track: Option<TrackId>,
    ) -> Result<(), CommandError> {
        if self.composite_structure.is_some() {
            return Err(CommandError {
                reason: Refusal::Busy,
                message: "a composite lifecycle operation is pending".into(),
            });
        }
        let mut affected = targets.clone();
        loop {
            let old = affected.len();
            for (&parent, owner) in &self.composites {
                if owner
                    .active
                    .iter()
                    .chain(owner.candidates.values())
                    .any(|plan| {
                        plan.document
                            .instances
                            .iter()
                            .any(|event| affected.contains(&event.loop_id))
                    })
                {
                    affected.insert(parent);
                }
            }
            if old == affected.len() {
                break;
            }
        }
        let mut original = BTreeMap::new();
        let mut final_plans = BTreeMap::new();
        for id in &affected {
            let Some(owner) = self.composites.get(id) else {
                continue;
            };
            if let Some(active) = &owner.active {
                original.insert(*id, active.clone());
            }
            if targets.contains(id) {
                continue;
            }
            if let Some(authored) = owner.authored() {
                let mut plan = authored.clone();
                if track.is_some() {
                    prune(&mut plan, &targets);
                }
                if !plan.valid() {
                    return Err(CommandError::invalid("invalid retained composite timing"));
                }
                final_plans.insert(*id, plan);
            }
        }
        let first = self.backend.mutation_cursor().saturating_add(1);
        let mut job = Job {
            operation: None,
            stopped: affected.difference(&targets).copied().collect(),
            targets,
            track,
            original,
            final_plans,
            phase: Phase::Detach,
            first,
            last: first.saturating_sub(1),
            completed: BTreeSet::new(),
            error: None,
        };
        tracing::debug!(
            parents = job.stopped.len(),
            targets = job.targets.len(),
            "service.composite_structure_prepare"
        );
        let result = (|| {
            for id in &affected {
                self.backend.transition_loop(
                    self.loops[id].backend,
                    BackendLoopMode::Stopped,
                    None,
                )?;
            }
            for id in &affected {
                let Some(owner) = self.composites.get(id) else {
                    continue;
                };
                if owner.active.is_none() {
                    let backend = BackendCompositeId::from_raw(self.loops[id].backend.raw());
                    for version in owner.candidates.keys() {
                        self.backend.cancel_composite_plan(backend, *version)?;
                    }
                    continue;
                }
                let mut empty = owner.authored().unwrap().document.clone();
                empty.instances.clear();
                self.submit_composite_document(*id, empty)?;
            }
            Ok::<(), CommandError>(())
        })();
        job.error = result.err();
        job.last = self.backend.mutation_cursor();
        self.composite_structure = Some(job);
        Ok(())
    }
    pub(in crate::service) fn observe_composite_structure(
        &mut self,
        completions: &[BackendMutationCompletion],
    ) {
        if let Some(job) = &mut self.composite_structure {
            for completion in completions {
                if (job.first..=job.last).contains(&completion.sequence) {
                    job.completed.insert(completion.sequence);
                    if let Err(message) = &completion.result {
                        job.error.get_or_insert(CommandError {
                            reason: Refusal::Engine,
                            message: message.clone(),
                        });
                    }
                }
            }
        }
    }
    pub(in crate::service) fn advance_composite_structure(&mut self) {
        let Some(mut job) = self.composite_structure.take() else {
            return;
        };
        if job.done() || !job.settled() {
            self.composite_structure = Some(job);
            return;
        }
        let first = self.backend.mutation_cursor().saturating_add(1);
        let next = if self.observed.status.driver_state == BackendDriverState::Failed {
            Phase::Done
        } else {
            match job.phase {
                Phase::Detach if job.error.is_some() => Phase::Rollback,
                Phase::Detach => Phase::Mutate,
                Phase::Mutate if job.error.is_some() => Phase::Rollback,
                Phase::Mutate => Phase::Restore,
                Phase::Restore | Phase::Rollback | Phase::Done => Phase::Done,
            }
        };
        job.phase = next;
        let result = (|| {
            match next {
                Phase::Mutate => {
                    if let Some(track) = job.track {
                        self.backend.remove_track(self.tracks[&track].backend)?;
                    } else {
                        for id in &job.targets {
                            self.backend.clear_loop(self.loops[id].backend)?;
                        }
                    }
                }
                Phase::Restore | Phase::Rollback => {
                    let plans = if next == Phase::Rollback {
                        &job.original
                    } else {
                        &job.final_plans
                    };
                    for (&id, plan) in plans {
                        if self
                            .observed
                            .composites
                            .contains_key(&BackendCompositeId::from_raw(
                                self.loops[&id].backend.raw(),
                            ))
                        {
                            self.submit_prepared_composite(
                                id,
                                plan.document.clone(),
                                plan.prepared.clone(),
                            )?;
                        }
                    }
                }
                Phase::Done => {
                    if let Some(id) = job.operation {
                        if let Some(pending) = self.pending.get_mut(&id) {
                            if let Some(error) = &job.error {
                                pending.error = Some(error.clone());
                            }
                        }
                    }
                }
                Phase::Detach => unreachable!(),
            }
            Ok::<(), CommandError>(())
        })();
        if let Err(error) = result {
            job.error.get_or_insert(error);
        }
        job.first = first;
        job.last = self.backend.mutation_cursor();
        job.completed.clear();
        self.composite_structure = Some(job);
    }
}

fn prune(plan: &mut Versioned, removed: &BTreeSet<LoopId>) {
    let mut ordinals = BTreeMap::new();
    for (old, event) in plan.document.instances.iter().enumerate() {
        if !removed.contains(&event.loop_id) {
            ordinals.insert(old as u32, ordinals.len() as u32);
        }
    }
    let retained = plan
        .document
        .instances
        .iter()
        .filter(|event| !removed.contains(&event.loop_id))
        .map(|event| event.instance_id)
        .collect::<BTreeSet<_>>();
    plan.document
        .instances
        .retain(|event| retained.contains(&event.instance_id));
    plan.prepared
        .instances
        .retain(|event| retained.contains(&event.instance_id));
    plan.prepared.spans.retain_mut(|span| {
        if let Some(ordinal) = ordinals.get(&span.ordinal) {
            span.ordinal = *ordinal;
            true
        } else {
            false
        }
    });
    plan.prepared.iterations = plan
        .prepared
        .instances
        .iter()
        .map(|event| event.end_frame / plan.prepared.unit_frames)
        .max()
        .unwrap_or(0);
}
