use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Capture,
    Rows,
    Stop,
    Stopped,
    Apply,
    Controls,
    Done,
}

enum Payload {
    Media(Option<BackendLoopContent>),
    Definition(shoop_app_protocol::composite::Versioned),
}

pub(super) struct Job {
    source: LoopId,
    target: Option<LoopId>,
    view: LoopState,
    sync: Option<BackendLoopId>,
    payload: Payload,
    version: Option<u64>,
    rows: VecDeque<TrackId>,
    added: Vec<(TrackId, LoopId)>,
    annotations: Vec<(bool, u32, Option<String>)>,
    stage: Stage,
    committed: bool,
    step: &'static str,
}

impl Service {
    fn copy_sync(&self, id: LoopId) -> Option<BackendLoopId> {
        let backend = self.loops[&id].backend;
        self.observed
            .composites
            .get(&BackendCompositeId::from_raw(backend.raw()))
            .map(|state| state.sync_source)
            .unwrap_or_else(|| {
                self.observed
                    .loops
                    .get(&backend)
                    .and_then(|state| state.sync_source)
            })
    }

    fn validate_copy_dependencies(
        &self,
        source: LoopId,
        target: LoopId,
    ) -> Result<(), CommandError> {
        let ids: BTreeMap<_, _> = self
            .loops
            .iter()
            .map(|(id, entry)| (entry.backend, *id))
            .collect();
        let mut edges: BTreeMap<LoopId, BTreeSet<LoopId>> = BTreeMap::new();
        let mut incoming: BTreeMap<_, usize> = self.loops.keys().map(|id| (*id, 0)).collect();
        for id in self.loops.keys().copied() {
            let owner = if id == target { source } else { id };
            if let Some(sync) = self.copy_sync(owner) {
                let sync = ids
                    .get(&sync)
                    .ok_or_else(|| CommandError::invalid("stale synchronization source"))?;
                edges.entry(*sync).or_default().insert(id);
            }
            if let Some(owner) = self.composites.get(&owner) {
                for plan in owner.active.iter().chain(owner.candidates.values()) {
                    edges
                        .entry(id)
                        .or_default()
                        .extend(plan.document.instances.iter().map(|event| event.loop_id));
                }
            }
        }
        for targets in edges.values() {
            for target in targets {
                *incoming
                    .get_mut(target)
                    .ok_or_else(|| CommandError::invalid("stale composite child"))? += 1;
            }
        }
        let mut ready: Vec<_> = incoming
            .iter()
            .filter_map(|(id, count)| (*count == 0).then_some(*id))
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop() {
            visited += 1;
            for child in edges.get(&id).into_iter().flatten() {
                let count = incoming.get_mut(child).unwrap();
                *count -= 1;
                if *count == 0 {
                    ready.push(*child);
                }
            }
        }
        if visited != self.loops.len() {
            return Err(CommandError::invalid(
                "copy would create a synchronization or composite dependency cycle",
            ));
        }
        Ok(())
    }

    fn copy_target_ready(&self, id: LoopId) -> Result<(), CommandError> {
        let entry = self.loop_entry(id)?;
        if entry.view.structural_state != StructuralState::Confirmed
            || self
                .composites
                .get(&id)
                .is_some_and(|owner| !owner.candidates.is_empty() || owner.has_retained())
            || self.capture_recording_busy(persistence::Target::Loop {
                loop_id: id,
                content_revision: entry.view.content_revision,
            })
        {
            return Err(CommandError {
                reason: Refusal::Busy,
                message: "settle affected recording and edits before copying".into(),
            });
        }
        Ok(())
    }

    fn copy_closure(&self, root: LoopId, seen: &mut BTreeSet<LoopId>) -> Result<(), CommandError> {
        if !seen.insert(root) {
            return Ok(());
        }
        self.copy_target_ready(root)?;
        if let Some(owner) = self.composites.get(&root) {
            let active = owner
                .active
                .as_ref()
                .ok_or_else(|| CommandError::invalid("composite has no confirmed definition"))?;
            self.validate_composite_events(&active.document)?;
            for event in &active.document.instances {
                self.copy_closure(event.loop_id, seen)?;
            }
        }
        Ok(())
    }

    fn copy_references(
        &self,
        root: LoopId,
        targets: &BTreeSet<LoopId>,
        seen: &mut BTreeSet<LoopId>,
    ) -> bool {
        seen.insert(root)
            && self.composites.get(&root).is_some_and(|owner| {
                owner
                    .active
                    .iter()
                    .chain(owner.candidates.values())
                    .any(|plan| {
                        plan.document.instances.iter().any(|event| {
                            targets.contains(&event.loop_id)
                                || self.copy_references(event.loop_id, targets, seen)
                        })
                    })
            })
    }

    fn copy_destination_ready(&self, target: LoopId) -> Result<(), CommandError> {
        self.copy_target_ready(target)?;
        let mut affected = BTreeSet::new();
        if self.composites.contains_key(&target) {
            self.copy_closure(target, &mut affected)?;
        } else {
            affected.insert(target);
        }
        if self.composites.keys().any(|id| {
            *id != target
                && (self.loops[id].view.mode == LoopMode::Playing
                    || self.loops[id].view.next_mode == LoopMode::Playing)
                && self.copy_references(*id, &affected, &mut BTreeSet::new())
        }) {
            return Err(CommandError {
                reason: Refusal::Busy,
                message:
                    "settle composite ownership of the destination and its children before copying"
                        .into(),
            });
        }
        Ok(())
    }

    pub(super) fn duplicate_loop(
        &mut self,
        source: LoopId,
        explicit: Option<LoopId>,
    ) -> Result<Effect, CommandError> {
        if !self
            .backend
            .capabilities()
            .supports(BackendCapability::LoopDuplication)
        {
            return Err(CommandError::unsupported(
                BackendCapability::LoopDuplication,
            ));
        }
        if self.pending.len() > 1
            || self.persistence_job.is_some()
            || self.composite_structure.is_some()
        {
            return Err(CommandError {
                reason: Refusal::Busy,
                message: "settle previous operations before copying".into(),
            });
        }
        self.copy_target_ready(source)?;
        if self.composites.contains_key(&source) {
            self.copy_closure(source, &mut BTreeSet::new())?;
        }
        let entry = self.loop_entry(source)?;
        if entry.view.sync {
            return Err(CommandError::invalid("the sync loop cannot be cloned"));
        }
        let track = entry.track;
        let order = &self.tracks[&track].loops;
        let index = order.iter().position(|id| *id == source).unwrap();
        let target = explicit.or_else(|| {
            order[index + 1..]
                .iter()
                .copied()
                .find(|id| self.loops[id].view.empty && !self.composites.contains_key(id))
        });
        if let Some(target) = target {
            if target == source || self.loop_entry(target)?.track != track {
                return Err(CommandError::invalid(
                    "copy requires a compatible distinct slot on the same track",
                ));
            }
            let source_composite = self.composites.contains_key(&source);
            let target_composite = self.composites.contains_key(&target);
            if source_composite != target_composite
                && (!source_composite || !self.loops[&target].view.empty)
            {
                return Err(CommandError::invalid(
                    "occupied primitive/composite conversion is not supported by Clone into",
                ));
            }
            self.copy_destination_ready(target)?;
            self.validate_copy_dependencies(source, target)?;
        }
        let previous = order.len();
        let payload = if let Some(active) = self
            .composites
            .get(&source)
            .and_then(|owner| owner.active.clone())
        {
            let mut active = active;
            let mut ids = BTreeMap::new();
            for event in &mut active.document.instances {
                let id = self.next_instance()?;
                ids.insert(event.instance_id, id);
                event.instance_id = id;
            }
            for event in &mut active.prepared.instances {
                event.instance_id = ids[&event.instance_id];
            }
            Payload::Definition(active)
        } else {
            Payload::Media(None)
        };
        let rows: VecDeque<_> = if target.is_none() {
            let max = self
                .tracks
                .values()
                .filter(|t| !t.view.is_sync)
                .map(|t| t.loops.len())
                .max()
                .unwrap_or(0)
                .max(previous + 1);
            self.track_order
                .iter()
                .copied()
                .filter(|id| {
                    let t = &self.tracks[id];
                    !t.view.is_sync
                        && (*id == track || previous + 1 == max && t.loops.len() == previous)
                })
                .collect()
        } else {
            VecDeque::new()
        };
        if self.loops.len() + rows.len() > MAX_LOOPS {
            return Err(CommandError::invalid(
                "aligned row exceeds the session loop limit",
            ));
        }
        self.next_loop
            .checked_add(rows.len() as u64)
            .ok_or_else(|| CommandError::invalid("loop identity exhausted"))?;
        let entry = &self.loops[&source];
        let job = Job {
            source,
            target,
            view: entry.view.clone(),
            sync: self.copy_sync(source),
            payload,
            version: None,
            rows,
            added: Vec::new(),
            annotations: self
                .channel_annotations
                .iter()
                .filter_map(|((id, midi, channel), (revision, value))| {
                    (*id == source && *revision == entry.view.content_revision)
                        .then(|| (*midi, *channel, value.clone()))
                })
                .collect(),
            stage: Stage::Capture,
            committed: false,
            step: "capturing the source",
        };
        self.backend.cancel_media_reads();
        self.media_loop = None;
        self.media_pending = None;
        self.duplication = Some(job);
        Ok(Effect::Duplication)
    }

    pub(super) fn advance_duplication(&mut self) {
        let Some(id) = self
            .pending
            .iter()
            .find_map(|(id, pending)| matches!(pending.effect, Effect::Duplication).then_some(*id))
        else {
            return;
        };
        let pending = &self.pending[&id];
        if pending.completed.len() as u64
            != pending.last.checked_sub(pending.first).map_or(0, |n| n + 1)
        {
            return;
        }
        if pending.error.is_some() {
            self.finish(id);
            return;
        }
        let Some(mut job) = self.duplication.take() else {
            return;
        };
        let before = self.backend.mutation_cursor();
        let finishing = job.stage == Stage::Done;
        let track_receipts = !matches!(job.stage, Stage::Capture | Stage::Apply);
        let result = self.step_duplication(&mut job);
        let after = self.backend.mutation_cursor();
        let pending = self.pending.get_mut(&id).unwrap();
        pending.first = if track_receipts {
            before + 1
        } else {
            after + 1
        };
        pending.last = after;
        pending.completed.clear();
        if let Err(error) = result {
            pending.error = Some(error);
        }
        let done = finishing && pending.first > pending.last;
        self.duplication = Some(job);
        if done {
            self.finish(id);
        }
    }

    fn step_duplication(&mut self, job: &mut Job) -> Result<(), CommandError> {
        job.step = match job.stage {
            Stage::Capture => "capturing the source",
            Stage::Rows => "adding destination rows",
            Stage::Stop | Stage::Stopped => "stopping the destination",
            Stage::Apply => "applying content",
            Stage::Controls => "copying gain, balance, synchronization and repeat",
            Stage::Done => "copying metadata",
        };
        match job.stage {
            Stage::Capture => {
                self.copy_target_ready(job.source)?;
                if let Payload::Media(content) = &mut job.payload {
                    if let BackendAsyncResult::Ready(captured) = self
                        .backend
                        .capture_loop_content_async(self.loops[&job.source].backend)?
                    {
                        *content = Some(captured);
                        job.stage = Stage::Rows;
                    }
                } else {
                    job.stage = Stage::Rows;
                }
            }
            Stage::Rows => {
                if let Some(track) = job.rows.pop_front() {
                    let before = self.next_loop;
                    let result = self.execute(Command::AddLoop { track });
                    if self.next_loop > before {
                        let id = LoopId::from_raw(before);
                        job.added.push((track, id));
                        if track == self.loops[&job.source].track {
                            job.target = Some(id);
                        }
                    }
                    result?;
                } else {
                    job.stage = Stage::Stop;
                }
            }
            Stage::Stop => {
                let target = job.target.unwrap();
                self.copy_destination_ready(target)?;
                self.backend.transition_loop(
                    self.loops[&target].backend,
                    BackendLoopMode::Stopped,
                    None,
                )?;
                job.stage = Stage::Stopped;
            }
            Stage::Stopped => {
                let target = &self.loops[&job.target.unwrap()];
                if target.view.mode != LoopMode::Stopped
                    || target.view.position != 0.
                    || target.view.next_transition_delay.is_some()
                {
                    return Err(CommandError::invalid(
                        "destination did not confirm stopped at position zero",
                    ));
                }
                job.stage = Stage::Apply;
            }
            Stage::Apply => {
                let target = job.target.unwrap();
                if job.version.is_none() {
                    self.copy_target_ready(target)?;
                }
                let applied = match &job.payload {
                    Payload::Media(content) => matches!(
                        self.backend.load_loop_content_async(
                            self.loops[&target].backend,
                            content.as_ref().unwrap()
                        )?,
                        BackendAsyncResult::Ready(())
                    ),
                    Payload::Definition(active) => {
                        if let Some(version) = job.version {
                            let owner = self.composites.get(&target).ok_or_else(|| {
                                CommandError::invalid("composite activation failed")
                            })?;
                            if owner
                                .active
                                .as_ref()
                                .is_some_and(|active| active.version == version)
                            {
                                true
                            } else if owner.candidates.contains_key(&version) {
                                false
                            } else {
                                return Err(CommandError::invalid(
                                    "composite candidate did not activate",
                                ));
                            }
                        } else {
                            let geometry = active
                                .prepared
                                .instances
                                .iter()
                                .map(|event| {
                                    (event.instance_id, (event.start_frame, event.end_frame))
                                })
                                .collect();
                            self.submit_composite_geometry(
                                target,
                                active.document.clone(),
                                &geometry,
                                Some(active.prepared.unit_frames),
                            )?;
                            job.version = self.composites[&target].latest;
                            false
                        }
                    }
                };
                if applied {
                    job.committed = true;
                    job.stage = Stage::Controls;
                }
            }
            Stage::Controls => {
                let backend = self.loops[&job.target.unwrap()].backend;
                if matches!(job.payload, Payload::Media(_)) {
                    self.backend.set_loop_gain(backend, job.view.gain)?;
                    self.backend.set_loop_balance(backend, job.view.balance)?;
                }
                self.backend.set_loop_sync_source(backend, job.sync)?;
                self.backend
                    .set_loop_repeat_sync(backend, job.view.repeat_sync)?;
                job.stage = Stage::Done;
            }
            Stage::Done => {
                let target = job.target.unwrap();
                self.loops
                    .get_mut(&target)
                    .unwrap()
                    .view
                    .name
                    .clone_from(&job.view.name);
                self.channel_annotations
                    .retain(|(id, _, _), _| *id != target);
                for (midi, channel, value) in &job.annotations {
                    self.channel_annotations.insert(
                        (target, *midi, *channel),
                        (self.loops[&target].view.content_revision, value.clone()),
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn settle_duplication(&mut self, id: OperationId) {
        let Some(job) = self.duplication.take() else {
            return;
        };
        for (track, loop_id) in job.added {
            self.settle_effect(&Effect::LoopCreated(track, loop_id), false);
        }
        let pending = self.pending.get_mut(&id).unwrap();
        if let Some(error) = &mut pending.error {
            error.message = alloc::format!(
                "{} while {}: {}",
                if job.committed {
                    "Content copied; remaining controls or metadata failed"
                } else if job.stage == Stage::Apply {
                    "Copy application failed; its outcome may be unknown"
                } else {
                    "Copy failed before content application; destination may remain stopped and empty rows may remain"
                },
                job.step,
                error.message
            );
        } else if job.stage != Stage::Done {
            pending.error = Some(CommandError {
                reason: Refusal::Engine,
                message: "copy interrupted; outcome may be unknown".into(),
            });
        }
    }
}
