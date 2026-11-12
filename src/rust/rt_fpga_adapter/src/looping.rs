//! A primitive loop profile over the production owning transport.
use rt_runtime::{
    PlannerId, Q824,
    executor::{Command, Control, PreparedNode},
};
use shoop_loops::{
    PreparedTopology,
    runtime::{Binding, LoopPlanner},
};
use shoop_rt_engine::{dsp::GainOperation, primitive::Node};
pub type Processor = Node<Q824>;
pub type Operation = GainOperation<Q824>;
pub type Controller = Control<Processor, LoopPlanner<Q824>, Operation, Q824>;
pub type Request = Command<Processor, LoopPlanner<Q824>, Operation, Q824>;
pub type Prepared = PreparedNode<Processor, LoopPlanner<Q824>, Operation, Q824>;
pub struct Startup {
    pub planner: PlannerId,
    pub topology: PreparedTopology,
    pub binding: Binding<Q824>,
    pub max_chunks: u32,
    pub exports: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct MediaCommand {
    pub opcode: u8,
    pub view: u32,
    pub start: u64,
    pub length: u64,
    pub count: u32,
}
#[derive(Debug, Default)]
pub struct MediaResult {
    pub view: u32,
    pub empty: bool,
    pub samples: Vec<i32>,
}
#[derive(Clone, Copy, Debug)]
pub struct TpReply {
    pub error: u8,
    pub words: [u32; 26],
}
pub trait LoopDevice: crate::adapter::Device {
    fn tp(
        &mut self,
        opcode: u8,
        index: u32,
        words: [u32; 20],
    ) -> Result<TpReply, rt_runtime::Error>;
    fn media(&mut self, command: MediaCommand) -> Result<MediaResult, rt_runtime::Error>;
    fn chunk_limit(&mut self, limit: u32) -> Result<(), rt_runtime::Error>;
}

use crate::{
    adapter::Metrics,
    compiler::{self, NodeDescriptor},
    image::*,
    loop_media, protocol,
};
use rt_runtime::{
    Epoch, Error, Frame, PreparedConfig, Quantum, Sequence,
    executor::{Diagnostics, Ingress, Retired},
    graph::{Catalog, PlanValidator, PreparedPlan, ValidationStatus},
    schedule::{Action, ReceiptToken, Scheduler},
    transport::{Lane, Outcome},
};
use shoop_loops::{
    self as loops, RequestId, State,
    runtime::{Failure, Operation as LoopOperation, Reply, Selection},
};
use std::time::Instant;
struct Pending {
    token: ReceiptToken,
    sequence: Sequence,
    lane: Lane,
    kind: PendingKind,
}
enum PendingKind {
    Transition(RequestId),
    Query(LoopOperation<Q824>),
}
pub struct LoopAdapter<D: LoopDevice> {
    device: D,
    config: PreparedConfig,
    startup: Startup,
    nodes: Vec<Prepared>,
    ingress: Ingress<Processor, LoopPlanner<Q824>, Operation, Q824>,
    scheduler: Scheduler<LoopOperation<Q824>, Reply<Q824>, Operation, ()>,
    active: Option<PreparedPlan>,
    image: Option<Image>,
    frame: Frame,
    epoch: Epoch,
    fault: Option<Error>,
    state: State,
    events: Vec<loops::Event>,
    pending: Vec<Pending>,
    media: loop_media::Media,
    pub metrics: Metrics,
}
impl<D: LoopDevice> LoopAdapter<D> {
    pub fn prepare(
        config: PreparedConfig,
        startup: Startup,
        nodes: Vec<Prepared>,
        device: D,
    ) -> Result<(Controller, Self), Error> {
        let limits = config.limits();
        let binding = startup.binding;
        if limits.audio_inputs != 1
            || limits.audio_outputs != 1
            || limits.midi_inputs != 0
            || limits.midi_outputs != 0
            || limits.max_frames < 32
            || nodes.len() != 3
            || startup.topology.nodes().len() != 1
            || startup.max_chunks == 0
            || startup.max_chunks > 128
            || startup.exports == 0
            || startup.exports > 4
        {
            return Err(Error::InvalidConfig);
        }
        if binding.channels != 1
            || binding.auxiliary_channels != 0
            || binding.dry_wet
            || binding.midi
            || binding.gain != Q824(1 << 24)
            || binding.smoothing != loops::audio::Smoothing::None
        {
            return Err(Error::InvalidParameter);
        }
        let spec = startup.topology.nodes()[0];
        if spec.id != binding.target
            || spec.kind != loops::LoopKind::Primitive
            || spec.length != 0
            || spec.sync_source.is_some()
        {
            return Err(Error::InvalidParameter);
        }
        let catalog = Catalog::new(
            config,
            &nodes.iter().map(Prepared::spec).collect::<Vec<_>>(),
        )?;
        catalog.node(binding.playback)?;
        catalog.node(binding.capture)?;
        descriptions(&startup, &nodes)?;
        let media = loop_media::Media::new(startup.exports)?;
        let (control, ingress, scheduler) =
            Controller::channel(config, Quantum::new(32)?, 0, true)?;
        Ok((
            control,
            Self {
                device,
                config,
                state: State::from_observation(binding.target, rt_runtime::Revision(0)),
                startup,
                nodes,
                ingress,
                scheduler,
                active: None,
                image: None,
                frame: Frame(0),
                epoch: Epoch(0),
                fault: None,
                events: vec![],
                pending: vec![],
                media,
                metrics: Metrics::default(),
            },
        ))
    }
    pub fn frame(&self) -> Frame {
        self.frame
    }
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn events(&self) -> &[loops::Event] {
        &self.events
    }
    pub fn fault(&self) -> Option<Error> {
        self.fault
    }
    pub fn device(&self) -> &D {
        &self.device
    }
    pub fn device_mut(&mut self) -> &mut D {
        &mut self.device
    }
    pub fn image(&self) -> Option<&Image> {
        self.image.as_ref()
    }
    fn hardware_state(&mut self) -> Result<State, Error> {
        let reply = self
            .device
            .tp(12, self.startup.binding.target.slot(), [0; 20])?;
        if reply.error != 0 {
            return Err(Error::InvalidState);
        }
        let state = protocol::state(&reply.words, self.startup.binding.target.slot())?;
        if state.id != self.startup.binding.target {
            return Err(Error::InvalidId);
        }
        Ok(state)
    }
    pub fn service(&mut self) -> Result<(), Error> {
        self.scheduler.begin_callback();
        self.ingress.begin_cycle(self.frame, self.epoch);
        while let Some(operation) = self.ingress.next_operation() {
            let mut admission = None;
            let result = (|| -> Result<(), Error> {
                if let Some(error) = self.fault {
                    return Err(error);
                }
                match operation.request {
                    Request::Commit {
                        expected_epoch,
                        plan,
                    } => {
                        if operation.lane != Lane::Normal {
                            return Err(Error::InvalidState);
                        }
                        if *expected_epoch != self.epoch {
                            return Err(Error::StaleEpoch);
                        }
                        if self.active.is_some() || self.frame != Frame(0) {
                            return Err(Error::Busy);
                        }
                        let next = self.epoch.checked_next()?;
                        let time = Instant::now();
                        // The request is borrowed independently of the compiler's immutable inputs.
                        let catalog = Catalog::new(
                            self.config,
                            &self.nodes.iter().map(Prepared::spec).collect::<Vec<_>>(),
                        )?;
                        let candidate = plan.as_ref().ok_or(Error::InvalidGraph)?;
                        let image = compile_profile(
                            self.config,
                            &self.startup,
                            &self.nodes,
                            &catalog,
                            candidate,
                        )?;
                        self.metrics.compile_time += time.elapsed();
                        self.metrics.compilations += 1;
                        let time = Instant::now();
                        if let Err(error) = self
                            .device
                            .load(&image)
                            .and_then(|_| self.device.chunk_limit(self.startup.max_chunks))
                        {
                            self.fault = Some(error);
                            return Err(error);
                        }
                        self.metrics.configuration_time += time.elapsed();
                        self.metrics.instructions = image.program.instructions.len();
                        self.metrics.state_words = image.initial_state.memory.len();
                        self.metrics.image_bytes = self.metrics.instructions * 12
                            + self.metrics.state_words * 17
                            + 80 * (image.transition.limits.loop_slots as usize + 1)
                            + image
                                .transition
                                .components
                                .iter()
                                .map(|c| c.len() * 80)
                                .sum::<usize>()
                            + 100;
                        self.image = Some(image);
                        self.active = plan.take();
                        self.epoch = next;
                        Ok(())
                    }
                    Request::Schedule {
                        at,
                        action,
                        payload,
                    } => {
                        if !payload.is_empty() {
                            return Err(Error::InvalidParameter);
                        }
                        if at.0 % 32 != 0 {
                            return Err(Error::InvalidFrame);
                        }
                        match *action {
                            Action::Processor {
                                node,
                                operation: gain,
                            } => {
                                if operation.lane != Lane::Normal {
                                    return Err(Error::InvalidState);
                                }
                                if !self.nodes.iter().any(|n| {
                                    n.spec().id == node
                                        && matches!(n.processor(), Processor::Gain(_))
                                }) {
                                    return Err(Error::InvalidId);
                                }
                                if gain.duration != 0 {
                                    return Err(Error::InvalidParameter);
                                }
                            }
                            Action::Planner {
                                planner,
                                operation: op,
                            } => {
                                if planner != self.startup.planner {
                                    return Err(Error::InvalidId);
                                }
                                if !supported(op) {
                                    return Err(Error::InvalidParameter);
                                }
                                if operation.lane == Lane::Recovery
                                    && (!recovery_safe(op) || *at > self.frame)
                                {
                                    return Err(Error::InvalidState);
                                }
                            }
                            _ => return Err(Error::InvalidParameter),
                        }
                        admission = Some(if operation.lane == Lane::Recovery {
                            self.scheduler.admit_recovery(
                                operation.sequence,
                                *at,
                                self.frame,
                                *action,
                                payload,
                            )?
                        } else {
                            self.scheduler.admit(
                                operation.sequence,
                                *at,
                                self.frame,
                                *action,
                                payload,
                            )?
                        });
                        Ok(())
                    }
                    Request::Cancel { sequence } => self.scheduler.cancel(self.frame, *sequence),
                    Request::Resource(request) => {
                        if operation.lane != Lane::Normal {
                            return Err(Error::InvalidState);
                        }
                        let result = request.external_read(|cursor, bytes| {
                            self.media.read(
                                cursor,
                                bytes,
                                self.config.limits().page_bytes,
                                &mut self.device,
                            )
                        });
                        result
                    }
                    Request::Install { .. } | Request::Remove { .. } => Err(Error::Busy),
                    _ => Err(Error::InvalidParameter),
                }
            })();
            self.ingress.set_epoch(self.epoch);
            let outcome = match result {
                Ok(()) => admission.map_or(Outcome::Applied, |a| Outcome::Scheduled {
                    effective_frame: a.effective_frame,
                    late_by: a.late_by,
                }),
                Err(e) => Outcome::Rejected(e),
            };
            self.ingress
                .complete(outcome, None::<Retired<Processor>>)
                .map_err(|e| e.error)?;
        }
        Ok(())
    }
    pub fn process(&mut self, inputs: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
        if inputs.len() != 1 || inputs[0].len() != 32 {
            return Err(Error::InvalidShape);
        }
        self.service()?;
        if let Some(error) = self.fault {
            self.fail_pending(error)?;
            return Err(error);
        }
        let result = self.execute(inputs);
        if let Err(error) = result {
            self.fault = Some(error);
            self.fail_pending(error)?;
            return Err(error);
        }
        result
    }
    fn execute(&mut self, inputs: &[Vec<i32>]) -> Result<Vec<Vec<i32>>, Error> {
        let next = self.frame.checked_add(32)?;
        if self.image.is_none() {
            return Err(Error::InvalidState);
        }
        while let Some(mut delivery) = self.scheduler.deliver(self.frame) {
            match delivery.action {
                Action::Processor { node, operation } => {
                    let binding = self
                        .image
                        .as_ref()
                        .and_then(|i| {
                            i.gains
                                .iter()
                                .find(|b| b.id == identity(node.slot(), node.generation()))
                        })
                        .ok_or(Error::InvalidId)?;
                    let result = (|| {
                        for &address in &binding.ramps {
                            self.device.write_state(
                                address,
                                Value::Ramp(Ramp {
                                    start: operation.target.0,
                                    target: operation.target.0,
                                    duration: 0,
                                    elapsed: 0,
                                }),
                            )?;
                            self.metrics.gain_writes += 1;
                        }
                        Ok(())
                    })();
                    delivery
                        .receipts
                        .complete_processor(delivery.token, result)?;
                    result?;
                }
                Action::Planner { operation, .. } => {
                    let result = deliver_planner(
                        &mut self.device,
                        &mut self.media,
                        &mut self.pending,
                        &mut delivery.receipts,
                        &self.startup,
                        operation,
                        delivery.token,
                        delivery.lane,
                        delivery.requested_frame,
                    );
                    match result {
                        Ok(Some(reply)) => delivery.receipts.complete(delivery.token, Ok(reply))?,
                        Ok(None) => {}
                        Err(Failure::Timeline(error)) => delivery.receipts.complete(
                            delivery.token,
                            Ok(Reply::Failed(Failure::Timeline(error))),
                        )?,
                        Err(Failure::Runtime(error)) => delivery
                            .receipts
                            .complete(delivery.token, Ok(Reply::Failed(Failure::Runtime(error))))?,
                    }
                }
                _ => return Err(Error::InvalidParameter),
            }
        }
        let settled = self.device.tp(16, 0, [0; 20])?;
        if settled.error != 0 {
            return Err(Error::InvalidState);
        }
        self.state = self.hardware_state()?;
        self.media
            .observe(self.state, self.frame, &mut self.device)?;
        let mut completed = vec![];
        for pending in &self.pending {
            let result = match pending.kind {
                PendingKind::Transition(id) => {
                    let reply = self.device.tp(13, id.slot(), [0; 20])?;
                    if reply.error != 0 {
                        return Err(Error::InvalidState);
                    }
                    let Some(completion) = protocol::completion(&reply.words, id.slot())? else {
                        return Err(Error::InvalidState);
                    };
                    if completion.id != id || completion.sequence != pending.sequence {
                        return Err(Error::InvalidId);
                    }
                    completed.push(id);
                    Ok(Reply::Transition(completion))
                }
                PendingKind::Query(op) => match op {
                    LoopOperation::State { .. } => self
                        .media
                        .cut(self.state, self.frame, self.epoch)
                        .map(Reply::State),
                    LoopOperation::Export { selection, .. } => self
                        .media
                        .export(self.state, self.frame, self.epoch, selection),
                    _ => Err(Error::InvalidParameter),
                },
            };
            self.scheduler.receipts(self.frame).complete(
                pending.token,
                Ok(result.unwrap_or_else(|error| Reply::Failed(Failure::Runtime(error)))),
            )?;
        }
        self.pending.clear();
        let output = self
            .device
            .process(self.image.as_ref().ok_or(Error::InvalidState)?, inputs)?;
        for id in completed {
            let mut w = [0; 20];
            w[0] = id.generation();
            let reply = self.device.tp(6, id.slot(), w)?;
            if reply.error != 0 {
                return Err(Error::InvalidState);
            }
        }
        self.state = self.hardware_state()?;
        self.media.observe(self.state, next, &mut self.device)?;
        let status = self.device.tp(0, 0, [0; 20])?;
        if status.error != 0 {
            return Err(Error::InvalidState);
        }
        if protocol::get(&status.words, 0, 64) != next.0 {
            return Err(Error::InvalidFrame);
        }
        self.events.clear();
        for i in 0..protocol::get(&status.words, 192, 32) {
            let event = self.device.tp(15, i as u32, [0; 20])?;
            if event.error != 0 {
                return Err(Error::InvalidState);
            }
            self.events.push(protocol::event(&event.words)?);
        }
        self.frame = next;
        self.ingress
            .publish(Diagnostics {
                frame: next,
                epoch: self.epoch,
                ..Default::default()
            })
            .ok();
        Ok(output)
    }
    fn fail_pending(&mut self, error: Error) -> Result<(), Error> {
        for pending in self.pending.drain(..) {
            self.scheduler
                .receipts(self.frame)
                .complete(pending.token, Err(error))?;
        }
        while let Some(due) = self.scheduler.due(Frame(u64::MAX)) {
            let (sequence, action) = (due.sequence, due.action);
            if matches!(action, Action::Planner { .. }) {
                self.scheduler
                    .receipts(self.frame)
                    .cancel_queued(sequence, Reply::Failed(Failure::Runtime(error)))?;
            } else {
                self.scheduler.cancel(self.frame, sequence)?;
            }
        }
        Ok(())
    }
}
fn identity(slot: u32, generation: u32) -> Identity {
    Identity { slot, generation }
}
fn supported(operation: LoopOperation<Q824>) -> bool {
    matches!(
        operation,
        LoopOperation::Transition {
            timing: loops::Timing::At,
            transition: loops::Transition::Record {
                monitor_previous: false
            } | loops::Transition::Stop
                | loops::Transition::Play {
                    position: 0,
                    retrigger: false
                },
            ..
        } | LoopOperation::State { .. }
            | LoopOperation::Export {
                selection: Selection::Latest | Selection::Playback,
                ..
            }
            | LoopOperation::Cursor { midi: false, .. }
            | LoopOperation::Release { .. }
            | LoopOperation::Cancel { .. }
    )
}
fn recovery_safe(operation: LoopOperation<Q824>) -> bool {
    matches!(
        operation,
        LoopOperation::Cancel { .. }
            | LoopOperation::Transition {
                timing: loops::Timing::At,
                transition: loops::Transition::Stop,
                ..
            }
    )
}
fn command(
    device: &mut impl LoopDevice,
    op: u8,
    index: u32,
    w: [u32; 20],
) -> Result<[u32; 26], Failure> {
    let result = device.tp(op, index, w)?;
    if result.error != 0 {
        return Err(Failure::Timeline(protocol::error(u64::from(result.error))?));
    }
    Ok(result.words)
}
#[allow(clippy::too_many_arguments)]
fn deliver_planner(
    device: &mut impl LoopDevice,
    media: &mut loop_media::Media,
    pending: &mut Vec<Pending>,
    receipts: &mut rt_runtime::schedule::Receipts<
        '_,
        LoopOperation<Q824>,
        Reply<Q824>,
        Operation,
        (),
    >,
    startup: &Startup,
    operation: LoopOperation<Q824>,
    token: ReceiptToken,
    lane: Lane,
    requested: Frame,
) -> Result<Option<Reply<Q824>>, Failure> {
    match operation {
        LoopOperation::Transition { target, .. }
        | LoopOperation::State { target }
        | LoopOperation::Export { target, .. }
            if target != startup.binding.target =>
        {
            return Err(Failure::Timeline(loops::Error::InvalidId));
        }
        _ => {}
    }
    match operation {
        LoopOperation::Transition {
            target,
            timing,
            transition,
        } => {
            if pending.iter().filter(|p| p.lane == lane).count()
                >= if lane == Lane::Normal { 4 } else { 1 }
            {
                return Err(Error::Capacity.into());
            }
            let words = command(
                device,
                4,
                0,
                protocol::transition(loops::Request {
                    target,
                    sequence: token.sequence(),
                    at: requested,
                    timing,
                    transition,
                })?,
            )?;
            let id = RequestId::from_parts(
                protocol::get(&words, 0, 32) as u32,
                protocol::get(&words, 32, 32) as u32,
            )?;
            pending.push(Pending {
                token,
                sequence: token.sequence(),
                lane,
                kind: PendingKind::Transition(id),
            });
            Ok(None)
        }
        LoopOperation::State { .. } | LoopOperation::Export { .. } => {
            if pending.iter().filter(|p| p.lane == lane).count()
                >= if lane == Lane::Normal { 4 } else { 1 }
            {
                return Err(Error::Capacity.into());
            }
            pending.push(Pending {
                token,
                sequence: token.sequence(),
                lane,
                kind: PendingKind::Query(operation),
            });
            Ok(None)
        }
        LoopOperation::Cursor { export, .. } => Ok(Some(Reply::Cursor(media.cursor(export)?))),
        LoopOperation::Release { export } => {
            media.release(export, device)?;
            Ok(Some(Reply::Done))
        }
        LoopOperation::Cancel { sequence } => {
            if let Some(target) = pending.iter().find(|p| p.sequence == sequence) {
                if let PendingKind::Transition(id) = target.kind {
                    let mut w = [0; 20];
                    w[0] = id.generation();
                    command(device, 5, id.slot(), w)?;
                    Ok(Some(Reply::Cancelled {
                        sequence,
                        delivered: true,
                    }))
                } else {
                    Err(Error::InvalidState.into())
                }
            } else {
                receipts.planner().cancel_queued(
                    sequence,
                    Reply::Cancelled {
                        sequence,
                        delivered: false,
                    },
                )?;
                Ok(Some(Reply::Cancelled {
                    sequence,
                    delivered: false,
                }))
            }
        }
        _ => Err(Error::InvalidParameter.into()),
    }
}

fn descriptions(startup: &Startup, nodes: &[Prepared]) -> Result<Vec<NodeDescriptor>, Error> {
    let mut kinds = [0; 3];
    let mut descriptors = vec![];
    for prepared in nodes {
        let id = prepared.spec().id;
        let operation = match prepared.processor() {
            Processor::Loop(node) => {
                let binding = node.binding();
                if binding.target != startup.binding.target
                    || binding.playback != startup.binding.playback
                    || binding.capture != startup.binding.capture
                    || binding.channels != 1
                    || binding.auxiliary_channels != 0
                    || binding.midi
                    || binding.dry_wet
                    || binding.gain != Q824(1 << 24)
                    || binding.smoothing != loops::audio::Smoothing::None
                {
                    return Err(Error::InvalidShape);
                }
                match node {
                    loops::runtime::LoopNode::Playback(_) if id == binding.playback => {
                        kinds[0] += 1;
                        NodeKind::Playback {
                            target: identity(binding.target.slot(), binding.target.generation()),
                        }
                    }
                    loops::runtime::LoopNode::Capture(_) if id == binding.capture => {
                        kinds[1] += 1;
                        NodeKind::Capture {
                            target: identity(binding.target.slot(), binding.target.generation()),
                        }
                    }
                    _ => return Err(Error::InvalidId),
                }
            }
            Processor::Gain(gain) => {
                if gain.channels() != 1 || gain.frames() != 32 || !gain.steady() {
                    return Err(Error::InvalidParameter);
                }
                kinds[2] += 1;
                NodeKind::Gain {
                    channels: 1,
                    initial: gain.gain().0,
                }
            }
        };
        descriptors.push(NodeDescriptor {
            id: identity(id.slot(), id.generation()),
            channels: 1,
            operation,
        });
    }
    if kinds != [1, 1, 1] {
        return Err(Error::InvalidShape);
    }
    Ok(descriptors)
}

fn compile_profile(
    config: PreparedConfig,
    startup: &Startup,
    nodes: &[Prepared],
    catalog: &Catalog,
    plan: &PreparedPlan,
) -> Result<Image, Error> {
    let mut validator = PlanValidator::new(config);
    validator.begin();
    loop {
        match validator.advance(plan, &catalog, 1024).status {
            ValidationStatus::Valid => break,
            ValidationStatus::Invalid(e) => return Err(e),
            ValidationStatus::Pending => {}
        }
    }
    if nodes.iter().any(|n| !plan.contains(n.spec().id)) {
        return Err(Error::InvalidShape);
    }
    if plan
        .descriptor()
        .connections
        .iter()
        .any(|e| e.source.kind != rt_runtime::graph::PortType::Audio)
    {
        return Err(Error::InvalidParameter);
    }
    let transition = compiler::transition_image(&startup.topology, 32, |_| false)?;
    let id = startup.binding.target;
    let image = compiler::lower(
        plan.descriptor(),
        &descriptions(startup, nodes)?,
        Image {
            format_version: 1,
            name: "runtime-mono-loop".into(),
            transition,
            program: Program {
                instructions: vec![],
                quantum: 32,
                chunk_frames: 32,
                jobs_per_quantum: 0,
            },
            initial_state: Memory { memory: vec![] },
            buffers: 0,
            input_ports: 1,
            output_ports: 1,
            media: MediaImage {
                chunk_frames: 32,
                samples: vec![],
                free_chunks: vec![],
                views: vec![View {
                    chunks: vec![],
                    frames: 0,
                }],
            },
            loop_views: vec![LoopViews {
                id: identity(id.slot(), id.generation()),
                views: vec![0],
            }],
            gains: vec![],
            node_order: vec![],
        },
    )
    .map_err(|_| Error::InvalidGraph)?;
    if image.program.instructions.len() > 64
        || image.initial_state.memory.len() > 32
        || image.program.jobs_per_quantum > 64
        || image.buffers > 16
        || image.transition.limits.loop_slots > 4
        || image.transition.limits.pending_requests > 8
        || image.transition.limits.events > 16
    {
        return Err(Error::Capacity);
    }
    Ok(image)
}
