//! The application's real owning queues, backed by TP/SP/JP execution.
mod compile;
use crate::{
    image::{Image, Ramp, Value},
    loop_media,
    looping::LoopDevice,
    protocol,
};
use compile::{Compiled, Node, ONE, Parameter};
use rt_audio_math::{Q824, Sample};
use rt_runtime::{
    Epoch, Error, Frame, NodeId, PlannerId, PreparedConfig,
    executor::{Control, Diagnostics, Ingress, Retired},
    graph::{Catalog, activation::PreparedGraphs},
    host_buffer::{AudioIo, HostBuffer},
    schedule::{Action, ReceiptToken, ReceiptWriter, Receipts, Scheduler},
    transport::{Lane, Outcome},
};
use shoop_loops::{
    self as loops, LoopId, PreparedTopology, RequestId, State,
    runtime::{
        Failure, LoopPlanner, NodePairInitial, NodePairScope, NodePairStageId, NodePairStatus,
        Operation, Reply,
    },
    topology::staging::{RECORD_BYTES, StageId, Stager, Status},
};
use shoop_rt_engine::{
    EngineCommand, EngineNode, EngineOp,
    external::{Consumer, Observations, Preparation, SignalKind},
};
use std::time::{Duration, Instant};

/// The profile admitted by the RTL graph consumer.
pub fn profile_description() -> shoop_rt_engine::EngineDescription {
    use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
    EngineDescription {
        quantum: 32,
        host: HostPorts::numbered(2, 2, 0),
        segments: vec![Segment {
            strips: 1,
            loops: 1,
        }],
        buses: 1,
        bus_channels: 2,
        master_connections: vec![],
        telemetry_interval_frames: 0,
        reliable_credits: 32,
        bulk_credits: 2,
        pending_transitions: 16,
        ..EngineDescription::default()
    }
}

pub trait EngineDevice: LoopDevice {
    fn program(&mut self, image: &Image) -> Result<(), Error>;
    fn read_state(&mut self, address: u16) -> Result<Value, Error>;
    fn scratch(&mut self, buffer: u16) -> Result<Vec<i32>, Error>;
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Metrics {
    pub compilations: u64,
    pub activations: u64,
    pub compile_time: Duration,
    pub configure_time: Duration,
    pub instructions: usize,
    pub state_words: usize,
    pub buffers: u32,
}
struct Pending {
    token: ReceiptToken,
    kind: PendingKind,
    lane: Lane,
}
enum PendingKind {
    Transition { id: RequestId, requested: Frame },
    Query(Operation),
}
struct PairStage {
    scope: NodePairScope,
    generation: u32,
    status: NodePairStatus,
    initial: Option<NodePairInitial>,
}
impl PairStage {
    fn id(&self) -> Result<NodePairStageId, Error> {
        self.scope.stage(0, self.generation)
    }
    fn check(&self, id: NodePairStageId) -> Result<(), Error> {
        if self.id()? != id {
            Err(Error::InvalidId)
        } else {
            Ok(())
        }
    }
}
struct Core<D: EngineDevice> {
    device: D,
    nodes: Vec<Node>,
    graphs: PreparedGraphs,
    catalog: Catalog,
    topology: PreparedTopology,
    stager: Stager,
    stage_id: Option<StageId>,
    ready_topology: Option<PreparedTopology>,
    retired_topology: Option<PreparedTopology>,
    pairs: PairStage,
    topology_epoch: Epoch,
    image: Compiled,
    loaded: bool,
    origin: Frame,
    frame: Frame,
    observations: Observations,
    pending: Vec<Pending>,
    seed_media: Vec<loops::media::MediaId>,
    media: loop_media::Media,
    state: Option<State>,
    smoothing: loops::audio::Smoothing,
    loop_gain: f32,
    loop_balance: f32,
    metrics: Metrics,
    page_bytes: usize,
    read_budget: usize,
}
pub struct Adapter<D: EngineDevice> {
    core: Core<D>,
    config: PreparedConfig,
    ingress: Ingress<EngineNode, LoopPlanner, EngineOp, f32>,
    scheduler: Scheduler<Operation, Reply, EngineOp, ()>,
    buffer: Option<HostBuffer>,
    frame: Frame,
    fault: Option<Error>,
    /// Frames between storage reports, matching the native runtime's cadence.
    memory_interval: u64,
    /// Frame of the latest report attempt since activation.
    memory_frame: Option<Frame>,
}
impl<D: EngineDevice> Adapter<D> {
    pub fn prepare(parts: Preparation, mut device: D) -> Result<Consumer<Self>, Error> {
        let profile = profile_description();
        if parts.quantum.frames() != profile.quantum
            || parts.description.host.audio_inputs.len() != profile.host.audio_inputs.len()
            || parts.description.host.audio_outputs.len() != profile.host.audio_outputs.len()
            || !parts.description.host.midi_inputs.is_empty()
            || !parts.description.host.midi_outputs.is_empty()
            || parts.loop_limits.loop_slots != profile.loops()
        {
            return Err(Error::InvalidConfig);
        }
        let memory_interval = u64::from((parts.description.sample_rate / 5).max(1));
        let catalog = Catalog::new(
            parts.config,
            &parts
                .nodes
                .iter()
                .map(|node| node.spec())
                .collect::<Vec<_>>(),
        )?;
        let operation_bytes = rt_runtime::planner::Planner::operation_bytes(&parts.planner);
        let (control, ingress, scheduler) =
            Control::channel(parts.config, parts.quantum, operation_bytes, true)?;
        let nodes = parts
            .nodes
            .into_iter()
            .map(|node| {
                let id = node.spec().id;
                Node::new(id, node.into_processor())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let empty = loops::compile(parts.loop_limits, &loops::TopologyDescription::default())
            .map_err(|_| Error::InvalidConfig)?;
        let mut spare = Stager::prepare(parts.loop_limits).map_err(|_| Error::InvalidConfig)?;
        let mut header = [0; RECORD_BYTES];
        empty.encode_header(&mut header);
        let spare_id = spare.begin(&header).map_err(|_| Error::InvalidConfig)?;
        spare
            .validate(spare_id, 400)
            .map_err(|_| Error::InvalidConfig)?;
        let topology = spare.take(spare_id).map_err(|_| Error::InvalidConfig)?;
        let image = compile::lower(
            &rt_runtime::graph::PlanDescriptor::default(),
            &nodes,
            &topology,
            0,
        )
        .map_err(|_| Error::InvalidGraph)?;
        device.load(&image.image)?;
        let core = Core {
            device,
            nodes,
            graphs: PreparedGraphs::new(1, parts.config)?,
            catalog,
            stager: Stager::prepare(parts.loop_limits).map_err(|_| Error::InvalidConfig)?,
            topology,
            stage_id: None,
            ready_topology: None,
            retired_topology: None,
            pairs: PairStage {
                scope: NodePairScope::new()?,
                generation: 0,
                status: NodePairStatus::Idle,
                initial: None,
            },
            topology_epoch: Epoch(0),
            image,
            loaded: false,
            origin: Frame(0),
            frame: Frame(0),
            observations: parts.observations,
            pending: vec![],
            seed_media: parts.initial_media,
            media: loop_media::Media::with_channels(4, 2, loop_media::Encoding::Float32)?,
            state: None,
            smoothing: loops::audio::Smoothing::None,
            loop_gain: 1.,
            loop_balance: 0.,
            metrics: Metrics::default(),
            page_bytes: parts.config.limits().page_bytes,
            read_budget: parts.config.limits().budgets.transfer_bytes,
        };
        Ok(Consumer {
            control,
            execution: Self {
                core,
                config: parts.config,
                ingress,
                scheduler,
                buffer: Some(HostBuffer::prepare(parts.config, parts.quantum)?),
                frame: Frame(0),
                fault: None,
                memory_interval,
                memory_frame: None,
            },
            latency_frames: 32,
            capabilities: shoop_rt_protocol::Capabilities(0),
        })
    }
    pub fn metrics(&self) -> &Metrics {
        &self.core.metrics
    }
    pub fn frame(&self) -> Frame {
        self.frame
    }
    pub fn fault(&self) -> Option<Error> {
        self.fault
    }
    pub fn device_mut(&mut self) -> &mut D {
        &mut self.core.device
    }
    /// Query between quanta, without changing media, audio frames or program state.
    /// The report carries this adapter's frame; it is refused before activation.
    pub fn memory_report(&mut self) -> Result<crate::memory::MemoryReport, Error> {
        if let Some(error) = self.fault {
            return Err(error);
        }
        self.core.memory_report(self.frame)
    }
    /// Publishes storage pressure once after activation, then at the native interval.
    fn publish_memory(&mut self) {
        if !self.core.loaded
            || self
                .memory_frame
                .is_some_and(|last| self.frame.0.saturating_sub(last.0) < self.memory_interval)
        {
            return;
        }
        self.memory_frame = Some(self.frame);
        if let Ok(report) = self.memory_report() {
            self.core.observations.memory(&report);
        }
    }
    fn service(&mut self) -> Result<(), Error> {
        self.scheduler.begin_callback();
        self.ingress
            .begin_cycle(self.frame, self.core.graphs.epoch());
        while let Some(delivery) = self.ingress.next_operation() {
            let mut retired = None;
            let mut admission = None;
            let result = if let Some(error) = self.fault {
                Err(error)
            } else {
                match delivery.request {
                    EngineCommand::StageGraph {
                        slot,
                        expected_epoch,
                        plan,
                        id,
                    } => {
                        if let Some(owner) = plan.take() {
                            match self.core.graphs.stage(
                                *slot,
                                *expected_epoch,
                                owner,
                                &self.core.catalog,
                            ) {
                                Ok(stage) => {
                                    *id = Some(stage);
                                    Ok(())
                                }
                                Err((error, owner)) => {
                                    *plan = Some(owner);
                                    Err(error)
                                }
                            }
                        } else {
                            Err(Error::InvalidGraph)
                        }
                    }
                    EngineCommand::ReleaseGraph { id } => {
                        self.core.graphs.release(*id).map(|owner| {
                            retired = owner.map(Retired::Plan);
                        })
                    }
                    EngineCommand::Schedule {
                        at,
                        action,
                        payload,
                    } => {
                        let valid = match action {
                            Action::Planner { planner, .. } => {
                                *planner == PlannerId::from_parts(0, 1)?
                            }
                            Action::Processor { node, .. } => {
                                self.core.nodes.iter().any(|n| n.id == *node)
                            }
                            _ => false,
                        };
                        if !valid {
                            Err(Error::InvalidId)
                        } else {
                            let result = if delivery.lane == Lane::Recovery {
                                self.scheduler.admit_recovery(
                                    delivery.sequence,
                                    *at,
                                    self.frame,
                                    *action,
                                    payload,
                                )
                            } else {
                                self.scheduler.admit(
                                    delivery.sequence,
                                    *at,
                                    self.frame,
                                    *action,
                                    payload,
                                )
                            };
                            result.map(|value| admission = Some(value))
                        }
                    }
                    EngineCommand::Cancel { sequence } => {
                        self.scheduler.cancel(self.frame, *sequence)
                    }
                    EngineCommand::ResetMidi => Ok(()),
                    EngineCommand::Install { .. }
                    | EngineCommand::Remove { .. }
                    | EngineCommand::Commit { .. } => Err(Error::Busy),
                    EngineCommand::Resource(request) => request.external_read(|cursor, bytes| {
                        self.core.media.read(
                            cursor,
                            bytes,
                            self.config.limits().page_bytes,
                            &mut self.core.device,
                        )
                    }),
                }
            };
            self.ingress.set_epoch(self.core.graphs.epoch());
            let outcome = match result {
                Ok(()) => admission.map_or(Outcome::Applied, |a| Outcome::Scheduled {
                    effective_frame: a.effective_frame,
                    late_by: a.late_by,
                }),
                Err(error) => Outcome::Rejected(error),
            };
            self.ingress
                .complete(outcome, retired)
                .map_err(|error| error.error)?;
        }
        Ok(())
    }
    fn fail_pending(&mut self, error: Error) -> Result<(), Error> {
        let frame = self.frame.max(self.core.frame);
        for pending in self.core.pending.drain(..) {
            self.scheduler
                .receipts(frame)
                .complete(pending.token, Err(error))?;
        }
        while let Some(due) = self.scheduler.due(Frame(u64::MAX)) {
            let (sequence, action) = (due.sequence, due.action);
            if matches!(action, Action::Planner { .. }) {
                self.scheduler
                    .receipts(frame)
                    .cancel_queued(sequence, Reply::Failed(Failure::Runtime(error)))?;
            } else {
                self.scheduler.cancel(frame, sequence)?;
            }
        }
        Ok(())
    }
    pub fn process<'a>(
        &mut self,
        inputs: &[&'a [f32]],
        outputs: &mut [&'a mut [f32]],
    ) -> Result<(), Error> {
        if inputs.len() != 2
            || outputs.len() != 2
            || outputs[0].len() != outputs[1].len()
            || inputs.iter().any(|input| input.len() != outputs[0].len())
        {
            return Err(Error::InvalidShape);
        }
        let frames = outputs[0].len();
        self.service()?;
        if let Some(error) = self.fault {
            self.fail_pending(error)?;
            for output in outputs {
                output.fill(0.);
            }
            return Err(error);
        }
        let mut buffer = self.buffer.take().ok_or(Error::InvalidState)?;
        let callback = self.frame;
        let result = buffer.process(
            &mut rt_runtime::buffer::AudioBlock {
                frame: self.frame,
                frames,
                inputs,
                outputs,
            },
            &mut rt_runtime::buffer::MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
            |audio, _| {
                self.core.frame = audio.frame();
                while let Some(mut delivery) = self.scheduler.deliver(audio.frame()) {
                    match delivery.action {
                        Action::Processor { node, operation } => {
                            let result = self.core.processor(node, operation);
                            delivery
                                .receipts
                                .complete_processor(delivery.token, result)?;
                            if result == Err(Error::Disconnected) {
                                return result;
                            }
                        }
                        Action::Planner { operation, .. } => {
                            let result = self.core.deliver(
                                operation,
                                delivery.token,
                                delivery.lane,
                                delivery.requested_frame,
                                delivery.payload,
                                &mut delivery.receipts,
                            );
                            match result {
                                Ok(Some(reply)) => {
                                    delivery.receipts.complete(delivery.token, Ok(reply))?
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    delivery
                                        .receipts
                                        .complete(delivery.token, Ok(Reply::Failed(error)))?;
                                    if error == Failure::Runtime(Error::Disconnected) {
                                        return Err(Error::Disconnected);
                                    }
                                }
                            }
                        }
                        _ => return Err(Error::InvalidParameter),
                    }
                }
                let output = self.core.quantum(
                    &[audio.input(0), audio.input(1)],
                    callback,
                    &mut self.scheduler.receipts(audio.frame()),
                )?;
                for (channel, source) in output.iter().enumerate() {
                    for (sample, &word) in audio.output(channel).iter_mut().zip(source) {
                        *sample = Q824(word).to_f32();
                    }
                }
                Ok(())
            },
        );
        self.buffer = Some(buffer);
        if let Err(error) = result {
            self.fault = Some(error);
            self.core.observations.fault();
            for output in outputs.iter_mut() {
                output.fill(0.);
            }
            self.fail_pending(error)?;
            return Err(error);
        }
        self.frame = self.frame.checked_add(frames)?;
        let peak = |samples: &[f32]| samples.iter().fold(0f32, |peak, s| peak.max(s.abs()));
        self.core.observations.clock(
            self.frame,
            inputs.iter().map(|input| peak(input)).fold(0., f32::max),
            outputs.iter().map(|output| peak(output)).fold(0., f32::max),
        );
        self.ingress.set_epoch(self.core.graphs.epoch());
        self.ingress
            .publish(Diagnostics {
                frame: self.frame,
                epoch: self.core.graphs.epoch(),
                ..Default::default()
            })
            .ok();
        self.publish_memory();
        Ok(())
    }
}
impl<D: EngineDevice> Core<D> {
    fn memory_report(&mut self, frame: Frame) -> Result<crate::memory::MemoryReport, Error> {
        if !self.loaded {
            return Err(Error::InvalidState);
        }
        let mut report = self.device.memory_report()?;
        if !report.valid() {
            return Err(Error::InvalidState);
        }
        // The device counts frames from the latest program load.
        report.frame = frame.0;
        Ok(report)
    }
    fn processor(&mut self, id: NodeId, operation: EngineOp) -> Result<(), Error> {
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .ok_or(Error::InvalidId)?;
        match operation {
            EngineOp::Configure(configuration) => {
                if self.loaded {
                    return Err(Error::Busy);
                }
                if configuration.processor.is_some()
                    || configuration.recorded_state.is_some()
                    || configuration.gain >= 128.
                    || configuration.generation == 0
                {
                    return Err(Error::InvalidParameter);
                }
                node.processor
                    .apply_signal(operation, self.graphs.epoch())?;
                node.configuration = Some(configuration);
                Ok(())
            }
            EngineOp::InputSource { .. } | EngineOp::DiscardConfiguration { .. } => {
                node.processor.apply_signal(operation, self.graphs.epoch())
            }
            EngineOp::Crossfade => {
                node.processor
                    .apply_signal(operation, self.graphs.epoch())?;
                let ramp = node.ramps[&Parameter::Bank];
                let target = ONE - ramp.target;
                self.set_parameter(id, Parameter::Bank, target)?;
                Ok(())
            }
            EngineOp::ReadStart { .. } | EngineOp::ReadStop => {
                node.processor.apply_signal(operation, self.graphs.epoch())
            }
            EngineOp::Gain(value) | EngineOp::Balance(value) => {
                if !value.is_finite()
                    || value.abs() >= 128.
                    || matches!(operation, EngineOp::Balance(_)) && !(-1.0..=1.0).contains(&value)
                {
                    return Err(Error::InvalidParameter);
                }
                let parameter = if matches!(operation, EngineOp::Gain(_)) {
                    Parameter::Gain
                } else {
                    Parameter::Balance
                };
                node.processor
                    .apply_signal(operation, self.graphs.epoch())?;
                self.set_parameter(id, parameter, Q824::from_f32(value).0)
            }
            EngineOp::Mute(enabled) | EngineOp::Presence(enabled) => {
                let parameter = if matches!(operation, EngineOp::Mute(_)) {
                    Parameter::Mute
                } else {
                    Parameter::Presence
                };
                node.processor
                    .apply_signal(operation, self.graphs.epoch())?;
                self.set_parameter(
                    id,
                    parameter,
                    match operation {
                        EngineOp::Mute(_) => i32::from(!enabled) * ONE,
                        _ => i32::from(enabled) * ONE,
                    },
                )
            }
            EngineOp::Monitor(enabled) => {
                node.processor
                    .apply_signal(operation, self.graphs.epoch())?;
                node.manual_monitor = enabled;
                Ok(())
            }
            _ => Err(Error::InvalidParameter),
        }
    }
    fn set_parameter(
        &mut self,
        id: NodeId,
        parameter: Parameter,
        target: i32,
    ) -> Result<(), Error> {
        let mut current = self
            .nodes
            .iter()
            .find(|node| node.id == id)
            .ok_or(Error::InvalidId)?
            .ramps[&parameter];
        if let Some(binding) = self
            .image
            .bindings
            .iter()
            .find(|b| b.node == id && b.parameter == parameter)
        {
            if let Value::Ramp(value) = self.device.read_state(binding.address)? {
                current = value;
            }
        }
        let start = rt_audio_math::Ramp {
            start: Q824(current.start),
            target: Q824(current.target),
            duration: current.duration,
            elapsed: current.elapsed,
        }
        .at(0)
        .0;
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .ok_or(Error::InvalidId)?;
        let ramp = Ramp {
            start,
            target,
            duration: node.duration(),
            elapsed: 0,
        };
        node.ramps.insert(parameter, ramp);
        for binding in self
            .image
            .bindings
            .iter()
            .filter(|b| b.node == id && b.parameter == parameter)
        {
            self.device
                .write_state(binding.address, Value::Ramp(ramp))?;
        }
        Ok(())
    }
    fn tp(&mut self, opcode: u8, index: u32, data: [u32; 20]) -> Result<[u32; 26], Failure> {
        let reply = self.device.tp(opcode, index, data)?;
        if reply.error != 0 {
            return Err(Failure::Timeline(protocol::error(u64::from(reply.error))?));
        }
        Ok(reply.words)
    }
    fn stage_status(&self, id: StageId) -> Result<Status, Error> {
        if self.stage_id != Some(id) {
            return Err(Error::InvalidId);
        }
        Ok(if self.ready_topology.is_some() {
            Status::Ready
        } else {
            self.stager.status()
        })
    }
    fn deliver(
        &mut self,
        operation: Operation,
        token: ReceiptToken,
        lane: Lane,
        requested: Frame,
        payload: &[u8],
        receipts: &mut Receipts<'_, Operation, Reply, EngineOp, ()>,
    ) -> Result<Option<Reply>, Failure> {
        let reply = match operation {
            Operation::NodePairBegin { slot, changes } => {
                if slot != 0 {
                    return Err(Error::InvalidId.into());
                }
                if self.pairs.status != NodePairStatus::Idle {
                    return Err(Error::Busy.into());
                }
                if changes > 1 {
                    return Err(Error::Capacity.into());
                }
                self.pairs.generation = self
                    .pairs
                    .generation
                    .checked_add(1)
                    .ok_or(Error::Exhausted)?;
                self.pairs.status = if changes == 0 {
                    NodePairStatus::Ready
                } else {
                    NodePairStatus::Receiving {
                        received: 0,
                        expected: changes,
                    }
                };
                Reply::NodePairStatus {
                    id: self.pairs.id()?,
                    status: self.pairs.status,
                }
            }
            Operation::NodePairAppend {
                id,
                previous,
                initial,
            } => {
                self.pairs.check(id)?;
                if self.pairs.status
                    != (NodePairStatus::Receiving {
                        received: 0,
                        expected: 1,
                    })
                {
                    return Err(Error::InvalidState.into());
                }
                if previous.is_some()
                    || initial.is_none_or(|i| {
                        i.frames != 0
                            || i.audio.is_some()
                            || i.auxiliary.is_some()
                            || i.midi.is_some()
                            || i.dry_wet
                            || i.target.slot() != 0
                    })
                {
                    return Err(Error::InvalidParameter.into());
                }
                self.pairs.initial = initial;
                self.pairs.status = NodePairStatus::Ready;
                Reply::NodePairStatus {
                    id,
                    status: self.pairs.status,
                }
            }
            Operation::NodePairStatus { id } => {
                self.pairs.check(id)?;
                Reply::NodePairStatus {
                    id,
                    status: self.pairs.status,
                }
            }
            Operation::NodePairCancel { id } | Operation::NodePairRecycle { id } => {
                self.pairs.check(id)?;
                let valid = match operation {
                    Operation::NodePairRecycle { .. } => self.pairs.status == NodePairStatus::Taken,
                    _ => !matches!(
                        self.pairs.status,
                        NodePairStatus::Taken | NodePairStatus::Idle
                    ),
                };
                if !valid {
                    return Err(Error::InvalidState.into());
                }
                self.pairs.status = NodePairStatus::Idle;
                self.pairs.initial = None;
                Reply::NodePairStatus {
                    id,
                    status: self.pairs.status,
                }
            }
            Operation::TopologyBegin { slot } => {
                if slot != 0 {
                    return Err(Error::InvalidId.into());
                }
                let header: &[u8; RECORD_BYTES] =
                    payload.try_into().map_err(|_| Error::InvalidShape)?;
                let id = self.stager.begin(header).map_err(Failure::Timeline)?;
                self.stage_id = Some(id);
                Reply::TopologyStatus {
                    id,
                    status: self.stager.status(),
                }
            }
            Operation::TopologyAppend { id, first_record } => {
                self.stager
                    .append(id, first_record, payload, 32)
                    .map_err(Failure::Timeline)?;
                Reply::TopologyStatus {
                    id,
                    status: self.stager.status(),
                }
            }
            Operation::TopologyValidate { id } => {
                let progress = self.stager.validate(id, 400).map_err(Failure::Timeline)?;
                if progress.ready {
                    self.ready_topology = Some(self.stager.take(id).map_err(Failure::Timeline)?);
                }
                Reply::TopologyProgress { id, progress }
            }
            Operation::TopologyStatus { id } => Reply::TopologyStatus {
                id,
                status: self.stage_status(id)?,
            },
            Operation::TopologyCancel { id } => {
                if self.stage_id != Some(id) {
                    return Err(Error::InvalidId.into());
                }
                if let Some(topology) = self.ready_topology.take() {
                    self.stager
                        .recycle(topology)
                        .map_err(|error| Failure::Timeline(error.0))?;
                } else {
                    self.stager.cancel(id).map_err(Failure::Timeline)?;
                }
                Reply::TopologyStatus {
                    id,
                    status: self.stager.status(),
                }
            }
            Operation::TopologyRecycle { id } => {
                if self.stage_id != Some(id) {
                    return Err(Error::InvalidId.into());
                }
                let old = self.retired_topology.take().ok_or(Error::InvalidState)?;
                self.stager
                    .recycle(old)
                    .map_err(|e| Failure::Timeline(e.0))?;
                Reply::TopologyStatus {
                    id,
                    status: self.stager.status(),
                }
            }
            Operation::ConfigurationActivate {
                node_pairs,
                topology,
                graph,
                graph_epoch,
                topology_epoch,
            } => {
                self.graphs.preflight(graph, graph_epoch)?;
                self.pairs.check(node_pairs)?;
                if self.pairs.status != NodePairStatus::Ready
                    || self.stage_status(topology)? != Status::Ready
                {
                    return Err(Error::InvalidState.into());
                }
                if self.topology_epoch != topology_epoch {
                    return Err(Error::StaleEpoch.into());
                }
                if self.loaded {
                    return Err(Error::Busy.into());
                }
                let candidate = self.ready_topology.as_ref().ok_or(Error::InvalidState)?;
                if candidate.nodes().len() != 1
                    || candidate.nodes()[0].kind != loops::LoopKind::Primitive
                    || candidate.nodes()[0].length != 0
                    || candidate.nodes()[0]
                        .sync_source
                        .is_some_and(|id| id != candidate.nodes()[0].id)
                {
                    return Err(Error::InvalidParameter.into());
                }
                let initial = self.pairs.initial.ok_or(Error::InvalidState)?;
                if initial.target != candidate.nodes()[0].id {
                    return Err(Error::InvalidId.into());
                }
                let next_topology = self.topology_epoch.checked_next()?;
                let next_graph = graph_epoch.checked_next()?;
                let saved = self
                    .nodes
                    .iter()
                    .map(|node| node.signal.clone())
                    .collect::<Vec<_>>();
                for node in &mut self.nodes {
                    if matches!(node.kind, SignalKind::Playback { .. }) {
                        node.outputs = initial.audio_widths.map_or(2, |w| w[0] as usize);
                    }
                    if let Some(configuration) = node.configuration {
                        node.configure(configuration);
                    }
                }
                let time = Instant::now();
                let image = compile::lower(
                    self.graphs.candidate(graph)?.descriptor(),
                    &self.nodes,
                    candidate,
                    self.smoothing_frames(),
                );
                let image = match image {
                    Ok(image) => image,
                    Err(_) => {
                        for (node, saved) in self.nodes.iter_mut().zip(saved) {
                            node.signal = saved;
                        }
                        return Err(Error::InvalidGraph.into());
                    }
                };

                self.metrics.compile_time += time.elapsed();
                self.metrics.compilations += 1;
                let time = Instant::now();
                self.device
                    .load(&image.image)
                    .map_err(|_| Error::Disconnected)?;
                self.metrics.configure_time += time.elapsed();
                for node in &mut self.nodes {
                    node.processor.apply_configuration(next_graph);
                    node.configuration = None;
                }
                self.origin = self.frame;
                self.image = image;
                self.metrics.instructions = self.image.image.program.instructions.len();
                self.metrics.state_words = self.image.image.initial_state.memory.len();
                self.metrics.buffers = self.image.image.buffers;
                self.loaded = true;
                self.retired_topology = Some(std::mem::replace(
                    &mut self.topology,
                    self.ready_topology.take().ok_or(Error::InvalidState)?,
                ));
                let graph_epoch = self.graphs.activate(graph, graph_epoch)?;
                self.topology_epoch = next_topology;
                self.pairs.status = NodePairStatus::Taken;
                self.observations
                    .loop_owner(initial.target, initial.history_source);
                self.metrics.activations += 1;
                Reply::ConfigurationActivated {
                    node_pairs,
                    topology,
                    graph,
                    graph_epoch,
                    topology_epoch: self.topology_epoch,
                }
            }
            Operation::GraphStatus { id } => Reply::GraphStatus {
                id,
                status: self.graphs.status(id)?,
            },
            Operation::GraphActivate {
                id,
                graph_epoch,
                topology_epoch,
            } => {
                if self.topology_epoch != topology_epoch {
                    return Err(Error::StaleEpoch.into());
                }
                self.graphs.preflight(id, graph_epoch)?;
                self.snapshot_parameters()?;
                let time = Instant::now();
                let image = compile::lower(
                    self.graphs.candidate(id)?.descriptor(),
                    &self.nodes,
                    &self.topology,
                    self.smoothing_frames(),
                )
                .map_err(|_| Error::InvalidGraph)?;
                self.metrics.compile_time += time.elapsed();
                self.metrics.compilations += 1;
                let time = Instant::now();
                self.device
                    .program(&image.image)
                    .map_err(|_| Error::Disconnected)?;
                self.metrics.configure_time += time.elapsed();
                self.image = image;
                self.metrics.instructions = self.image.image.program.instructions.len();
                self.metrics.state_words = self.image.image.initial_state.memory.len();
                self.metrics.buffers = self.image.image.buffers;
                let graph_epoch = self.graphs.activate(id, graph_epoch)?;
                for node in &mut self.nodes {
                    node.processor.apply_configuration(graph_epoch);
                }
                self.metrics.activations += 1;
                Reply::GraphActivated {
                    id,
                    graph_epoch,
                    topology_epoch: self.topology_epoch,
                }
            }
            Operation::RepeatSync { target, enabled } => {
                self.check_loop(target)?;
                let mut data = [0; 20];
                data[0] = target.generation();
                data[1] = u32::from(enabled);
                self.tp(17, target.slot(), data)?;
                Reply::Done
            }
            Operation::Gain { target, value } | Operation::Balance { target, value } => {
                self.check_loop(target)?;
                if !value.is_finite()
                    || value.abs() >= 128.
                    || matches!(operation, Operation::Balance { .. })
                        && !(-1.0..=1.0).contains(&value)
                {
                    return Err(Error::InvalidParameter.into());
                }
                let parameter = if matches!(operation, Operation::Gain { .. }) {
                    self.loop_gain = value;
                    Parameter::Gain
                } else {
                    self.loop_balance = value;
                    Parameter::Balance
                };
                let nodes = self
                    .nodes
                    .iter()
                    .filter(|node| matches!(node.kind, SignalKind::Playback { .. }))
                    .map(|node| node.id)
                    .collect::<Vec<_>>();
                for node in nodes {
                    self.set_parameter(node, parameter, Q824::from_f32(value).0)?;
                }
                Reply::Done
            }
            Operation::Clear { target } => {
                self.check_loop(target)?;
                let words = self.tp(12, target.slot(), [0; 20])?;
                let revision = rt_runtime::Revision(protocol::get(&words, 448, 64))
                    .checked_next()
                    .map_err(|_| Error::Exhausted)?;
                let data = protocol::transition(loops::Request {
                    target,
                    sequence: token.sequence(),
                    at: Frame(requested.0.saturating_sub(self.origin.0)),
                    timing: loops::Timing::At,
                    transition: loops::Transition::Replace {
                        length: 0,
                        revision,
                        preserve_position: false,
                    },
                })?;
                let result = self.tp(4, 0, data)?;
                let id = RequestId::from_parts(result[0], result[1]).map_err(Failure::Timeline)?;
                self.pending.push(Pending {
                    token,
                    kind: PendingKind::Transition { id, requested },
                    lane,
                });
                return Ok(None);
            }
            Operation::Smoothing { target, smoothing } => {
                self.check_loop(target)?;
                self.smoothing = smoothing;
                for &(address, base) in &self.image.smoothing {
                    self.device.write_state(
                        address,
                        Value::Word(base | (u64::from(self.smoothing_frames()) << 32)),
                    )?;
                }
                Reply::Done
            }
            Operation::State { .. } | Operation::Export { .. } => {
                self.pending.push(Pending {
                    token,
                    kind: PendingKind::Query(operation),
                    lane,
                });
                return Ok(None);
            }
            Operation::Retire { media } => {
                if let Some(at) = self.seed_media.iter().position(|&id| id == media) {
                    self.seed_media.remove(at);
                } else {
                    self.media.retire_external(media, &mut self.device)?;
                }
                Reply::Done
            }
            Operation::Transition {
                target,
                timing,
                transition,
            } => {
                self.check_loop(target)?;
                let data = protocol::transition(loops::Request {
                    target,
                    sequence: token.sequence(),
                    at: Frame(requested.0.saturating_sub(self.origin.0)),
                    timing,
                    transition,
                })?;
                let result = self.tp(4, 0, data)?;
                let id = RequestId::from_parts(result[0], result[1]).map_err(Failure::Timeline)?;
                self.pending.push(Pending {
                    token,
                    kind: PendingKind::Transition { id, requested },
                    lane,
                });
                return Ok(None);
            }
            Operation::Cancel { sequence } => {
                if let Some(p) = self.pending.iter().find(|p| p.token.sequence() == sequence) {
                    let PendingKind::Transition { id, .. } = p.kind else {
                        return Err(Error::InvalidState.into());
                    };
                    let mut words = [0; 20];
                    words[0] = id.generation();
                    self.tp(5, id.slot(), words)?;
                    Reply::Cancelled {
                        sequence,
                        delivered: true,
                    }
                } else {
                    receipts.planner().cancel_queued(
                        sequence,
                        Reply::Cancelled {
                            sequence,
                            delivered: false,
                        },
                    )?;
                    Reply::Cancelled {
                        sequence,
                        delivered: false,
                    }
                }
            }
            Operation::Cursor {
                export,
                midi: false,
            } => Reply::Cursor(self.media.cursor(export)?),
            Operation::Release { export } => {
                self.media.release(export, &mut self.device)?;
                Reply::Done
            }
            _ => return Err(Error::InvalidParameter.into()),
        };
        Ok(Some(reply))
    }
    fn smoothing_frames(&self) -> u32 {
        match self.smoothing {
            loops::audio::Smoothing::None => 0,
            loops::audio::Smoothing::LinearRestart { frames } => frames,
        }
    }
    fn check_loop(&self, target: LoopId) -> Result<(), Failure> {
        if self.topology.nodes().iter().any(|spec| spec.id == target) {
            Ok(())
        } else {
            Err(Failure::Timeline(loops::Error::InvalidId))
        }
    }
    fn snapshot_parameters(&mut self) -> Result<(), Error> {
        for binding in &self.image.bindings {
            if let Value::Ramp(value) = self.device.read_state(binding.address)? {
                if let Some(node) = self.nodes.iter_mut().find(|node| node.id == binding.node) {
                    node.ramps.insert(binding.parameter, value);
                }
            }
        }
        Ok(())
    }
    fn state(&mut self) -> Result<Option<State>, Error> {
        let Some(id) = self.topology.nodes().first().map(|spec| spec.id) else {
            return Ok(None);
        };
        let reply = self.device.tp(12, id.slot(), [0; 20])?;
        if reply.error != 0 {
            return Err(Error::InvalidState);
        }
        let mut state = protocol::state(&reply.words, id.slot())?;
        if let Some(frame) = state.last_pulse {
            state.last_pulse = Some(Frame(frame.0 + self.origin.0));
        }
        Ok(Some(state))
    }
    fn quantum(
        &mut self,
        inputs: &[&[f32]],
        callback: Frame,
        receipts: &mut ReceiptWriter<'_, Operation, Reply>,
    ) -> Result<Vec<Vec<i32>>, Error> {
        self.tp(16, 0, [0; 20]).map_err(|_| Error::InvalidState)?;
        self.state = self.state()?;
        if let Some(state) = self.state {
            self.media.observe(state, self.frame, &mut self.device)?;
            let mut observed = None;
            let mut pre_recording = false;
            for p in &self.pending {
                if let PendingKind::Transition { id, .. } = p.kind {
                    let reply = self.device.tp(13, id.slot(), [0; 20])?;
                    if reply.error != 0 {
                        return Err(Error::InvalidState);
                    }
                    if protocol::get(&reply.words, 37, 3) != 0
                        || protocol::get(&reply.words, 44, 1) != 0
                    {
                        continue;
                    }
                    let mode = match protocol::get(&reply.words, 34, 2) {
                        0 => loops::Mode::Stopped,
                        1 => loops::Mode::Playing,
                        2 => loops::Mode::Recording,
                        _ => continue,
                    };
                    let ticks = protocol::get(&reply.words, 33, 1) != 0;
                    let delay = if ticks {
                        protocol::get(&reply.words, 512, 64)
                    } else {
                        (protocol::get(&reply.words, 448, 64) + self.origin.0)
                            .saturating_sub(self.frame.0)
                            .div_ceil(32)
                    };
                    pre_recording |= mode == loops::Mode::Recording && ticks && delay == 1;
                    if let Ok(delay) = u32::try_from(delay) {
                        if observed.is_none_or(|(d, _)| delay < d) {
                            observed = Some((delay, mode));
                        }
                    }
                }
            }
            self.observations
                .loop_state(self.frame, self.topology_epoch, state, observed);
            let inputs = self
                .nodes
                .iter()
                .filter_map(|node| {
                    if let SignalKind::StripInput { slot, .. } = node.kind {
                        Some((
                            node.id,
                            slot,
                            node.generation,
                            node.manual_monitor,
                            node.ramps[&Parameter::Monitor].target,
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for (id, slot, generation, manual, current) in inputs {
                let enabled = self.observations.monitoring(
                    slot,
                    generation,
                    manual,
                    state.mode == loops::Mode::Recording,
                    state.mode == loops::Mode::Playing,
                    pre_recording,
                );
                let target = if enabled { ONE } else { 0 };
                if current != target {
                    self.set_parameter(id, Parameter::Monitor, target)?;
                }
            }
        }
        let mut collected = vec![];
        let mut pending = std::mem::take(&mut self.pending);
        pending.sort_by_key(|p| u8::from(p.lane == Lane::Recovery));
        for pending in pending {
            let reply = match pending.kind {
                PendingKind::Transition { id, requested } => {
                    let words = self
                        .tp(13, id.slot(), [0; 20])
                        .map_err(|_| Error::InvalidState)?;
                    let Some(mut completion) = protocol::completion(&words, id.slot())? else {
                        self.pending.push(pending);
                        continue;
                    };
                    completion.requested = requested;
                    completion.effective.0 += self.origin.0;
                    match &mut completion.outcome {
                        loops::Outcome::Applied { frame }
                        | loops::Outcome::Superseded { frame }
                        | loops::Outcome::Cancelled { frame }
                        | loops::Outcome::Failed { frame, .. } => frame.0 += self.origin.0,
                    }
                    collected.push(id);
                    Ok(Reply::Transition(completion))
                }
                PendingKind::Query(operation) => self.query(operation),
            };
            receipts.complete(pending.token, Ok(reply.unwrap_or_else(Reply::Failed)))?;
        }
        let inputs = inputs
            .iter()
            .map(|input| {
                input
                    .iter()
                    .map(|&sample| Q824::from_f32(sample).0)
                    .collect()
            })
            .collect::<Vec<Vec<_>>>();
        let output = self.device.process(&self.image.image, &inputs)?;
        let status = self.tp(0, 0, [0; 20]).map_err(|_| Error::InvalidState)?;
        if protocol::get(&status, 0, 64) + self.origin.0 != self.frame.checked_add(32)?.0 {
            return Err(Error::InvalidFrame);
        }
        for (kind, buffers) in &self.image.meters {
            let mut peaks = vec![];
            for &buffer in buffers {
                peaks.push(
                    self.device
                        .scratch(buffer)?
                        .into_iter()
                        .map(|word| Q824(word).to_f32().abs())
                        .fold(0f32, f32::max),
                );
            }
            match *kind {
                SignalKind::StripInput { slot, .. } => {
                    self.observations.strip_peaks(slot, &peaks, &[])
                }
                SignalKind::StripOutput { slot, .. } => {
                    self.observations.strip_peaks(slot, &[], &peaks)
                }
                SignalKind::Bus { slot, .. } => self.observations.bus_peaks(slot, &peaks),
                SignalKind::Fader { slot, .. } => self.observations.loop_peaks(slot, &peaks),
                _ => {}
            }
        }

        for node in &mut self.nodes {
            if node.kind == SignalKind::Reader {
                let media = &self.media;
                let device = &mut self.device;
                let page_bytes = self.page_bytes;
                node.processor
                    .read_external(callback, self.read_budget, |cursor, bytes| {
                        media.read(cursor, bytes, page_bytes, device).map(|r| r.0)
                    })?;
            }
        }
        for id in collected {
            let mut words = [0; 20];
            words[0] = id.generation();
            self.tp(6, id.slot(), words)
                .map_err(|_| Error::InvalidState)?;
        }
        if let Some(state) = self.state()? {
            self.media
                .observe(state, self.frame.checked_add(32)?, &mut self.device)?;
        }
        Ok(output)
    }
    fn cut(&self, q: loops::runtime::Cut<Q824>) -> loops::runtime::Cut {
        loops::runtime::Cut {
            state: q.state,
            raw_audio_frames: q.raw_audio_frames,
            raw_midi_frames: 0,
            windows: q.windows,
            timing_revision: q.timing_revision,
            frame: q.frame,
            graph_epoch: q.graph_epoch,
            topology_epoch: self.topology_epoch,
            playback: q.playback,
            latest: q.latest,
            gain: self.loop_gain,
            balance: self.loop_balance,
            smoothing: self.smoothing,
            capture_failure: None,
            midi_overflows: 0,
        }
    }
    fn query(&mut self, operation: Operation) -> Result<Reply, Failure> {
        let state = self.state.ok_or(Error::InvalidState)?;
        match operation {
            Operation::State { target } => {
                self.check_loop(target)?;
                let q = self.media.cut(state, self.frame, self.graphs.epoch())?;
                Ok(Reply::State(self.cut(q)))
            }
            Operation::Export { target, selection } => {
                self.check_loop(target)?;
                let Reply::Export { id, cut, media } =
                    self.media
                        .export(state, self.frame, self.graphs.epoch(), selection)?
                else {
                    return Err(Error::InvalidState.into());
                };
                Ok(Reply::Export {
                    id,
                    cut: self.cut(cut),
                    media,
                })
            }
            _ => Err(Error::InvalidParameter.into()),
        }
    }
}
