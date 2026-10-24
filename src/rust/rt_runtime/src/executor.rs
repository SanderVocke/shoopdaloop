//! Single-owner graph executor with stable processor slots and transactional plans.

use crate::buffer::{AudioBlock, AudioPorts, MidiBlock, MidiBuffer};
use crate::graph::{
    Catalog, Connection, Endpoint, NodeSpec, PlanValidator, PortCounts, PortType, PreparedPlan,
    ValidationStatus,
};
use crate::processor::{ProcessBlock, Processor};
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
}
impl<P> Command<P> {
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
}

pub type Control<P> = ControlEndpoint<Command<P>, Retired<P>, Diagnostics>;
type Ingress<P> = RenderEndpoint<Command<P>, Retired<P>, Diagnostics>;

struct Buffers {
    input: Box<[f32]>,
    output: Box<[f32]>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
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
        };
        for sample in buffers.input.iter_mut().chain(&mut buffers.output) {
            *std::hint::black_box(sample) = 0.0;
        }
        Ok(buffers)
    }
    fn clear(&mut self, stride: usize, frames: usize) {
        for channel in self.input.chunks_exact_mut(stride) {
            channel[..frames].fill(0.0);
        }
        for channel in self.output.chunks_exact_mut(stride) {
            channel[..frames].fill(0.0);
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
}

impl<P: Processor> GraphRuntime<P> {
    /// Allocate slots, buffers, validation scratch, and ownership queues before
    /// callbacks begin. Only declared slots can be reused after startup.
    pub fn prepare(
        config: PreparedConfig,
        nodes: Vec<PreparedNode<P>>,
    ) -> Result<(Control<P>, Self), Error> {
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
        Ok((
            control,
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
        self.ingress.begin_cycle(frame, self.epoch);
        self.apply_commands();
        self.diagnostics.epoch = self.epoch;
        let result = self.run_graph(&mut audio, &mut midi);
        if let Err(error) = result {
            audio.clear_bounded(limits.audio_outputs, frames);
            for port in midi.outputs.iter_mut() {
                port.clear();
            }
            self.diagnostics.render_error = Some(error);
        }
        let _ = self.ingress.publish(self.diagnostics);
        result
    }

    fn apply_commands(&mut self) {
        if self.ingress_fault {
            return;
        }
        let limits = self.clock.config().limits();
        let mut remaining = limits.budgets.validation_steps;
        for _ in 0..limits.budgets.commands {
            let Some(operation) = self.ingress.next_operation() else {
                break;
            };
            let mut retired = None;
            let result = match operation.request {
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
                        let progress = self.validator.advance(candidate, &self.catalog, remaining);
                        remaining -= progress.steps;
                        self.diagnostics.validation_steps += progress.steps;
                        match progress.status {
                            ValidationStatus::Pending => break,
                            ValidationStatus::Invalid(error) => Err(error),
                            ValidationStatus::Valid => match self.epoch.checked_next() {
                                Err(error) => Err(error),
                                Ok(epoch) => {
                                    if let Some(candidate) = plan.take() {
                                        retired = self.active.replace(candidate).map(Retired::Plan);
                                        self.epoch = epoch;
                                        Ok(())
                                    } else {
                                        Err(Error::InvalidState)
                                    }
                                }
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
                Ok(()) => Outcome::Applied,
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

    fn run_graph(
        &mut self,
        audio: &mut AudioBlock<'_, '_>,
        midi: &mut MidiBlock<'_>,
    ) -> Result<(), Error> {
        let Some(plan) = &self.active else {
            return Ok(());
        };
        let stride = self.clock.config().limits().max_frames;
        let frames = audio.frames;
        let mut first_edge = 0;
        for step in &plan.descriptor().steps {
            let slot = self.slots[step.node.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?;
            slot.buffers.clear(stride, frames);
            for edge in &plan.descriptor().connections[first_edge..step.input_end] {
                route(&mut self.slots, *edge, audio, midi, stride)?;
                self.diagnostics.connections_processed += 1;
            }
            let slot = self.slots[step.node.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?;
            let buffers = &mut slot.buffers;
            let mut block = ProcessBlock {
                frame: audio.frame,
                frames,
                audio: AudioPorts::new(&buffers.input, &mut buffers.output, stride, 0, frames)?,
                midi_inputs: &buffers.midi_input,
                midi_outputs: &mut buffers.midi_output,
            };
            slot.processor
                .as_mut()
                .ok_or(Error::InvalidId)?
                .process(&mut block)?;
            self.diagnostics.nodes_processed += 1;
            first_edge = step.input_end;
        }
        for edge in &plan.descriptor().connections[first_edge..] {
            route(&mut self.slots, *edge, audio, midi, stride)?;
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
                    channel(&source.output, edge.source, stride, audio.frames),
                    channel_mut(&mut target.input, edge.destination, stride, audio.frames),
                ),
                PortType::Midi => target.midi_input[edge.destination.port as usize]
                    .merge(&source.midi_output[edge.source.port as usize], audio.frames)?,
            }
        }
        (None, Some(target)) => {
            let target = &mut slots[target.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?
                .buffers;
            match edge.source.kind {
                PortType::Audio => mix(
                    &audio.inputs[edge.source.port as usize][..audio.frames],
                    channel_mut(&mut target.input, edge.destination, stride, audio.frames),
                ),
                PortType::Midi => target.midi_input[edge.destination.port as usize]
                    .merge(&midi.inputs[edge.source.port as usize], audio.frames)?,
            }
        }
        (Some(source), None) => {
            let source = &slots[source.slot() as usize]
                .as_ref()
                .ok_or(Error::InvalidId)?
                .buffers;
            match edge.source.kind {
                PortType::Audio => mix(
                    channel(&source.output, edge.source, stride, audio.frames),
                    &mut audio.outputs[edge.destination.port as usize][..audio.frames],
                ),
                PortType::Midi => midi.outputs[edge.destination.port as usize]
                    .merge(&source.midi_output[edge.source.port as usize], audio.frames)?,
            }
        }
        (None, None) => match edge.source.kind {
            PortType::Audio => mix(
                &audio.inputs[edge.source.port as usize][..audio.frames],
                &mut audio.outputs[edge.destination.port as usize][..audio.frames],
            ),
            PortType::Midi => midi.outputs[edge.destination.port as usize]
                .merge(&midi.inputs[edge.source.port as usize], audio.frames)?,
        },
    }
    Ok(())
}
fn channel(data: &[f32], endpoint: Endpoint, stride: usize, frames: usize) -> &[f32] {
    let first = endpoint.port as usize * stride;
    &data[first..first + frames]
}
fn channel_mut(data: &mut [f32], endpoint: Endpoint, stride: usize, frames: usize) -> &mut [f32] {
    let first = endpoint.port as usize * stride;
    &mut data[first..first + frames]
}
fn mix(source: &[f32], target: &mut [f32]) {
    for (source, target) in source.iter().zip(target) {
        *target += source;
    }
}
