use super::*;
use shoop_app_protocol::composite::Versioned;

pub(super) struct Job {
    waiting: Option<(LoopId, Option<LoopId>)>,
    source: BackendLoopId,
    pub(super) target: Option<LoopId>,
    source_track: TrackId,
    view: LoopState,
    composite: Option<Versioned>,
    additions: Vec<TrackId>,
}

impl Job {
    pub(super) fn frozen_target(&self) -> Option<LoopId> {
        if self.waiting.is_none() {
            self.target
        } else {
            None
        }
    }
}

impl Service {
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
        if self.pending.len() > 1 && self.persistence_job.is_none() {
            let entry = self.loop_entry(source)?;
            self.duplication = Some(Job {
                waiting: Some((source, explicit)),
                source: entry.backend,
                target: explicit,
                source_track: entry.track,
                view: entry.view.clone(),
                composite: None,
                additions: Vec::new(),
            });
            return Ok(Effect::Duplication);
        }
        if self.persistence_job.is_some() {
            return Err(CommandError::invalid(
                "settle previous operations before duplication",
            ));
        }
        let source_entry = self.loop_entry(source)?;
        let track = source_entry.track;
        if source_entry.view.sync {
            return Err(CommandError::invalid("the sync loop cannot be cloned"));
        }
        let order = &self.tracks[&track].loops;
        let index = order.iter().position(|id| *id == source).unwrap();
        let target = explicit.or_else(|| {
            order[index + 1..]
                .iter()
                .copied()
                .find(|id| self.loops[id].view.empty && !self.composites.contains_key(id))
        });
        let mut affected = vec![source];
        if let Some(target) = target {
            if source == target || self.loop_entry(target)?.track != track {
                return Err(CommandError::invalid(
                    "duplication needs a distinct destination on the same track",
                ));
            }
            affected.push(target);
        }
        for id in &affected {
            let entry = &self.loops[id];
            if entry.view.structural_state != StructuralState::Confirmed
                || self.capture_recording_busy(persistence::Target::Loop {
                    loop_id: *id,
                    content_revision: entry.view.content_revision,
                })
                || self
                    .composites
                    .get(id)
                    .is_some_and(|owner| !owner.candidates.is_empty() || owner.has_retained())
            {
                return Err(CommandError::invalid(
                    "settle affected capture and composite edits before duplication",
                ));
            }
        }
        let mut composite = self
            .composites
            .get(&source)
            .and_then(|owner| owner.active.clone());
        if let (Some(target), Some(active)) = (target, composite.as_ref()) {
            self.validate_composite_dependencies(target, &active.document)?;
        }
        let additions = if target.is_none() {
            let previous = order.len();
            let after = previous + 1;
            let max = self
                .tracks
                .values()
                .filter(|track| !track.view.is_sync)
                .map(|track| track.loops.len())
                .max()
                .unwrap_or(0)
                .max(after);
            self.track_order
                .iter()
                .copied()
                .filter(|id| {
                    let entry = &self.tracks[id];
                    !entry.view.is_sync
                        && (*id == track || after == max && entry.loops.len() == previous)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if self.loops.len() + additions.len() > MAX_LOOPS {
            return Err(CommandError::invalid(
                "aligned row exceeds the session loop limit",
            ));
        }
        self.next_loop
            .checked_add(additions.len() as u64)
            .ok_or_else(|| CommandError::invalid("loop identity exhausted"))?;
        if let Some(active) = &mut composite {
            let end = self
                .next_composite_event
                .checked_add(active.document.instances.len() as u64)
                .ok_or_else(|| CommandError::invalid("event identity exhausted"))?;
            let mut ids = BTreeMap::new();
            for event in &mut active.document.instances {
                let id = self.next_composite_event;
                self.next_composite_event += 1;
                ids.insert(event.instance_id, id);
                event.instance_id = id;
            }
            for event in &mut active.prepared.instances {
                event.instance_id = ids[&event.instance_id];
            }
            debug_assert_eq!(self.next_composite_event, end);
        }
        let source_entry = &self.loops[&source];
        self.backend.cancel_media_reads();
        self.media_loop = None;
        self.media_pending = None;
        self.duplication = Some(Job {
            waiting: None,
            source: source_entry.backend,
            target,
            source_track: track,
            view: source_entry.view.clone(),
            composite,
            additions,
        });
        Ok(Effect::Duplication)
    }

    pub(super) fn advance_duplication(&mut self) {
        let Some(job) = self.duplication.take() else {
            return;
        };
        let operation = self
            .pending
            .iter()
            .find_map(|(id, pending)| matches!(pending.effect, Effect::Duplication).then_some(*id));
        if let Some((source, target)) = job.waiting {
            if self.pending.len() > 1 {
                let deferred = self
                    .pending
                    .values()
                    .any(|pending| matches!(pending.effect, Effect::CompositeEdit(_)))
                    || self
                        .observed
                        .loops
                        .values()
                        .any(|entry| entry.next_mode.is_some());
                if deferred {
                    if let Some(operation) = operation {
                        self.pending.get_mut(&operation).unwrap().error = Some(CommandError::invalid("settle scheduled transitions or pending composite edits before duplication"));
                        self.finish(operation);
                    }
                } else {
                    self.duplication = Some(job);
                }
                return;
            }
            if let Err(error) = self.duplicate_loop(source, target) {
                if let Some(operation) = operation {
                    self.pending.get_mut(&operation).unwrap().error = Some(error);
                    self.finish(operation);
                }
            }
            return;
        }
        let target = job.target.map(|id| self.loops[&id].backend);
        let additions = job
            .additions
            .iter()
            .map(|id| self.tracks[id].backend)
            .collect::<Vec<_>>();
        let row_sync = self.global.sync.then(|| self.sync_backend()).flatten();
        match self
            .backend
            .duplicate_loop_async(job.source, target, &additions, row_sync)
        {
            Ok(BackendAsyncResult::Pending(_)) => {
                self.duplication = Some(job);
            }
            Ok(BackendAsyncResult::Ready(result)) => {
                let mut target = job.target;
                for (track, backend) in result.added {
                    let owner = job
                        .additions
                        .iter()
                        .copied()
                        .find(|id| self.tracks[id].backend == track)
                        .expect("reserved row track");
                    let entry = &self.tracks[&owner];
                    let id = self
                        .insert_loop(owner, backend, entry.audio_channels, entry.midi, false)
                        .expect("preflighted loop identity");
                    self.tracks.get_mut(&owner).unwrap().loops.push(id);
                    if backend == result.target {
                        target = Some(id);
                    }
                }
                let target = target.expect("reserved destination");
                let mut view = job.view;
                view.id = target;
                view.selected = false;
                view.targeted = false;
                view.mode = LoopMode::Stopped;
                view.position = 0.;
                view.next_mode = LoopMode::Unknown;
                view.next_transition_delay = None;
                view.structural_state = StructuralState::Confirmed;
                view.active_composite_children = Arc::from([]);
                self.loops.get_mut(&target).unwrap().view = view;
                self.loops.get_mut(&target).unwrap().track = job.source_track;
                self.composites.remove(&target);
                if let Some(active) = job.composite {
                    let revisions = active
                        .document
                        .instances
                        .iter()
                        .map(|event| {
                            let view = &self.loops[&event.loop_id].view;
                            (event.loop_id, (view.content_revision, view.length_frames))
                        })
                        .collect();
                    self.composites
                        .insert(target, composites::Owner::restored(active, revisions));
                }
                if let Ok(observed) = self.backend.poll() {
                    self.apply_observation(observed);
                }
                if let Some(operation) = operation {
                    self.pending.get_mut(&operation).unwrap().effect = Effect::None;
                    self.finish(operation);
                }
            }
            Err(error) => {
                if let Some(operation) = operation {
                    self.pending.get_mut(&operation).unwrap().error = Some(error.into());
                    self.finish(operation);
                }
            }
        }
    }
}
