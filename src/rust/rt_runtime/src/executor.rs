//! Single-owner graph executor with stable processor slots and transactional plans.

use crate::buffer::{AudioBlock, AudioPorts, MidiBlock, MidiBuffer};
use crate::graph::activation::{GraphAccess, GraphStages, StageId};
use crate::graph::{
    compile, Catalog, Connection, Endpoint, GraphDescription, NodeSpec, PlanValidator, PortCounts,
    PortType, PreparedPlan, ValidationStatus,
};
use crate::midi::ResetState;
use crate::planner::{
    Disposition, FinishContext, Interval, NoPlanner, OperationContext, PlanContext, Planner,
};
use crate::processor::{ProcessBlock, Processor, ProcessorOperation};
use crate::resource::{self, ResourceStore};
use crate::schedule::{self, Action, Execution, Scheduler};
use crate::transport::{self, ControlEndpoint, Lane, Outcome, RenderEndpoint, TransportConfig};
use crate::Sample;
use crate::{Epoch, Error, Frame, NodeId, PreparedConfig, RenderRuntime, Sequence};
use alloc::{boxed::Box, vec, vec::Vec};
use core::marker::PhantomData;

#[path = "executor_audio.rs"]
mod referenced_audio;

/// Capture a prepared processor's port signature outside rendering. A runtime slot
/// can later accept another generation with the same signature, not a larger one.
pub struct PreparedNode<P, L: Planner<S> = NoPlanner, Q: ProcessorOperation = (), S: Sample = f32> {
    id: NodeId,
    ports: PortCounts,
    operation_bytes: usize,
    midi_settings: Result<crate::processor::MidiBufferSettings, Error>,
    processor: P,
    marker: PhantomData<fn() -> (L, Q, S)>,
}
impl<P: Processor<L, Q, S>, L: Planner<S>, Q: ProcessorOperation, S: Sample>
    PreparedNode<P, L, Q, S>
{
    pub fn new(id: NodeId, processor: P) -> Self {
        Self {
            id,
            ports: processor.ports(),
            operation_bytes: processor.operation_bytes(),
            midi_settings: processor.midi_buffer_settings(),
            processor,
            marker: PhantomData,
        }
    }
    pub fn processor(&self) -> &P {
        &self.processor
    }
    pub fn into_processor(self) -> P {
        self.processor
    }
    pub fn spec(&self) -> NodeSpec {
        NodeSpec {
            id: self.id,
            ports: self.ports,
        }
    }
}

pub enum Command<P, L: Planner<S> = NoPlanner, Q: ProcessorOperation = (), S: Sample = f32> {
    StageGraph {
        slot: usize,
        expected_epoch: Epoch,
        plan: Option<PreparedPlan>,
        /// Filled only after complete validation and owning admission.
        id: Option<StageId>,
    },
    ReleaseGraph {
        id: StageId,
    },
    Commit {
        expected_epoch: Epoch,
        plan: Option<PreparedPlan>,
    },
    Remove {
        node: NodeId,
    },
    Install {
        prepared: Option<PreparedNode<P, L, Q, S>>,
    },
    Schedule {
        at: Frame,
        action: Action<L::Operation, Q>,
        payload: Box<[u8]>,
    },
    /// Cancel a scheduled parameter, MIDI or processor action before its delivery.
    Cancel {
        sequence: Sequence,
    },
    ResetMidi,
    Resource(resource::ResourceCommand),
}
impl<P, L: Planner<S>, Q: ProcessorOperation, S: Sample> Command<P, L, Q, S> {
    pub fn stage_graph(slot: usize, expected_epoch: Epoch, plan: PreparedPlan) -> Self {
        Self::StageGraph {
            slot,
            expected_epoch,
            plan: Some(plan),
            id: None,
        }
    }
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
    pub counter_overflow: bool,
    pub render_operations: usize,
    pub render_copied_bytes: usize,
    pub render_validation_steps: usize,
    pub resource_index_work: crate::resource::IndexWork,
    pub midi_index_work: crate::resource::MidiIndexWork,
}

pub struct Control<P, L: Planner<S> = NoPlanner, Q: ProcessorOperation = (), S: Sample = f32> {
    transport: ControlEndpoint<Command<P, L, Q, S>, Retired<P>, Diagnostics>,
    executions: schedule::Results<L::Operation, L::Result, Q, Q::Result>,
    config: PreparedConfig,
}
impl<P, L: Planner<S>, Q: ProcessorOperation, S: Sample> Control<P, L, Q, S> {
    /// Construct the existing owning queues and receipt scheduler for another consumer.
    pub fn channel(
        config: PreparedConfig,
        quantum: crate::Quantum,
        operation_bytes: usize,
        planner_operations: bool,
    ) -> Result<
        (
            Self,
            Ingress<P, L, Q, S>,
            Scheduler<L::Operation, L::Result, Q, Q::Result>,
        ),
        Error,
    > {
        if quantum.frames() > config.limits().max_frames {
            return Err(Error::InvalidConfig);
        }
        let limits = config.limits();
        let (transport, ingress) = transport::channel(TransportConfig {
            normal_slots: limits.command_slots,
            recovery_slots: 1,
            telemetry_slots: 3,
            operations_per_cycle: limits.budgets.commands,
            recovery_per_cycle: 1,
            publications_per_cycle: limits.budgets.publications,
        })?;
        let (executions, mut scheduler) =
            schedule::typed_scheduler(config, operation_bytes, planner_operations)?;
        scheduler.set_quantum(quantum);
        Ok((
            Self {
                transport,
                executions,
                config,
            },
            ingress,
            scheduler,
        ))
    }
    pub fn prepare_submissions(&mut self) -> Result<(), Error> {
        self.transport.prepare_submissions()
    }
    pub fn submit(
        &mut self,
        lane: Lane,
        request: Command<P, L, Q, S>,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<P, L, Q, S>>> {
        self.submit_inner(lane, request, false)
    }
    pub fn submit_prepared(
        &mut self,
        lane: Lane,
        request: Command<P, L, Q, S>,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<P, L, Q, S>>> {
        self.submit_inner(lane, request, true)
    }
    fn submit_inner(
        &mut self,
        lane: Lane,
        mut request: Command<P, L, Q, S>,
        prepared: bool,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<P, L, Q, S>>> {
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
    pub fn try_completion(
        &mut self,
    ) -> Option<transport::Completion<Command<P, L, Q, S>, Retired<P>>> {
        self.transport.try_completion()
    }
    pub fn try_execution(&mut self) -> Option<Execution<L::Operation, L::Result, Q, Q::Result>> {
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
pub type Ingress<P, L, Q, S> = RenderEndpoint<Command<P, L, Q, S>, Retired<P>, Diagnostics>;

struct Buffers<S: Sample = f32> {
    input_sequences: Box<[crate::audio_refs::AudioSequence<S>]>,
    output_sequences: Box<[crate::audio_refs::AudioSequence<S>]>,
    input_mixed: Box<[bool]>,
    input: Box<[S]>,
    output: Box<[S]>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
    midi_output_recovery: Vec<bool>,
    midi_input_recovery: Vec<bool>,
    injected: Vec<MidiBuffer>,
    midi_settings: crate::processor::MidiBufferSettings,
    midi_forwarded: Vec<bool>,
}
impl<S: Sample> Buffers<S> {
    fn new(
        config: PreparedConfig,
        ports: PortCounts,
        frames: usize,
        settings: crate::processor::MidiBufferSettings,
    ) -> Result<Self, Error> {
        let limits = config.limits();
        let default = crate::buffer::MidiCapacity {
            events: limits.midi_events_per_port,
            bytes: limits.midi_bytes_per_port,
        };
        settings.prepared_bytes(ports, default)?;
        let midi = |count, input, overrides| {
            (0..count)
                .map(|port| {
                    let capacity = if overrides {
                        settings.capacity(input, port, default)
                    } else {
                        default
                    };
                    MidiBuffer::new(capacity.events, capacity.bytes)
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let mut buffers = Self {
            input_sequences: vec![crate::audio_refs::AudioSequence::default(); ports.audio_inputs]
                .into_boxed_slice(),
            output_sequences: vec![
                crate::audio_refs::AudioSequence::default();
                ports.audio_outputs
            ]
            .into_boxed_slice(),
            input_mixed: vec![false; ports.audio_inputs].into_boxed_slice(),
            input: vec![S::ZERO; ports.audio_inputs * frames].into_boxed_slice(),
            output: vec![S::ZERO; ports.audio_outputs * frames].into_boxed_slice(),
            midi_input: midi(ports.midi_inputs, true, true)?,
            midi_output: midi(ports.midi_outputs, false, true)?,
            midi_output_recovery: vec![false; ports.midi_outputs],
            midi_input_recovery: vec![false; ports.midi_inputs],
            injected: midi(ports.midi_inputs, true, false)?,
            midi_settings: settings,
            midi_forwarded: vec![false; ports.midi_outputs],
        };
        for sample in buffers.input.iter_mut().chain(&mut buffers.output) {
            *core::hint::black_box(sample) = S::ZERO;
        }
        Ok(buffers)
    }
    fn midi_source(&self, port: usize) -> Result<&MidiBuffer, Error> {
        if self.midi_forwarded.get(port).copied().unwrap_or(false) {
            self.midi_input
                .get(
                    self.midi_settings
                        .forward_input
                        .ok_or(Error::InvalidGraph)?,
                )
                .ok_or(Error::InvalidGraph)
        } else {
            self.midi_output.get(port).ok_or(Error::InvalidGraph)
        }
    }
    fn clear(&mut self, stride: usize, start: usize, frames: usize) {
        self.midi_forwarded.fill(false);
        for channel in self.input.chunks_exact_mut(stride) {
            channel[start..start + frames].fill(S::ZERO);
        }
        for channel in self.output.chunks_exact_mut(stride) {
            channel[start..start + frames].fill(S::ZERO);
        }
        for (port, recovery) in self
            .midi_input
            .iter_mut()
            .zip(&mut self.midi_input_recovery)
        {
            *recovery |= port.is_recovery();
            port.clear();
        }
        for (port, recovery) in self
            .midi_output
            .iter_mut()
            .zip(&mut self.midi_output_recovery)
        {
            *recovery |= port.is_recovery();
            port.clear();
        }
    }
    fn begin_callback(&mut self) {
        self.midi_output_recovery.fill(false);
        self.midi_input_recovery.fill(false);
        for port in self.midi_input.iter_mut().chain(&mut self.midi_output) {
            port.clear();
        }
    }
}
struct Slot<P, S: Sample = f32> {
    id: NodeId,
    ports: PortCounts,
    operation_bytes: usize,
    processor: Option<P>,
    buffers: Buffers<S>,
}

fn activation_compatible<P, S: Sample>(
    slots: &[Option<Slot<P, S>>],
    candidate: &PreparedPlan,
) -> Result<(), Error> {
    for slot in slots.iter().flatten() {
        if !candidate.contains(slot.id) && slot.buffers.injected.iter().any(|port| !port.is_empty())
        {
            return Err(Error::Busy);
        }
    }
    Ok(())
}

pub struct GraphRuntime<P, L: Planner<S> = NoPlanner, Q: ProcessorOperation = (), S: Sample = f32> {
    audio_arena: Option<crate::audio_refs::RenderArena<S>>,
    host_sequences: Box<[crate::audio_refs::AudioSequence<S>]>,
    profiling: rt_profiling::Recorder,
    quantum: crate::Quantum,
    tick_frame: Frame,
    boundary_at: Option<(Sequence, Frame)>,
    host_buffer: Option<crate::host_buffer::HostBuffer<S>>,
    fault: Option<Error>,
    clock: RenderRuntime,
    catalog: Catalog,
    slots: Vec<Option<Slot<P, S>>>,
    active: Option<PreparedPlan>,
    graph_stages: GraphStages,
    validator: PlanValidator,
    validating: Option<Sequence>,
    epoch: Epoch,
    ingress: Ingress<P, L, Q, S>,
    diagnostics: Diagnostics,
    emergency: Option<Retired<P>>,
    ingress_fault: bool,
    scheduler: Scheduler<L::Operation, L::Result, Q, Q::Result>,
    recovery: Vec<ResetState>,
    recovery_remaining: Vec<usize>,
    resources: ResourceStore<S>,
    planner: L,
    planner_authority: Option<(crate::PlannerId, usize)>,
}

impl<P: Processor<NoPlanner, (), S>, S: Sample> GraphRuntime<P, NoPlanner, (), S> {
    /// Allocate slots, buffers, validation scratch, and ownership queues before
    /// callbacks begin. Only declared slots can be reused after startup.
    pub fn prepare(
        config: PreparedConfig,
        quantum: crate::Quantum,
        nodes: Vec<PreparedNode<P, NoPlanner, (), S>>,
    ) -> Result<(Control<P, NoPlanner, (), S>, Self), Error> {
        Self::prepare_with_resources(
            config,
            quantum,
            nodes,
            ResourceStore::<S>::new_numeric(config, quantum)?,
        )
    }

    /// Move prepared resource identities/content into the runtime before starting
    /// callbacks. Processors may retain plain IDs/leases created in this store.
    pub fn prepare_with_resources(
        config: PreparedConfig,
        quantum: crate::Quantum,
        nodes: Vec<PreparedNode<P, NoPlanner, (), S>>,
        resources: ResourceStore<S>,
    ) -> Result<(Control<P, NoPlanner, (), S>, Self), Error> {
        Self::prepare_with_planner_and_resources(config, quantum, nodes, NoPlanner, resources)
    }
}

impl<P: Processor<L, Q, S>, L: Planner<S>, Q: ProcessorOperation, S: Sample>
    GraphRuntime<P, L, Q, S>
{
    /// Compile and install the initial graph during off-render preparation.
    /// Returns epoch one at frame zero; no callback, ingress credit, receipt or
    /// planner/processor execution is used. Failure drops inputs off-render.
    pub fn prepare_with_initial_graph(
        config: PreparedConfig,
        quantum: crate::Quantum,
        nodes: Vec<PreparedNode<P, L, Q, S>>,
        planner: L,
        resources: ResourceStore<S>,
        graph: &GraphDescription,
    ) -> Result<(Control<P, L, Q, S>, Self), Error> {
        let (control, mut runtime) =
            Self::prepare_with_planner_and_resources(config, quantum, nodes, planner, resources)?;
        runtime.prepare_initial_graph(graph)?;
        Ok((control, runtime))
    }
    /// Install the startup graph off-render, before any nonempty callback.
    pub fn prepare_initial_graph(&mut self, graph: &GraphDescription) -> Result<(), Error> {
        if self.next_frame() != Frame(0) || self.epoch != Epoch(0) || self.active.is_some() {
            return Err(Error::InvalidState);
        }
        let plan = compile(&self.catalog, graph)?;

        self.planner.admit_graph(&plan)?;
        self.epoch = self.epoch.checked_next()?;
        self.active = Some(plan);
        Ok(())
    }

    pub fn prepare_with_planner(
        config: PreparedConfig,
        quantum: crate::Quantum,
        nodes: Vec<PreparedNode<P, L, Q, S>>,
        planner: L,
    ) -> Result<(Control<P, L, Q, S>, Self), Error> {
        Self::prepare_with_planner_and_resources(
            config,
            quantum,
            nodes,
            planner,
            ResourceStore::<S>::new_numeric(config, quantum)?,
        )
    }

    pub fn prepare_with_planner_and_resources(
        config: PreparedConfig,
        quantum: crate::Quantum,
        nodes: Vec<PreparedNode<P, L, Q, S>>,
        planner: L,
        resources: ResourceStore<S>,
    ) -> Result<(Control<P, L, Q, S>, Self), Error> {
        Self::prepare_owned(config, nodes, planner, resources, quantum)
    }

    fn prepare_owned(
        config: PreparedConfig,
        nodes: Vec<PreparedNode<P, L, Q, S>>,
        planner: L,
        resources: ResourceStore<S>,
        quantum: crate::Quantum,
    ) -> Result<(Control<P, L, Q, S>, Self), Error> {
        planner.validate_quantum(quantum)?;
        if quantum.frames() > config.limits().max_frames
            || resources.config() != config
            || resources.quantum() != quantum
        {
            return Err(Error::InvalidConfig);
        }
        if config
            .limits()
            .max_nodes
            .checked_mul(size_of::<Option<Slot<P, S>>>())
            .is_none_or(|bytes| bytes > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
        let specs: Vec<_> = nodes.iter().map(PreparedNode::spec).collect();
        let catalog = Catalog::new(config, &specs)?;
        let planner_authority = planner.id().map(|id| (id, planner.operation_bytes()));
        let operation_bytes = nodes.iter().fold(
            planner_authority.map_or(0, |(_, bytes)| bytes),
            |bytes, node| bytes.max(node.operation_bytes),
        );
        if operation_bytes > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        for node in &nodes {
            node.processor.admit(node.id, &planner)?;
        }
        let limits = config.limits();
        let default = crate::buffer::MidiCapacity {
            events: limits.midi_events_per_port,
            bytes: limits.midi_bytes_per_port,
        };
        let mut midi_bytes = 0usize;
        for node in &nodes {
            midi_bytes = midi_bytes
                .checked_add(node.midi_settings?.prepared_bytes(node.ports, default)?)
                .ok_or(Error::Capacity)?;
        }
        if midi_bytes > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        let mut slots: Vec<_> = (0..limits.max_nodes).map(|_| None).collect();
        let audio_ports = nodes.iter().try_fold(0usize, |count, node| {
            count
                .checked_add(node.ports.audio_inputs + node.ports.audio_outputs)
                .ok_or(Error::Capacity)
        })?;
        let audio_arena = if resources.audio_render_mode().chunked() {
            let samples = audio_ports
                .checked_mul(4)
                .and_then(|ports| ports.checked_add(limits.audio_inputs))
                .and_then(|ports| ports.checked_mul(quantum.frames()))
                .filter(|&samples| samples <= i32::MAX as usize / 4)
                .ok_or(Error::Capacity)?;
            let pins = audio_ports
                .checked_mul(2)
                .filter(|&pins| {
                    pins.checked_add(1)
                        .and_then(|pins| {
                            pins.checked_mul(size_of::<Option<crate::resource::AudioChunkRef>>())
                        })
                        .is_some_and(|bytes| bytes <= i32::MAX as usize)
                })
                .ok_or(Error::Capacity)?;
            Some(crate::audio_refs::RenderArena::prepare(samples, pins))
        } else {
            None
        };
        for node in nodes {
            let buffers = Buffers::new(config, node.ports, quantum.frames(), node.midi_settings?)?;
            slots[node.id.slot() as usize] = Some(Slot {
                id: node.id,
                ports: node.ports,
                operation_bytes: node.operation_bytes,
                processor: Some(node.processor),
                buffers,
            });
        }
        let (control, ingress, scheduler) = Control::channel(
            config,
            quantum,
            operation_bytes,
            planner_authority.is_some(),
        )?;
        Ok((
            control,
            Self {
                audio_arena,
                host_sequences: vec![
                    crate::audio_refs::AudioSequence::default();
                    limits.audio_inputs
                ]
                .into_boxed_slice(),
                profiling: rt_profiling::Recorder::default(),
                quantum,
                tick_frame: Frame(0),
                boundary_at: None,
                host_buffer: None,
                fault: None,
                clock: RenderRuntime::new(config),
                catalog,
                slots,
                active: None,
                graph_stages: GraphStages::disabled(),
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
                planner,
                planner_authority,
            },
        ))
    }

    /// Allocate graph-owner slots before the first nonempty callback.
    /// May be called once; validation and later activation never allocate storage.
    pub fn prepare_graph_staging(&mut self, candidates: usize) -> Result<(), Error> {
        if self.clock.next_frame() != Frame(0) || self.graph_stages.enabled() {
            return Err(Error::InvalidState);
        }
        self.graph_stages = GraphStages::prepare(candidates, self.clock.config())?;
        Ok(())
    }

    /// Borrow the catalog only from the owner while no callback is active. Hosts
    /// should retain their control-side signature description for compiling edits.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
    /// Consume a stopped runtime off-render and retain its resource store for
    /// explicit owner/lease cleanup or reuse. Other runtime owners, including the
    /// planner, are destroyed; plain resource handles hidden there are not released.
    pub fn into_resources(self) -> resource::ResourceStore<S> {
        self.resources
    }

    /// Consume a stopped runtime off-render, retaining the planner's owner
    /// ledger with its store for explicit domain cleanup. Other owners drop here.
    /// This does not settle pending operations or emit device cleanup messages.
    pub fn into_planner_and_resources(self) -> (L, resource::ResourceStore<S>) {
        (self.planner, self.resources)
    }

    /// Borrow settled domain state between callbacks from the runtime owner.
    pub fn planner(&self) -> &L {
        &self.planner
    }
    pub fn next_frame(&self) -> Frame {
        self.clock.next_frame()
    }
    pub fn quantum(&self) -> crate::Quantum {
        self.quantum
    }
    pub fn buffering_latency(&self) -> usize {
        self.host_buffer
            .as_ref()
            .map_or(0, |_| self.quantum.frames())
    }
    pub fn prepare_host_buffering(&mut self) -> Result<(), Error> {
        if self.next_frame() != Frame(0) || self.host_buffer.is_some() {
            return Err(Error::InvalidState);
        }
        self.host_buffer = Some(crate::host_buffer::HostBuffer::prepare(
            self.resources.config(),
            self.quantum,
        )?);
        Ok(())
    }
    pub fn prepare_host_buffering_with_midi_output(
        &mut self,
        output: crate::buffer::MidiCapacity,
    ) -> Result<(), Error> {
        if self.next_frame() != Frame(0) || self.host_buffer.is_some() {
            return Err(Error::InvalidState);
        }
        self.host_buffer = Some(crate::host_buffer::HostBuffer::prepare_with_output(
            self.resources.config(),
            self.quantum,
            output,
        )?);
        Ok(())
    }
    pub fn prepare_for_host(
        config: PreparedConfig,
        max_host_frames: usize,
        quantum: crate::Quantum,
        nodes: Vec<PreparedNode<P, L, Q, S>>,
        domain: L,
        resources: ResourceStore<S>,
    ) -> Result<(Control<P, L, Q, S>, Self), Error> {
        if config.limits().max_frames != quantum.frames() || max_host_frames == 0 {
            return Err(Error::InvalidConfig);
        }
        let mut host = config.limits();
        host.max_frames = max_host_frames;
        let host = host.prepare()?;
        let (control, mut runtime) =
            Self::prepare_owned(config, nodes, domain, resources, quantum)?;
        runtime.clock = RenderRuntime::new(host);
        Ok((control, runtime))
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }

    /// Coherent pool counters, read by the runtime owner between callbacks.
    pub fn midi_index_usage(&self) -> resource::MidiIndexUsage {
        self.resources.midi_index_usage()
    }
    pub fn capture_group_usage(&self) -> resource::CaptureGroupUsage {
        self.resources.capture_group_usage()
    }
    pub fn resource_usage(&self) -> resource::PoolUsage {
        self.resources.usage()
    }
    pub fn resource_work(&self) -> resource::Work {
        self.resources.work()
    }

    pub fn render_work(&self) -> resource::RenderWork {
        self.resources.render_work()
    }

    pub fn set_profiling(&mut self, recorder: rt_profiling::Recorder) {
        self.profiling = recorder;
    }

    pub fn set_profiling_clock(&mut self, clock: rt_profiling::Clock) {
        self.profiling.set_clock(clock);
    }

    pub fn process(
        &mut self,
        mut audio: AudioBlock<'_, '_, S>,
        mut midi: MidiBlock<'_>,
    ) -> Result<(), Error> {
        if audio.frames == 0 {
            return Ok(());
        }
        if self.host_buffer.is_none()
            && (audio.frames % self.quantum.frames() != 0 || !self.quantum.aligned(audio.frame))
        {
            return Err(Error::InvalidFrame);
        }
        let limits = self.clock.config().limits();
        audio.clear_bounded(limits.audio_outputs, audio.frames.min(limits.max_frames));
        for output in midi.outputs.iter_mut().take(limits.midi_outputs) {
            output.clear();
        }
        if let Some(error) = self.fault {
            return Err(error);
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
        self.profiling.begin_callback(
            frame.0,
            self.epoch.0,
            frames as u32,
            (frames as u64).saturating_mul(1_000_000_000) / u64::from(limits.sample_rate),
        );
        if let Err(error) = self.clock.process(AudioBlock {
            frame,
            frames,
            inputs: audio.inputs,
            outputs: audio.outputs,
        }) {
            self.profiling.end_callback(false);
            return Err(error);
        }
        self.diagnostics = Diagnostics {
            frame,
            epoch: self.epoch,
            ..Diagnostics::default()
        };
        self.recovery_remaining.fill(limits.budgets.events);
        for slot in self.slots.iter_mut().flatten() {
            slot.buffers.begin_callback();
        }
        self.scheduler.begin_callback();
        self.resources.begin_callback();
        self.resources.control_phase(true);
        self.ingress.begin_cycle(frame, self.epoch);
        self.apply_commands();
        self.resources.control_phase(false);
        self.profiling.callback_phase(rt_profiling::Phase::Commands);
        self.diagnostics.epoch = self.epoch;
        let result = self.run_ticks(&mut audio, &mut midi, self.quantum);
        self.diagnostics.epoch = self.epoch;
        self.ingress.set_epoch(self.epoch);
        if let Err(error) = result {
            self.fault = Some(error);
            for recovery in &mut self.recovery {
                recovery.request();
            }
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
            if let Some(buffer) = &mut self.host_buffer {
                buffer.defer_midi_recovery();
            }
            for recovery in &mut self.recovery {
                recovery.request();
            }
        }
        for (port, (output, recovery)) in midi.outputs.iter_mut().zip(&self.recovery).enumerate() {
            if recovery.pending() {
                output.clear();
                if let Some(buffer) = &mut self.host_buffer {
                    buffer.clear_midi_output(port);
                }
            }
        }
        self.resources.reclaim();
        let work = self.resources.work();
        self.diagnostics.transfer_bytes = work.copied_bytes;
        self.diagnostics.validation_steps = work.validation_steps;
        self.diagnostics.reclaimed_pages = work.reclaimed_pages;
        self.diagnostics.reclaimed_versions = work.reclaimed_versions;
        let rendering = self.resources.render_work();
        self.diagnostics.render_operations = rendering.operations;
        self.diagnostics.render_copied_bytes = rendering.copied_bytes;
        self.diagnostics.render_validation_steps = rendering.validation_steps;
        self.diagnostics.resource_index_work = self.resources.index_work();
        self.diagnostics.midi_index_work = self.resources.midi_index_work();
        self.diagnostics.counter_overflow |= rendering.counter_overflow
            | self.diagnostics.resource_index_work.counter_overflow
            | self.diagnostics.midi_index_work.counter_overflow;
        let _ = self.ingress.publish(self.diagnostics);
        self.profiling.end_callback(result.is_ok());
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
                Command::StageGraph {
                    slot,
                    expected_epoch,
                    plan,
                    id,
                } => {
                    if operation.lane != Lane::Normal || id.is_some() {
                        Err(Error::InvalidState)
                    } else if *expected_epoch != self.epoch {
                        Err(Error::StaleEpoch)
                    } else if let Err(error) = self.graph_stages.can_stage(*slot) {
                        Err(error)
                    } else if let Some(error) = plan
                        .as_ref()
                        .and_then(|p| self.graph_stages.accepts(p).err())
                    {
                        Err(error)
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
                                ValidationStatus::Valid => {
                                    *id = Some(self.graph_stages.install(
                                        *slot,
                                        self.epoch,
                                        plan.take().expect("validated candidate"),
                                    ));
                                    Ok(())
                                }
                            },
                        }
                    } else {
                        Err(Error::InvalidGraph)
                    }
                }
                Command::ReleaseGraph { id } => {
                    if operation.lane != Lane::Normal {
                        Err(Error::InvalidState)
                    } else {
                        self.graph_stages
                            .release(*id)
                            .map(|owner| retired = owner.map(Retired::Plan))
                    }
                }
                Command::Resource(command) => {
                    let boundary = matches!(
                        command.request(),
                        resource::Request::BeginCapture { .. }
                            | resource::Request::StopCapture { .. }
                            | resource::Request::Snapshot { .. }
                    );
                    if operation.lane == Lane::Recovery && !command.recovery_safe() {
                        Err(Error::InvalidState)
                    } else if boundary && self.quantum.ceil(self.diagnostics.frame).is_err() {
                        Err(Error::InvalidFrame)
                    } else {
                        if boundary {
                            if self
                                .boundary_at
                                .is_none_or(|(sequence, _)| sequence != operation.sequence)
                            {
                                self.boundary_at = Some((
                                    operation.sequence,
                                    self.quantum
                                        .ceil(self.diagnostics.frame)
                                        .expect("checked boundary"),
                                ));
                            }
                            if self.tick_frame < self.boundary_at.expect("boundary command").1 {
                                break;
                            }
                        }
                        command.apply(&mut self.resources, self.tick_frame)
                    }
                }
                Command::ResetMidi => {
                    if let Some(buffer) = &mut self.host_buffer {
                        buffer.defer_midi_recovery();
                    }
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
                    let recovery_safe = matches!(*action, Action::Planner { operation, .. } if L::recovery_safe(operation));
                    if operation.lane == Lane::Recovery && !recovery_safe {
                        Err(Error::InvalidState)
                    } else if operation.lane == Lane::Recovery
                        && (!payload.is_empty() || *at > self.diagnostics.frame)
                    {
                        Err(Error::InvalidEvent)
                    } else if payload.len() > self.scheduler.payload_limit()
                        || payload.len() > limits.budgets.transfer_bytes - limits.resource_io_bytes
                    {
                        Err(Error::Capacity)
                    } else if payload.len() > self.resources.copy_remaining() {
                        break;
                    } else {
                        validate_action(
                            &self.catalog,
                            self.planner_authority,
                            &self.slots,
                            *action,
                            payload.len(),
                        )
                        .and_then(|()| {
                            if operation.lane == Lane::Recovery {
                                self.scheduler.admit_recovery(
                                    operation.sequence,
                                    *at,
                                    self.diagnostics.frame,
                                    *action,
                                    payload,
                                )
                            } else {
                                self.scheduler.admit(
                                    operation.sequence,
                                    *at,
                                    self.diagnostics.frame,
                                    *action,
                                    payload,
                                )
                            }
                        })
                        .and_then(|admission| {
                            self.resources.charge_external(payload.len(), 0)?;
                            admitted = Some(admission);
                            Ok(())
                        })
                    }
                }
                Command::Cancel { sequence } => {
                    self.scheduler.cancel(self.diagnostics.frame, *sequence)
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
                    } else if self.quantum.ceil(self.diagnostics.frame).is_err() {
                        Err(Error::InvalidFrame)
                    } else if let Some(candidate) = plan.as_ref() {
                        if self
                            .boundary_at
                            .is_none_or(|(sequence, _)| sequence != operation.sequence)
                        {
                            self.boundary_at = Some((
                                operation.sequence,
                                self.quantum
                                    .ceil(self.diagnostics.frame)
                                    .expect("validated clock headroom"),
                            ));
                        }
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
                                ValidationStatus::Valid => match self
                                    .planner
                                    .admit_graph(candidate)
                                    .and_then(|()| self.epoch.checked_next())
                                {
                                    Err(error) => Err(error),
                                    Ok(epoch) => {
                                        if self.tick_frame
                                            < self.boundary_at.expect("admitted commit").1
                                        {
                                            break;
                                        }
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
                    } else if let Err(error) = self.planner.admit_processor_change(*node) {
                        Err(error)
                    } else {
                        remove_processor(&mut self.slots, &mut self.catalog, &mut self.epoch, *node)
                            .map(|processor| retired = Some(Retired::Processor(processor)))
                    }
                }
                Command::Install { prepared } => {
                    if operation.lane != Lane::Normal {
                        Err(Error::InvalidState)
                    } else if let Some(Err(error)) = prepared.as_ref().map(|node| {
                        self.planner.admit_processor_change(node.id)?;
                        node.processor.admit(node.id, &self.planner)
                    }) {
                        Err(error)
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
            if self.validating == Some(operation.sequence) {
                self.validating = None;
            }
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
                // error bundle in a reserved emergency slot rather than drop it.
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
    /// A partially collected tick finishes before reset acknowledgment, preserving
    /// recording input and due actions while discarding their ordinary output.
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
        if self.fault.is_none()
            && self
                .host_buffer
                .as_ref()
                .is_some_and(|buffer| !buffer.midi_recovery_ready())
        {
            return Ok(0);
        }
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

    fn deliver_actions(
        &mut self,
        at: Frame,
        callback_start: Frame,
        remaining_events: &mut usize,
    ) -> Result<(), Error> {
        for _ in 0..*remaining_events {
            let Some(mut event) = self.scheduler.deliver(at) else {
                break;
            };
            *remaining_events -= 1;
            match event.action {
                Action::Planner { planner, operation } => {
                    let result = if self.planner_authority.is_none_or(|(id, _)| id != planner) {
                        Err(Error::InvalidId)
                    } else {
                        let compatible = |candidate: &PreparedPlan| {
                            activation_compatible(&self.slots, candidate)
                        };
                        let context = OperationContext {
                            graph_epoch: self.epoch,
                            graphs: GraphAccess::new(
                                &mut self.graph_stages,
                                &mut self.active,
                                &mut self.epoch,
                                event.lane == Lane::Normal,
                                &compatible,
                                &mut self.recovery,
                            ),
                            callback_start,
                            token: event.token,
                            frame: at,
                            requested_frame: event.requested_frame,
                            recovery: event.lane == Lane::Recovery,
                            payload: event.payload,
                            resources: self.resources.planner_io(at, at),
                            receipts: event.receipts.planner(),
                        };
                        self.planner.operation(operation, context)
                    };
                    match result {
                        Ok(Disposition::Complete(value)) => {
                            event.receipts.complete(event.token, Ok(value))?
                        }
                        Ok(Disposition::Pending) => {}
                        Err(error) => event.receipts.complete(event.token, Err(error))?,
                    }
                }
                action => {
                    let node = action.node().ok_or(Error::InvalidId)?;
                    let result = self
                        .slots
                        .get_mut(node.slot() as usize)
                        .and_then(Option::as_mut)
                        .filter(|slot| slot.id == node && slot.processor.is_some())
                        .ok_or(Error::InvalidId)
                        .and_then(|slot| match action {
                            Action::Parameter {
                                parameter, value, ..
                            } => slot
                                .processor
                                .as_mut()
                                .ok_or(Error::InvalidId)?
                                .set_parameter(parameter, value)
                                .map(|()| None),
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
                                result.map(|()| None)
                            }
                            Action::Processor { operation, .. } => slot
                                .processor
                                .as_mut()
                                .ok_or(Error::InvalidId)?
                                .operation(
                                    operation,
                                    crate::processor::OperationContext {
                                        quantum: self.quantum,
                                        graph_epoch: self.epoch,
                                        frame: at,
                                        requested_frame: event.requested_frame,
                                        sequence: event.token.sequence(),
                                        payload: event.payload,
                                        resources: self.resources.io(),
                                    },
                                )
                                .map(Some),
                            Action::Planner { .. } => Err(Error::InvalidState),
                        });
                    if matches!(action, Action::Processor { .. }) {
                        event.receipts.complete_processor(
                            event.token,
                            result.and_then(|value| value.ok_or(Error::InvalidState)),
                        )?;
                    } else {
                        if matches!(result, Err(Error::Full)) {
                            increment(
                                &mut self.diagnostics.midi_overflows,
                                &mut self.diagnostics.counter_overflow,
                            );
                        }
                        event.receipts.ordinary(event.token, result.map(|_| ()))?;
                    }
                }
            }
            increment(
                &mut self.diagnostics.scheduled_events,
                &mut self.diagnostics.counter_overflow,
            );
        }
        Ok(())
    }
    fn run_ticks(
        &mut self,
        audio: &mut AudioBlock<'_, '_, S>,
        midi: &mut MidiBlock<'_>,
        quantum: crate::Quantum,
    ) -> Result<(), Error> {
        let mut remaining = self.scheduler.delivery_limit();
        let callback_start = audio.frame;
        if let Some(mut buffer) = self.host_buffer.take() {
            let result = buffer.process(audio, midi, |a, m| {
                self.run_tick(a, m, 0, callback_start, quantum, &mut remaining)
            });
            for _ in 0..buffer.overflows() {
                increment(
                    &mut self.diagnostics.midi_overflows,
                    &mut self.diagnostics.counter_overflow,
                );
            }
            self.host_buffer = Some(buffer);
            return result;
        }
        for start in (0..audio.frames).step_by(quantum.frames()) {
            self.run_tick(audio, midi, start, callback_start, quantum, &mut remaining)?;
        }
        Ok(())
    }
    fn run_tick(
        &mut self,
        audio: &mut impl crate::host_buffer::AudioIo<S>,
        midi: &mut MidiBlock<'_>,
        start: usize,
        callback_start: Frame,
        quantum: crate::Quantum,
        remaining: &mut usize,
    ) -> Result<(), Error> {
        let frame = audio.frame().checked_add(start)?;
        let end = frame.checked_add(quantum.frames())?;
        self.profiling.begin_tick(frame.0, quantum.frames() as u32);

        self.deliver_actions(frame, callback_start, remaining)?;
        self.planner.begin_tick(PlanContext {
            graph_epoch: self.epoch,
            graphs: GraphAccess::new(
                &mut self.graph_stages,
                &mut self.active,
                &mut self.epoch,
                true,
                &|candidate| activation_compatible(&self.slots, candidate),
                &mut self.recovery,
            ),
            callback_start,
            frame,
            ceiling: end,
            resources: self.resources.planner_io(frame, end),
            receipts: self.scheduler.receipts(frame),
        })?;
        if self.scheduler.recovery_pending() {
            return Err(Error::InvalidState);
        }
        self.profiling.phase(rt_profiling::Phase::StateSettled);
        self.profiling.graph_start(self.epoch.0, frame.0);
        self.run_graph(audio, midi, start, callback_start, quantum)?;
        self.planner.finish_tick(FinishContext {
            interval: Interval { start: frame, end },
            resources: self.resources.planner_io(end, end),
            receipts: self.scheduler.receipts(end),
        })?;

        self.profiling.phase(rt_profiling::Phase::StateAdvanced);
        self.profiling.end_tick(true);
        self.tick_frame = end;
        increment(
            &mut self.diagnostics.subranges,
            &mut self.diagnostics.counter_overflow,
        );
        Ok(())
    }
    fn run_graph(
        &mut self,
        audio: &mut impl crate::host_buffer::AudioIo<S>,
        midi: &mut MidiBlock<'_>,
        start: usize,
        callback_start: Frame,
        quantum: crate::Quantum,
    ) -> Result<(), Error> {
        if self.audio_arena.is_some() {
            return self.run_graph_references(audio, midi, start, callback_start, quantum);
        }
        let Some(plan) = &self.active else {
            return Ok(());
        };
        let q = quantum.frames();
        let frame = audio.frame().checked_add(start)?;
        let mut first = 0;
        if self.profiling.needs_layout() {
            for step in &plan.descriptor().steps {
                self.profiling.layout_node(u64::from(step.node.slot()));
            }
            self.profiling.layout_done();
        }
        for step in &plan.descriptor().steps {
            self.slots[step.node.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?
                .buffers
                .clear(q, 0, q);
            for edge in &plan.descriptor().connections[first..step.input_end] {
                record_midi_overflow(
                    route_offset(&mut self.slots, *edge, audio, midi, q, start, q, 0),
                    &mut self.diagnostics,
                )?;
                increment(
                    &mut self.diagnostics.connections_processed,
                    &mut self.diagnostics.counter_overflow,
                );
            }
            let slot = self.slots[step.node.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?;
            let buffers = &mut slot.buffers;
            for (input, injected) in buffers.midi_input.iter_mut().zip(&mut buffers.injected) {
                record_midi_overflow(input.merge(injected, q), &mut self.diagnostics)?;
                injected.clear();
            }
            for (input, recovery) in buffers
                .midi_input
                .iter_mut()
                .zip(&buffers.midi_input_recovery)
            {
                if *recovery {
                    input.clear();
                }
            }
            let processor = slot.processor.as_mut().ok_or(Error::InvalidId)?;
            let result = processor.process(&mut ProcessBlock {
                node: step.node,
                graph_epoch: self.epoch,
                callback_start,
                frame,
                frames: q,
                quantum,
                audio: AudioPorts::new(&buffers.input, &mut buffers.output, q, 0, q)?,
                midi_inputs: &buffers.midi_input,
                midi_outputs: &mut buffers.midi_output,
                midi_forwarded: &mut buffers.midi_forwarded,
                resources: self.resources.io(),
                domain: &mut self.planner,
            });
            if result == Err(Error::Full) {
                buffers.midi_forwarded.fill(false);
                for out in &mut buffers.midi_output {
                    out.force_reset(0);
                }
            }
            for ((output, recovery), forwarded) in buffers
                .midi_output
                .iter_mut()
                .zip(&buffers.midi_output_recovery)
                .zip(&mut buffers.midi_forwarded)
            {
                if *recovery {
                    *forwarded = false;
                    output.clear();
                }
            }
            record_midi_overflow(result, &mut self.diagnostics)?;
            increment(
                &mut self.diagnostics.nodes_processed,
                &mut self.diagnostics.counter_overflow,
            );
            self.profiling.node_ready(u64::from(step.node.slot()));
            first = step.input_end;
        }
        for edge in &plan.descriptor().connections[first..] {
            record_midi_overflow(
                route_offset(&mut self.slots, *edge, audio, midi, q, start, q, 0),
                &mut self.diagnostics,
            )?;
            increment(
                &mut self.diagnostics.connections_processed,
                &mut self.diagnostics.counter_overflow,
            );
        }
        self.profiling.phase(rt_profiling::Phase::OutputRouting);
        Ok(())
    }
}

fn validate_action<P, O: Copy, Q: Copy, S: Sample>(
    catalog: &Catalog,
    planner_authority: Option<(crate::PlannerId, usize)>,
    slots: &[Option<Slot<P, S>>],
    action: Action<O, Q>,
    bytes: usize,
) -> Result<(), Error> {
    let limit = match action {
        Action::Planner { planner, .. } => {
            planner_authority
                .filter(|(id, _)| *id == planner)
                .ok_or(Error::InvalidId)?
                .1
        }
        Action::Processor { node, .. } => {
            catalog.node(node)?;
            slots
                .get(node.slot() as usize)
                .and_then(Option::as_ref)
                .ok_or(Error::InvalidId)?
                .operation_bytes
        }
        Action::Midi { node, port } => {
            let spec = catalog.node(node)?;
            if port as usize >= spec.ports.midi_inputs {
                return Err(Error::InvalidGraph);
            }
            catalog.config().limits().midi_bytes_per_port
        }
        Action::Parameter { node, .. } => {
            catalog.node(node)?;
            return Ok(());
        }
    };
    if bytes > limit {
        return Err(Error::Capacity);
    }
    Ok(())
}

fn remove_processor<P, S: Sample>(
    slots: &mut [Option<Slot<P, S>>],
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

fn install_processor<P, L: Planner<S>, Q: ProcessorOperation, S: Sample>(
    slots: &mut [Option<Slot<P, S>>],
    catalog: &mut Catalog,
    epoch: &mut Epoch,
    prepared: &mut Option<PreparedNode<P, L, Q, S>>,
) -> Result<(), Error> {
    let node = prepared.as_ref().ok_or(Error::InvalidState)?;
    let slot = slots
        .get_mut(node.id.slot() as usize)
        .and_then(Option::as_mut)
        .ok_or(Error::InvalidId)?;
    if slot.id != node.id || slot.processor.is_some() {
        return Err(Error::InvalidId);
    }
    if slot.ports != node.ports
        || slot.operation_bytes != node.operation_bytes
        || slot.buffers.midi_settings != node.midi_settings?
    {
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

fn route_offset<P, S: Sample>(
    slots: &mut [Option<Slot<P, S>>],
    edge: Connection,
    audio: &mut impl crate::host_buffer::AudioIo<S>,
    midi: &mut MidiBlock<'_>,
    stride: usize,
    start: usize,
    frames: usize,
    buffer_start: usize,
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
                    channel(&source.output, edge.source, stride, buffer_start, frames),
                    channel_mut(
                        &mut target.input,
                        edge.destination,
                        stride,
                        buffer_start,
                        frames,
                    ),
                ),
                PortType::Midi => target.midi_input[edge.destination.port as usize]
                    .merge(source.midi_source(edge.source.port as usize)?, frames)?,
            }
        }
        (None, Some(target)) => {
            let target = &mut slots[target.slot() as usize]
                .as_mut()
                .ok_or(Error::InvalidId)?
                .buffers;
            match edge.source.kind {
                PortType::Audio => mix(
                    &audio.input(edge.source.port as usize)[start..start + frames],
                    channel_mut(
                        &mut target.input,
                        edge.destination,
                        stride,
                        buffer_start,
                        frames,
                    ),
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
                    channel(&source.output, edge.source, stride, buffer_start, frames),
                    &mut audio.output(edge.destination.port as usize)[start..start + frames],
                ),
                PortType::Midi => midi.outputs[edge.destination.port as usize].merge_window(
                    source.midi_source(edge.source.port as usize)?,
                    0,
                    frames,
                    start,
                    audio.frames(),
                )?,
            }
        }
        (None, None) => match edge.source.kind {
            PortType::Audio => {
                let (input, output) =
                    audio.pair(edge.source.port as usize, edge.destination.port as usize);
                mix(
                    &input[start..start + frames],
                    &mut output[start..start + frames],
                );
            }
            PortType::Midi => midi.outputs[edge.destination.port as usize].merge_window(
                &midi.inputs[edge.source.port as usize],
                start,
                frames,
                start,
                audio.frames(),
            )?,
        },
    }
    Ok(())
}
fn channel<S: Sample>(
    data: &[S],
    endpoint: Endpoint,
    stride: usize,
    start: usize,
    frames: usize,
) -> &[S] {
    let first = endpoint.port as usize * stride + start;
    &data[first..first + frames]
}
fn channel_mut<S: Sample>(
    data: &mut [S],
    endpoint: Endpoint,
    stride: usize,
    start: usize,
    frames: usize,
) -> &mut [S] {
    let first = endpoint.port as usize * stride + start;
    &mut data[first..first + frames]
}
fn record_midi_overflow(
    result: Result<(), Error>,
    diagnostics: &mut Diagnostics,
) -> Result<(), Error> {
    if result == Err(Error::Full) {
        increment(
            &mut diagnostics.midi_overflows,
            &mut diagnostics.counter_overflow,
        );
        Ok(())
    } else {
        result
    }
}
fn increment(counter: &mut usize, overflow: &mut bool) {
    match counter.checked_add(1) {
        Some(value) => *counter = value,
        None => *overflow = true,
    }
}

#[cfg(test)]
mod tests {
    #[shoop_wasm_test_support::shoop_test]
    fn diagnostic_overflow_is_saturating_and_non_semantic() {
        let mut value = usize::MAX - 1;
        let mut overflow = false;
        super::increment(&mut value, &mut overflow);
        assert_eq!(value, usize::MAX);
        assert!(!overflow);
        super::increment(&mut value, &mut overflow);
        assert_eq!(value, usize::MAX);
        assert!(overflow);
    }
}

fn mix<S: Sample>(source: &[S], target: &mut [S]) {
    for (source, target) in source.iter().zip(target) {
        *target = target.mix(*source);
    }
}
