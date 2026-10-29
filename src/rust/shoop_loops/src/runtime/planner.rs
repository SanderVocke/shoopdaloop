use super::*;

impl LoopPlanner {
    fn validate_graph(&self, graph: &PreparedPlan) -> Result<(), Error> {
        let bindings = &graph.planning().bindings;
        if bindings.len()
            != self
                .rows
                .iter()
                .flatten()
                .filter(|row| row.assigned)
                .count()
                * 2
        {
            return Err(Error::InvalidShape);
        }
        for row in self.rows.iter().flatten().filter(|row| row.assigned) {
            for consumer in [row.binding.playback, row.binding.capture] {
                if !graph.contains(consumer) {
                    return Err(Error::InvalidId);
                }
                if !bindings.iter().any(|binding| {
                    binding.consumer == consumer
                        && binding.planner == self.settings.planner
                        && binding.decision_slot == row.binding.target.slot() as usize
                }) {
                    return Err(Error::InvalidShape);
                }
            }
        }
        Ok(())
    }
    fn activate_graph(
        &mut self,
        graph: GraphStageId,
        topology: Option<StageId>,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
        graphs: &mut rt_runtime::graph::activation::GraphAccess<'_>,
    ) -> Result<(Epoch, Epoch), Failure> {
        if topology_epoch != self.resolver.epoch() {
            return Err(crate::Error::InvalidState.into());
        }
        let permit = graphs.prepare_activation(graph, graph_epoch)?;
        self.validate_graph(permit.candidate())?;
        if let Some(id) = topology {
            self.topologies
                .activate(id, topology_epoch, &mut self.resolver)?;
        }
        self.graph_epoch = permit.commit();
        Ok((self.graph_epoch, self.resolver.epoch()))
    }
    fn row(&self, target: LoopId) -> Result<Row, Error> {
        self.rows
            .get(target.slot() as usize)
            .and_then(|r| *r)
            .filter(|r| r.assigned && r.binding.target == target)
            .ok_or(Error::InvalidId)
    }
    fn cut(&self, target: LoopId, frame: Frame) -> Result<Cut, Failure> {
        let state = self.resolver.state(target)?;
        if let Ok(row) = self.row(target) {
            Ok(Cut {
                frame,
                graph_epoch: self.graph_epoch,
                topology_epoch: self.resolver.epoch(),
                state,
                playback: self.ledger.playback(target)?,
                latest: self.ledger.latest(target)?,
                gain: row.binding.gain,
                capture_failure: row.failure,
                midi_overflows: row.overflows,
            })
        } else {
            Ok(Cut {
                frame,
                graph_epoch: self.graph_epoch,
                topology_epoch: self.resolver.epoch(),
                state,
                playback: None,
                latest: None,
                gain: 1.,
                capture_failure: None,
                midi_overflows: 0,
            })
        }
    }
    fn pending_slot(&self, recovery: bool) -> Result<usize, Error> {
        if recovery {
            let slot = self.settings.normal_requests;
            if self.pending[slot].is_none() {
                Ok(slot)
            } else {
                Err(Error::Capacity)
            }
        } else {
            self.pending[..self.settings.normal_requests]
                .iter()
                .position(Option::is_none)
                .ok_or(Error::Capacity)
        }
    }
    fn cancel_intervals(&mut self, io: &mut PlannerResourceIo<'_>) -> Result<(), Error> {
        for slot in 0..self.rows.len() {
            if let Some(row) = self.rows[slot] {
                if let Some(interval) = row.interval {
                    io.capture().cancel_interval(interval)?;
                    self.rows[slot].as_mut().expect("row").interval = None;
                }
            }
        }
        Ok(())
    }
    fn cancel_prepared(
        &mut self,
        slot: usize,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<(), Error> {
        if let Some(take) = self.rows[slot].and_then(|r| r.prepared) {
            if let Some(token) = take.token {
                io.capture().cancel_take(token)?;
            }
            self.ledger.retire(io, take.media)?;
            self.rows[slot].as_mut().expect("row").prepared = None;
        }
        Ok(())
    }
    fn prepare_take(
        &mut self,
        slot: usize,
        decision: Decision,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<(), Error> {
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        let id = self.ledger.reserve(media::Description {
            owner: row.binding.target,
            revision: decision.state.capture_revision,
            frames: 0,
            channels: row.binding.channels as u32,
            midi: row.binding.midi,
        })?;
        let bank = row.active.map_or(row.next_bank, |take| 1 - take.bank);
        let token = if let Some(group) = row.groups[bank] {
            match io.capture().prepare(
                group,
                if row.binding.midi {
                    std::slice::from_ref(&self.inputs[slot])
                } else {
                    &[]
                },
            ) {
                Ok(token) => Some(token),
                Err(error) => {
                    self.ledger.retire(io, id)?;
                    return Err(error);
                }
            }
        } else {
            None
        };
        self.rows[slot].as_mut().expect("row").prepared = Some(Take {
            revision: decision.state.capture_revision,
            media: id,
            bank,
            token,
        });
        Ok(())
    }
    fn latch(
        &mut self,
        slot: usize,
        decision: Decision,
        error: Error,
        starting: bool,
    ) -> Result<(), Error> {
        if !matches!(error, Error::Capacity | Error::Exhausted) {
            return Err(error);
        }
        let target = decision.state.id;
        let observation = if starting {
            decision.origin.map_or(
                crate::Observation::CaptureUnavailable {
                    target,
                    reason: timeline_error(error),
                },
                |request| crate::Observation::Refused {
                    request,
                    reason: timeline_error(error),
                },
            )
        } else {
            crate::Observation::CaptureExhausted { target }
        };
        let duplicate = self.failures.iter().any(|old| match (*old, observation) {
            (
                crate::Observation::CaptureExhausted { target: a },
                crate::Observation::CaptureExhausted { target: b },
            )
            | (
                crate::Observation::CaptureUnavailable { target: a, .. },
                crate::Observation::CaptureUnavailable { target: b, .. },
            ) => a == b,
            (
                crate::Observation::Refused { request: a, .. },
                crate::Observation::Refused { request: b, .. },
            ) => a == b,
            _ => false,
        });
        if duplicate || self.failures.len() == self.failures.capacity() {
            return Err(Error::InvalidState);
        }
        self.failures.push(observation);
        self.rows[slot].as_mut().expect("row").failure = Some(error);
        Ok(())
    }
    fn preflight(
        &mut self,
        frame: Frame,
        end: Frame,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<bool, Error> {
        let frames = usize::try_from(end.0 - frame.0).map_err(|_| Error::InvalidFrame)?;
        for slot in 0..self.rows.len() {
            let Some(mut row) = self.rows[slot].filter(|row| row.assigned) else {
                continue;
            };
            let decision = self.views[slot].decision;
            let recording = decision.state.mode == Mode::Recording;
            let starting = recording
                && row
                    .active
                    .is_none_or(|take| take.revision != decision.state.capture_revision);
            if row
                .prepared
                .is_some_and(|take| !starting || take.revision != decision.state.capture_revision)
            {
                self.cancel_prepared(slot, io)?;
                row = self.rows[slot].expect("row");
            }
            if !recording {
                continue;
            }
            if starting && row.prepared.is_none() {
                if let Err(error) = self.prepare_take(slot, decision, io) {
                    self.latch(slot, decision, error, true)?;
                    return Ok(false);
                }
                row = self.rows[slot].expect("row");
            }
            let take =
                if starting { row.prepared } else { row.active }.ok_or(Error::InvalidState)?;
            if let Some(token) = take.token {
                let result = if starting {
                    io.capture().reserve_prepared(token, frame, frames)
                } else {
                    io.capture().reserve(token, frame, frames)
                };
                match result {
                    Ok(interval) => {
                        self.rows[slot].as_mut().expect("row").interval = Some(interval)
                    }
                    Err(error) => {
                        self.latch(slot, decision, error, starting)?;
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
    fn input_from_frozen(
        &self,
        slot: usize,
        take: Take,
        frames: u64,
        io: &mut ResourceIo<'_>,
    ) -> Result<media::Input, Error> {
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        let mut position = 0;
        let mut source = || -> Result<Option<media::Source>, Error> {
            let snapshot = row.frozen[position].ok_or(Error::InvalidState)?;
            position += 1;
            Ok(Some(media::Source {
                snapshot,
                index: io.read_index(snapshot.id)?,
            }))
        };
        let audio = if row.binding.channels > 0 {
            source()?
        } else {
            None
        };
        let midi = if row.binding.midi { source()? } else { None };
        Ok(media::Input {
            description: media::Description {
                owner: row.binding.target,
                revision: take.revision,
                frames,
                channels: row.binding.channels as u32,
                midi: row.binding.midi,
            },
            audio,
            midi,
        })
    }
    fn freeze(
        &mut self,
        slot: usize,
        frame: Frame,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<media::MediaId, Error> {
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        let take = row.active.ok_or(Error::InvalidState)?;
        let incomplete=self.failures.iter().any(|o|matches!(o,crate::Observation::CaptureExhausted { target } if *target==row.binding.target));
        if let Some(token) = take.token {
            let mut output = [None; 2];
            let members = usize::from(row.binding.channels > 0) + usize::from(row.binding.midi);
            io.capture().freeze(
                token,
                frame,
                if incomplete { row.failure } else { None },
                &mut output[..members],
            )?;
            self.rows[slot].as_mut().expect("row").frozen = output;
        }
        self.rows[slot].as_mut().expect("row").active = None;
        let frames = self
            .resolver
            .state(row.binding.target)
            .map_err(runtime_error)?
            .capture_frames;
        let input = self.input_from_frozen(slot, take, frames, io)?;
        self.ledger
            .publish(io, take.media, input)
            .map_err(|e| e.error)?;
        self.rows[slot].as_mut().expect("row").frozen = [None; 2];
        self.ledger
            .set_latest(io, row.binding.target, Some(take.media))?;
        self.ledger.retire(io, take.media)?;
        Ok(take.media)
    }
    fn settle(&mut self, frame: Frame, io: &mut PlannerResourceIo<'_>) -> Result<(), Error> {
        for slot in 0..self.rows.len() {
            let Some(row) = self.rows[slot].filter(|row| row.assigned) else {
                continue;
            };
            let decision = self.views[slot].decision;
            if let Some(take) = row.active {
                if decision.state.mode != Mode::Recording
                    || decision.state.capture_revision != take.revision
                {
                    let media = self.freeze(slot, frame, io)?;
                    if decision.state.content_revision == take.revision {
                        self.ledger
                            .set_playback(io, row.binding.target, Some(media))?;
                    }
                }
            }
            if let Some(origin) = decision.origin {
                for pending in self.pending.iter().flatten() {
                    if let PendingKind::Transition {
                        request,
                        pin: Some(pin),
                    } = pending.kind
                    {
                        if request == origin {
                            let media = self.ledger.pinned(pin)?;
                            if self.ledger.view(media)?.description.owner == row.binding.target {
                                self.ledger
                                    .set_playback(io, row.binding.target, Some(media))?;
                            }
                        }
                    }
                }
            }
            if let Some(take) = self.rows[slot].and_then(|r| r.prepared) {
                if let Some(token) = take.token {
                    io.capture().activate(token, frame)?;
                }
                let row = self.rows[slot].as_mut().expect("row");
                row.active = Some(take);
                row.prepared = None;
                row.next_bank = 1 - take.bank;
                row.failure = None;
            }
            let row = self.rows[slot].expect("row");
            let media = self
                .ledger
                .playback(row.binding.target)?
                .map(|id| self.ledger.view(id))
                .transpose()?;
            if media.is_none_or(|m| {
                m.description.frames != decision.state.content_length
                    || m.description.revision != decision.state.content_revision
            }) {
                return Err(Error::InvalidState);
            }
            let playing = decision.state.plays();
            self.views[slot] = View {
                decision,
                audio: media
                    .filter(|_| playing)
                    .and_then(|m| m.audio)
                    .map(|s| s.index),
                midi: media
                    .filter(|_| playing)
                    .and_then(|m| m.midi)
                    .map(|s| s.index),
                capture: row.interval,
                gain: row.binding.gain,
                playback_node: row.binding.playback,
                capture_node: row.binding.capture,
                reset_input: row.reset_input,
            };
        }
        Ok(())
    }
    fn export_recording(
        &mut self,
        target: LoopId,
        frame: Frame,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<media::ExportId, Error> {
        let row = self.row(target)?;
        let take = row.active.ok_or(Error::InvalidState)?;
        let state = self.resolver.state(target).map_err(runtime_error)?;
        if self.ledger.usage().free_exports == 0 {
            return Err(Error::Capacity);
        }
        let id = self.ledger.reserve(media::Description {
            owner: target,
            revision: take.revision,
            frames: 0,
            channels: row.binding.channels as u32,
            midi: row.binding.midi,
        })?;
        let mut sources = [None; 2];
        let result = (|| {
            for (member, resource) in row.resources[take.bank].iter().enumerate() {
                if let Some(resource) = resource {
                    let snapshot = io.snapshot(*resource, frame)?;
                    sources[member] = Some(snapshot);
                }
            }
            let audio = sources[0]
                .map(|snapshot| {
                    io.read_index(snapshot.id)
                        .map(|index| media::Source { snapshot, index })
                })
                .transpose()?;
            let midi = sources[1]
                .map(|snapshot| {
                    io.read_index(snapshot.id)
                        .map(|index| media::Source { snapshot, index })
                })
                .transpose()?;
            self.ledger
                .publish(
                    io,
                    id,
                    media::Input {
                        description: media::Description {
                            owner: target,
                            revision: take.revision,
                            frames: state.capture_frames,
                            channels: row.binding.channels as u32,
                            midi: row.binding.midi,
                        },
                        audio,
                        midi,
                    },
                )
                .map_err(|e| e.error)?;
            sources = [None; 2];
            self.ledger.export(id, frame)
        })();
        for snapshot in sources.into_iter().flatten() {
            io.release_snapshot(snapshot.id)?;
        }
        self.ledger.retire(io, id)?;
        result
    }
    fn query(
        &mut self,
        operation: Operation,
        frame: Frame,
        io: &mut PlannerResourceIo<'_>,
    ) -> Result<Reply, Failure> {
        match operation {
            Operation::State { target } => Ok(Reply::State(self.cut(target, frame)?)),
            Operation::Export { target, selection } => {
                let cut = self.cut(target, frame)?;
                let id = match selection {
                    Selection::Recording => self.export_recording(target, frame, io)?,
                    Selection::Playback | Selection::Latest => {
                        let id = if selection == Selection::Playback {
                            cut.playback
                        } else {
                            cut.latest
                        }
                        .ok_or(Error::InvalidState)?;
                        self.ledger.export(id, frame)?
                    }
                };
                Ok(Reply::Export {
                    id,
                    cut,
                    media: self.ledger.exported(id)?,
                })
            }
            _ => Err(Error::InvalidEvent.into()),
        }
    }
    fn complete(
        &mut self,
        frame: Frame,
        io: &mut PlannerResourceIo<'_>,
        receipts: &mut schedule::ReceiptWriter<'_, Operation, Reply>,
    ) -> Result<(), Error> {
        for index in 0..self.pending.len() {
            let Some(pending) = self.pending[index] else {
                continue;
            };
            if pending.forwarded {
                continue;
            }
            let reply = match pending.kind {
                PendingKind::Program {
                    id, request, graph, ..
                } => self
                    .resolver
                    .completion(request)
                    .map_err(runtime_error)?
                    .map(|completion| match graph {
                        Some(graph) => Reply::ProgramGraphChanged {
                            id,
                            graph,
                            graph_epoch: self.graph_epoch,
                            topology_epoch: self.resolver.epoch(),
                            completion,
                        },
                        None => Reply::ProgramChanged {
                            id,
                            graph_epoch: self.graph_epoch,
                            topology_epoch: self.resolver.epoch(),
                            completion,
                        },
                    }),
                PendingKind::Transition { request, .. } => self
                    .resolver
                    .completion(request)
                    .map_err(runtime_error)?
                    .map(Reply::Transition),
                PendingKind::Query(operation) => Some(
                    self.query(operation, frame, io)
                        .unwrap_or_else(Reply::Failed),
                ),
            };
            if let Some(reply) = reply {
                receipts.complete(pending.token, Ok(reply))?;
                self.pending[index].as_mut().expect("pending").forwarded = true;
            }
        }
        Ok(())
    }
    /// Permanently quiesce and explicitly release domain owners off-render, after
    /// callbacks stopped and the runtime handed back its planner and store.
    /// Repeatable after success or partial cleanup. Does not advance/finish
    /// unrendered time or resume execution. Imported resource identities remain
    /// the caller's responsibility.
    pub fn cleanup(&mut self, resources: &mut ResourceStore) -> Result<(), Error> {
        self.resolver.quiesce();
        self.node_pairs.cleanup(&mut resources.io())?;
        self.topologies
            .cleanup(&mut self.resolver)
            .map_err(|error| match error {
                Failure::Runtime(error) => error,
                Failure::Timeline(error) => runtime_error(error),
            })?;
        for slot in 0..self.rows.len() {
            let Some(row) = self.rows[slot] else { continue };
            if let Some(interval) = row.interval {
                resources.cancel_capture_interval(interval)?;
                self.rows[slot].as_mut().expect("row").interval = None;
            }
            if let Some(take) = row.prepared {
                if let Some(token) = take.token {
                    resources.cancel_capture_take(token)?;
                }
                self.rows[slot].as_mut().expect("row").prepared = None;
            }
            if let Some(take) = row.active {
                if let Some(token) = take.token {
                    let info = resources
                        .capture_group_info(row.groups[take.bank].ok_or(Error::InvalidState)?)?;
                    let mut output = [None; 2];
                    let members =
                        usize::from(row.binding.channels > 0) + usize::from(row.binding.midi);
                    resources.freeze_capture_take(
                        token,
                        info.next_frame.ok_or(Error::InvalidState)?,
                        None,
                        &mut output[..members],
                    )?;
                    self.rows[slot].as_mut().expect("row").frozen = output;
                }
                self.rows[slot].as_mut().expect("row").active = None;
            }
            for member in 0..2 {
                if let Some(snapshot) = self.rows[slot].expect("row").frozen[member] {
                    resources.release_snapshot(snapshot.id)?;
                    self.rows[slot].as_mut().expect("row").frozen[member] = None;
                }
            }
        }
        self.ledger.release_all(&mut resources.io())?;
        for row in self.rows.iter_mut().flatten() {
            for bank in 0..2 {
                if let Some(group) = row.groups[bank] {
                    resources.release_capture_group(group)?;
                    row.groups[bank] = None;
                }
                for resource in &mut row.resources[bank] {
                    if let Some(id) = *resource {
                        resources.remove(id)?;
                        *resource = None;
                    }
                }
            }
        }
        self.pending.fill(None);
        Ok(())
    }
}
impl Planner for LoopPlanner {
    type Decision = View;
    type Observation = Observed;
    type Operation = Operation;
    type Result = Reply;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: self.settings.planner,
            decision_slots: self.views.len(),
            observation_capacity: self.rows.iter().flatten().count() * 2,
            operation_bytes: self.topologies.operation_bytes(),
        })
    }
    fn decisions(&self) -> &[View] {
        &self.views
    }
    fn admit_graph(&self, candidate: &PreparedPlan) -> Result<(), Error> {
        self.validate_graph(candidate)
    }
    fn admit_processor_change(&self, node: NodeId) -> Result<(), Error> {
        if self.rows.iter().flatten().any(|row| {
            row.binding.playback.slot() == node.slot() || row.binding.capture.slot() == node.slot()
        }) {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
    fn recovery_safe(operation: Operation) -> bool {
        matches!(
            operation,
            Operation::Cancel { .. }
                | Operation::TopologyCancel { .. }
                | Operation::NodePairCancel { .. }
                | Operation::NodePairSourceRelease { .. }
                | Operation::Transition {
                    timing: Timing::At,
                    transition: Transition::Stop,
                    ..
                }
        )
    }
    fn operation(
        &mut self,
        operation: Operation,
        mut context: OperationContext<'_, Operation, Reply>,
    ) -> Result<Disposition<Reply>, Error> {
        self.graph_epoch = context.graph_epoch;
        self.topologies.callback(context.callback_start);
        self.node_pairs.callback(context.callback_start);
        let result = (|| -> Result<Disposition<Reply>, Failure> {
            if !context.payload.is_empty()
                && !matches!(
                    operation,
                    Operation::TopologyBegin { .. } | Operation::TopologyAppend { .. }
                )
            {
                return Err(Error::InvalidShape.into());
            }
            let topology_status = match operation {
                Operation::NodePairSnapshot { resource } => {
                    let (id, snapshot) = self.node_pairs.snapshot(
                        resource,
                        context.frame,
                        &mut context.resources,
                    )?;
                    return Ok(Disposition::Complete(Reply::NodePairSource {
                        id,
                        snapshot,
                        status: self.node_pairs.source_status(id)?,
                    }));
                }
                Operation::NodePairSourceStatus { id } => {
                    return Ok(Disposition::Complete(Reply::NodePairSourceStatus {
                        id,
                        status: self.node_pairs.source_status(id)?,
                    }))
                }
                Operation::NodePairSourceValidate { id } => {
                    return Ok(Disposition::Complete(Reply::NodePairSourceStatus {
                        id,
                        status: self
                            .node_pairs
                            .validate_source(id, &mut context.resources)?,
                    }))
                }
                Operation::NodePairSourceRelease { id } => {
                    self.node_pairs.release_source(id, &mut context.resources)?;
                    return Ok(Disposition::Complete(Reply::NodePairSourceReleased { id }));
                }
                Operation::NodePairBegin { slot, changes } => {
                    let id = self.node_pairs.begin(slot, changes)?;
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }));
                }
                Operation::NodePairAppend {
                    id,
                    previous,
                    initial,
                } => {
                    self.append_node_pair(id, previous, initial, &mut context.resources)?;
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }));
                }
                Operation::NodePairStatus { id } => {
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }))
                }
                Operation::NodePairCancel { id } => {
                    self.node_pairs.cancel(id, &mut context.resources)?;
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }));
                }
                Operation::NodePairRecycle { id } => {
                    self.node_pairs.recycle(id)?;
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }));
                }
                Operation::ConfigurationActivate {
                    node_pairs,
                    topology,
                    graph,
                    graph_epoch,
                    topology_epoch,
                } => {
                    let (graph_epoch, topology_epoch) = self.activate_configuration(
                        node_pairs,
                        topology,
                        graph,
                        graph_epoch,
                        topology_epoch,
                        &mut context.graphs,
                        &mut context.resources,
                    )?;
                    return Ok(Disposition::Complete(Reply::ConfigurationActivated {
                        node_pairs,
                        topology,
                        graph,
                        graph_epoch,
                        topology_epoch,
                    }));
                }
                Operation::GraphStatus { id } => {
                    return Ok(Disposition::Complete(Reply::GraphStatus {
                        id,
                        status: context.graphs.status(id)?,
                    }));
                }
                Operation::GraphActivate {
                    id,
                    graph_epoch,
                    topology_epoch,
                } => {
                    let (graph_epoch, topology_epoch) = self.activate_graph(
                        id,
                        None,
                        graph_epoch,
                        topology_epoch,
                        &mut context.graphs,
                    )?;
                    return Ok(Disposition::Complete(Reply::GraphActivated {
                        id,
                        graph_epoch,
                        topology_epoch,
                    }));
                }
                Operation::TopologyGraphActivate {
                    id,
                    graph,
                    graph_epoch,
                    topology_epoch,
                } => {
                    let (graph_epoch, topology_epoch) = self.activate_graph(
                        graph,
                        Some(id),
                        graph_epoch,
                        topology_epoch,
                        &mut context.graphs,
                    )?;
                    return Ok(Disposition::Complete(Reply::TopologyGraphActivated {
                        id,
                        graph,
                        graph_epoch,
                        topology_epoch,
                    }));
                }
                Operation::TopologyBegin { slot } => {
                    Some(self.topologies.begin(slot, context.payload)?)
                }
                Operation::TopologyAppend { id, first_record } => {
                    self.topologies.append(id, first_record, context.payload)?;
                    Some(id)
                }
                Operation::TopologyStatus { id } => Some(id),
                Operation::TopologyValidate { id } => {
                    let progress = self.topologies.validate(id)?;
                    return Ok(Disposition::Complete(Reply::TopologyProgress {
                        id,
                        progress,
                    }));
                }
                Operation::TopologyCancel { id } => {
                    self.topologies.cancel(id, &mut self.resolver)?;
                    Some(id)
                }
                Operation::TopologyRecycle { id } => {
                    self.topologies.recycle(id)?;
                    Some(id)
                }
                Operation::TopologyActivate {
                    id,
                    graph_epoch,
                    topology_epoch,
                } => {
                    if graph_epoch != context.graph_epoch {
                        return Err(Error::InvalidState.into());
                    }
                    let topology_epoch =
                        self.topologies
                            .activate(id, topology_epoch, &mut self.resolver)?;
                    return Ok(Disposition::Complete(Reply::TopologyActivated {
                        id,
                        graph_epoch,
                        topology_epoch,
                    }));
                }
                _ => None,
            };
            if let Some(id) = topology_status {
                return Ok(Disposition::Complete(Reply::TopologyStatus {
                    id,
                    status: self.topologies.status(id)?,
                }));
            }
            match operation {
                Operation::ProgramActivate {
                    id,
                    target,
                    graph_epoch,
                    topology_epoch,
                }
                | Operation::ProgramGraphActivate {
                    id,
                    target,
                    graph_epoch,
                    topology_epoch,
                    ..
                } => {
                    if graph_epoch != context.graph_epoch {
                        return Err(Error::InvalidState.into());
                    }
                    let graph = if let Operation::ProgramGraphActivate { graph, .. } = operation {
                        let permit = context.graphs.prepare_activation(graph, graph_epoch)?;
                        self.validate_graph(permit.candidate())?;
                        Some(graph)
                    } else {
                        None
                    };
                    let slot = self.pending_slot(context.recovery)?;
                    let request = self.topologies.stage_program(
                        id,
                        target,
                        topology_epoch,
                        context.token.sequence(),
                        context.requested_frame,
                        &mut self.resolver,
                    )?;
                    self.pending[slot] = Some(Pending {
                        token: context.token,
                        kind: PendingKind::Program {
                            id,
                            request,
                            graph_epoch,
                            graph,
                        },
                        forwarded: false,
                    });
                    Ok(Disposition::Pending)
                }
                Operation::Cancel { sequence } => {
                    if let Some(pending) = self
                        .pending
                        .iter()
                        .flatten()
                        .find(|p| p.token.sequence() == sequence)
                        .copied()
                    {
                        if let PendingKind::Transition { request, .. }
                        | PendingKind::Program { request, .. } = pending.kind
                        {
                            self.resolver.cancel(request)?;
                        } else {
                            return Err(Error::InvalidState.into());
                        }
                        Ok(Disposition::Complete(Reply::Cancelled {
                            sequence,
                            delivered: true,
                        }))
                    } else {
                        context.receipts.cancel_queued(
                            sequence,
                            Reply::Cancelled {
                                sequence,
                                delivered: false,
                            },
                        )?;
                        Ok(Disposition::Complete(Reply::Cancelled {
                            sequence,
                            delivered: false,
                        }))
                    }
                }
                Operation::Gain { target, value } => {
                    self.row(target)?;
                    if !value.is_finite() {
                        return Err(Error::InvalidParameter.into());
                    }
                    self.rows[target.slot() as usize]
                        .as_mut()
                        .expect("row")
                        .binding
                        .gain = value;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::Release { export } => {
                    self.ledger.release_export(&mut context.resources, export)?;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::Retire { media } => {
                    self.ledger.view(media)?;
                    self.ledger.retire(&mut context.resources, media)?;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::MidiInitial { export, channel } => {
                    if channel >= 16 {
                        return Err(Error::InvalidShape.into());
                    }
                    let source = self
                        .ledger
                        .exported(export)?
                        .view
                        .midi
                        .ok_or(Error::InvalidShape)?;
                    let initial = context.resources.render().midi_seek(source.index, 0)?;
                    Ok(Disposition::Complete(Reply::MidiInitial {
                        channel,
                        state: *initial.state.channel(channel).ok_or(Error::InvalidShape)?,
                        system_reset_seen: initial.state.system_reset_seen(),
                    }))
                }
                Operation::Cursor { export, midi } => {
                    let view = self.ledger.exported(export)?.view;
                    let source =
                        if midi { view.midi } else { view.audio }.ok_or(Error::InvalidShape)?;
                    Ok(Disposition::Complete(Reply::Cursor(
                        context.resources.cursor(source.snapshot.id)?,
                    )))
                }
                Operation::State { .. } | Operation::Export { .. } => {
                    let slot = self.pending_slot(context.recovery)?;
                    self.pending[slot] = Some(Pending {
                        token: context.token,
                        kind: PendingKind::Query(operation),
                        forwarded: false,
                    });
                    Ok(Disposition::Pending)
                }
                Operation::Transition { target, timing, .. }
                | Operation::Select { target, timing, .. } => {
                    let slot = self.pending_slot(context.recovery)?;
                    let (transition, pin) = match operation {
                        Operation::Select {
                            media,
                            preserve_position,
                            ..
                        } => {
                            let view = self.ledger.view(media)?;
                            if view.description.owner != target {
                                return Err(Error::InvalidId.into());
                            }
                            (
                                Transition::Replace {
                                    length: view.description.frames,
                                    revision: view.description.revision,
                                    preserve_position,
                                },
                                Some(self.ledger.pin(media)?),
                            )
                        }
                        Operation::Transition { transition, .. } => {
                            if matches!(
                                transition,
                                Transition::Program | Transition::Replace { .. }
                            ) {
                                return Err(Error::InvalidEvent.into());
                            }
                            (transition, None)
                        }
                        _ => unreachable!(),
                    };
                    let request = match self.resolver.submit(crate::Request {
                        target,
                        sequence: context.token.sequence(),
                        at: context.requested_frame,
                        timing,
                        transition,
                    }) {
                        Ok(request) => request,
                        Err(error) => {
                            if let Some(pin) = pin {
                                self.ledger.release_pin(&mut context.resources, pin)?;
                            }
                            return Err(error.into());
                        }
                    };
                    self.pending[slot] = Some(Pending {
                        token: context.token,
                        kind: PendingKind::Transition { request, pin },
                        forwarded: false,
                    });
                    Ok(Disposition::Pending)
                }
                Operation::NodePairSnapshot { .. }
                | Operation::NodePairSourceStatus { .. }
                | Operation::NodePairSourceValidate { .. }
                | Operation::NodePairSourceRelease { .. }
                | Operation::NodePairBegin { .. }
                | Operation::NodePairAppend { .. }
                | Operation::NodePairStatus { .. }
                | Operation::NodePairCancel { .. }
                | Operation::NodePairRecycle { .. }
                | Operation::ConfigurationActivate { .. }
                | Operation::GraphStatus { .. }
                | Operation::GraphActivate { .. }
                | Operation::TopologyGraphActivate { .. }
                | Operation::TopologyBegin { .. }
                | Operation::TopologyAppend { .. }
                | Operation::TopologyValidate { .. }
                | Operation::TopologyStatus { .. }
                | Operation::TopologyCancel { .. }
                | Operation::TopologyRecycle { .. }
                | Operation::TopologyActivate { .. } => unreachable!("topology operation handled"),
            }
        })();
        Ok(result.unwrap_or_else(|error| Disposition::Complete(Reply::Failed(error))))
    }
    fn plan(&mut self, mut context: PlanContext<'_, Operation, Reply>) -> Result<Frame, Error> {
        self.graph_epoch = context.graph_epoch;
        self.topologies.callback(context.callback_start);
        if context.frame != self.resolver.frame() {
            return Err(Error::InvalidFrame);
        }
        self.failures.clear();
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
            self.cancel_intervals(&mut context.resources)?;
            let interval = self
                .resolver
                .preview(context.ceiling, &self.failures)
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
            if !self.preflight(context.frame, interval.end, &mut context.resources)? {
                continue;
            }
            self.settle(context.frame, &mut context.resources)?;
            self.resolver.commit().map_err(runtime_error)?;
            if let Some(permit) = permit {
                self.graph_epoch = permit.commit();
            }
            self.complete(context.frame, &mut context.resources, &mut context.receipts)?;
            self.observer.boundary(
                context.frame,
                self.resolver.decisions().map_err(runtime_error)?,
                self.resolver.events(),
                self.resolver.dropped_events(),
            );
            for row in self.rows.iter_mut().flatten() {
                row.seen_playback = false;
                row.seen_capture = false;
            }
            return Ok(interval.end);
        }
    }
    fn finish(
        &mut self,
        mut context: FinishContext<'_, Observed, Operation, Reply>,
    ) -> Result<(), Error> {
        for observation in context.observations {
            let (target, capture) = match &observation.value {
                Observed::Playback { target, .. } => (*target, false),
                Observed::Input { target, .. } | Observed::NoMidiInput { target } => {
                    (*target, true)
                }
            };
            let row = self.row(target)?;
            let slot = target.slot() as usize;
            if observation.producer
                != if capture {
                    row.binding.capture
                } else {
                    row.binding.playback
                }
                || if capture {
                    row.seen_capture
                } else {
                    row.seen_playback
                }
            {
                return Err(Error::InvalidId);
            }
            match &observation.value {
                Observed::Input { state, .. } => {
                    if !row.binding.midi {
                        return Err(Error::InvalidShape);
                    }
                    self.inputs[slot] = *state;
                }
                Observed::NoMidiInput { .. } => {
                    if row.binding.midi {
                        return Err(Error::InvalidShape);
                    }
                }
                Observed::Playback { overflow, .. } => {
                    if *overflow {
                        self.rows[slot].as_mut().expect("row").overflows =
                            row.overflows.saturating_add(1);
                    }
                }
            }
            let row = self.rows[slot].as_mut().expect("row");
            if capture {
                row.seen_capture = true;
            } else {
                row.seen_playback = true;
            }
        }
        for row in self.rows.iter_mut().flatten().filter(|row| row.assigned) {
            if !row.seen_playback || !row.seen_capture {
                return Err(Error::InvalidState);
            }
            if let Some(interval) = row.interval {
                context.resources.capture().commit(interval)?;
                row.interval = None;
            }
            row.reset_input = false;
        }
        self.resolver
            .finish(context.interval.end)
            .map_err(runtime_error)?;
        for index in 0..self.pending.len() {
            if let Some(pending) = self.pending[index].filter(|p| p.forwarded) {
                if let PendingKind::Transition { request, pin } = pending.kind {
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
