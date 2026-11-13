//! Logical transition/timing settlement, independent of signal-node order.
//! State preparation can refuse a preview; observations are fed back before commit.
use super::*;
use rt_runtime::Sample;

pub(super) struct TransitionProcessor;
impl TransitionProcessor {
    pub(super) fn settle<S: Sample>(
        owner: &mut LoopPlanner<S>,
        context: PlanContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<Frame, Error> {
        owner.settle_boundary(context)
    }
    pub(super) fn finish<S: Sample>(
        owner: &mut LoopPlanner<S>,
        context: &mut FinishContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<(), Error> {
        owner.finish_boundary(context)
    }
}
impl<S: Sample> LoopPlanner<S> {
    pub(super) fn settle_boundary(
        &mut self,
        mut context: PlanContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<Frame, Error> {
        self.graph_epoch = context.graph_epoch;
        self.topologies.callback(context.callback_start);
        if context.frame != self.resolver.frame() {
            return Err(Error::InvalidFrame);
        }
        self.failures.clear();
        for row in self.rows.iter().flatten() {
            if row.aborted {
                self.failures.push(crate::Observation::CaptureAborted {
                    target: row.binding.target,
                    reason: timeline_error(row.failure.unwrap_or(Error::InvalidState)),
                });
            } else if row.active.as_ref().is_some_and(|take| take.edit.is_some())
                && row.failure.is_some()
            {
                self.failures.push(crate::Observation::CaptureExhausted {
                    target: row.binding.target,
                });
            }
        }
        for pending in self.pending.iter().flatten() {
            if let PendingKind::Program {
                request,
                graph_epoch,
                graph,
                ..
            } = pending.kind
            {
                if self
                    .resolver
                    .completion(request)
                    .map_err(runtime_error)?
                    .is_none()
                    && (graph_epoch != context.graph_epoch
                        || graph.is_some_and(|id| {
                            context.graphs.status(id)
                                != Ok(GraphStageStatus::Ready {
                                    validated_epoch: graph_epoch,
                                })
                        }))
                {
                    self.failures.push(crate::Observation::Refused {
                        request,
                        reason: crate::Error::InvalidState,
                    });
                }
            }
        }
        loop {
            state_boundary::StateProcessor::cancel_preparation(self, &mut context.resources)?;
            let interval = self
                .resolver
                .preview_tick(&self.failures)
                .map_err(runtime_error)?;
            for decision in self.resolver.decisions().map_err(runtime_error)? {
                self.views[decision.state.id.slot() as usize].decision = *decision;
            }
            let selected = self
                .resolver
                .preview_program_change()
                .map_err(runtime_error)?;
            let graph = selected.and_then(|selected| {
                self.pending
                    .iter()
                    .flatten()
                    .find_map(|pending| match pending.kind {
                        PendingKind::Program {
                            request,
                            graph: Some(graph),
                            graph_epoch,
                            ..
                        } if selected == request => Some((request, graph, graph_epoch)),
                        _ => None,
                    })
            });
            let permit = if let Some((request, graph, epoch)) = graph {
                match context.graphs.prepare_activation(graph, epoch) {
                    Ok(permit) => Some(permit),
                    Err(error) => {
                        if self.failures.len() == self.failures.capacity() {
                            return Err(Error::InvalidState);
                        }
                        self.failures.push(crate::Observation::Refused {
                            request,
                            reason: timeline_error(error),
                        });
                        continue;
                    }
                }
            } else {
                None
            };
            if matches!(
                state_boundary::StateProcessor::prepare(
                    self,
                    context.frame,
                    interval.end,
                    &mut context.resources,
                )?,
                state_boundary::Preparation::Retry
            ) {
                continue;
            }
            state_boundary::StateProcessor::apply(self, context.frame, &mut context.resources)?;
            self.resolver.commit().map_err(runtime_error)?;
            let _ = self.prepare_program_inputs(context.frame, &mut context.resources);
            if let Some(permit) = permit {
                self.graph_epoch = permit.commit();
            }
            self.complete(context.frame, &mut context.resources, &mut context.receipts)?;
            self.observer.topology_epoch(self.resolver.epoch());
            self.observer.settled(&self.resolver);
            self.observer.boundary(
                context.frame,
                self.resolver.decisions().map_err(runtime_error)?,
                self.resolver.events(),
                self.resolver.dropped_events(),
            );
            return Ok(interval.end);
        }
    }
    pub(super) fn finish_boundary(
        &mut self,
        context: &mut FinishContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<(), Error> {
        self.resolver.finish().map_err(runtime_error)?;
        self.collect_automatic_finishes()?;
        for row in self.rows.iter().flatten() {
            if let Some(take) = row.active.as_ref().filter(|take| take.writer.is_some()) {
                let job = self
                    .shared
                    .as_ref()
                    .ok_or(Error::InvalidState)?
                    .live_job(take.writer.unwrap())?;
                if matches!(job.kind, indexed_midi::WriteKind::Growing { .. }) {
                    self.resolver
                        .confirm_capture_frames(row.binding.target, take.revision, job.frames)
                        .map_err(runtime_error)?;
                }
            }
        }
        for index in 0..self.pending.len() {
            if let Some(pending) = self.pending[index].filter(|p| p.forwarded) {
                if let PendingKind::InputBatch = pending.kind {
                    self.collect_input_batch(&mut context.resources)?;
                }
                if let PendingKind::Transition { request, pin, .. } = pending.kind {
                    self.resolver
                        .take_completion(request)
                        .map_err(runtime_error)?
                        .ok_or(Error::InvalidState)?;
                    if let Some(pin) = pin {
                        self.ledger.release_pin(&mut context.resources, pin)?;
                    }
                }
                if let PendingKind::Program { id, request, .. } = pending.kind {
                    self.topologies
                        .collect_program(id, request, &mut self.resolver)
                        .map_err(|error| match error {
                            Failure::Runtime(error) => error,
                            Failure::Timeline(error) => runtime_error(error),
                        })?;
                }
                self.pending[index] = None;
            }
        }
        Ok(())
    }
}
