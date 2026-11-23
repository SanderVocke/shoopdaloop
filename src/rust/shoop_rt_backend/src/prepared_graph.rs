//! Control-owned graph preparation and explicit execution-program handoff.
use crate::requests::{Pending, Purpose};
use crate::*;
use shoop_backend::BackendMutationCompletion;
use shoop_rt_protocol::{
    immediate::{
        Batch, Code, Command, IterationTransition, PlannedTransition, TransitionMode,
        TransitionPlan,
    },
    Direction, HostPort, HostPortId, HostPortKind, LoopMode, Name, Outcome, Refusal, Request,
};

pub(crate) enum PendingGraph {
    Layout(u64),
    Composite(u64),
    Edit(Pending),
}

impl PendingGraph {
    pub fn sequence(&self) -> u64 {
        match self {
            Self::Layout(sequence) | Self::Composite(sequence) => *sequence,
            Self::Edit(pending) => pending.sequence,
        }
    }
}

#[derive(Default)]
pub(crate) struct PreparedGraph {
    pub active: bool,
    pub pending: Option<PendingGraph>,
    pub revision: u64,
    pub dirty: bool,
    pub committed: Option<Mirror>,
    outgoing: alloc::collections::VecDeque<Batch>,
    transport: Option<(u64, Command)>,
    pub ui_immediate_in_flight: usize,
}

fn transport_command(request: &Request, mirror: &Mirror) -> Option<Command> {
    match *request {
        Request::LoopIteration {
            loop_id,
            iteration,
            retrigger,
            seek,
        } => Some(Command::LoopIteration(
            loop_id,
            IterationTransition {
                iteration,
                retrigger,
                seek,
            },
        )),
        Request::LoopRepeatSync { loop_id, enabled } => {
            Some(Command::LoopRepeatSync(loop_id, enabled))
        }
        Request::LoopTransition {
            loop_id,
            mode,
            delay,
        } => {
            if mode == LoopMode::Playing
                && delay.is_none()
                && mirror
                    .slot_loops
                    .get(&loop_id)
                    .and_then(|id| mirror.loops.get(id))
                    .is_some_and(|entry| entry.program.is_some())
            {
                return Some(Command::LoopIteration(
                    loop_id,
                    IterationTransition {
                        iteration: 0,
                        retrigger: false,
                        seek: false,
                    },
                ));
            }
            let mode = match mode {
                LoopMode::Stopped => TransitionMode::Stopped,
                LoopMode::Playing => TransitionMode::Playing,
                LoopMode::Recording => TransitionMode::Recording,
                _ => return None,
            };
            let plan = match delay {
                Some(skip) => TransitionPlan {
                    first: Some(PlannedTransition { mode, skip }),
                    ..TransitionPlan::default()
                },
                None => mode.into(),
            };
            Some(Command::Transition(loop_id.0.index, plan))
        }
        _ => None,
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn prepared_graph_welcome(&mut self) {
        self.prepared_graph = Some(PreparedGraph::default());
        self.link = Link::Ready;
        self.telemetry_fresh = true;
        self.mirror.host_ports = (0..3)
            .map(|index| HostPort {
                id: HostPortId(index),
                kind: HostPortKind::Audio,
                direction: if index == 0 {
                    Direction::Input
                } else {
                    Direction::Output
                },
                name: Name::new(if index == 0 {
                    "reference_in"
                } else if index == 1 {
                    "reference_left"
                } else {
                    "reference_right"
                })
                .unwrap(),
                preferred: index != 0,
            })
            .collect();
        self.prepared_graph.as_mut().unwrap().committed = Some(self.mirror.clone());
    }
    pub(crate) fn structure_mutable(&self) -> Result<()> {
        if self
            .prepared_graph
            .as_ref()
            .is_some_and(|g| g.pending.is_some() || self.prepared_composite_confirming())
        {
            return Err(anyhow!("a program ownership handoff is pending"));
        }
        if self.prepared_graph.as_ref().is_some_and(|g| {
            g.active
                && !self
                    .capabilities
                    .contains(shoop_rt_protocol::Capabilities::LIVE_PROGRAMS)
        }) {
            return Err(anyhow!(
                "the prepared graph is immutable; restart the reference host to change it"
            ));
        }
        Ok(())
    }
    pub(crate) fn send_prepared_graph_requests(&mut self) -> usize {
        let mut count = 0;
        while let Some(pending) = self.requests.outgoing.pop_front() {
            let graph = self.prepared_graph.as_mut().unwrap();
            if graph.pending.is_some() {
                self.requests.outgoing.push_front(pending);
                break;
            }
            let composite_slot = |slot| {
                self.mirror
                    .slot_loops
                    .get(&slot)
                    .and_then(|id| self.mirror.loops.get(id))
                    .is_some_and(|entry| entry.program.is_some())
            };
            let edit = match pending.request {
                Request::CreateComposite { .. } | Request::ResetComposite { .. } => true,
                Request::RemoveLoop { .. } => {
                    matches!(pending.purpose, Purpose::RemoveComposite(_))
                }
                Request::LoopSync { loop_id, source } => {
                    composite_slot(loop_id)
                        || source.is_some_and(composite_slot)
                        || graph
                            .committed
                            .as_ref()
                            .and_then(|committed| {
                                committed
                                    .slot_loops
                                    .get(&loop_id)
                                    .and_then(|id| committed.loops.get(id))
                                    .and_then(|entry| entry.sync)
                                    .and_then(|source| committed.loops.get(&source))
                            })
                            .is_some_and(|entry| entry.program.is_some())
                }
                _ => false,
            };
            if graph.active && edit {
                if graph.transport.is_some() || self.prepared_composite_confirming() {
                    self.requests.outgoing.push_front(pending);
                    break;
                }
                if let Err(error) = self.submit_prepared_edit(&pending) {
                    self.answered(pending, Outcome::Refused(error));
                }
                count += 1;
                continue;
            }
            let draft_repeat = match pending.request {
                Request::LoopRepeatSync { loop_id, .. } => {
                    self.mirror.slot_loops.contains_key(&loop_id)
                        && !graph
                            .committed
                            .as_ref()
                            .is_some_and(|layout| layout.slot_loops.contains_key(&loop_id))
                }
                _ => false,
            };
            if let Some(command) = transport_command(&pending.request, &self.mirror)
                .filter(|_| graph.active && !draft_repeat)
            {
                // The shared immediate ring replies in order. Drain older UI
                // groups before sending a tracked command with the same payload.
                if graph.transport.is_some() || graph.ui_immediate_in_flight != 0 {
                    self.requests.outgoing.push_front(pending);
                    break;
                }
                if let Request::LoopTransition { loop_id, .. } = pending.request {
                    if !self.mirror.slot_loops.contains_key(&loop_id) {
                        self.answered(pending, Outcome::Refused(Refusal::InvalidId));
                        count += 1;
                        continue;
                    }
                }
                let batch = Batch::new(graph.revision, graph.revision, &[command]).unwrap();
                match self.client.immediate(batch) {
                    Ok(()) => {
                        graph.transport = Some((pending.sequence, command));
                        self.requests.in_flight.insert(pending.sequence, pending);
                        count += 1;
                        continue;
                    }
                    Err(
                        shoop_rt_transport::client::ClientError::Busy
                        | shoop_rt_transport::client::ClientError::Carrier(
                            shoop_rt_transport::carrier::CarrierError::Full,
                        ),
                    ) => {
                        self.requests.outgoing.push_front(pending);
                        break;
                    }
                    Err(error) => {
                        self.requests.outgoing.push_front(pending);
                        self.lose(alloc::format!(
                            "transport command submission failed: {error:?}"
                        ));
                        break;
                    }
                }
            }
            let supported = match pending.request {
                Request::CreateStrip { .. }
                | Request::CreateProcessedStrip { .. }
                | Request::CreateBus { .. }
                | Request::CreateProcessedBus { .. }
                | Request::RemoveBus { .. }
                | Request::CreateLoop { .. }
                | Request::RemoveStrip { .. }
                | Request::LoopSync { .. }
                | Request::Connect { .. }
                | Request::Route { .. }
                | Request::StripControl { .. }
                | Request::BusControl { .. }
                | Request::LoopBalance { .. } => true,
                Request::CreateComposite { .. } => !graph.active,
                Request::LoopRepeatSync { .. } => draft_repeat,
                Request::SetSmoothing { .. } => true,
                Request::LoopGain { .. } => !self.prepared_graph.as_ref().unwrap().active,
                Request::CancelBulk => true,
                _ => false,
            };
            if supported && !matches!(pending.request, Request::CancelBulk) {
                self.prepared_graph.as_mut().unwrap().dirty = true;
            }
            self.answered(
                pending,
                if supported {
                    Outcome::Done
                } else {
                    Outcome::Refused(Refusal::Unsupported)
                },
            );
            count += 1;
        }
        count
    }

    fn submit_prepared_edit(&mut self, pending: &Pending) -> core::result::Result<(), Refusal> {
        let graph = self.prepared_graph.as_ref().unwrap();
        if graph.dirty {
            return Err(Refusal::Busy);
        }
        let mut candidate = self.mirror.clone();
        match (&pending.purpose, pending.request) {
            (Purpose::CreateLoop(_), Request::CreateComposite { .. }) => {}
            (_, Request::ResetComposite { previous, loop_id }) => {
                let id = candidate
                    .slot_loops
                    .get(&previous)
                    .copied()
                    .ok_or(Refusal::InvalidId)?;
                Self::reset_composite_in_mirror(&mut candidate, id, loop_id);
            }
            (Purpose::RemoveComposite(id), Request::RemoveLoop { loop_id }) => {
                if candidate.loops.values().any(|entry| {
                    entry
                        .program_spans
                        .iter()
                        .any(|span| span.target == loop_id)
                }) {
                    return Err(Refusal::Busy);
                }
                candidate.drop_loop(*id);
            }
            (_, Request::LoopSync { loop_id, source }) => {
                let id = candidate
                    .slot_loops
                    .get(&loop_id)
                    .ok_or(Refusal::InvalidId)?;
                let source = source
                    .map(|slot| {
                        candidate
                            .slot_loops
                            .get(&slot)
                            .copied()
                            .ok_or(Refusal::InvalidId)
                    })
                    .transpose()?;
                candidate.loops.get_mut(id).ok_or(Refusal::InvalidId)?.sync = source;
            }
            _ => unreachable!(),
        }
        let outgoing =
            shoop_rt_protocol::graph::upload(&candidate, self.limits().quantum, graph.revision)
                .map_err(|_| Refusal::InvalidValue)?;
        self.queue_prepared_image(outgoing, PendingGraph::Edit(pending.clone()));
        Ok(())
    }
    pub(crate) fn install_prepared_graph(&mut self) -> Result<()> {
        self.ready()?;
        self.structure_mutable()?;
        self.pump();
        let state = self
            .prepared_graph
            .as_ref()
            .ok_or_else(|| anyhow!("this backend does not support prepared graph installation"))?;
        if state.pending.is_some() || !self.requests.is_empty() {
            return Err(anyhow!("initial layout has not settled"));
        }
        if state.active && !state.dirty {
            return Err(anyhow!("the prepared layout has not changed"));
        }
        let outgoing =
            shoop_rt_protocol::graph::upload(&self.mirror, self.limits().quantum, state.revision)?;
        let sequence = self.requests.reserve_mutation();
        self.queue_prepared_image(outgoing, PendingGraph::Layout(sequence));
        Ok(())
    }

    pub(crate) fn submit_composite_image(&mut self, sequence: u64, image: &Mirror) -> Result<()> {
        let graph = self.prepared_graph.as_ref().unwrap();
        let outgoing =
            shoop_rt_protocol::graph::upload(image, self.limits().quantum, graph.revision)?;
        self.queue_prepared_image(outgoing, PendingGraph::Composite(sequence));
        Ok(())
    }

    pub(crate) fn cancel_composite_upload(&mut self, sequence: u64) -> bool {
        let graph = self.prepared_graph.as_mut().unwrap();
        if matches!(graph.pending, Some(PendingGraph::Composite(s)) if s == sequence)
            // ActivateGraph is the last batch; until it leaves the queue, only
            // the endpoint's replaceable input buffer has received the image.
            && !graph.outgoing.is_empty()
        {
            graph.outgoing.clear();
            graph.pending = None;
            return true;
        }
        false
    }

    fn queue_prepared_image(&mut self, outgoing: Vec<Batch>, pending: PendingGraph) {
        let graph = self.prepared_graph.as_mut().unwrap();
        graph.pending = Some(pending);
        graph.outgoing = outgoing.into();
    }
    pub(crate) fn send_prepared_graph_chunks(&mut self) -> usize {
        if self.link != Link::Ready {
            return 0;
        }
        let Some(graph) = &mut self.prepared_graph else {
            return 0;
        };
        let Some(batch) = graph.outgoing.front().copied() else {
            return 0;
        };
        match self.client.immediate(batch) {
            Ok(()) => {
                graph.outgoing.pop_front();
                1
            }
            Err(shoop_rt_transport::client::ClientError::Carrier(
                shoop_rt_transport::carrier::CarrierError::Full,
            ))
            | Err(shoop_rt_transport::client::ClientError::Busy) => 0,
            Err(error) => {
                self.lose(alloc::format!("graph submission failed: {error:?}"));
                0
            }
        }
    }
    pub(crate) fn prepared_graph_response(
        &mut self,
        response: &shoop_rt_protocol::immediate::Response,
    ) {
        if let [record] = response.records() {
            if let Some(graph) = &mut self.prepared_graph {
                if let Some((sequence, _)) = graph
                    .transport
                    .filter(|&(_, command)| command == record.command())
                {
                    graph.transport = None;
                    let pending = self.requests.in_flight.remove(&sequence).unwrap();
                    let code = if response.header.code() == Code::Applied {
                        record.code()
                    } else {
                        response.header.code()
                    };
                    let outcome = match code {
                        Code::Applied | Code::Scheduled => Outcome::Done,
                        Code::TargetUnavailable => Outcome::Refused(Refusal::InvalidId),
                        Code::InvalidValue => Outcome::Refused(Refusal::InvalidValue),
                        Code::Unsupported => Outcome::Refused(Refusal::Unsupported),
                        _ => Outcome::Refused(Refusal::Busy),
                    };
                    if outcome == Outcome::Done {
                        if let (Command::LoopRepeatSync(id, enabled), Some(committed)) =
                            (record.command(), graph.committed.as_mut())
                        {
                            if let Some(entry) = committed
                                .slot_loops
                                .get(&id)
                                .and_then(|key| committed.loops.get_mut(key))
                            {
                                entry.repeat_sync = enabled;
                            }
                        }
                    }
                    self.answered(pending, outcome);
                    return;
                }
            }
        }
        if !response
            .records()
            .iter()
            .any(|r| r.command() == Command::ActivateGraph)
        {
            if let Some(graph) = &mut self.prepared_graph {
                if response.records().iter().all(|r| {
                    !matches!(
                        r.command(),
                        Command::BeginGraph { .. }
                            | Command::ConfigureLoop(_)
                            | Command::BeginGraphImage { .. }
                            | Command::GraphImageData { .. }
                    )
                }) {
                    graph.ui_immediate_in_flight = graph.ui_immediate_in_flight.saturating_sub(1);
                }
            }
            return;
        }
        let Some(graph) = &mut self.prepared_graph else {
            return;
        };
        let Some(pending) = graph.pending.take() else {
            return;
        };
        let success = response.header.code() == Code::Applied
            && response.records().iter().all(|r| r.code() == Code::Applied);
        if success {
            graph.active = true;
            graph.revision = response.header.graph();
        }
        let result = if success {
            Ok(())
        } else {
            Err(response.header.code().capacity_resource().map_or_else(
                || {
                    response
                        .header
                        .code()
                        .unsupported_graph_feature()
                        .map_or_else(
                            || "prepared graph was refused".into(),
                            |feature| alloc::format!("graph compiler does not support {feature}"),
                        )
                },
                |resource| {
                    alloc::format!("insufficient capacity for {resource} while staging the graph")
                },
            ))
        };
        match pending {
            PendingGraph::Layout(sequence) => {
                graph.dirty = false;
                if success {
                    graph.committed = Some(self.mirror.clone());
                } else if let Some(previous) = &graph.committed {
                    self.mirror.restore_prepared(previous.clone());
                }
                self.mutation_completions
                    .push(BackendMutationCompletion { sequence, result });
            }
            PendingGraph::Composite(sequence) => {
                self.prepared_composite_answer(sequence, result, response.header.frame());
            }
            PendingGraph::Edit(pending) => {
                self.answered(
                    pending,
                    if success {
                        Outcome::Done
                    } else {
                        let code = response.header.code();
                        Outcome::Refused(match code {
                            Code::InvalidValue => Refusal::InvalidValue,
                            code if code.capacity_resource().is_some() => Refusal::Capacity,
                            code if code.unsupported_graph_feature().is_some() => {
                                Refusal::Unsupported
                            }
                            _ => Refusal::Busy,
                        })
                    },
                );
                if success {
                    self.prepared_graph.as_mut().unwrap().committed = Some(self.mirror.clone());
                }
            }
        }
    }
}
