use super::{Quantum, Tick};
use crate::buffer::{AudioBlock, AudioPorts, MidiBlock, MidiBuffer};
use crate::graph::{self, Catalog, Connection, GraphDescription, NodeSpec, PortCounts, PortType};
use crate::resource::{ResourceIo, ResourceStore};
use crate::{Error, Frame, NodeId};
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
    fn begin_callback(
        &mut self,
        _frame: Frame,
        _resources: &mut ResourceStore,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn begin_tick(&mut self, _tick: Tick, _resources: &mut ResourceStore) -> Result<(), Error> {
        Ok(())
    }
    fn end_tick(&mut self, _tick: Tick, _resources: &mut ResourceStore) -> Result<(), Error> {
        Ok(())
    }
}
impl State for () {}

pub struct Block<'a> {
    pub tick: Tick,
    pub audio: AudioPorts<'a>,
    pub midi_inputs: &'a [MidiBuffer],
    pub midi_outputs: &'a mut [MidiBuffer],
    pub resources: ResourceIo<'a>,
}
pub trait Processor<S: State>: Send {
    fn validate_state(&self, _id: NodeId, _state: &S) -> Result<(), Error> {
        Ok(())
    }
    fn ports(&self) -> PortCounts;
    fn process(&mut self, block: &mut Block<'_>, state: &mut S) -> Result<(), Error>;
}

struct Slot<P> {
    processor: P,
    input: Box<[f32]>,
    output: Box<[f32]>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
    input_end: usize,
}
#[derive(Clone, Copy)]
struct Address {
    node: Option<usize>,
    port: usize,
}
#[derive(Clone, Copy)]
struct Route {
    source: Address,
    destination: Address,
    kind: PortType,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    pub ticks: usize,
    pub nodes: usize,
    pub edges: usize,
    pub fault: Option<Error>,
}

pub struct Runtime<P, S> {
    pub(super) config: PreparedConfig,
    slots: Vec<Slot<P>>,
    routes: Vec<Route>,
    state: S,
    resources: ResourceStore,
    pub(super) frame: Frame,
    diagnostics: Diagnostics,
    fault: Option<Error>,
    recovery: Vec<crate::midi::ResetState>,
    recovery_remaining: Vec<usize>,
}

impl<P: Processor<S>, S: State> Runtime<P, S> {
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
        let catalog = Catalog::new(config.resources, &specs)?;
        let plan = graph::compile(&catalog, description)?;
        let descriptor = plan.descriptor();
        let order: Vec<_> = descriptor.steps.iter().map(|s| s.node).collect();
        let bind = |edge: &Connection| -> Result<Route, Error> {
            let address = |e: graph::Endpoint| -> Result<Address, Error> {
                Ok(Address {
                    node: e
                        .node
                        .map(|id| order.iter().position(|&n| n == id).ok_or(Error::InvalidId))
                        .transpose()?,
                    port: e.port as usize,
                })
            };
            Ok(Route {
                source: address(edge.source)?,
                destination: address(edge.destination)?,
                kind: edge.source.kind,
            })
        };
        let routes = descriptor
            .connections
            .iter()
            .map(bind)
            .collect::<Result<Vec<_>, _>>()?;
        let mut nodes: Vec<_> = nodes.into_iter().map(Some).collect();
        let mut slots = Vec::with_capacity(order.len());
        let limits = config.resources.limits();
        let q = config.quantum.frames();
        for step in &descriptor.steps {
            let index = nodes
                .iter()
                .position(|n| n.as_ref().is_some_and(|(id, _)| *id == step.node))
                .ok_or(Error::InvalidId)?;
            let (_, processor) = nodes[index].take().ok_or(Error::InvalidId)?;
            let ports = catalog.node(step.node)?.ports;
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
                input_end: step.input_end,
            });
        }
        Ok(Self {
            config,
            slots,
            routes,
            state,
            resources,
            frame: Frame(0),
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
        self.resources.begin_callback();
        self.state.begin_callback(self.frame, &mut self.resources)
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
        self.state.begin_tick(tick, &mut self.resources)?;
        let mut first = 0;
        for index in 0..self.slots.len() {
            let slot = &mut self.slots[index];
            slot.input.fill(0.);
            slot.output.fill(0.);
            for m in slot.midi_input.iter_mut().chain(&mut slot.midi_output) {
                m.clear();
            }
            let input_end = slot.input_end;
            for edge in first..input_end {
                self.route(self.routes[edge], audio, midi, start)?;
            }
            let slot = &mut self.slots[index];
            slot.processor.process(
                &mut Block {
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
        for edge in first..self.routes.len() {
            self.route(self.routes[edge], audio, midi, start)?;
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
                let (before, after) = self.slots.split_at_mut(b);
                let source = &before[a];
                let target = &mut after[0];
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
