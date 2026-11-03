//! Opt-in feed-forward callback plans with independently placed media consumers.
use super::{runtime_error, Binding, Failure, Observed, Settings, View};
use crate::{
    audio, media, midi, Decision, LoopId, LoopKind, PreparedTopology, Request, RequestId, Resolver,
    State, Timing, Transition,
};
use alloc::{boxed::Box, vec, vec::Vec};
use rt_runtime::{
    buffer::MidiBuffer,
    graph::{PortCounts, PreparedPlan},
    midi_state::MidiState,
    planner::*,
    processor::{Cadence, ProcessBlock, Processor},
    resource::*,
    schedule::ReceiptToken,
    Error, Frame, NodeId, PreparedConfig, Revision,
};

#[derive(Clone, Copy)]
pub struct CallbackDecision {
    pub media: View,
    pub capture_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operation {
    Transition {
        target: LoopId,
        at: Frame,
        timing: Timing,
        transition: Transition,
    },
    Capture {
        target: LoopId,
        start: Frame,
        end: Frame,
    },
    State {
        target: LoopId,
    },
    Export {
        target: LoopId,
        latest: bool,
    },
    Release {
        export: media::ExportId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reply {
    Transition(crate::Completion),
    Captured {
        target: LoopId,
        start: Frame,
        end: Frame,
        media: media::MediaId,
    },
    State {
        frame: Frame,
        state: State,
    },
    Export {
        id: media::ExportId,
        media: media::Export,
    },
    Released,
}
#[derive(Clone, Copy)]
struct Capture {
    start: Frame,
    end: Frame,
    token: ReceiptToken,
    media: media::MediaId,
    take: Option<CaptureTakeId>,
    interval: Option<CaptureIntervalId>,
}
struct Row {
    binding: Binding,
    group: CaptureGroupId,
    capture: Option<Capture>,
    revision: u64,
    frozen: [Option<Snapshot>; 2],
    seen_playback: bool,
    seen_capture: bool,
}

pub struct CallbackPlanner {
    settings: Settings,
    resolver: Resolver,
    ledger: media::Ledger,
    rows: Box<[Row]>,
    inputs: Box<[MidiState]>,
    current: Box<[CallbackDecision]>,
    spans: Box<[DecisionSpan<CallbackDecision>]>,
    lengths: Box<[usize]>,
    span_limit: usize,
    bindings: Box<[DecisionBinding]>,
    pending: Box<[Option<(RequestId, ReceiptToken)>]>,
    callbacks: u64,
    timing_intervals: u64,
}
pub struct Prepared {
    pub planner: CallbackPlanner,
    pub nodes: Vec<(NodeId, CallbackNode)>,
}
impl CallbackPlanner {
    /// Imports transfer their snapshot leases to the returned planner on success.
    /// `span_limit` must cover every possible one-frame boundary in an admitted callback.
    pub fn prepare(
        config: PreparedConfig,
        topology: PreparedTopology,
        resources: &mut ResourceStore,
        settings: Settings,
        mut bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        span_limit: usize,
    ) -> Result<Prepared, Failure> {
        if resources.config() != config
            || span_limit != config.limits().max_frames
            || bindings.is_empty()
            || settings.normal_requests == 0
        {
            return Err(Error::InvalidConfig.into());
        }
        if bindings.len() != topology.nodes().len()
            || topology
                .nodes()
                .iter()
                .any(|node| node.kind != LoopKind::Primitive)
        {
            return Err(Error::InvalidGraph.into());
        }
        bindings.sort_unstable_by_key(|b| b.target);
        let storage = bindings
            .len()
            .checked_mul(span_limit)
            .and_then(|n| n.checked_mul(size_of::<DecisionSpan<CallbackDecision>>()))
            .ok_or(Error::Capacity)?;
        if storage > i32::MAX as usize {
            return Err(Error::Capacity.into());
        }
        let resolver = Resolver::prepare(topology)?;
        let mut ledger = media::Ledger::prepare(
            resolver.topology().limits().loop_slots,
            settings.media_slots,
            settings.normal_requests,
            settings.export_slots,
        )?;
        let mut rows = Vec::with_capacity(bindings.len());
        let mut nodes = Vec::with_capacity(bindings.len() * 2);
        let mut decision_bindings = Vec::with_capacity(bindings.len() * 2);
        let mut current = Vec::with_capacity(bindings.len());
        for (slot, binding) in bindings.into_iter().enumerate() {
            let spec = resolver.topology().node(binding.target)?;
            if rows
                .iter()
                .any(|row: &Row| row.binding.target == binding.target)
                || binding.channels == 0
                || !binding.gain.is_finite()
                || binding.playback == binding.capture
            {
                return Err(Error::InvalidShape.into());
            }
            if binding.midi && binding.packet_bytes < binding.midi_bytes.max(3) {
                return Err(Error::Capacity.into());
            }
            ledger.bind(binding.target)?;
            let mut members = Vec::with_capacity(2);
            let resource = resources.create()?;
            members.push(CaptureMember {
                resource,
                format: Format::Audio {
                    channels: binding.channels.try_into().map_err(|_| Error::Capacity)?,
                },
                midi_events: 0,
                midi_bytes: 0,
            });
            if binding.midi {
                let resource = resources.create()?;
                members.push(CaptureMember {
                    resource,
                    format: Format::Midi,
                    midi_events: binding.midi_events,
                    midi_bytes: binding.midi_bytes,
                });
            }
            let group = resources.bind_capture_group(&members)?;
            current.push(CallbackDecision {
                media: View {
                    decision: Decision {
                        state: State::new(binding.target, spec.length),
                        playback_discontinuity: false,
                        origin: None,
                    },
                    audio: None,
                    midi: None,
                    capture: None,
                    gain: binding.gain,
                    balance: 0.0,
                    smoothing: binding.smoothing,
                    playback_node: binding.playback,
                    capture_node: binding.capture,
                    reset_input: false,
                },
                capture_enabled: false,
            });
            for consumer in [binding.playback, binding.capture] {
                decision_bindings.push(DecisionBinding {
                    planner: settings.planner,
                    consumer,
                    decision_slot: slot,
                });
            }
            nodes.push((
                binding.playback,
                CallbackNode::Playback(Playback::prepare(binding, config)?),
            ));
            nodes.push((
                binding.capture,
                CallbackNode::Capture(CaptureNode::prepare(binding, config)?),
            ));
            rows.push(Row {
                binding,
                group,
                capture: None,
                revision: 0,
                frozen: [None; 2],
                seen_playback: false,
                seen_capture: false,
            });
        }
        for input in imports {
            let target = input.description.owner;
            let row = rows
                .iter()
                .find(|row| row.binding.target == target)
                .ok_or(Error::InvalidId)?;
            if input.description.frames != resolver.state(target)?.content_length
                || input.description.channels as usize != row.binding.channels
                || input.description.midi != row.binding.midi
                || ledger.playback(target)?.is_some()
            {
                return Err(Error::InvalidShape.into());
            }
            let id = ledger
                .adopt(&mut resources.io(), input)
                .map_err(|e| e.error)?;
            ledger.set_playback(&mut resources.io(), target, Some(id))?;
            ledger.retire(&mut resources.io(), id)?;
        }
        for row in &rows {
            if ledger.playback(row.binding.target)?.is_none() {
                return Err(Error::InvalidShape.into());
            }
        }
        decision_bindings.sort_unstable_by_key(|binding| binding.consumer);
        let initial = DecisionSpan {
            interval: Interval {
                start: Frame(0),
                end: Frame(0),
            },
            value: current[0],
        };
        let count = rows.len();
        let mut spans = vec![initial; count * span_limit].into_boxed_slice();
        for span in &mut spans {
            *core::hint::black_box(span) = initial;
        }
        Ok(Prepared {
            planner: Self {
                settings,
                resolver,
                ledger,
                rows: rows.into_boxed_slice(),
                inputs: vec![MidiState::default(); count].into_boxed_slice(),
                current: current.into_boxed_slice(),
                spans,
                lengths: vec![0; count].into_boxed_slice(),
                span_limit,
                bindings: decision_bindings.into_boxed_slice(),
                pending: vec![None; settings.normal_requests].into_boxed_slice(),
                callbacks: 0,
                timing_intervals: 0,
            },
            nodes,
        })
    }
    pub fn bindings(&self) -> &[DecisionBinding] {
        &self.bindings
    }
    pub fn frame(&self) -> Frame {
        self.resolver.frame()
    }
    pub fn state(&self, target: LoopId) -> Result<State, crate::Error> {
        self.resolver.state(target)
    }
    pub fn work_counts(&self) -> (u64, u64) {
        (self.callbacks, self.timing_intervals)
    }
    pub fn plan_storage_bytes(&self) -> usize {
        self.spans.len() * size_of::<DecisionSpan<CallbackDecision>>()
    }
    pub fn ledger(&self) -> &media::Ledger {
        &self.ledger
    }
    fn slot(&self, target: LoopId) -> Result<usize, Error> {
        self.rows
            .binary_search_by_key(&target, |row| row.binding.target)
            .map_err(|_| Error::InvalidId)
    }
    fn node_slot(&self, node: NodeId) -> Option<(usize, bool)> {
        self.rows.iter().enumerate().find_map(|(slot, row)| {
            if row.binding.playback == node {
                Some((slot, false))
            } else if row.binding.capture == node {
                Some((slot, true))
            } else {
                None
            }
        })
    }
    fn range(&self, slot: usize) -> core::ops::Range<usize> {
        slot * self.span_limit..slot * self.span_limit + self.lengths[slot]
    }
    fn capture_range(&self, slot: usize) -> Option<Interval> {
        let mut active = self.spans[self.range(slot)]
            .iter()
            .filter(|span| span.value.capture_enabled);
        let first = active.next()?;
        Some(Interval {
            start: first.interval.start,
            end: active.last().unwrap_or(first).interval.end,
        })
    }
    fn abandon(&mut self, slot: usize, io: &mut PlannerResourceIo<'_>) -> Result<(), Error> {
        let Some(mut capture) = self.rows[slot].capture else {
            return Ok(());
        };
        if let Some(interval) = capture.interval.take() {
            io.capture().cancel_interval(interval)?;
        }
        if let Some(take) = capture.take {
            let info = io.capture().info(self.rows[slot].group)?;
            if matches!(
                info.phase,
                CapturePhase::Prepared | CapturePhase::PreparedReserved
            ) {
                io.capture().cancel_take(take)?;
            } else {
                let frame = info.next_frame.ok_or(Error::InvalidState)?;
                let mut snapshots = [None; 2];
                let count = 1 + usize::from(self.rows[slot].binding.midi);
                io.subrange(frame, frame)?.capture().freeze(
                    take,
                    frame,
                    Some(Error::Incomplete),
                    &mut snapshots[..count],
                )?;
                for snapshot in snapshots.into_iter().flatten() {
                    io.release_snapshot(snapshot.id)?;
                }
            }
        }
        for snapshot in &mut self.rows[slot].frozen {
            if let Some(snapshot) = snapshot.take() {
                io.release_snapshot(snapshot.id)?;
            }
        }
        self.ledger.retire(io, capture.media)?;
        self.rows[slot].capture = None;
        Ok(())
    }
    /// Release leases and idle capture groups after callbacks have stopped.
    pub fn release_all(&mut self, resources: &mut ResourceStore) -> Result<(), Error> {
        let at = self.frame();
        for slot in 0..self.rows.len() {
            self.abandon(slot, &mut resources.planner_io(Frame(0), at))?;
        }
        self.ledger.release_all(&mut resources.io())?;
        for row in &self.rows {
            resources
                .planner_io(at, at)
                .capture()
                .release_group(row.group)?;
        }
        Ok(())
    }
}
impl Planner for CallbackPlanner {
    type Decision = CallbackDecision;
    type Observation = Observed;
    type Operation = Operation;
    type Result = Reply;
    fn mode(&self) -> PlanningMode {
        PlanningMode::Callback
    }
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: self.settings.planner,
            decision_slots: self.rows.len(),
            observation_capacity: self.rows.len() * 2,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[CallbackDecision] {
        &self.current
    }
    fn spans(&self, slot: usize) -> &[DecisionSpan<CallbackDecision>] {
        self.lengths
            .get(slot)
            .map_or(&[], |_| &self.spans[self.range(slot)])
    }
    fn admit_graph(&self, plan: &PreparedPlan) -> Result<(), Error> {
        if plan.planning().bindings != *self.bindings {
            return Err(Error::InvalidGraph);
        }
        Ok(())
    }
    fn admit_processor_change(&self, node: NodeId) -> Result<(), Error> {
        if self.node_slot(node).is_some() {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
    fn operation(
        &mut self,
        operation: Operation,
        mut c: OperationContext<'_, Operation, Reply>,
    ) -> Result<Disposition<Reply>, Error> {
        match operation {
            Operation::Capture { target, start, end } => {
                let slot = self.slot(target)?;
                if start < c.frame || end <= start {
                    return Err(Error::InvalidFrame);
                }
                if self.rows[slot].capture.is_some() {
                    return Err(Error::Busy);
                }
                let revision = self.rows[slot]
                    .revision
                    .checked_add(1)
                    .ok_or(Error::Exhausted)?;
                let binding = self.rows[slot].binding;
                let media = self.ledger.reserve(media::Description {
                    owner: target,
                    revision: Revision(revision),
                    frames: 0,
                    channels: binding.channels as u32,
                    midi: binding.midi,
                })?;
                self.rows[slot].revision = revision;
                self.rows[slot].capture = Some(Capture {
                    start,
                    end,
                    token: c.token,
                    media,
                    take: None,
                    interval: None,
                });
                Ok(Disposition::Pending)
            }
            Operation::Transition {
                target,
                at,
                timing,
                transition,
            } => {
                self.slot(target)?;
                if !matches!(
                    transition,
                    Transition::Play { .. }
                        | Transition::Seek { .. }
                        | Transition::Stop
                        | Transition::PlayIteration { .. }
                        | Transition::SeekIteration { .. }
                ) {
                    return Err(Error::InvalidState);
                }
                let pending = self
                    .pending
                    .iter_mut()
                    .find(|entry| entry.is_none())
                    .ok_or(Error::Capacity)?;
                let request = self
                    .resolver
                    .submit(Request {
                        target,
                        sequence: c.token.sequence(),
                        at,
                        timing,
                        transition,
                    })
                    .map_err(runtime_error)?;
                *pending = Some((request, c.token));
                Ok(Disposition::Pending)
            }
            Operation::State { target } => Ok(Disposition::Complete(Reply::State {
                frame: c.frame,
                state: self.resolver.state(target).map_err(runtime_error)?,
            })),
            Operation::Export { target, latest } => {
                self.slot(target)?;
                let media = if latest {
                    self.ledger.latest(target)?
                } else {
                    self.ledger.playback(target)?
                }
                .ok_or(Error::InvalidState)?;
                let id = self.ledger.export(media, c.frame)?;
                Ok(Disposition::Complete(Reply::Export {
                    id,
                    media: self.ledger.exported(id)?,
                }))
            }
            Operation::Release { export } => {
                self.ledger.release_export(&mut c.resources, export)?;
                Ok(Disposition::Complete(Reply::Released))
            }
        }
    }
    fn plan(&mut self, c: PlanContext<'_, Operation, Reply>) -> Result<Frame, Error> {
        let frames = usize::try_from(
            c.ceiling
                .0
                .checked_sub(c.frame.0)
                .ok_or(Error::InvalidFrame)?,
        )
        .map_err(|_| Error::Capacity)?;
        if c.frame != c.callback_start || c.frame != self.frame() {
            return Err(Error::InvalidFrame);
        }
        // Check the worst case before advancing any timing authority.
        if frames > self.span_limit {
            return Err(Error::Capacity);
        }
        self.lengths.fill(0);
        for row in &mut self.rows {
            row.seen_playback = false;
            row.seen_capture = false;
        }
        while self.frame() < c.ceiling {
            let at = self.frame();
            let mut ceiling = c.ceiling;
            for row in &self.rows {
                if let Some(capture) = row.capture {
                    for boundary in [capture.start, capture.end] {
                        if boundary > at {
                            ceiling = ceiling.min(boundary);
                        }
                    }
                }
            }
            let interval = self.resolver.preview(ceiling, &[]).map_err(runtime_error)?;
            for slot in 0..self.rows.len() {
                let row = &self.rows[slot];
                let decision = *self
                    .resolver
                    .decisions()
                    .map_err(runtime_error)?
                    .iter()
                    .find(|decision| decision.state.id == row.binding.target)
                    .ok_or(Error::InvalidId)?;
                let source = self.ledger.view(
                    self.ledger
                        .playback(row.binding.target)?
                        .ok_or(Error::InvalidState)?,
                )?;
                let enabled = decision.state.plays();
                let value = CallbackDecision {
                    media: View {
                        decision,
                        audio: if enabled {
                            source.audio.map(|source| source.index)
                        } else {
                            None
                        },
                        midi: if enabled {
                            source.midi.map(|source| source.index)
                        } else {
                            None
                        },
                        capture: None,
                        ..self.current[slot].media
                    },
                    capture_enabled: row
                        .capture
                        .is_some_and(|capture| capture.start <= at && at < capture.end),
                };
                let index = slot * self.span_limit + self.lengths[slot];
                self.spans[index] = DecisionSpan {
                    interval: Interval {
                        start: interval.start,
                        end: interval.end,
                    },
                    value,
                };
                self.lengths[slot] += 1;
            }
            self.resolver.commit().map_err(runtime_error)?;
            self.resolver.finish(interval.end).map_err(runtime_error)?;
            self.timing_intervals = self.timing_intervals.saturating_add(1);
        }
        for slot in 0..self.rows.len() {
            self.current[slot] = self.spans[slot * self.span_limit].value;
        }
        self.callbacks = self.callbacks.saturating_add(1);
        Ok(c.ceiling)
    }
    fn before_consumer(
        &mut self,
        mut c: ConsumerContext<'_, Operation, Reply>,
    ) -> Result<(), Error> {
        let Some((slot, true)) = self.node_slot(c.node) else {
            return Ok(());
        };
        let Some(range) = self.capture_range(slot) else {
            return Ok(());
        };
        let binding = self.rows[slot].binding;
        let mut capture = self.rows[slot].capture.ok_or(Error::InvalidState)?;
        let result = (|| {
            let mut io = c.resources.subrange(range.start, c.interval.end)?;
            if capture.take.is_none() {
                if binding.midi {
                    let input = c.midi_inputs.first().ok_or(Error::InvalidShape)?;
                    if input.is_recovery() {
                        return Err(Error::Full);
                    }
                    for event in 0..input.len() {
                        let (offset, bytes) = input.get(event).ok_or(Error::InvalidEvent)?;
                        if c.interval.start.checked_add(offset as usize)? >= range.start {
                            break;
                        }
                        self.inputs[slot].apply_packet(bytes, binding.packet_bytes)?;
                    }
                }
                capture.take = Some(io.capture().prepare(
                    self.rows[slot].group,
                    if binding.midi {
                        core::slice::from_ref(&self.inputs[slot])
                    } else {
                        &[]
                    },
                )?);
                self.rows[slot].capture = Some(capture);
                let take = capture.take.expect("prepared take");
                capture.interval = Some(io.capture().reserve_prepared(
                    take,
                    range.start,
                    (range.end.0 - range.start.0) as usize,
                )?);
                self.rows[slot].capture = Some(capture);
                io.capture().activate(take, range.start)?;
            } else {
                if binding.midi && c.midi_inputs.first().is_some_and(MidiBuffer::is_recovery) {
                    return Err(Error::Full);
                }
                capture.interval = Some(io.capture().reserve(
                    capture.take.expect("active take"),
                    range.start,
                    (range.end.0 - range.start.0) as usize,
                )?);
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.abandon(slot, &mut c.resources)?;
            c.receipts.complete(capture.token, Err(error))?;
            for span in
                &mut self.spans[slot * self.span_limit..slot * self.span_limit + self.lengths[slot]]
            {
                span.value.capture_enabled = false;
                span.value.media.capture = None;
            }
        } else {
            self.rows[slot].capture = Some(capture);
            for span in
                &mut self.spans[slot * self.span_limit..slot * self.span_limit + self.lengths[slot]]
            {
                span.value.media.capture = if span.value.capture_enabled {
                    capture.interval
                } else {
                    None
                };
            }
        }
        Ok(())
    }
    fn after_consumer(
        &mut self,
        mut c: ConsumerFinishContext<'_, Observed, Operation, Reply>,
    ) -> Result<(), Error> {
        let Some((slot, is_capture)) = self.node_slot(c.node) else {
            return Ok(());
        };
        let target = self.rows[slot].binding.target;
        let observed = c
            .observations
            .first()
            .filter(|_| c.observations.len() == 1)
            .ok_or_else(|| c.result.err().unwrap_or(Error::Incomplete))?;
        if observed.producer != c.node {
            return Err(Error::InvalidId);
        }
        match observed.value {
            Observed::Playback {
                target: observed, ..
            } if !is_capture && observed == target => self.rows[slot].seen_playback = true,
            Observed::Input {
                target: observed,
                state,
            } if is_capture && observed == target && self.rows[slot].binding.midi => {
                self.inputs[slot] = state;
                self.rows[slot].seen_capture = true;
            }
            Observed::NoMidiInput { target: observed }
                if is_capture && observed == target && !self.rows[slot].binding.midi =>
            {
                self.rows[slot].seen_capture = true
            }
            _ => return Err(Error::InvalidState),
        }
        if !is_capture {
            return if c.result == Err(Error::Full) {
                Ok(())
            } else {
                c.result
            };
        }
        let Some(mut capture) = self.rows[slot].capture else {
            return c.result;
        };
        let Some(interval) = capture.interval else {
            return c.result;
        };
        if let Err(error) = c.result {
            self.abandon(slot, &mut c.resources)?;
            c.receipts.complete(capture.token, Err(error))?;
            return Ok(());
        }
        let range = self.capture_range(slot).ok_or(Error::InvalidState)?;
        let mut io = c.resources.subrange(range.end, range.end)?;
        io.capture().commit(interval)?;
        capture.interval = None;
        self.rows[slot].capture = Some(capture);
        if range.end == capture.end {
            let binding = self.rows[slot].binding;
            io.capture().freeze(
                capture.take.ok_or(Error::InvalidState)?,
                range.end,
                None,
                &mut self.rows[slot].frozen[..1 + usize::from(binding.midi)],
            )?;
            self.rows[slot]
                .capture
                .as_mut()
                .expect("active request")
                .take = None;
            let audio = self.rows[slot].frozen[0]
                .map(|snapshot| {
                    io.read_index(snapshot.id)
                        .map(|index| media::Source { snapshot, index })
                })
                .transpose()?;
            let midi = self.rows[slot].frozen[1]
                .map(|snapshot| {
                    io.read_index(snapshot.id)
                        .map(|index| media::Source { snapshot, index })
                })
                .transpose()?;
            self.ledger
                .publish(
                    &mut io,
                    capture.media,
                    media::Input {
                        description: media::Description {
                            owner: target,
                            revision: Revision(self.rows[slot].revision),
                            frames: capture.end.0 - capture.start.0,
                            channels: binding.channels as u32,
                            midi: binding.midi,
                        },
                        audio,
                        midi,
                    },
                )
                .map_err(|e| e.error)?;
            self.rows[slot].frozen = [None; 2];
            self.ledger
                .set_latest(&mut io, target, Some(capture.media))?;
            self.ledger.retire(&mut io, capture.media)?;
            self.rows[slot].capture = None;
            c.receipts.complete(
                capture.token,
                Ok(Reply::Captured {
                    target,
                    start: capture.start,
                    end: capture.end,
                    media: capture.media,
                }),
            )?;
        }
        Ok(())
    }
    fn finish(
        &mut self,
        mut c: FinishContext<'_, Observed, Operation, Reply>,
    ) -> Result<(), Error> {
        if self.rows.iter().any(|row| {
            !row.seen_playback
                || !row.seen_capture
                || row
                    .capture
                    .is_some_and(|capture| capture.interval.is_some())
        }) {
            return Err(Error::Incomplete);
        }
        for pending in &mut self.pending {
            if let Some((request, token)) = *pending {
                if let Some(completion) = self
                    .resolver
                    .take_completion(request)
                    .map_err(runtime_error)?
                {
                    c.receipts
                        .complete(token, Ok(Reply::Transition(completion)))?;
                    *pending = None;
                }
            }
        }
        Ok(())
    }
    fn abort(
        &mut self,
        mut c: FinishContext<'_, Observed, Operation, Reply>,
        error: Error,
    ) -> Result<(), Error> {
        for slot in 0..self.rows.len() {
            if let Some(capture) = self.rows[slot].capture {
                self.abandon(slot, &mut c.resources)?;
                c.receipts.complete(capture.token, Err(error))?;
            }
        }
        Ok(())
    }
}

pub enum CallbackNode {
    Playback(Playback),
    Capture(CaptureNode),
}
pub struct Playback {
    binding: Binding,
    audio: audio::AudioRenderer,
    midi: Option<midi::MidiRenderer>,
    scratch: MidiBuffer,
}
pub struct CaptureNode {
    binding: Binding,
    audio: audio::AudioCapture,
    midi: Option<midi::MidiCapture>,
    scratch: MidiBuffer,
}
impl Playback {
    fn prepare(binding: Binding, config: PreparedConfig) -> Result<Self, Error> {
        Ok(Self {
            binding,
            audio: audio::AudioRenderer::prepare(
                binding.channels,
                config.limits().max_frames,
                binding.smoothing,
            )?,
            midi: if binding.midi {
                Some(midi::MidiRenderer::prepare(
                    config.limits().max_frames,
                    binding.packet_bytes,
                )?)
            } else {
                None
            },
            scratch: MidiBuffer::new(
                config.limits().midi_events_per_port,
                config.limits().midi_bytes_per_port,
            )?,
        })
    }
}
impl CaptureNode {
    fn prepare(binding: Binding, config: PreparedConfig) -> Result<Self, Error> {
        Ok(Self {
            binding,
            audio: audio::AudioCapture::prepare(binding.channels, config.limits().max_frames)?,
            midi: if binding.midi {
                Some(midi::MidiCapture::prepare(
                    config.limits().max_frames,
                    binding.midi_events,
                    binding.midi_bytes,
                )?)
            } else {
                None
            },
            scratch: MidiBuffer::new(
                config.limits().midi_events_per_port,
                config.limits().midi_bytes_per_port,
            )?,
        })
    }
}
impl Processor<CallbackPlanner> for CallbackNode {
    fn cadence(&self) -> Cadence {
        Cadence::Callback
    }
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess {
            decision_required: true,
            observations: 1,
            operation_bytes: 0,
        }
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Playback(p) => PortCounts {
                audio_outputs: p.binding.channels,
                midi_outputs: usize::from(p.binding.midi),
                ..PortCounts::default()
            },
            Self::Capture(p) => PortCounts {
                audio_inputs: p.binding.channels,
                midi_inputs: usize::from(p.binding.midi),
                ..PortCounts::default()
            },
        }
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, CallbackPlanner>) -> Result<(), Error> {
        if b.spans.is_empty()
            || b.spans[0].interval.start != b.frame
            || b.spans.last().ok_or(Error::Incomplete)?.interval.end
                != b.frame.checked_add(b.frames)?
        {
            return Err(Error::Incomplete);
        }
        match self {
            Self::Playback(p) => {
                let mut error = None;
                for span in b.spans {
                    let view = span.value.media;
                    if view.playback_node != p.binding.playback
                        || view.decision.state.id != p.binding.target
                    {
                        return Err(Error::InvalidId);
                    }
                    let offset = (span.interval.start.0 - b.frame.0) as usize;
                    let frames = (span.interval.end.0 - span.interval.start.0) as usize;
                    p.audio.render_ports(
                        &mut b.resources.render(),
                        audio::Playback {
                            source: view.audio,
                            position: view.decision.state.position,
                            gain: view.gain,
                            balance: view.balance,
                            discontinuity: view.decision.playback_discontinuity,
                        },
                        &mut b.audio.subrange(offset, frames)?,
                    )?;
                    if let Some(midi) = &mut p.midi {
                        p.scratch.clear();
                        let report = midi.render(
                            &mut b.resources.render(),
                            midi::Playback {
                                source: view.midi,
                                position: view.decision.state.position,
                                discontinuity: view.decision.playback_discontinuity,
                            },
                            frames,
                            &mut p.scratch,
                        )?;
                        error = error.or(report.output_error);
                        for event in 0..p.scratch.len() {
                            let (at, packet) = p.scratch.get(event).ok_or(Error::InvalidEvent)?;
                            if let Err(e) =
                                b.midi_outputs[0].push(at + offset as u32, b.frames, packet)
                            {
                                error = Some(e);
                                break;
                            }
                        }
                    }
                }
                b.observations.push(Observed::Playback {
                    target: p.binding.target,
                    overflow: error.is_some(),
                })?;
                error.map_or(Ok(()), Err)
            }
            Self::Capture(p) => {
                let mut result = Ok(());
                let mut first = None;
                let mut end = None;
                for span in b.spans {
                    if span.value.capture_enabled {
                        first = first.or(Some(span));
                        end = Some(span.interval.end);
                    }
                }
                if let (Some(first), Some(end)) = (first, end) {
                    let offset = (first.interval.start.0 - b.frame.0) as usize;
                    let frames = (end.0 - first.interval.start.0) as usize;
                    result = p.audio.stage_ports(
                        &mut b.resources.render(),
                        first.value.media.capture.ok_or(Error::InvalidState)?,
                        0,
                        &b.audio.subrange(offset, frames)?,
                    );
                }
                if let Some(midi) = &mut p.midi {
                    midi.process(&mut b.resources.render(), &b.midi_inputs[0], b.frames, None)?;
                    if let (Some(first), Some(end)) = (first, end) {
                        let start = (first.interval.start.0 - b.frame.0) as usize;
                        let frames = (end.0 - first.interval.start.0) as usize;
                        p.scratch.clear();
                        for event in 0..b.midi_inputs[0].len() {
                            let (at, packet) =
                                b.midi_inputs[0].get(event).ok_or(Error::InvalidEvent)?;
                            if (start..start + frames).contains(&(at as usize)) {
                                p.scratch.push(at - start as u32, frames, packet)?;
                            }
                        }
                        if result.is_ok() {
                            result = b.resources.render().stage_midi(
                                first.value.media.capture.ok_or(Error::InvalidState)?,
                                1,
                                &p.scratch,
                            );
                        }
                    }
                    b.observations.push(Observed::Input {
                        target: p.binding.target,
                        state: *midi.state(),
                    })?;
                } else {
                    b.observations.push(Observed::NoMidiInput {
                        target: p.binding.target,
                    })?;
                }
                result
            }
        }
    }
}
