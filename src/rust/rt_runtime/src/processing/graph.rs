use super::{
    CommandQueue, GraphCompiler, GraphControl, NodeOperation, OperationContext, OperationControl,
    PreparedGraph, Quantum, Tick,
};
use crate::buffer::{AudioBlock, AudioPorts, MidiBlock, MidiBuffer};
use crate::graph::{GraphDescription, NodeSpec, PortCounts, PortType};
use crate::processor::ProcessorOperation;
use crate::resource::{ResourceIo, ResourceStore};
use crate::{Epoch, Error, Frame, NodeId};
use alloc::{boxed::Box, vec, vec::Vec};

pub const MAX_HOST_PORTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub quantum: Quantum,
    pub max_host_frames: usize,
    /// Graph working storage uses Q frames; `max_frames` is replaced during preparation.
    pub capacities: crate::Config,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            quantum: Quantum::default(),
            max_host_frames: 512,
            capacities: crate::Config::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedConfig {
    pub(super) quantum: Quantum,
    pub(super) max_host_frames: usize,
    pub(super) resources: crate::PreparedConfig,
}
impl Config {
    pub fn prepare(mut self) -> Result<PreparedConfig, Error> {
        if self.max_host_frames == 0
            || self.max_host_frames > u32::MAX as usize
            || [
                self.capacities.audio_inputs,
                self.capacities.audio_outputs,
                self.capacities.midi_inputs,
                self.capacities.midi_outputs,
            ]
            .iter()
            .any(|&n| n > MAX_HOST_PORTS)
        {
            return Err(Error::InvalidConfig);
        }
        self.capacities.max_frames = self.quantum.frames();
        Ok(PreparedConfig {
            quantum: self.quantum,
            max_host_frames: self.max_host_frames,
            resources: self.capacities.prepare()?,
        })
    }
}
impl PreparedConfig {
    pub fn quantum(self) -> Quantum {
        self.quantum
    }
    pub fn max_host_frames(self) -> usize {
        self.max_host_frames
    }
    pub fn resources(self) -> crate::PreparedConfig {
        self.resources
    }
}

pub trait State: Send {
    fn validate_graph(&self, _graph: &GraphDescription) -> Result<(), Error> {
        Ok(())
    }
    /// Bounded, allocation-free preflight before publishing prepared routing.
    fn validate_activation(&self, _graph: &GraphDescription) -> Result<(), Error> {
        Err(Error::InvalidState)
    }
    fn begin_callback(
        &mut self,
        _frame: Frame,
        _resources: &mut ResourceStore,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn begin_callback_with_budget(
        &mut self,
        frame: Frame,
        resources: &mut ResourceStore,
        _commands: usize,
    ) -> Result<(), Error> {
        self.begin_callback(frame, resources)
    }
    fn begin_tick(&mut self, _tick: Tick, _resources: &mut ResourceStore) -> Result<(), Error> {
        Ok(())
    }
    fn end_tick(&mut self, _tick: Tick, _resources: &mut ResourceStore) -> Result<(), Error> {
        Ok(())
    }
}
impl State for () {
    fn validate_activation(&self, _: &GraphDescription) -> Result<(), Error> {
        Ok(())
    }
}

pub struct Block<'a> {
    pub graph_epoch: Epoch,
    pub callback_frame: Frame,
    pub tick: Tick,
    pub audio: AudioPorts<'a>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_outputs: &'a mut [MidiBuffer],
    pub resources: ResourceIo<'a>,
}
pub trait Processor<S: State, O: ProcessorOperation = ()>: Send {
    fn validate_state(&self, _id: NodeId, _state: &S) -> Result<(), Error> {
        Ok(())
    }
    fn ports(&self) -> PortCounts;
    fn process(&mut self, block: &mut Block<'_>, state: &mut S) -> Result<(), Error>;
    fn operation(
        &mut self,
        _value: O,
        _context: OperationContext<'_>,
        _state: &mut S,
    ) -> Result<O::Result, Error> {
        Err(Error::InvalidEvent)
    }
}

struct Slot<P> {
    processor: P,
    input: Box<[f32]>,
    output: Box<[f32]>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
}
#[derive(Clone, Copy)]
pub(super) struct Address {
    pub node: Option<usize>,
    pub port: usize,
}
#[derive(Clone, Copy)]
pub(super) struct Route {
    pub source: Address,
    pub destination: Address,
    pub kind: PortType,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    pub ticks: usize,
    pub nodes: usize,
    pub edges: usize,
    pub fault: Option<Error>,
}

pub struct Runtime<P, S, O: ProcessorOperation = ()> {
    pub(super) config: PreparedConfig,
    slots: Vec<Slot<P>>,
    graph: PreparedGraph,
    epoch: Epoch,
    graph_inbox: Option<super::publication::Inbox>,
    operations: Option<CommandQueue<NodeOperation<O>, O::Result>>,
    state: S,
    resources: ResourceStore,
    pub(super) frame: Frame,
    callback_frame: Frame,
    diagnostics: Diagnostics,
    fault: Option<Error>,
    recovery: Vec<crate::midi::ResetState>,
    recovery_remaining: Vec<usize>,
}

impl<P: Processor<S, O>, S: State, O: ProcessorOperation> Runtime<P, S, O> {
    pub fn prepare(
        config: PreparedConfig,
        nodes: Vec<(NodeId, P)>,
        state: S,
        resources: ResourceStore,
        description: &GraphDescription,
    ) -> Result<Self, Error> {
        if resources.config() != config.resources {
            return Err(Error::InvalidConfig);
        }
        state.validate_graph(description)?;
        for (id, processor) in &nodes {
            processor.validate_state(*id, &state)?;
        }
        let specs: Vec<_> = nodes
            .iter()
            .map(|(id, p)| NodeSpec {
                id: *id,
                ports: p.ports(),
            })
            .collect();
        let compiler = GraphCompiler::new(config.resources, &specs)?;
        let graph = compiler.compile(description)?;
        let mut slots = Vec::with_capacity(nodes.len());
        let limits = config.resources.limits();
        let q = config.quantum.frames();
        for ((_, processor), spec) in nodes.into_iter().zip(&specs) {
            let ports = spec.ports;
            let midi = |n| {
                (0..n)
                    .map(|_| {
                        MidiBuffer::new(limits.midi_events_per_port, limits.midi_bytes_per_port)
                    })
                    .collect::<Result<Vec<_>, _>>()
            };
            let mut input = vec![0.; ports.audio_inputs * q].into_boxed_slice();
            let mut output = vec![0.; ports.audio_outputs * q].into_boxed_slice();
            for sample in input.iter_mut().chain(&mut output) {
                *core::hint::black_box(sample) = 0.;
            }
            slots.push(Slot {
                processor,
                input,
                output,
                midi_input: midi(ports.midi_inputs)?,
                midi_output: midi(ports.midi_outputs)?,
            });
        }
        Ok(Self {
            config,
            slots,
            graph,
            epoch: Epoch(0),
            graph_inbox: None,
            operations: None,
            state,
            resources,
            frame: Frame(0),
            callback_frame: Frame(0),
            diagnostics: Diagnostics::default(),
            fault: None,
            recovery: (0..limits.midi_outputs)
                .map(|_| crate::midi::ResetState::default())
                .collect(),
            recovery_remaining: vec![0; limits.midi_outputs],
        })
    }
    pub fn next_frame(&self) -> Frame {
        self.frame
    }
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
    pub fn graph_compiler(&self) -> GraphCompiler {
        self.graph.compiler()
    }
    /// Prepare typed node control, sharing the callback's normal ingress budget with state.
    pub fn prepare_operations(
        &mut self,
        capacity: usize,
        ingress: usize,
    ) -> Result<OperationControl<O>, Error>
    where
        O: Default,
        O::Result: Default,
    {
        if self.frame != Frame(0) || self.fault.is_some() || self.operations.is_some() {
            return Err(Error::InvalidState);
        }
        let limits = self.config.resources.limits();
        if capacity > limits.command_slots || ingress > limits.budgets.commands {
            return Err(Error::Capacity);
        }
        let (commands, queue) =
            super::command_channel_with_budget(self.config.quantum, capacity, ingress)?;
        self.operations = Some(queue);
        Ok(OperationControl {
            commands,
            compiler: self.graph.compiler(),
        })
    }
    pub fn shutdown_operations(&mut self, control: &mut OperationControl<O>) -> Result<(), Error> {
        self.operations
            .as_mut()
            .ok_or(Error::InvalidState)?
            .shutdown(&mut control.commands, self.frame)
    }
    /// Prepare the single-credit owning publication transport before callbacks.
    pub fn prepare_graph_changes(&mut self) -> Result<GraphControl, Error> {
        if self.frame != Frame(0) || self.fault.is_some() || self.graph_inbox.is_some() {
            return Err(Error::InvalidState);
        }
        let (control, inbox) = super::publication::channel(&self.graph)?;
        self.graph_inbox = Some(inbox);
        Ok(control)
    }
    /// Only after callbacks stop; terminal results retain every routing owner.
    pub fn shutdown_graph_changes(&mut self, control: &mut GraphControl) -> Result<(), Error> {
        self.graph_inbox
            .as_mut()
            .ok_or(Error::InvalidState)?
            .shutdown(control, self.frame)
    }
    fn exchange_graph(
        &mut self,
        expected: Epoch,
        graph: &mut PreparedGraph,
    ) -> Result<Epoch, Error> {
        if expected != self.epoch {
            return Err(Error::StaleEpoch);
        }
        if !self.graph.accepts(graph) {
            return Err(Error::InvalidId);
        }
        self.state.validate_activation(&graph.description)?;
        let next = self.epoch.checked_next()?;
        core::mem::swap(&mut self.graph, graph);
        self.epoch = next;
        Ok(next)
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }
    pub fn state(&self) -> &S {
        &self.state
    }
    /// Render-owner access between callbacks; concurrent producers use a prepared transport.
    pub fn state_mut(&mut self) -> &mut S {
        &mut self.state
    }
    pub fn into_parts(self) -> (S, ResourceStore) {
        (self.state, self.resources)
    }

    pub fn midi_recovery_pending(&self, port: usize) -> Result<bool, Error> {
        self.recovery
            .get(port)
            .map(crate::midi::ResetState::pending)
            .ok_or(Error::InvalidId)
    }
    /// Reserved reset output, independent of a faulted graph's ordinary output.
    pub fn drain_midi_recovery(
        &mut self,
        port: usize,
        sink: &mut MidiBuffer,
        frames: usize,
    ) -> Result<usize, Error> {
        if frames == 0 || frames > self.config.max_host_frames {
            return Err(Error::InvalidShape);
        }
        let reset = self.recovery.get_mut(port).ok_or(Error::InvalidId)?;
        let remaining = self
            .recovery_remaining
            .get_mut(port)
            .ok_or(Error::InvalidId)?;
        let mut emitted = 0;
        while *remaining > 0 {
            let Some(packet) = reset.peek() else { break };
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
    pub(super) fn validate(
        &self,
        audio: &AudioBlock<'_, '_>,
        midi: &MidiBlock<'_>,
        next: Frame,
    ) -> Result<(), Error> {
        let c = self.config.resources.limits();
        if audio.frame != next || audio.frames > self.config.max_host_frames {
            return Err(Error::InvalidFrame);
        }
        next.checked_add(audio.frames)?;
        if audio.inputs.len() != c.audio_inputs
            || audio.outputs.len() != c.audio_outputs
            || audio.inputs.iter().any(|v| v.len() < audio.frames)
            || audio.outputs.iter().any(|v| v.len() < audio.frames)
            || midi.inputs.len() != c.midi_inputs
            || midi.outputs.len() != c.midi_outputs
        {
            return Err(Error::InvalidShape);
        }
        for input in midi.inputs {
            if input.len() > c.midi_events_per_port || input.byte_len() > c.midi_bytes_per_port {
                return Err(Error::Capacity);
            }
            for i in 0..input.len() {
                if input.get(i).ok_or(Error::InvalidEvent)?.0 as usize >= audio.frames {
                    return Err(Error::InvalidEvent);
                }
            }
        }
        Ok(())
    }
    pub(super) fn begin_callback(&mut self) -> Result<(), Error> {
        self.recovery_remaining
            .fill(self.config.resources.limits().budgets.events);
        if let Some(e) = self.fault {
            return Err(e);
        }
        self.diagnostics = Diagnostics::default();
        self.callback_frame = self.frame;
        self.resources.begin_callback();
        if let Some(inbox) = &mut self.graph_inbox {
            inbox.begin_callback(self.frame, self.config.quantum);
        }
        let commands = self.config.resources.limits().budgets.commands;
        let admitted = match &mut self.operations {
            Some(queue) => queue.begin_callback_with_budget(self.frame, commands)?,
            None => 0,
        };
        self.state
            .begin_callback_with_budget(self.frame, &mut self.resources, commands - admitted)
    }
    pub(super) fn end_callback(&mut self, result: Result<(), Error>) -> Result<(), Error> {
        self.resources.reclaim();
        if let Err(e) = result {
            if self.fault.is_none() {
                for recovery in &mut self.recovery {
                    recovery.request();
                }
            }
            self.fault = Some(e);
            self.diagnostics.fault = Some(e);
        }
        result
    }
    pub fn process(
        &mut self,
        mut audio: AudioBlock<'_, '_>,
        mut midi: MidiBlock<'_>,
    ) -> Result<(), Error> {
        if audio.frames == 0 {
            return Ok(());
        }
        self.validate(&audio, &midi, self.frame)?;
        if audio.frames % self.config.quantum.frames() != 0 {
            return Err(Error::InvalidFrame);
        }
        for out in audio.outputs.iter_mut() {
            out[..audio.frames].fill(0.);
        }
        for out in midi.outputs.iter_mut() {
            out.clear();
        }
        let result = (|| {
            self.begin_callback()?;
            for start in (0..audio.frames).step_by(self.config.quantum.frames()) {
                self.render_tick(&mut audio, &mut midi, start)?;
            }
            Ok(())
        })();
        let result = self.end_callback(result);
        if result.is_err() {
            for out in audio.outputs.iter_mut() {
                out[..audio.frames].fill(0.);
            }
            for out in midi.outputs.iter_mut() {
                out.clear();
            }
        }
        result
    }
    pub(super) fn render_tick(
        &mut self,
        audio: &mut AudioBlock<'_, '_>,
        midi: &mut MidiBlock<'_>,
        start: usize,
    ) -> Result<(), Error> {
        let tick = Tick {
            frame: self.frame,
            quantum: self.config.quantum,
        };
        let end = tick.end()?;
        let q = tick.quantum.frames();
        if let Some(queue) = &mut self.operations {
            let slots = &mut self.slots;
            let state = &mut self.state;
            let resources = &mut self.resources;
            let graph_epoch = self.epoch;
            queue.dispatch(tick.frame, |command| {
                slots
                    .get_mut(command.slot)
                    .ok_or(Error::InvalidId)?
                    .processor
                    .operation(
                        command.value,
                        OperationContext {
                            graph_epoch,
                            tick,
                            resources: resources.io(),
                        },
                        state,
                    )
            })?;
        }
        if let Some(mut submission) = self
            .graph_inbox
            .as_mut()
            .and_then(|i| i.take_due(self.frame))
        {
            let result = self.exchange_graph(submission.expected, &mut submission.graph);
            if let Some(inbox) = &mut self.graph_inbox {
                inbox.complete(submission, self.frame, result);
            }
        }
        self.state.begin_tick(tick, &mut self.resources)?;
        let mut first = 0;
        for step in 0..self.graph.steps.len() {
            let routing = self.graph.steps[step];
            let index = routing.slot;
            let slot = &mut self.slots[index];
            slot.input.fill(0.);
            slot.output.fill(0.);
            for m in slot.midi_input.iter_mut().chain(&mut slot.midi_output) {
                m.clear();
            }
            let input_end = routing.input_end;
            for edge in first..input_end {
                self.route(self.graph.routes[edge], audio, midi, start)?;
            }
            let slot = &mut self.slots[index];
            slot.processor.process(
                &mut Block {
                    graph_epoch: self.epoch,
                    callback_frame: self.callback_frame,
                    tick,
                    audio: AudioPorts::new(&slot.input, &mut slot.output, q, 0, q)?,
                    midi_inputs: &slot.midi_input,
                    midi_outputs: &mut slot.midi_output,
                    resources: self.resources.io(),
                },
                &mut self.state,
            )?;
            self.diagnostics.nodes = self.diagnostics.nodes.saturating_add(1);
            first = input_end;
        }
        for edge in first..self.graph.routes.len() {
            self.route(self.graph.routes[edge], audio, midi, start)?;
        }
        self.state.end_tick(tick, &mut self.resources)?;
        self.frame = end;
        self.diagnostics.ticks = self.diagnostics.ticks.saturating_add(1);
        Ok(())
    }
    fn route(
        &mut self,
        edge: Route,
        audio: &mut AudioBlock<'_, '_>,
        midi: &mut MidiBlock<'_>,
        start: usize,
    ) -> Result<(), Error> {
        let q = self.config.quantum.frames();
        let s = edge.source;
        let d = edge.destination;
        match (s.node, d.node) {
            (Some(a), Some(b)) => {
                let (source, target) = if a < b {
                    let (before, after) = self.slots.split_at_mut(b);
                    (&before[a], &mut after[0])
                } else {
                    let (before, after) = self.slots.split_at_mut(a);
                    (&after[0], &mut before[b])
                };
                match edge.kind {
                    PortType::Audio => mix(
                        &source.output[s.port * q..(s.port + 1) * q],
                        &mut target.input[d.port * q..(d.port + 1) * q],
                    ),
                    PortType::Midi => {
                        target.midi_input[d.port].merge(&source.midi_output[s.port], q)?
                    }
                }
            }
            (None, Some(b)) => {
                let target = &mut self.slots[b];
                match edge.kind {
                    PortType::Audio => mix(
                        &audio.inputs[s.port][start..start + q],
                        &mut target.input[d.port * q..(d.port + 1) * q],
                    ),
                    PortType::Midi => target.midi_input[d.port].merge_window(
                        &midi.inputs[s.port],
                        start,
                        q,
                        0,
                        q,
                    )?,
                }
            }
            (Some(a), None) => {
                let source = &self.slots[a];
                match edge.kind {
                    PortType::Audio => mix(
                        &source.output[s.port * q..(s.port + 1) * q],
                        &mut audio.outputs[d.port][start..start + q],
                    ),
                    PortType::Midi => midi.outputs[d.port].merge_window(
                        &source.midi_output[s.port],
                        0,
                        q,
                        start,
                        audio.frames,
                    )?,
                }
            }
            (None, None) => match edge.kind {
                PortType::Audio => mix(
                    &audio.inputs[s.port][start..start + q],
                    &mut audio.outputs[d.port][start..start + q],
                ),
                PortType::Midi => midi.outputs[d.port].merge_window(
                    &midi.inputs[s.port],
                    start,
                    q,
                    start,
                    audio.frames,
                )?,
            },
        }
        self.diagnostics.edges = self.diagnostics.edges.saturating_add(1);
        Ok(())
    }
}
fn mix(source: &[f32], destination: &mut [f32]) {
    for (s, d) in source.iter().zip(destination) {
        *d += *s;
    }
}
