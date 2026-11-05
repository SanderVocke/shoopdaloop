//! Logical controls accepted while a stable slot is changing runtime identity.
use super::*;

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Transport {
        mode: LoopMode,
        delay: Option<u32>,
        iteration: Option<u64>,
    },
    Clear,
    Repeat(bool),
    Sync(Option<BackendLoopId>),
}

pub(crate) struct Control {
    id: BackendLoopId,
    action: Action,
    require_composite: bool,
    required_version: Option<u64>,
    sent: bool,
    reset: Option<LoopSlotId>,
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    fn pending_initial_plan(&self, id: BackendLoopId) -> bool {
        self.mirror
            .loops
            .get(&id)
            .and_then(|entry| entry.program)
            .is_some_and(|program| program.version == 0)
            && self.composites.jobs.iter().any(|job| job.id == id)
    }
    pub(crate) fn pending_conversion(&self, id: BackendLoopId) -> bool {
        self.mirror
            .loops
            .get(&id)
            .is_some_and(|entry| entry.program.is_none())
            && self.composites.jobs.iter().any(|job| job.id == id)
    }
    pub(crate) fn queue_clear(&mut self, id: BackendLoopId) {
        for job in &mut self.composites.jobs {
            if job.id == id && job.cancel.is_none() {
                job.cancel = Some(BackendCompositeOutcome::Cancelled);
            }
        }
        self.defer_composite_control(id, Action::Clear, false);
    }
    pub(crate) fn pending_clear(&self, id: BackendLoopId) -> bool {
        self.composites
            .controls
            .values()
            .any(|control| control.id == id && matches!(control.action, Action::Clear))
    }
    pub(crate) fn pending_kind_change(&self, id: BackendLoopId) -> bool {
        self.pending_clear(id) || self.pending_conversion(id) || self.pending_initial_plan(id) || self.requests.outgoing.iter().chain(self.requests.in_flight.values()).any(|pending| matches!(pending.purpose, Purpose::ResetComposite(target, _) if target == id))
    }
    pub(crate) fn defer_composite_control(
        &mut self,
        id: BackendLoopId,
        action: Action,
        require_composite: bool,
    ) {
        let sequence = self.requests.reserve_mutation();
        let required_version = (require_composite && self.pending_initial_plan(id))
            .then(|| {
                self.composites
                    .jobs
                    .iter()
                    .rev()
                    .find(|job| job.id == id)
                    .map(|job| job.version)
            })
            .flatten();
        tracing::debug!(
            sequence,
            loop_id = id.raw(),
            "backend.rt.composite_control_queued"
        );
        self.composites.controls.insert(
            sequence,
            Control {
                id,
                action,
                require_composite,
                required_version,
                sent: false,
                reset: None,
            },
        );
    }
    pub(super) fn advance_composite_controls(&mut self) {
        let sequences = self.composites.controls.keys().copied().collect::<Vec<_>>();
        let mut room = 32usize.saturating_sub(
            self.requests
                .outgoing
                .iter()
                .chain(self.requests.in_flight.values())
                .filter(|pending| matches!(pending.purpose, Purpose::CompositeControl(_)))
                .count(),
        );
        for sequence in sequences {
            let Some(mut control) = self.composites.controls.remove(&sequence) else {
                continue;
            };
            let source_pending = match control.action {
                Action::Sync(Some(source)) => self.pending_kind_change(source),
                _ => false,
            };
            let earlier = self
                .composites
                .controls
                .range(..sequence)
                .any(|(_, prior)| prior.id == control.id);
            let changing = self.pending_conversion(control.id) || self.pending_initial_plan(control.id) || self.requests.outgoing.iter().chain(self.requests.in_flight.values()).any(|pending| matches!(pending.purpose, Purpose::ResetComposite(target, _) if target == control.id));
            if control.sent || room == 0 || changing || earlier || source_pending {
                self.composites.controls.insert(sequence, control);
                continue;
            }
            let Some(entry) = self.mirror.live_loop(control.id) else {
                self.finish_composite_control(
                    sequence,
                    control,
                    Err("the loop was removed before its control could apply".to_string()),
                );
                continue;
            };
            if control.require_composite
                && (entry.program.is_none()
                    || control.required_version.is_some_and(|version| {
                        entry
                            .program
                            .is_none_or(|program| program.version < version)
                    }))
            {
                self.finish_composite_control(
                    sequence,
                    control,
                    Err("the prepared composite did not install".to_string()),
                );
                continue;
            }
            let loop_id = entry.slot;
            let request = match control.action {
                Action::Clear => {
                    if entry.program.is_some() {
                        if entry.track.is_none()
                            || self.mirror.loops.values().any(|parent| {
                                parent
                                    .program_spans
                                    .iter()
                                    .any(|span| span.target == loop_id)
                            })
                        {
                            self.finish_composite_control(
                                sequence,
                                control,
                                Err(
                                    "remove dependent composite events before clearing this slot"
                                        .to_string(),
                                ),
                            );
                            continue;
                        }
                        let Some(slot) = self
                            .mirror
                            .loop_slots
                            .bump_generation(loop_id.0)
                            .map(LoopSlotId)
                        else {
                            self.finish_composite_control(
                                sequence,
                                control,
                                Err("loop generation space exhausted".to_string()),
                            );
                            continue;
                        };
                        control.reset = Some(slot);
                        Request::ResetComposite {
                            previous: loop_id,
                            loop_id: slot,
                        }
                    } else {
                        Request::LoopClear { loop_id }
                    }
                }
                Action::Transport {
                    mode,
                    delay,
                    iteration: None,
                } => Request::LoopTransition {
                    loop_id,
                    mode,
                    delay,
                },
                Action::Transport {
                    iteration: Some(iteration),
                    ..
                } => Request::LoopIteration {
                    loop_id,
                    iteration,
                    retrigger: false,
                    seek: false,
                },
                Action::Repeat(enabled) => Request::LoopRepeatSync { loop_id, enabled },
                Action::Sync(source) => {
                    let source = if let Some(source) = source {
                        let Some(entry) = self.mirror.live_loop(source) else {
                            self.finish_composite_control(
                                sequence,
                                control,
                                Err("the synchronization source was removed".to_string()),
                            );
                            continue;
                        };
                        Some(entry.slot)
                    } else {
                        None
                    };
                    Request::LoopSync { loop_id, source }
                }
            };
            control.sent = true;
            room -= 1;
            self.requests
                .push_internal(request, Purpose::CompositeControl(sequence));
            self.composites.controls.insert(sequence, control);
        }
    }
    pub(crate) fn composite_control_answer(&mut self, sequence: u64, outcome: Outcome) {
        let Some(control) = self.composites.controls.remove(&sequence) else {
            return;
        };
        let result = match outcome {
            Outcome::Done => Ok(()),
            Outcome::Refused(reason) => Err(crate::incoming::refusal_text(reason).to_string()),
            _ => Err("unexpected composite control receipt".to_string()),
        };
        if result.is_ok() {
            if let Some(slot) = control.reset {
                self.confirm_composite_reset(control.id, slot);
            }
            if matches!(control.action, Action::Clear) {
                self.canonical_content.remove(&control.id);
                self.prepared_content.remove(&control.id);
                self.canonical_imports.remove(&control.id);
            }
            if let Some(entry) = self.mirror.loops.get_mut(&control.id) {
                match control.action {
                    Action::Repeat(enabled) => entry.repeat_sync = enabled,
                    Action::Sync(source) => {
                        entry.sync = source;
                        for job in &mut self.composites.jobs {
                            if job.id == control.id {
                                job.plan.sync_source = source;
                            }
                        }
                    }
                    Action::Clear => entry.content += 1,
                    Action::Transport { .. } => {}
                }
            }
        }
        self.finish_composite_control(sequence, control, result);
    }
    pub(crate) fn confirm_composite_reset(&mut self, id: BackendLoopId, slot: LoopSlotId) {
        self.composites.applied_frames.remove(&id);
        if let Some(entry) = self.mirror.loops.get_mut(&id) {
            let old = entry.slot;
            entry.slot = slot;
            entry.timing = Default::default();
            entry.rt_content_revision = 0;
            entry.recorded = None;
            entry.program = None;
            entry.program_spans.clear();
            entry.composite = None;
            entry.view = crate::mirror::LoopView::default();
            self.mirror.slot_loops.remove(&old);
            self.mirror.slot_loops.insert(slot, id);
        }
        self.composites
            .removed
            .push(BackendCompositeId::from_raw(id.raw()));
    }
    fn finish_composite_control(
        &mut self,
        sequence: u64,
        control: Control,
        result: Result<(), alloc::string::String>,
    ) {
        if let Err(message) = &result {
            self.failure(
                shoop_backend::BackendMutationKind::LoopControl,
                Some(control.id.raw()),
                None,
                message.clone(),
            );
        }
        self.mutation_completions
            .push(BackendMutationCompletion { sequence, result });
    }
    pub(super) fn disconnect_composite_controls(&mut self) {
        let controls = core::mem::take(&mut self.composites.controls);
        for (sequence, control) in controls {
            self.finish_composite_control(
                sequence,
                control,
                Err("the connection ended before the composite control was confirmed".to_string()),
            );
        }
    }
}
