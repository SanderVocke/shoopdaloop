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

pub(super) struct Job {
    source: LoopId,
    target: Option<LoopId>,
    view: LoopState,
    sync: Option<BackendLoopId>,
    content: Option<BackendLoopContent>,
    rows: VecDeque<TrackId>,
    added: Vec<(TrackId, LoopId)>,
    annotations: Vec<(bool, u32, Option<String>)>,
    stage: Stage,
    committed: bool,
    step: &'static str,
}

impl Service {
    fn copy_target_ready(&self, id: LoopId) -> Result<(), CommandError> {
        let entry = self.loop_entry(id)?;
        if entry.view.structural_state != StructuralState::Confirmed
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
        let entry = self.loop_entry(source)?;
        if entry.view.sync || self.composites.contains_key(&source) {
            return Err(CommandError::invalid("this source cannot be cloned"));
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
            if target == source
                || self.loop_entry(target)?.track != track
                || self.composites.contains_key(&target)
            {
                return Err(CommandError::invalid(
                    "copy requires a compatible distinct slot on the same track",
                ));
            }
            self.copy_target_ready(target)?;
        }
        let rows: VecDeque<_> = if target.is_none() {
            let previous = order.len();
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
            sync: self.observed.loops[&entry.backend].sync_source,
            content: None,
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
                if let BackendAsyncResult::Ready(content) = self
                    .backend
                    .capture_loop_content_async(self.loops[&job.source].backend)?
                {
                    job.content = Some(content);
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
                self.copy_target_ready(target)?;
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
                self.copy_target_ready(target)?;
                if let BackendAsyncResult::Ready(()) = self.backend.load_loop_content_async(
                    self.loops[&target].backend,
                    job.content.as_ref().unwrap(),
                )? {
                    job.committed = true;
                    job.stage = Stage::Controls;
                }
            }
            Stage::Controls => {
                let backend = self.loops[&job.target.unwrap()].backend;
                self.backend.set_loop_gain(backend, job.view.gain)?;
                self.backend.set_loop_balance(backend, job.view.balance)?;
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
