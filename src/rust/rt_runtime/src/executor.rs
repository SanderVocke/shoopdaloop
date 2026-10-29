//! Single-owner graph executor with stable processor slots and transactional plans.

use crate::buffer::{AudioBlock, AudioPorts, MidiBlock, MidiBuffer};
use crate::graph::{
    Catalog, Connection, Endpoint, NodeSpec, PlanValidator, PortCounts, PortType, PreparedPlan,
    ValidationStatus,
};
use crate::midi::ResetState;
use crate::processor::{ProcessBlock, Processor};
use crate::resource::{self, ResourceStore};
use crate::schedule::{self, Action, Execution, Scheduler};
use crate::transport::{self, ControlEndpoint, Lane, Outcome, RenderEndpoint, TransportConfig};
use crate::{Epoch, Error, Frame, NodeId, PreparedConfig, RenderRuntime, Sequence};

/// Capture a prepared processor's port signature outside rendering. A runtime slot
/// can later accept another generation with the same signature, not a larger one.
pub struct PreparedNode<P> {
    id: NodeId,
    ports: PortCounts,
    processor: P,
}
impl<P: Processor> PreparedNode<P> {
    pub fn new(id: NodeId, processor: P) -> Self {
        Self {
            id,
            ports: processor.ports(),
            processor,
        }
    }
    pub fn spec(&self) -> NodeSpec {
        NodeSpec {
            id: self.id,
            ports: self.ports,
        }
    }
}

pub enum Command<P> {
    Commit {
        expected_epoch: Epoch,
        plan: Option<PreparedPlan>,
    },
    Remove {
        node: NodeId,
    },
    Install {
        prepared: Option<PreparedNode<P>>,
    },
    Schedule {
        at: Frame,
        action: Action,
        payload: Box<[u8]>,
    },
    ResetMidi,
    Resource(resource::ResourceCommand),
}
impl<P> Command<P> {
    pub fn resource(request: resource::Request) -> Self {
        Self::Resource(resource::ResourceCommand::new(request))
    }
    pub fn commit(expected_epoch: Epoch, plan: PreparedPlan) -> Self {
        Self::Commit {
            expected_epoch,
            plan: Some(plan),
        }
    }
}

pub enum Retired<P> {
    Plan(PreparedPlan),
    Processor(P),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    pub frame: Frame,
    pub epoch: Epoch,
    pub validation_steps: usize,
    pub nodes_processed: usize,
    pub connections_processed: usize,
    pub render_error: Option<Error>,
    pub scheduled_events: usize,
    pub subranges: usize,
    pub transfer_bytes: usize,
    pub midi_overflows: usize,
    pub reclaimed_pages: usize,
    pub reclaimed_versions: usize,
}

pub struct Control<P> {
    transport: ControlEndpoint<Command<P>, Retired<P>, Diagnostics>,
    executions: schedule::Results,
    config: PreparedConfig,
}
impl<P> Control<P> {
    pub fn prepare_submissions(&mut self) -> Result<(), Error> {
        self.transport.prepare_submissions()
    }
    pub fn submit(
        &mut self,
        lane: Lane,
        request: Command<P>,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<P>>> {
        self.submit_inner(lane, request, false)
    }
    pub fn submit_prepared(
        &mut self,
        lane: Lane,
        request: Command<P>,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<P>>> {
        self.submit_inner(lane, request, true)
    }
    fn submit_inner(
        &mut self,
        lane: Lane,
        mut request: Command<P>,
        prepared: bool,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<P>>> {
        if let Command::Resource(command) = &mut request {
            command.reset_reply();
            if let Err(error) = command.envelope(self.config) {
                return Err(transport::SubmitError {
                    error,
                    value: request,
                });
            }
        }
        if prepared {
            self.transport.submit_prepared(lane, request)
        } else {
            self.transport.submit(lane, request)
        }
    }
    pub fn try_completion(&mut self) -> Option<transport::Completion<Command<P>, Retired<P>>> {
        self.transport.try_completion()
    }
    pub fn try_execution(&mut self) -> Option<Execution> {
        self.executions.try_execution()
    }
    pub fn try_telemetry(&mut self) -> Option<Diagnostics> {
        self.transport.try_telemetry()
    }
    pub fn outstanding(&self) -> usize {
        self.transport.outstanding()
    }
    pub fn connected(&self) -> bool {
        self.transport.connected()
    }
}
type Ingress<P> = RenderEndpoint<Command<P>, Retired<P>, Diagnostics>;

struct Buffers {
    input: Box<[f32]>,
    output: Box<[f32]>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
    injected: Vec<MidiBuffer>,
}
impl Buffers {
    fn new(config: PreparedConfig, ports: PortCounts) -> Result<Self, Error> {
        let limits = config.limits();
        let midi = |count| {
            (0..count)
                .map(|_| MidiBuffer::new(limits.midi_events_per_port, limits.midi_bytes_per_port))
                .collect::<Result<Vec<_>, _>>()
        };
        let mut buffers = Self {
            input: vec![0.0; ports.audio_inputs * limits.max_frames].into_boxed_slice(),
            output: vec![0.0; ports.audio_outputs * limits.max_frames].into_boxed_slice(),
            midi_input: midi(ports.midi_inputs)?,
            midi_output: midi(ports.midi_outputs)?,
            injected: midi(ports.midi_inputs)?,
        };
        for sample in buffers.input.iter_mut().chain(&mut buffers.output) {
            *std::hint::black_box(sample) = 0.0;
        }
        Ok(buffers)
    }
    fn clear(&mut self, stride: usize, start: usize, frames: usize) {
        for channel in self.input.chunks_exact_mut(stride) {
            channel[start..start + frames].fill(0.0);
        }
        for channel in self.output.chunks_exact_mut(stride) {
            channel[start..start + frames].fill(0.0);
        }
        for port in self.midi_input.iter_mut().chain(&mut self.midi_output) {
            port.clear();
        }
    }
}
struct Slot<P> {
    id: NodeId,
    ports: PortCounts,
    processor: Option<P>,
    buffers: Buffers,
}

pub struct GraphRuntime<P> {
    clock: RenderRuntime,
    catalog: Catalog,
    slots: Vec<Option<Slot<P>>>,
    active: Option<PreparedPlan>,
    validator: PlanValidator,
    validating: Option<Sequence>,
    epoch: Epoch,
    ingress: Ingress<P>,
    diagnostics: Diagnostics,
    emergency: Option<Retired<P>>,
    ingress_fault: bool,
    scheduler: Scheduler,
    recovery: Vec<ResetState>,
    recovery_remaining: Vec<usize>,
    resources: ResourceStore,
}

impl<P: Processor> GraphRuntime<P> {
    /// Allocate slots, buffers, validation scratch, and ownership queues before
    /// callbacks begin. Only declared slots can be reused after startup.
    pub fn prepare(
        config: PreparedConfig,
        nodes: Vec<PreparedNode<P>>,
    ) -> Result<(Control<P>, Self), Error> {
        Self::prepare_with_resources(config, nodes, ResourceStore::new(config)?)
    }

    /// Move prepared resource identities/content into the runtime before starting
    /// callbacks. Processors may retain plain IDs/leases created in this store.
    pub fn prepare_with_resources(
        config: PreparedConfig,
        nodes: Vec<PreparedNode<P>>,
        resources: ResourceStore,
    ) -> Result<(Control<P>, Self), Error> {
        if resources.config() != config {
            return Err(Error::InvalidConfig);
        }
        if config
            .limits()
            .max_nodes
            .checked_mul(size_of::<Option<Slot<P>>>())
            .is_none_or(|bytes| bytes > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
        let specs: Vec<_> = nodes.iter().map(PreparedNode::spec).collect();
        let catalog = Catalog::new(config, &specs)?;
        let limits = config.limits();
        let mut slots: Vec<_> = (0..limits.max_nodes).map(|_| None).collect();
        for node in nodes {
            let buffers = Buffers::new(config, node.ports)?;
            slots[node.id.slot() as usize] = Some(Slot {
                id: node.id,
                ports: node.ports,
                processor: Some(node.processor),
                buffers,
            });
        }
        let (control, ingress) = transport::channel(TransportConfig {
            normal_slots: limits.command_slots,
            recovery_slots: 1,
            telemetry_slots: 3,
            operations_per_cycle: limits.budgets.commands,
            recovery_per_cycle: 1,
            publications_per_cycle: limits.budgets.publications,
        })?;
        let (executions, scheduler) = schedule::scheduler(config)?;
        Ok((
            Control {
                transport: control,
                executions,
                config,
            },
            Self {
                clock: RenderRuntime::new(config),
                catalog,
                slots,
                active: None,
                validator: PlanValidator::new(config),
                validating: None,
                epoch: Epoch(0),
                ingress,
                diagnostics: Diagnostics::default(),
                emergency: None,
                ingress_fault: false,
                scheduler,
                recovery: vec![ResetState::default(); limits.midi_outputs],
                recovery_remaining: vec![0; limits.midi_outputs],
                resources,
            },
        ))
    }

    /// Borrow the catalog only from the owner while no callback is active. Hosts
    /// should retain their control-side signature description for compiling edits.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
    pub fn next_frame(&self) -> Frame {
        self.clock.next_frame()
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }

    /// Coherent pool counters, read by the runtime owner between callbacks.
    pub fn resource_usage(&self) -> resource::PoolUsage {
        self.resources.usage()
    }

    pub fn process(
        &mut self,
        mut audio: AudioBlock<'_, '_>,
        mut midi: MidiBlock<'_>,
    ) -> Result<(), Error> {
        if audio.frames == 0 {
            return Ok(());
        }
        let limits = self.clock.config().limits();
        audio.clear_bounded(limits.audio_outputs, audio.frames.min(limits.max_frames));
        for output in midi.outputs.iter_mut().take(limits.midi_outputs) {
            output.clear();
        }
        if midi.inputs.len() != limits.midi_inputs || midi.outputs.len() != limits.midi_outputs {
            return Err(Error::InvalidShape);
        }
        for port in midi.inputs {
            if port.len() > limits.midi_events_per_port
                || port.byte_len() > limits.midi_bytes_per_port
            {
                return Err(Error::Capacity);
            }
            for index in 0..port.len() {
                if port.get(index).ok_or(Error::InvalidEvent)?.0 as usize >= audio.frames {
                    return Err(Error::InvalidEvent);
                }
            }
        }
        let frame = audio.frame;
        let frames = audio.frames;
        self.clock.process(AudioBlock {
            frame,
            frames,
            inputs: audio.inputs,
            outputs: audio.outputs,
        })?;
        self.diagnostics = Diagnostics {
            frame,
            epoch: self.epoch,
            ..Diagnostics::default()
        };
        self.recovery_remaining.fill(limits.budgets.events);
        self.scheduler.begin_callback();
        self.resources.begin_callback();
        self.resources.control_phase(true);
        self.ingress.begin_cycle(frame, self.epoch);
        self.apply_commands();
        self.resources.control_phase(false);
        self.diagnostics.epoch = self.epoch;
        let result = self.run_scheduled(&mut audio, &mut midi);
        if let Err(error) = result {
            audio.clear_bounded(limits.audio_outputs, frames);
            for port in midi.outputs.iter_mut() {
                port.clear();
            }
            for slot in self.slots.iter_mut().flatten() {
                for input in &mut slot.buffers.injected {
                    input.clear();
                }
            }
            self.diagnostics.render_error = Some(error);
        }
        if self.diagnostics.midi_overflows > 0 {
            for recovery in &mut self.recovery {
                recovery.request();
            }
        }
        for (output, recovery) in midi.outputs.iter_mut().zip(&self.recovery) {
            if recovery.pending() {
                output.clear();
            }
        }
        self.resources.reclaim();
        let work = self.resources.work();
        self.diagnostics.transfer_bytes = work.copied_bytes;
        self.diagnostics.validation_steps = work.validation_steps;
        self.diagnostics.reclaimed_pages = work.reclaimed_pages;
        self.diagnostics.reclaimed_versions = work.reclaimed_versions;
        let _ = self.ingress.publish(self.diagnostics);
        result
    }

    fn apply_commands(&mut self) {
        if self.ingress_fault {
            return;
        }
        let limits = self.clock.config().limits();
        for _ in 0..limits.budgets.commands {
            let Some(operation) = self.ingress.next_operation() else {
                break;
            };
            let mut retired = None;
            let mut admitted = None;
            let result = match operation.request {
                Command::Resource(command) => {
                    if operation.lane == Lane::Recovery && !command.recovery_safe() {
                        Err(Error::InvalidState)
                    } else {
                        command.apply(&mut self.resources, self.diagnostics.frame)
                    }
                }
                Command::ResetMidi => {
                    for reset in &mut self.recovery {
                        reset.request();
                    }
                    Ok(())
                }
                Command::Schedule {
                    at,
                    action,
                    payload,
                } => {
                    if operation.lane != Lane::Normal {
                        Err(Error::InvalidState)
                    } else if payload.len() > self.scheduler.payload_limit()
                        || payload.len() > limits.budgets.transfer_bytes - limits.resource_io_bytes
                    {
                        Err(Error::Capacity)
                    } else if payload.len() > self.resources.copy_remaining() {
                        break;
                    } else {
                        let target =
                            self.catalog
                                .node(action.node())
                                .and_then(|spec| match action {
                                    Action::Midi { port, .. }
                                        if *port as usize >= spec.ports.midi_inputs =>
                                    {
                                        Err(Error::InvalidGraph)
                                    }
                                    _ => Ok(()),
                                });
                        target
                            .and_then(|()| {
                                self.scheduler.admit(
                                    operation.sequence,
                                    *at,
                                    self.diagnostics.frame,
                                    *action,
                                    payload,
                                )
                            })
                            .and_then(|admission| {
                                self.resources.charge_external(payload.len(), 0)?;
                                admitted = Some(admission);
                                Ok(())
                            })
                    }
                }
                Command::Commit {
                    expected_epoch,
                    plan,
                } => {
                    // Structural edits are ordered on the normal lane only.
                    if operation.lane != Lane::Normal {
                        Err(Error::InvalidState)
                    } else if *expected_epoch != self.epoch {
                        Err(Error::StaleEpoch)
                    } else if let Some(candidate) = plan.as_ref() {
                        if self.validating != Some(operation.sequence) {
                            self.validator.begin();
                            self.validating = Some(operation.sequence);
                        }
                        let progress = self.validator.advance(
                            candidate,
                            &self.catalog,
                            self.resources.steps_remaining(),
                        );
                        match self.resources.charge_external(0, progress.steps) {
                            Err(error) => Err(error),
                            Ok(()) => match progress.status {
                                ValidationStatus::Pending => break,
                                ValidationStatus::Invalid(error) => Err(error),
                                ValidationStatus::Valid => match self.epoch.checked_next() {
                                    Err(error) => Err(error),
                                    Ok(epoch) => {
                                        if let Some(candidate) = plan.take() {
                                            retired =
                                                self.active.replace(candidate).map(Retired::Plan);
                                            self.epoch = epoch;
                                            Ok(())
                                        } else {
                                            Err(Error::InvalidState)
                                        }
                                    }
                                },
                            },
                        }
                    } else {
                        Err(Error::InvalidGraph)
                    }
                }
                Command::Remove { node } => {
                    if operation.lane != Lane::Normal {
                        Err(Error::InvalidState)
                    } else if self
                        .active
                        .as_ref()
                        .is_some_and(|plan| plan.contains(*node))
                    {
                        Err(Error::Busy)
                    } else {
                        remove_processor(&mut self.slots, &mut self.catalog, &mut self.epoch, *node)
                            .map(|processor| retired = Some(Retired::Processor(processor)))
                    }
                }
                Command::Install { prepared } => {
                    if operation.lane != Lane::Normal {
                        Err(Error::InvalidState)
                    } else {
                        install_processor(
                            &mut self.slots,
                            &mut self.catalog,
                            &mut self.epoch,
                            prepared,
                        )
                    }
                }
            };
            self.validating = None;
            self.ingress.set_epoch(self.epoch);
            let outcome = match result {
                Ok(()) => admitted.map_or(Outcome::Applied, |admission| Outcome::Scheduled {
                    effective_frame: admission.effective_frame,
                    late_by: admission.late_by,
                }),
                Err(error) => Outcome::Rejected(error),
            };
            // next_operation reserves a completion budget before lending a request.
            // No other publication uses terminal budget while it is borrowed.
            if let Err(error) = self.ingress.complete(outcome, retired) {
                // This cannot occur under the above protocol. Preserve any owning
                // error bundle in a fixed emergency slot rather than drop it.
                self.emergency_retirement(error.retired);
                break;
            }
        }
    }

    fn emergency_retirement(&mut self, retired: Option<Retired<P>>) {
        // Faulted ingress is never entered again, so this slot cannot overwrite
        // another owner. The host reclaims it after stopping callbacks.
        self.ingress_fault = true;
        self.emergency = retired;
    }

    pub fn midi_recovery_pending(&self, port: usize) -> Result<bool, Error> {
        self.recovery
            .get(port)
            .map(ResetState::pending)
            .ok_or(Error::InvalidId)
    }

    /// Host recovery sink, separate from ordinary MIDI output. Reserve sink space
    /// for resets and call at most once per output per callback. Full sinks retain
    /// the exact next reset packet; ordinary output stays suppressed meanwhile.
    pub fn drain_midi_recovery(
        &mut self,
        port: usize,
        sink: &mut MidiBuffer,
        frames: usize,
    ) -> Result<usize, Error> {
        let limits = self.clock.config().limits();
        if frames == 0 || frames > limits.max_frames {
            return Err(Error::InvalidShape);
        }
        let reset = self.recovery.get_mut(port).ok_or(Error::InvalidId)?;
        let remaining = self
            .recovery_remaining
            .get_mut(port)
            .ok_or(Error::InvalidId)?;
        let mut emitted = 0;
        for _ in 0..*remaining {
            let Some(packet) = reset.peek() else {
                break;
            };
            *remaining -= 1;
            match sink.push(0, frames, &packet) {
                Ok(()) => {
                    reset.acknowledge()?;
                    emitted += 1;
                }
                Err(Error::Full) => break,
                Err(error) => return Err(error),
            }
        }
        Ok(emitted)
    }

    fn run_scheduled(
        &mut self,
        audio: &mut AudioBlock<'_, '_>,
        midi: &mut MidiBlock<'_>,
    ) -> Result<(), Error> {
        let limits = self.clock.config().limits();
        let mut start = 0;
        for _ in 0..limits.budgets.subranges {
            if start == audio.frames {
                return Ok(());
            }
            let at = audio.frame.checked_add(start)?;
            for _ in 0..limits.budgets.events {
                let Some(event) = self.scheduler.due(at) else {
                    break;
                };
                let sequence = event.sequence;
                let node = event.action.node();
                let result = self
                    .slots
                    .get_mut(node.slot() as usize)
                    .and_then(Option::as_mut)
                    .filter(|slot| slot.id == node && slot.processor.is_some())
                    .ok_or(Error::InvalidId)
                    .and_then(|slot| match event.action {
                        Action::Parameter {
                            parameter, value, ..
                        } => slot
                            .processor
                            .as_mut()
                            .ok_or(Error::InvalidId)?
                            .set_parameter(parameter, value),
                        Action::Midi { port, .. } => {
                            if !self.active.as_ref().is_some_and(|plan| plan.contains(node)) {
                                return Err(Error::InvalidId);
                            }
                            let input = slot
                                .buffers
                                .injected
                                .get_mut(port as usize)
                                .ok_or(Error::InvalidGraph)?;
                            let result = input.push(0, 1, event.payload);
                            if result == Err(Error::Full) {
                                input.force_reset(0);
                            }
                            result
                        }
                    });
                if result == Err(Error::Full) {
                    self.diagnostics.midi_overflows += 1;
                }
                self.scheduler.finish(sequence, at, result)?;
                self.diagnostics.scheduled_events += 1;
            }
            let end = self.scheduler.next_frame().map_or(audio.frames, |frame| {
                frame
                    .0
                    .saturating_sub(audio.frame.0)
                    .min(audio.frames as u64) as usize
            });
            if end <= start {
                return Err(Error::InvalidState);
            }
            self.run_graph_range(audio, midi, start, end - start)?;
            self.diagnostics.subranges += 1;
            start = end;
        }
        if start == audio.frames {
            Ok(())
        } else {
            Err(Error::Capacity)
        }
    }

    fn run_graph_range(
        &mut self,
        audio: &mut AudioBlock<'_, '_>,
        midi: &mut MidiBlock<'_>,
        start: usize,
        frames: usize,
    ) -> Result<(), Error> {
        let Some(plan) = &self.active else {
            return Ok(());
        };
        let stride = self.clock.config().limits().max_frames;
        let mut first_edge = 0;
        for step in &plan.descriptor().steps {
            let slot = self.slots[step.node.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?;
            slot.buffers.clear(stride, start, frames);
            for edge in &plan.descriptor().connections[first_edge..step.input_end] {
                record_midi_overflow(
                    route(&mut self.slots, *edge, audio, midi, stride, start, frames),
                    &mut self.diagnostics,
                )?;
                self.diagnostics.connections_processed += 1;
            }
            let slot = self.slots[step.node.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?;
            let buffers = &mut slot.buffers;
            for (input, injected) in buffers.midi_input.iter_mut().zip(&mut buffers.injected) {
                record_midi_overflow(input.merge(injected, frames), &mut self.diagnostics)?;
                injected.clear();
            }
            let mut block = ProcessBlock {
                frame: audio.frame.checked_add(start)?,
                frames,
                audio: AudioPorts::new(&buffers.input, &mut buffers.output, stride, start, frames)?,
                midi_inputs: &buffers.midi_input,
                midi_outputs: &mut buffers.midi_output,
                resources: self.resources.io(),
            };
            let result = slot
                .processor
                .as_mut()
                .ok_or(Error::InvalidId)?
                .process(&mut block);
            if result == Err(Error::Full) {
                for output in &mut buffers.midi_output {
                    output.force_reset(0);
                }
            }
            record_midi_overflow(result, &mut self.diagnostics)?;
            self.diagnostics.nodes_processed += 1;
            first_edge = step.input_end;
        }
        for edge in &plan.descriptor().connections[first_edge..] {
            record_midi_overflow(
                route(&mut self.slots, *edge, audio, midi, stride, start, frames),
                &mut self.diagnostics,
            )?;
            self.diagnostics.connections_processed += 1;
        }
        Ok(())
    }
}

fn remove_processor<P>(
    slots: &mut [Option<Slot<P>>],
    catalog: &mut Catalog,
    epoch: &mut Epoch,
    node: NodeId,
) -> Result<P, Error> {
    let slot = slots
        .get_mut(node.slot() as usize)
        .and_then(Option::as_mut)
        .ok_or(Error::InvalidId)?;
    if slot.id != node || slot.processor.is_none() {
        return Err(Error::InvalidId);
    }
    let next_id = node.checked_next_generation()?;
    let next_epoch = epoch.checked_next()?;
    catalog.remove(node)?;
    let processor = slot.processor.take().ok_or(Error::InvalidId)?;
    slot.id = next_id;
    *epoch = next_epoch;
    Ok(processor)
}

fn install_processor<P>(
    slots: &mut [Option<Slot<P>>],
    catalog: &mut Catalog,
    epoch: &mut Epoch,
    prepared: &mut Option<PreparedNode<P>>,
) -> Result<(), Error> {
    let node = prepared.as_ref().ok_or(Error::InvalidState)?;
    let slot = slots
        .get_mut(node.id.slot() as usize)
        .and_then(Option::as_mut)
        .ok_or(Error::InvalidId)?;
    if slot.id != node.id || slot.processor.is_some() {
        return Err(Error::InvalidId);
    }
    if slot.ports != node.ports {
        return Err(Error::InvalidShape);
    }
    let next_epoch = epoch.checked_next()?;
    catalog.install(NodeSpec {
        id: node.id,
        ports: node.ports,
    })?;
    let node = prepared.take().ok_or(Error::InvalidState)?;
    slot.processor = Some(node.processor);
    *epoch = next_epoch;
    Ok(())
}

fn route<P>(
    slots: &mut [Option<Slot<P>>],
    edge: Connection,
    audio: &mut AudioBlock<'_, '_>,
    midi: &mut MidiBlock<'_>,
    stride: usize,
    start: usize,
    frames: usize,
) -> Result<(), Error> {
    match (edge.source.node, edge.destination.node) {
        (Some(source), Some(target)) => {
            let source = source.slot() as usize;
            let target = target.slot() as usize;
            if source == target {
                return Err(Error::InvalidGraph);
            }
            let (source, target) = if source < target {
                let (left, right) = slots.split_at_mut(target);
                (left[source].as_ref(), right[0].as_mut())
            } else {
                let (left, right) = slots.split_at_mut(source);
                (right[0].as_ref(), left[target].as_mut())
            };
            let source = &source.ok_or(Error::InvalidId)?.buffers;
            let target = &mut target.ok_or(Error::InvalidId)?.buffers;
            match edge.source.kind {
                PortType::Audio => mix(
                    channel(&source.output, edge.source, stride, start, frames),
                    channel_mut(&mut target.input, edge.destination, stride, start, frames),
                ),
                PortType::Midi => target.midi_input[edge.destination.port as usize]
                    .merge(&source.midi_output[edge.source.port as usize], frames)?,
            }
        }
        (None, Some(target)) => {
            let target = &mut slots[target.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?
                .buffers;
            match edge.source.kind {
                PortType::Audio => mix(
                    &audio.inputs[edge.source.port as usize][start..start + frames],
                    channel_mut(&mut target.input, edge.destination, stride, start, frames),
                ),
                PortType::Midi => target.midi_input[edge.destination.port as usize].merge_window(
                    &midi.inputs[edge.source.port as usize],
                    start,
                    frames,
                    0,
                    frames,
                )?,
            }
        }
        (Some(source), None) => {
            let source = &slots[source.slot() as usize]
                .as_ref()
                .ok_or(Error::InvalidId)?
                .buffers;
            match edge.source.kind {
                PortType::Audio => mix(
                    channel(&source.output, edge.source, stride, start, frames),
                    &mut audio.outputs[edge.destination.port as usize][start..start + frames],
                ),
                PortType::Midi => midi.outputs[edge.destination.port as usize].merge_window(
                    &source.midi_output[edge.source.port as usize],
                    0,
                    frames,
                    start,
                    audio.frames,
                )?,
            }
        }
        (None, None) => match edge.source.kind {
            PortType::Audio => mix(
                &audio.inputs[edge.source.port as usize][start..start + frames],
                &mut audio.outputs[edge.destination.port as usize][start..start + frames],
            ),
            PortType::Midi => midi.outputs[edge.destination.port as usize].merge_window(
                &midi.inputs[edge.source.port as usize],
                start,
                frames,
                start,
                audio.frames,
            )?,
        },
    }
    Ok(())
}
fn channel(data: &[f32], endpoint: Endpoint, stride: usize, start: usize, frames: usize) -> &[f32] {
    let first = endpoint.port as usize * stride + start;
    &data[first..first + frames]
}
fn channel_mut(
    data: &mut [f32],
    endpoint: Endpoint,
    stride: usize,
    start: usize,
    frames: usize,
) -> &mut [f32] {
    let first = endpoint.port as usize * stride + start;
    &mut data[first..first + frames]
}
fn record_midi_overflow(
    result: Result<(), Error>,
    diagnostics: &mut Diagnostics,
) -> Result<(), Error> {
    if result == Err(Error::Full) {
        diagnostics.midi_overflows += 1;
        Ok(())
    } else {
        result
    }
}
fn mix(source: &[f32], target: &mut [f32]) {
    for (source, target) in source.iter().zip(target) {
        *target += source;
    }
}
