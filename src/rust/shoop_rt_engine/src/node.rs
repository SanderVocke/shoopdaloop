//! The engine's node type: the loop library's playback and capture nodes next
//! to the engine's own strip stages, loop faders and output buses.

use crate::dsp::{Gains, Ramp};
use alloc::boxed::Box;
use alloc::vec;
use core::sync::atomic::Ordering;

use crate::cells::{Cells, Peak};
use crate::stream::BulkReader;
use alloc::sync::Arc;
use rt_runtime::buffer::MidiBuffer;
use rt_runtime::graph::PortCounts;
use rt_runtime::midi_state::MidiState;
use rt_runtime::processor::{OperationContext, ProcessBlock, Processor, ProcessorOperation};
use rt_runtime::resource::Cursor;
use rt_runtime::Error;
use shoop_loops::runtime::{LoopNode, LoopPlanner};
use shoop_rt_protocol::MidiMessage;

/// Most injected MIDI messages a strip holds between callbacks.
const INJECTED: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeConfiguration {
    pub recorded_state: Option<shoop_rt_protocol::RecordedProcessorState>,
    pub processor: Option<shoop_rt_protocol::ProcessorKind>,
    pub generation: u32,
    pub token: u64,
    pub epoch: rt_runtime::Epoch,
    pub inputs: u8,
    pub outputs: u8,
    pub bank: u8,
    pub send_bank: u8,
    pub return_bank: u8,
    pub gain: f32,
    pub balance: f32,
    pub monitoring: bool,
    pub muted: bool,
}

/// Typed operations of the engine's own processors, delivered by the runtime
/// immediately before their frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineOp {
    RecordedState {
        generation: u32,
        revision: u64,
        state: Option<shoop_rt_protocol::RecordedProcessorState>,
    },
    ProcessorControl {
        generation: u32,
        control: shoop_rt_protocol::processors::Control,
    },
    ProcessorClaim {
        generation: u32,
    },
    ProcessorBindBus {
        generation: u32,
        bus: shoop_rt_protocol::BusId,
        epoch: rt_runtime::Epoch,
    },
    ProcessorBind {
        generation: u32,
        strip_generation: u32,
        source: u32,
        epoch: rt_runtime::Epoch,
    },
    ProcessorStart {
        generation: u32,
    },
    ProcessorSuspend {
        generation: u32,
    },
    ProcessorRetire {
        generation: u32,
    },
    ProcessorParameter {
        generation: u32,
        parameter: u32,
        value: f32,
    },
    /// Retain values for the next graph epoch without changing the active processor.
    Configure(NodeConfiguration),
    InputSource {
        epoch: rt_runtime::Epoch,
        changed: bool,
    },
    DiscardConfiguration {
        token: u64,
    },
    /// Fresh state for a newly occupied slot.
    Reset {
        inputs: u8,
        outputs: u8,
        generation: u32,
    },
    Gain(f32),
    Balance(f32),
    Monitor(bool),
    Mute(bool),
    /// Fade the node's whole output in or out over the prepared ramp.
    Presence(bool),
    /// Move to the other connection bank over the prepared ramp.
    Crossfade,
    CrossfadeSend,
    CrossfadeReturn,
    Inject(MidiMessage),
    /// Stream `bytes` of an exported medium from `cursor` into the bulk ring.
    ReadStart {
        cursor: Cursor,
        bytes: u64,
    },
    ReadRolesStart {
        cursors: [Cursor; 2],
        bytes: u64,
        channels: u8,
    },
    ReadStop,
    PreviewStart {
        cursor: Cursor,
        frames: u64,
        request_id: u64,
        source_channels: u8,
    },
    PreviewStop,
}
impl ProcessorOperation for EngineOp {
    type Result = ();
}
impl Default for EngineOp {
    fn default() -> Self {
        Self::Gain(1.)
    }
}

/// Largest absolute sample value. Non-negative IEEE 754 floats order like their
/// bit patterns, so integer max avoids per-sample float compares, which stall
/// the Cortex-A9 VFP pipeline. A NaN sample reads as the largest value.
pub(crate) fn peak(samples: &[f32]) -> f32 {
    f32::from_bits(
        samples
            .iter()
            .fold(0, |peak, sample| peak.max(sample.to_bits() & 0x7FFF_FFFF)),
    )
}

/// A strip's input stage: two connection banks of host inputs, input gain and
/// balance, a capture feed for its loops and a monitoring feed.
pub struct StripInput {
    midi_capacity: crate::midi_layout::Layout,
    global: Option<crate::global_midi::GlobalMidi>,
    processor: Option<shoop_rt_protocol::ProcessorKind>,
    generation: u32,
    pending_history: Option<(rt_runtime::Epoch, bool)>,
    history_token: u64,
    midi_history_token: u64,
    configuration: Option<NodeConfiguration>,
    slot: usize,
    cells: Arc<Cells>,
    channels: usize,
    ramp: u32,
    inputs: usize,
    bank: Ramp,
    send_bank: Ramp,
    return_bank: Ramp,
    send_weights: Box<[f32]>,
    return_weights: Box<[f32]>,
    monitor: Ramp,
    manual_monitoring: bool,
    automatic_enabled: bool,
    automatic_active: bool,
    unity: Ramp,
    gains: Gains,
    weights: Box<[f32]>,
    monitoring: Box<[f32]>,
    scratch: Box<[f32]>,
    sends: Box<[f32]>,
    injected: [Option<MidiMessage>; INJECTED],
    /// Notes sounding through the monitoring feed, released when monitoring stops.
    monitored: Box<MidiState>,
    live_midi: Box<MidiState>,
    live_midi_active: bool,
    release: bool,
}

/// A strip's output stage: the sum of its loops and monitored input; output gain,
/// balance, mute and a presence fade, written to two connection banks.
pub struct StripOutput {
    midi_capacity: rt_runtime::buffer::MidiCapacity,
    processor: Option<shoop_rt_protocol::ProcessorKind>,
    wet_gate: Box<[f32]>,
    configuration: Option<NodeConfiguration>,
    slot: usize,
    cells: Arc<Cells>,
    channels: usize,
    ramp: u32,
    outputs: usize,
    bank: Ramp,
    mute: Ramp,
    presence: Ramp,
    gains: Gains,
    weights: Box<[f32]>,
    scales: Box<[f32]>,
    scratch: Box<[f32]>,
    gate: Box<[f32]>,
}

/// A loop's playback fader, used to remove a sounding loop without a step, and
/// its level meter.
pub struct LoopFader {
    processor: Option<shoop_rt_protocol::ProcessorKind>,
    delay: Box<[f32]>,
    position: usize,
    configuration: Option<NodeConfiguration>,
    slot: usize,
    cells: Arc<Cells>,
    channels: usize,
    ramp: u32,
    presence: Ramp,
    levels: Box<[f32]>,
}

/// The Master bus: the sum of routed strips; gain, balance, mute and meters,
/// written to two connection banks.
pub struct BusMixer {
    generation: u32,
    preview: Option<(Cursor, u64, u64, usize)>,
    preview_bytes: Box<[u8]>,
    preview_samples: Box<[f32]>,
    slot: usize,
    active_channels: u8,
    presence: Ramp,
    presence_weights: Box<[f32]>,
    configuration: Option<NodeConfiguration>,
    cells: Arc<Cells>,
    channels: usize,
    ramp: u32,
    bank: Ramp,
    mute: Ramp,
    gains: Gains,
    weights: Box<[f32]>,
    scratch: Box<[f32]>,
}

pub enum EngineNode {
    Processor(crate::HostedProcessor),
    Loop(LoopNode),
    StripInput(StripInput),
    StripOutput(StripOutput),
    Fader(LoopFader),
    Bus(BusMixer),
    BusInput(BusMixer),
    Reader(BulkReader),
}

pub(crate) struct NodeSettings {
    pub midi: crate::midi_layout::Layout,
    pub external_midi: Option<(usize, usize)>,
    pub cells: Arc<Cells>,
    pub channels: usize,
    pub master_channels: usize,
    pub frames: usize,
    pub ramp: u32,
}

impl StripInput {
    fn reset(&mut self, inputs: u8) {
        if let Some(global) = &mut self.global {
            global.reset();
        }
        self.processor = None;
        self.cells.strips[self.slot]
            .forced_monitor_off
            .store(0, core::sync::atomic::Ordering::Relaxed);
        self.cells.strips[self.slot]
            .processor_demand
            .store(false, core::sync::atomic::Ordering::Relaxed);
        self.generation = 0;
        self.cells.strips[self.slot]
            .owner_generation
            .store(0, core::sync::atomic::Ordering::Relaxed);
        self.inputs = (inputs as usize).min(self.channels);
        self.bank = Ramp::at(0.);
        self.send_bank = Ramp::at(0.);
        self.return_bank = Ramp::at(0.);
        self.monitor = Ramp::at(0.);
        self.manual_monitoring = false;
        self.cells.strips[self.slot]
            .automatic_monitoring
            .store(false, core::sync::atomic::Ordering::Relaxed);
        self.gains.reset();
        self.injected = [None; INJECTED];
        *self.monitored = MidiState::default();
        *self.live_midi = MidiState::default();
        self.live_midi_active = false;
        self.release = false;
        let cells = &self.cells.strips[self.slot];
        cells.routing_state.store(0, Ordering::Relaxed);
        cells.latest_input_midi.store(0, Ordering::Relaxed);
        cells.input.clear();
        cells.input.reset_snapshot();
        cells.mark_midi(true, false);
    }
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        let frames = settings.frames;
        Self {
            midi_capacity: settings.midi,
            global: settings.external_midi.map(|_| {
                crate::global_midi::GlobalMidi::new(settings.midi.send, settings.midi.global)
                    .expect("prepared MIDI capacity")
            }),
            processor: None,
            generation: 0,
            history_token: 1,
            midi_history_token: 1,
            pending_history: None,
            configuration: None,
            slot,
            cells: settings.cells.clone(),
            channels: settings.channels,
            ramp: settings.ramp,
            inputs: 0,
            bank: Ramp::at(0.),
            send_bank: Ramp::at(0.),
            return_bank: Ramp::at(0.),
            send_weights: vec![0.; frames].into_boxed_slice(),
            return_weights: vec![0.; frames].into_boxed_slice(),
            monitor: Ramp::at(0.),
            manual_monitoring: false,
            automatic_enabled: false,
            automatic_active: false,
            unity: Ramp::at(1.),
            gains: Gains::new(frames),
            weights: vec![0.; frames].into_boxed_slice(),
            monitoring: vec![0.; frames].into_boxed_slice(),
            scratch: vec![0.; frames].into_boxed_slice(),
            sends: vec![0.; frames].into_boxed_slice(),
            injected: [None; INJECTED],
            monitored: Box::default(),
            live_midi: Box::default(),
            live_midi_active: false,
            release: false,
        }
    }
}
impl StripOutput {
    fn reset(&mut self, outputs: u8) {
        self.processor = None;
        self.wet_gate.fill(0.);
        self.outputs = (outputs as usize).min(self.channels);
        self.cells.strips[self.slot]
            .snapshot_output_channels
            .store(self.outputs as u32, Ordering::Relaxed);
        self.bank = Ramp::at(0.);
        self.mute = Ramp::at(1.);
        self.presence = Ramp::at(1.);
        self.gains.reset();
        let cells = &self.cells.strips[self.slot];
        cells.output.clear();
        cells.output.reset_snapshot();
        cells.mark_midi(false, false);
    }
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        let frames = settings.frames;
        Self {
            midi_capacity: settings.midi.mixer,
            processor: None,
            wet_gate: vec![0.; frames].into_boxed_slice(),
            configuration: None,
            slot,
            cells: settings.cells.clone(),
            channels: settings.channels,
            ramp: settings.ramp,
            outputs: 0,
            bank: Ramp::at(0.),
            mute: Ramp::at(1.),
            presence: Ramp::at(1.),
            gains: Gains::new(frames),
            weights: vec![0.; frames].into_boxed_slice(),
            scales: vec![0.; frames].into_boxed_slice(),
            scratch: vec![0.; frames].into_boxed_slice(),
            gate: vec![0.; frames].into_boxed_slice(),
        }
    }
}
impl LoopFader {
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        Self {
            processor: None,
            delay: vec![0.; settings.frames * settings.channels].into_boxed_slice(),
            position: 0,
            configuration: None,
            slot,
            cells: settings.cells.clone(),
            channels: settings.channels,
            ramp: settings.ramp,
            presence: Ramp::at(1.),
            levels: vec![0.; settings.frames].into_boxed_slice(),
        }
    }
}
impl BusMixer {
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        Self {
            generation: if slot == 0 { 1 } else { 0 },
            preview: None,
            preview_bytes: vec![0; settings.frames * settings.channels * 4].into_boxed_slice(),
            preview_samples: vec![0.; settings.frames].into_boxed_slice(),
            slot,
            active_channels: if slot == 0 { 2 } else { 0 },
            presence: Ramp::at(1.),
            presence_weights: vec![0.; settings.frames].into_boxed_slice(),
            configuration: None,
            cells: settings.cells.clone(),
            channels: settings.master_channels,
            ramp: settings.ramp,
            bank: Ramp::at(0.),
            mute: Ramp::at(1.),
            gains: Gains::new(settings.frames),
            weights: vec![0.; settings.frames].into_boxed_slice(),
            scratch: vec![0.; settings.frames].into_boxed_slice(),
        }
    }
}

fn copy_midi(source: &MidiBuffer, target: &mut MidiBuffer, frames: usize) -> Result<bool, Error> {
    for index in 0..source.len() {
        let (offset, packet) = source.get(index).ok_or(Error::InvalidState)?;
        target.insert(offset, frames, packet)?;
    }
    Ok(!source.is_empty())
}

impl Processor<LoopPlanner, EngineOp> for StripInput {
    fn midi_buffer_settings(&self) -> Result<rt_runtime::processor::MidiBufferSettings, Error> {
        use rt_runtime::processor::{MidiBufferSettings, MidiPortCapacity};
        let mut settings = MidiBufferSettings::default();
        for (index, (input, port, capacity)) in [
            (true, 1, self.midi_capacity.playback_merge),
            (true, 2, self.midi_capacity.global),
            (false, 0, self.midi_capacity.input),
            (false, 1, self.midi_capacity.monitor),
            (false, 2, self.midi_capacity.send),
            (false, 3, self.midi_capacity.global),
        ]
        .into_iter()
        .enumerate()
        {
            settings.ports[index] = Some(MidiPortCapacity {
                input,
                port,
                capacity,
            });
        }
        Ok(settings)
    }
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 5 * self.channels,
            audio_outputs: 5 * self.channels,
            midi_inputs: 3,
            midi_outputs: 4,
        }
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        self.apply(operation)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.render(block)
    }
}

impl Processor<LoopPlanner, EngineOp> for StripOutput {
    fn midi_buffer_settings(&self) -> Result<rt_runtime::processor::MidiBufferSettings, Error> {
        Ok(rt_runtime::processor::MidiBufferSettings {
            input: Some(self.midi_capacity),
            forward_input: Some(0),
            ..Default::default()
        })
    }
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2 * self.channels,
            audio_outputs: 2 * self.channels,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        self.apply(operation)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.render(block)
    }
}

impl Processor<LoopPlanner, EngineOp> for LoopFader {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2 * self.channels,
            audio_outputs: 2 * self.channels,
            ..PortCounts::default()
        }
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        self.apply(operation)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.render(block)
    }
}

impl Processor<LoopPlanner, EngineOp> for BusMixer {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: self.channels,
            audio_outputs: 2 * self.channels,
            ..PortCounts::default()
        }
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        self.apply(operation)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.render_output(block)
    }
}

impl EngineNode {
    fn configuration(&mut self) -> Option<&mut Option<NodeConfiguration>> {
        match self {
            Self::StripInput(node) => Some(&mut node.configuration),
            Self::StripOutput(node) => Some(&mut node.configuration),
            Self::Fader(node) => Some(&mut node.configuration),
            Self::Bus(node) => Some(&mut node.configuration),
            _ => None,
        }
    }
    fn configure(&mut self, configuration: NodeConfiguration) {
        match self {
            Self::StripInput(node) => {
                node.pending_history = None;
                node.history_token = node.history_token.checked_add(1).unwrap_or(0);
                node.midi_history_token = node.midi_history_token.checked_add(1).unwrap_or(0);
                node.reset(configuration.inputs);
                node.processor = configuration.processor;
                node.generation = configuration.generation;
                node.cells.strips[node.slot].owner_generation.store(
                    configuration.generation,
                    core::sync::atomic::Ordering::Relaxed,
                );
                node.bank = Ramp::at(f32::from(configuration.bank));
                node.send_bank = Ramp::at(f32::from(configuration.send_bank));
                node.return_bank = Ramp::at(f32::from(configuration.return_bank));
                node.gains.gain = Ramp::at(configuration.gain);
                node.gains.balance = Ramp::at(configuration.balance);
                node.manual_monitoring = configuration.monitoring;
                node.monitor = Ramp::at(f32::from(u8::from(configuration.monitoring)));
            }
            Self::StripOutput(node) => {
                node.reset(configuration.outputs);
                node.processor = configuration.processor;
                node.bank = Ramp::at(f32::from(configuration.bank));
                node.gains.gain = Ramp::at(configuration.gain);
                node.gains.balance = Ramp::at(configuration.balance);
                node.mute = Ramp::at(f32::from(u8::from(!configuration.muted)));
            }
            Self::Fader(node) => {
                if let Some(state) = configuration.recorded_state {
                    let cell = &node.cells.loops[node.slot];
                    cell.recorded
                        .publish(cell, configuration.generation, 0, Some(state));
                }
                node.processor = configuration.processor;
                node.delay.fill(0.);
                node.position = 0;
                node.presence = Ramp::at(1.);
                node.cells.loops[node.slot].peaks.clear();
                node.cells.loops[node.slot].peaks.reset_snapshot();
            }
            Self::Bus(node) => {
                node.generation = configuration.generation;
                node.active_channels = configuration.outputs;
                node.presence = Ramp::at(1.);
                node.cells.buses[node.slot].clear();
                node.cells.buses[node.slot].reset_snapshot();
                node.bank = Ramp::at(f32::from(configuration.bank));
                node.gains.gain = Ramp::at(configuration.gain);
                node.gains.balance = Ramp::at(configuration.balance);
                node.mute = Ramp::at(f32::from(u8::from(!configuration.muted)));
            }
            _ => unreachable!("configuration belongs to an engine control processor"),
        }
    }
}

impl Processor<LoopPlanner, EngineOp> for EngineNode {
    fn settle(&mut self, epoch: rt_runtime::Epoch, domain: &LoopPlanner) {
        self.apply_configuration(epoch);
        if let Self::StripInput(node) = self {
            node.settle_monitoring(domain);
        }
    }
    fn midi_buffer_settings(&self) -> Result<rt_runtime::processor::MidiBufferSettings, Error> {
        match self {
            Self::Loop(node) => node.midi_buffer_settings(),
            Self::StripInput(node) => node.midi_buffer_settings(),
            Self::StripOutput(node) => node.midi_buffer_settings(),
            Self::Processor(node) => node.midi_buffer_settings(),
            _ => Ok(Default::default()),
        }
    }
    fn admit(&self, node: rt_runtime::NodeId, domain: &LoopPlanner) -> Result<(), Error> {
        match self {
            Self::Loop(processor) => processor.admit(node, domain),
            _ => Ok(()),
        }
    }

    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(node) => node.ports(),
            Self::StripInput(node) => node.ports(),
            Self::StripOutput(node) => node.ports(),
            Self::Fader(node) => node.ports(),
            Self::Bus(node) => node.ports(),
            Self::BusInput(node) => PortCounts {
                audio_inputs: node.channels,
                audio_outputs: node.channels,
                ..PortCounts::default()
            },
            Self::Reader(node) => node.ports(),
            Self::Processor(node) => node.ports(),
        }
    }
    fn operation(
        &mut self,
        operation: EngineOp,
        context: OperationContext<'_>,
    ) -> Result<(), Error> {
        if let Self::Processor(node) = self {
            return match operation {
                EngineOp::ProcessorClaim { generation } => node.claim(context.frame, generation),
                EngineOp::ProcessorBindBus {
                    generation,
                    bus,
                    epoch,
                } => {
                    if context.graph_epoch.checked_next()? != epoch {
                        return Err(Error::StaleEpoch);
                    }
                    node.bind_bus(generation, bus, epoch)
                }
                EngineOp::ProcessorBind {
                    generation,
                    strip_generation,
                    source,
                    epoch,
                } => {
                    if context.graph_epoch.checked_next()? != epoch {
                        return Err(Error::StaleEpoch);
                    }
                    node.bind(generation, source, strip_generation, epoch)
                }
                EngineOp::ProcessorStart { generation } => node.start(context.frame, generation),
                EngineOp::ProcessorSuspend { generation } => {
                    node.suspend(context.frame, generation)
                }
                EngineOp::ProcessorRetire { generation } => node.retire(context.frame, generation),
                EngineOp::ProcessorParameter {
                    generation,
                    parameter,
                    value,
                } => node.parameter(context.frame, generation, parameter, value),
                EngineOp::ProcessorControl {
                    generation,
                    control,
                } => node.control(context.frame, generation, control),
                _ => Err(Error::InvalidEvent),
            };
        }
        self.apply_signal(operation, context.graph_epoch)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.apply_configuration(block.graph_epoch);
        match self {
            Self::Loop(node) => node.process(block),
            Self::StripInput(node) => {
                node.render(block)?;
                block.domain.observe_history(
                    node.slot as u32,
                    node.history_token,
                    node.midi_history_token,
                    block.frame,
                    block.quantum.frames(),
                    &mut block.audio,
                    block.midi_outputs.first(),
                    &mut block.resources,
                )
            }
            _ => self.render_signal(block),
        }
    }
}

impl StripInput {
    pub(crate) fn render(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        let result = self.render_sources(block);
        if result == Err(Error::Full) {
            *self.monitored = MidiState::default();
            *self.live_midi = MidiState::default();
            self.live_midi_active = false;
            self.release = false;
        }
        result
    }
    fn settle_monitoring(&mut self, domain: &LoopPlanner) {
        let activity = domain.source_activity(self.slot as u32);
        self.automatic_enabled = self.cells.strips[self.slot].policy_enabled(self.generation);
        let forced_off =
            self.processor.is_some() && (activity.rerecording || activity.pre_rerecording);
        let cells = &self.cells.strips[self.slot];
        if forced_off {
            self.manual_monitoring = false;
            if self.monitor.target() > 0. {
                self.release = true;
                self.monitor = Ramp::at(0.);
                cells
                    .forced_monitor_off
                    .store(self.generation, core::sync::atomic::Ordering::Relaxed);
            }
        }
        let automatic = !forced_off
            && self.automatic_enabled
            && cells
                .automatic_demand
                .load(core::sync::atomic::Ordering::Relaxed);
        self.automatic_active = automatic;
        cells
            .automatic_monitoring
            .store(automatic, core::sync::atomic::Ordering::Relaxed);
        let target = f32::from(u8::from(self.manual_monitoring || automatic));
        if self.monitor.target() != target {
            self.release |= target == 0. && self.monitor.target() > 0.;
            self.monitor.set(target, self.ramp);
        }
    }
    fn render_sources(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let frames = block.frames;
        let channels = self.channels;
        let stereo = self.inputs == 2;
        let activity = block.domain.source_activity(self.slot as u32);
        let processed = self.processor.is_some();
        let forced_off = processed && (activity.rerecording || activity.pre_rerecording);
        self.settle_monitoring(block.domain);
        self.gains.fill(frames, &mut self.unity);
        let cells = &self.cells.strips[self.slot];
        cells.routing_state.store(
            u32::from(self.monitor.target() > 0.)
                | (u32::from(activity.recording) << 1)
                | (u32::from(activity.pre_recording) << 2)
                | (u32::from(activity.playing_dry) << 3)
                | (u32::from(activity.rerecording) << 4)
                | (u32::from(activity.pre_rerecording) << 5),
            Ordering::Relaxed,
        );
        let live_record = activity.recording || activity.pre_recording;
        let dry_play = activity.playing_dry || activity.rerecording || activity.pre_rerecording;
        // Per-quantum decisions; the per-frame loops below are straight arithmetic.
        self.bank.fill(&mut self.weights[..frames]);
        self.send_bank.fill(&mut self.send_weights[..frames]);
        self.return_bank.fill(&mut self.return_weights[..frames]);
        let monitor_from = self.monitor.value;
        self.monitor.fill(&mut self.monitoring[..frames]);
        // A linear segment is bounded by its endpoints.
        let monitored = monitor_from > 0. || self.monitor.value > 0.;
        let demand = live_record || dry_play || monitored;
        let live = !forced_off && (live_record || monitored);
        if dry_play {
            for cell in &cells.wet_gate[..frames] {
                cell.store(1f32.to_bits(), core::sync::atomic::Ordering::Relaxed);
            }
        } else {
            for (cell, gate) in cells.wet_gate[..frames]
                .iter()
                .zip(&self.monitoring[..frames])
            {
                cell.store(gate.to_bits(), core::sync::atomic::Ordering::Relaxed);
            }
        }
        cells
            .processor_demand
            .store(demand, core::sync::atomic::Ordering::Relaxed);
        for channel in 0..channels {
            {
                let first = block.audio.input(channel).ok_or(Error::InvalidShape)?;
                let second = block
                    .audio
                    .input(channels + channel)
                    .ok_or(Error::InvalidShape)?;
                let gains = self.gains.channel(channel, stereo);
                for i in 0..frames {
                    let weight = self.weights[i];
                    self.scratch[i] = (first[i] * (1. - weight) + second[i] * weight) * gains[i];
                }
                if channel < self.inputs {
                    cells.input.raise(channel, peak(&self.scratch[..frames]));
                }
            }
            block
                .audio
                .output(channel)
                .ok_or(Error::InvalidShape)?
                .copy_from_slice(&self.scratch[..frames]);
            let monitor = block
                .audio
                .output(channels + channel)
                .ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                monitor[i] = self.scratch[i] * self.monitoring[i];
            }
            {
                let playback = block
                    .audio
                    .input(2 * channels + channel)
                    .ok_or(Error::InvalidShape)?;
                if live {
                    for i in 0..frames {
                        self.sends[i] = self.scratch[i] + playback[i];
                    }
                } else {
                    self.sends[..frames].copy_from_slice(&playback[..frames]);
                }
            }
            let dry = block
                .audio
                .output(2 * channels + channel)
                .ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                dry[i] = self.sends[i] * (1. - self.send_weights[i]);
            }
            let wet = block
                .audio
                .output(3 * channels + channel)
                .ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                wet[i] = self.sends[i] * self.send_weights[i];
            }
            // The return mix reads two inputs into one output; stage the second.
            self.sends[..frames].copy_from_slice(
                &block
                    .audio
                    .input(4 * channels + channel)
                    .ok_or(Error::InvalidShape)?[..frames],
            );
            let (first, returned) = block
                .audio
                .pair(3 * channels + channel, 4 * channels + channel)
                .ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                let weight = self.return_weights[i];
                returned[i] = first[i] * (1. - weight) + self.sends[i] * weight;
            }
        }
        let input = block.midi_inputs.first().ok_or(Error::InvalidShape)?;
        let [capture, monitor, send, global] = block.midi_outputs else {
            return Err(Error::InvalidShape);
        };
        if self.release {
            self.release = false;
            for channel in 0..16u8 {
                let state = self.monitored.channel(channel).ok_or(Error::InvalidState)?;
                for (key, _) in state.projected_notes() {
                    monitor.push(0, frames, &[0x80 | channel, key, 0])?;
                }
            }
            *self.monitored = MidiState::default();
        }
        let monitoring = self.monitor.target() > 0.;
        let mut active = false;
        for message in self.injected.iter_mut() {
            if let Some(packet) = message.take() {
                cells.observe_midi(packet.as_bytes());
                capture.push(0, frames, packet.as_bytes())?;
                if monitoring {
                    monitor.push(0, frames, packet.as_bytes())?;
                    let _ = self
                        .monitored
                        .apply_packet(packet.as_bytes(), packet.as_bytes().len());
                }
                active = true;
            }
        }
        active |= copy_midi(input, capture, frames)?;
        for index in 0..input.len() {
            let (_, packet) = input.get(index).ok_or(Error::InvalidState)?;
            cells.observe_midi(packet);
        }
        if monitoring {
            for index in 0..input.len() {
                let (offset, packet) = input.get(index).ok_or(Error::InvalidState)?;
                monitor.push(offset, frames, packet)?;
                let _ = self.monitored.apply_packet(packet, packet.len());
            }
        }
        if processed {
            let live = !forced_off && (live_record || monitoring);
            if self.live_midi_active && !live {
                for channel in 0..16u8 {
                    let state = self.live_midi.channel(channel).ok_or(Error::InvalidState)?;
                    for (key, _) in state.projected_notes() {
                        send.insert(0, frames, &[0x80 | channel, key, 0])?;
                    }
                    if state.controller(64).is_some_and(|value| value > 0) {
                        send.insert(0, frames, &[0xb0 | channel, 64, 0])?;
                    }
                }
                *self.live_midi = MidiState::default();
            }
            self.live_midi_active = live;
            if live {
                copy_midi(capture, send, frames)?;
                for index in 0..capture.len() {
                    let (_, packet) = capture.get(index).ok_or(Error::InvalidState)?;
                    self.live_midi.apply_packet(packet, packet.len())?;
                }
            }
            copy_midi(&block.midi_inputs[1], send, frames)?;
            copy_midi(&block.midi_inputs[2], global, frames)?;
        }
        if input.is_recovery() || block.midi_inputs[1].is_recovery() {
            *self.live_midi = MidiState::default();
            self.live_midi_active = false;
            return Err(Error::Full);
        }
        if self.processor == Some(shoop_rt_protocol::ProcessorKind::External) {
            if let Some(global) = &mut self.global {
                global.render(&block.midi_inputs[2], send, frames, demand)?;
                global.merge_into(send, frames)?;
            }
        }
        if active {
            cells.mark_midi(true, true);
        }
        Ok(())
    }
}

impl StripInput {
    pub(crate) fn apply(&mut self, operation: EngineOp) -> Result<(), Error> {
        match operation {
            EngineOp::Reset {
                inputs, generation, ..
            } => {
                self.generation = generation;
                self.pending_history = None;
                self.history_token = self.history_token.checked_add(1).unwrap_or(0);
                self.midi_history_token = self.midi_history_token.checked_add(1).unwrap_or(0);
                self.reset(inputs);
            }
            EngineOp::Gain(value) => self.gains.gain.set(value, self.ramp),
            EngineOp::Balance(value) => self.gains.balance.set(value, self.ramp),
            EngineOp::Monitor(on) => {
                self.cells.strips[self.slot]
                    .forced_monitor_off
                    .store(0, core::sync::atomic::Ordering::Relaxed);
                self.manual_monitoring = on;
            }
            EngineOp::CrossfadeSend => self.send_bank.set(1. - self.send_bank.target(), self.ramp),
            EngineOp::CrossfadeReturn => self
                .return_bank
                .set(1. - self.return_bank.target(), self.ramp),
            EngineOp::Crossfade => {
                self.history_token = self.history_token.checked_add(1).unwrap_or(0);
                let target = 1. - self.bank.target();
                self.bank.set(target, self.ramp);
            }
            EngineOp::Inject(message) => {
                let slot = self
                    .injected
                    .iter_mut()
                    .find(|m| m.is_none())
                    .ok_or(Error::Full)?;
                *slot = Some(message);
            }
            EngineOp::Mute(_)
            | EngineOp::InputSource { .. }
            | EngineOp::Configure(_)
            | EngineOp::DiscardConfiguration { .. }
            | EngineOp::Presence(_)
            | EngineOp::ReadStart { .. }
            | EngineOp::ReadRolesStart { .. }
            | EngineOp::ReadStop
            | EngineOp::PreviewStart { .. }
            | EngineOp::PreviewStop
            | EngineOp::ProcessorBindBus { .. }
            | EngineOp::ProcessorBind { .. }
            | EngineOp::ProcessorClaim { .. }
            | EngineOp::ProcessorStart { .. }
            | EngineOp::ProcessorSuspend { .. }
            | EngineOp::ProcessorRetire { .. }
            | EngineOp::ProcessorParameter { .. }
            | EngineOp::ProcessorControl { .. }
            | EngineOp::RecordedState { .. } => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
}

impl StripOutput {
    pub(crate) fn render(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        let frames = block.frames;
        let channels = self.channels;
        let stereo = self.outputs == 2;
        self.bank.fill(&mut self.weights[..frames]);
        self.mute.fill(&mut self.scales[..frames]);
        self.gains.fill(frames, &mut self.presence);
        let cells = &self.cells.strips[self.slot];
        // Choose the wet gate source once per quantum.
        let gate: Option<&[f32]> = match self.processor {
            None => None,
            Some(shoop_rt_protocol::ProcessorKind::External) => {
                for (gate, cell) in self.gate[..frames]
                    .iter_mut()
                    .zip(&cells.wet_gate[..frames])
                {
                    *gate = f32::from_bits(cell.load(core::sync::atomic::Ordering::Relaxed));
                }
                Some(&self.gate[..frames])
            }
            Some(_) => Some(&self.wet_gate[..frames]),
        };
        for channel in 0..channels {
            {
                let input = block.audio.input(channel).ok_or(Error::InvalidShape)?;
                let wet = block
                    .audio
                    .input(channels + channel)
                    .ok_or(Error::InvalidShape)?;
                let gains = self.gains.channel(channel, stereo);
                match gate {
                    Some(gate) => {
                        for i in 0..frames {
                            self.scratch[i] =
                                (input[i] + wet[i] * gate[i]) * gains[i] * self.scales[i];
                        }
                    }
                    None => {
                        for i in 0..frames {
                            self.scratch[i] = input[i] * gains[i] * self.scales[i];
                        }
                    }
                }
                if channel < self.outputs {
                    cells.output.raise(channel, peak(&self.scratch[..frames]));
                }
            }
            let first = block.audio.output(channel).ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                first[i] = self.scratch[i] * (1. - self.weights[i]);
            }
            let second = block
                .audio
                .output(channels + channel)
                .ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                second[i] = self.scratch[i] * self.weights[i];
            }
        }
        if self.processor.is_some() {
            for i in 0..frames {
                self.wet_gate[i] =
                    f32::from_bits(cells.wet_gate[i].load(core::sync::atomic::Ordering::Relaxed));
            }
        }
        if self.mute.target() > 0. && self.presence.target() > 0. {
            let input = block.midi_inputs.first().ok_or(Error::InvalidShape)?;
            if !input.is_empty() {
                cells.mark_midi(false, true);
            }
            block.forward_midi(0)?;
        }
        Ok(())
    }
}

impl StripOutput {
    pub(crate) fn apply(&mut self, operation: EngineOp) -> Result<(), Error> {
        match operation {
            EngineOp::Reset { outputs, .. } => {
                self.reset(outputs);
            }
            EngineOp::Gain(value) => self.gains.gain.set(value, self.ramp),
            EngineOp::Balance(value) => self.gains.balance.set(value, self.ramp),
            EngineOp::Mute(muted) => self.mute.set(f32::from(u8::from(!muted)), self.ramp),
            EngineOp::Presence(on) => self.presence.set(f32::from(u8::from(on)), self.ramp),
            EngineOp::Crossfade => {
                let target = 1. - self.bank.target();
                self.bank.set(target, self.ramp);
            }
            EngineOp::CrossfadeSend
            | EngineOp::CrossfadeReturn
            | EngineOp::Monitor(_)
            | EngineOp::InputSource { .. }
            | EngineOp::Configure(_)
            | EngineOp::DiscardConfiguration { .. }
            | EngineOp::Inject(_)
            | EngineOp::ReadStart { .. }
            | EngineOp::ReadRolesStart { .. }
            | EngineOp::ReadStop
            | EngineOp::PreviewStart { .. }
            | EngineOp::PreviewStop
            | EngineOp::ProcessorBindBus { .. }
            | EngineOp::ProcessorBind { .. }
            | EngineOp::ProcessorClaim { .. }
            | EngineOp::ProcessorStart { .. }
            | EngineOp::ProcessorSuspend { .. }
            | EngineOp::ProcessorRetire { .. }
            | EngineOp::ProcessorParameter { .. }
            | EngineOp::ProcessorControl { .. }
            | EngineOp::RecordedState { .. } => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
}

impl LoopFader {
    pub(crate) fn render(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        let frames = block.frames;
        let peaks: &Peak = &self.cells.loops[self.slot].peaks;
        let delayed = self
            .processor
            .is_some_and(|kind| kind != shoop_rt_protocol::ProcessorKind::External);
        let quantum = block.quantum.frames();
        let dry =
            crate::cells::mode_from_code(self.cells.loops[self.slot].mode.load(Ordering::Relaxed))
                .dry_playback();
        // One presence segment per quantum, shared by every port.
        let from = self.presence.value;
        self.presence.fill(&mut self.levels[..frames]);
        let constant = (from == self.presence.value).then_some(from);
        for port in 0..2 * self.channels {
            let auxiliary = port >= self.channels;
            let channel = if auxiliary {
                port - self.channels
            } else {
                port
            };
            let (input, output) = block.audio.pair(port, port).ok_or(Error::InvalidShape)?;
            if delayed && !auxiliary {
                let base = channel * quantum;
                let mut index = self.position;
                for i in 0..frames {
                    let sample = self.delay[base + index];
                    self.delay[base + index] = input[i];
                    output[i] = sample * self.levels[i];
                    index += 1;
                    if index == quantum {
                        index = 0;
                    }
                }
            } else {
                match constant {
                    Some(gain) if gain == 1. => output[..frames].copy_from_slice(&input[..frames]),
                    Some(gain) => {
                        for i in 0..frames {
                            output[i] = input[i] * gain;
                        }
                    }
                    None => {
                        for i in 0..frames {
                            output[i] = input[i] * self.levels[i];
                        }
                    }
                }
            }
            if auxiliary == dry {
                peaks.raise(channel, peak(&output[..frames]));
            }
        }
        self.position = (self.position + frames) % quantum;
        Ok(())
    }
}

impl LoopFader {
    pub(crate) fn apply(&mut self, operation: EngineOp) -> Result<(), Error> {
        match operation {
            EngineOp::RecordedState {
                generation,
                revision,
                state,
            } => {
                crate::recorded::validate_state(self.processor, state)
                    .map_err(|_| Error::InvalidParameter)?;
                let cell = &self.cells.loops[self.slot];
                if cell.generation.load(Ordering::Relaxed) != generation
                    || cell.content_revision.load(Ordering::Relaxed) != revision
                {
                    return Err(Error::InvalidId);
                }
                cell.recorded.publish(cell, generation, revision, state);
            }
            EngineOp::Reset { .. } => {
                self.processor = None;
                self.delay.fill(0.);
                self.position = 0;
                self.presence = Ramp::at(1.);
                self.cells.loops[self.slot].peaks.clear();
            }
            EngineOp::Presence(on) => self.presence.set(f32::from(u8::from(on)), self.ramp),
            _ => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
}

impl BusMixer {
    pub(crate) fn render_input(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        let frames = block.frames;
        let channels = self.channels;
        self.preview_samples[..frames].fill(0.);
        if let Some((mut cursor, remaining, request_id, source_channels)) = self.preview {
            let count = frames.min(remaining as usize);
            let bytes = count * source_channels * 4;
            if bytes > self.preview_bytes.len() {
                return Err(Error::InvalidShape);
            }
            let mut read = 0;
            while read < bytes {
                let count = block
                    .resources
                    .render()
                    .read(&mut cursor, &mut self.preview_bytes[read..bytes])?;
                if count == 0 {
                    return Err(Error::InvalidState);
                }
                read += count;
            }
            for i in 0..count {
                let offset = i * source_channels * 4;
                self.preview_samples[i] = f32::from_le_bytes(
                    self.preview_bytes[offset..offset + 4]
                        .try_into()
                        .map_err(|_| Error::InvalidShape)?,
                );
            }
            self.preview = if remaining == count as u64 {
                self.cells
                    .preview_finished
                    .store(request_id, core::sync::atomic::Ordering::Release);
                self.cells
                    .preview_playing
                    .store(0, core::sync::atomic::Ordering::Release);
                None
            } else {
                Some((
                    cursor,
                    remaining - count as u64,
                    request_id,
                    source_channels,
                ))
            };
        }
        for channel in 0..channels {
            for i in 0..frames {
                let value = block.audio.input(channel).ok_or(Error::InvalidShape)?[i]
                    + self.preview_samples[i];
                block.audio.output(channel).ok_or(Error::InvalidShape)?[i] = value;
            }
        }
        Ok(())
    }
    pub(crate) fn render_output(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        let frames = block.frames;
        let channels = self.channels;
        let stereo = self.active_channels == 2;
        self.gains.fill(frames, &mut self.mute);
        self.bank.fill(&mut self.weights[..frames]);
        self.presence.fill(&mut self.presence_weights[..frames]);
        for channel in 0..channels {
            {
                let input = block.audio.input(channel).ok_or(Error::InvalidShape)?;
                let gains = self.gains.channel(channel, stereo);
                for i in 0..frames {
                    self.scratch[i] = input[i] * gains[i] * self.presence_weights[i];
                }
                self.cells.buses[self.slot].raise(channel, peak(&self.scratch[..frames]));
            }
            let first = block.audio.output(channel).ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                first[i] = self.scratch[i] * (1. - self.weights[i]);
            }
            let second = block
                .audio
                .output(channels + channel)
                .ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                second[i] = self.scratch[i] * self.weights[i];
            }
        }
        Ok(())
    }
}

impl BusMixer {
    pub(crate) fn apply(&mut self, operation: EngineOp) -> Result<(), Error> {
        match operation {
            EngineOp::PreviewStart {
                cursor,
                frames,
                request_id,
                source_channels,
            } => {
                if frames == 0 || request_id == 0 || source_channels == 0 {
                    return Err(Error::InvalidParameter);
                }
                self.preview = Some((cursor, frames, request_id, source_channels as usize));
                self.cells
                    .preview_finished
                    .store(0, core::sync::atomic::Ordering::Release);
                self.cells
                    .preview_playing
                    .store(request_id, core::sync::atomic::Ordering::Release);
            }
            EngineOp::PreviewStop => {
                self.preview = None;
                self.cells
                    .preview_playing
                    .store(0, core::sync::atomic::Ordering::Release);
            }
            EngineOp::Reset {
                outputs,
                generation,
                ..
            } => {
                self.generation = generation;
                self.preview = None;
                self.active_channels = outputs;
                self.bank = Ramp::at(0.);
                self.mute = Ramp::at(1.);
                self.presence = Ramp::at(1.);
                self.gains.reset();
                self.cells.buses[self.slot].clear();
                self.cells.buses[self.slot].reset_snapshot();
            }
            EngineOp::Presence(present) => {
                self.presence.set(f32::from(u8::from(present)), self.ramp)
            }
            EngineOp::Gain(value) => self.gains.gain.set(value, self.ramp),
            EngineOp::Balance(value) => self.gains.balance.set(value, self.ramp),
            EngineOp::Mute(muted) => self.mute.set(f32::from(u8::from(!muted)), self.ramp),
            EngineOp::Crossfade => {
                let target = 1. - self.bank.target();
                self.bank.set(target, self.ramp);
            }
            _ => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
}

impl EngineNode {
    pub(crate) fn immediate_generation(&self) -> Option<u32> {
        match self {
            Self::StripInput(n) => Some(n.generation),
            Self::StripOutput(n) => Some(
                n.cells.strips[n.slot]
                    .owner_generation
                    .load(Ordering::Relaxed),
            ),
            Self::Bus(n) => Some(n.generation),
            _ => None,
        }
    }
    pub(crate) fn apply_signal(
        &mut self,
        operation: EngineOp,
        graph_epoch: rt_runtime::Epoch,
    ) -> Result<(), Error> {
        if let EngineOp::InputSource { epoch, changed } = operation {
            if graph_epoch != epoch && graph_epoch.checked_next()? != epoch {
                return Err(Error::StaleEpoch);
            }
            let Self::StripInput(node) = self else {
                return Err(Error::InvalidParameter);
            };
            node.pending_history = Some((epoch, changed));
            return Ok(());
        }
        if let EngineOp::DiscardConfiguration { token } = operation {
            let pending = self.configuration().ok_or(Error::InvalidParameter)?;
            if pending.is_some_and(|configuration| configuration.token != token) {
                return Err(Error::InvalidId);
            }
            *pending = None;
            return Ok(());
        }
        if let EngineOp::Configure(configuration) = operation {
            if graph_epoch.checked_next()? != configuration.epoch {
                return Err(Error::StaleEpoch);
            }
            if configuration.token == 0
                || configuration.bank > 1
                || configuration.send_bank > 1
                || configuration.return_bank > 1
                || !configuration.gain.is_finite()
                || configuration.gain < 0.
                || !(-1.0..=1.0).contains(&configuration.balance)
            {
                return Err(Error::InvalidParameter);
            }
            crate::recorded::validate_state(configuration.processor, configuration.recorded_state)
                .map_err(|_| Error::InvalidParameter)?;
            if configuration.recorded_state.is_some() && !matches!(self, Self::Fader(_)) {
                return Err(Error::InvalidParameter);
            }
            if matches!(self, Self::StripInput(node) if configuration.inputs as usize > node.channels)
                || matches!(self, Self::StripOutput(node) if configuration.outputs as usize > node.channels)
                || matches!(self, Self::Bus(node) if configuration.outputs as usize > node.channels)
            {
                return Err(Error::InvalidShape);
            }
            let pending = self.configuration().ok_or(Error::InvalidParameter)?;
            if pending.is_some_and(|previous| previous.token != configuration.token) {
                return Err(Error::Busy);
            }
            *pending = Some(configuration);
            return Ok(());
        }
        match self {
            Self::Loop(_) | Self::Processor(_) => Err(Error::InvalidEvent),
            Self::StripInput(node) => node.apply(operation),
            Self::StripOutput(node) => node.apply(operation),
            Self::Fader(node) => node.apply(operation),
            Self::Bus(node) | Self::BusInput(node) => node.apply(operation),
            Self::Reader(node) => node.apply(operation),
        }
    }
    fn apply_configuration(&mut self, graph_epoch: rt_runtime::Epoch) {
        if let Some(configuration) = self
            .configuration()
            .and_then(|pending| pending.take_if(|configuration| configuration.epoch == graph_epoch))
        {
            self.configure(configuration);
        }
        if let Self::StripInput(node) = self {
            if let Some((_, changed)) = node
                .pending_history
                .take_if(|(epoch, _)| *epoch == graph_epoch)
            {
                if changed {
                    node.history_token = node.history_token.checked_add(1).unwrap_or(0);
                    node.midi_history_token = node.midi_history_token.checked_add(1).unwrap_or(0);
                }
            }
        }
    }
    fn render_signal(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Loop(_) => Err(Error::InvalidState),
            Self::StripInput(node) => node.render(block),
            Self::StripOutput(node) => node.render(block),
            Self::Fader(node) => node.render(block),
            Self::Bus(node) => node.render_output(block),
            Self::BusInput(node) => node.render_input(block),
            Self::Reader(node) => node.render(block),
            Self::Processor(node) => node.process(block),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{testing::OfflineHost, EngineDescription, Segment};
    use shoop_rt_protocol::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn gain_blocks_preserve_ramps_balance_and_unwritten_suffixes() {
        let mut gains = Gains::new(8);
        let mut scale = Ramp::at(1.);
        gains.plain.fill(42.);
        gains.left.fill(42.);
        gains.right.fill(42.);
        gains.fill(4, &mut scale);
        assert_eq!(&gains.plain[..4], &[1.; 4]);
        assert_eq!(&gains.left[..4], &[1.; 4]);
        assert_eq!(&gains.right[..4], &[1.; 4]);
        gains.gain.set(0.5, 2);
        gains.balance.set(1., 2);
        scale.set(0.5, 2);
        gains.fill(4, &mut scale);
        // Sample zero uses the original value; duration two reaches its target
        // at sample two, even inside a larger block.
        assert_eq!(&gains.plain[..4], &[1., 0.5625, 0.25, 0.25]);
        assert_eq!(&gains.left[..4], &[1., 0.28125, 0., 0.]);
        assert_eq!(&gains.right[..4], &[1., 0.5625, 0.25, 0.25]);
        gains.fill(4, &mut scale);
        assert_eq!(&gains.plain[..4], &[0.25; 4]);
        assert_eq!(&gains.left[..4], &[0.; 4]);
        assert_eq!(&gains.right[..4], &[0.25; 4]);
        assert_eq!(&gains.plain[4..], &[42.; 4]);
        assert_eq!(&gains.left[4..], &[42.; 4]);
        assert_eq!(&gains.right[4..], &[42.; 4]);
    }

    #[shoop_test]
    fn peak_is_the_largest_magnitude_across_signs_and_exponents() {
        assert_eq!(peak(&[]), 0.);
        assert_eq!(peak(&[0.25, -0.5, 1e-30, -0.]), 0.5);
        assert_eq!(peak(&[-3., 2.5, 1.5]), 3.);
        assert!(peak(&[0.1, f32::NAN]).is_nan());
    }

    #[shoop_test]
    fn activating_a_prepared_strip_preserves_a_policy_change_confirmed_before_its_first_process() {
        let cells = Cells::new(
            1,
            1,
            1,
            32,
            DriverStatus {
                kind: DriverKind::Dummy,
                state: DriverRunState::Running,
                sample_rate: 1000,
                buffer_frames: 4,
            },
            4,
        );
        let settings = NodeSettings {
            midi: crate::midi_layout::Layout::new(&crate::EngineDescription::default()).unwrap(),
            external_midi: None,
            cells: cells.clone(),
            channels: 1,
            master_channels: 1,
            frames: 4,
            ramp: 0,
        };
        let mut node = EngineNode::StripInput(StripInput::new(0, &settings));
        let configuration = NodeConfiguration {
            recorded_state: None,
            processor: None,
            send_bank: 0,
            return_bank: 0,
            token: 1,
            epoch: rt_runtime::Epoch(1),
            inputs: 1,
            outputs: 1,
            bank: 0,
            gain: 1.,
            balance: 0.,
            monitoring: false,
            generation: 1,
            muted: false,
        };
        shoop_wasm_test_support::heap::assert_no_heap(|| {
            cells.strips[0].configure_policy(1, 0, true);
            node.configure(configuration);
            cells.strips[0].configure_policy(2, 1, false);
            assert!(cells.strips[0].policy_enabled(1));
            assert!(!cells.strips[0].policy_enabled(2));
            assert!(cells.strips[0].set_policy(2, true));
            node.configure(NodeConfiguration {
                generation: 2,
                ..configuration
            });
            let EngineNode::StripInput(ref input) = node else {
                unreachable!()
            };
            assert!(cells.strips[0].policy_enabled(input.generation));
            cells.strips[0].configure_policy(3, 2, true);
            cells.strips[0].configure_policy(3, 2, false);
            assert!(cells.strips[0].policy_enabled(2));
            node.configure(NodeConfiguration {
                generation: 3,
                ..configuration
            });
            let EngineNode::StripInput(ref input) = node else {
                unreachable!()
            };
            assert!(!cells.strips[0].policy_enabled(input.generation));
            assert!(!cells.strips[0].set_policy(1, false));
        });
    }

    fn tick(host: &mut OfflineHost) {
        host.input_mut(0).fill(1.);
        host.process(16).unwrap();
    }
    fn request(host: &mut OfflineHost, request: Request) {
        let id = host.request(request).unwrap();
        for _ in 0..1000 {
            while let Some(message) = host.receive() {
                if let EngineMessage::Result {
                    correlation,
                    outcome,
                } = message
                {
                    if correlation == id {
                        assert_eq!(outcome, Outcome::Done);
                        return;
                    }
                }
            }
            tick(host);
        }
        panic!("request did not complete");
    }
    #[shoop_test]
    fn configuration_for_an_unpublished_epoch_leaves_live_monitoring_untouched() {
        let mut host = OfflineHost::new(&EngineDescription {
            quantum: 1,
            sample_rate: 1000,
            max_frames: 16,
            ramp_frames: 4,
            segments: vec![Segment {
                strips: 1,
                loops: 2,
            }],
            resource_pages: 128,
            page_bytes: 256,
            midi_events: 64,
            midi_bytes: 512,
            ..EngineDescription::default()
        })
        .unwrap();
        host.connect().unwrap();
        while host.receive().is_some() {}
        let strip = StripId(SlotId::new(0, 1));
        request(
            &mut host,
            Request::CreateStrip {
                strip,
                shape: StripShape {
                    audio_inputs: 1,
                    audio_outputs: 1,
                    midi_input: false,
                    midi_output: false,
                },
            },
        );
        for (kind, host_id) in [
            (PortKind::AudioIn, HostPortId(0)),
            (PortKind::AudioOut, HostPortId(2)),
        ] {
            request(
                &mut host,
                Request::Connect {
                    port: EnginePort::Strip(StripPort {
                        strip,
                        kind,
                        index: 0,
                    }),
                    host: host_id,
                    connected: true,
                },
            );
        }
        request(
            &mut host,
            Request::StripControl {
                strip,
                control: StripControl::Monitoring(true),
            },
        );
        for _ in 0..4 {
            tick(&mut host);
            while host.receive().is_some() {}
        }
        assert_eq!(host.output(0)[..16], [1.; 16]);
        let epoch = host.endpoint().graph_epoch;
        let node = host.endpoint().pool.strip_input(0);
        let at = host.endpoint().at();
        let configuration = NodeConfiguration {
            recorded_state: None,
            processor: None,
            generation: 1,
            token: 1,
            epoch: epoch.checked_next().unwrap(),
            inputs: 1,
            outputs: 1,
            bank: 1,
            send_bank: 0,
            return_bank: 0,
            gain: 0.5,
            balance: 0.,
            monitoring: true,
            muted: false,
        };
        host.endpoint()
            .submit_op(
                at,
                node,
                EngineOp::Configure(configuration),
                crate::endpoint::Purpose::Silent,
            )
            .unwrap();
        tick(&mut host);
        while host.receive().is_some() {}
        assert_eq!(host.output(0)[..16], [1.; 16]);
        let at = host.endpoint().at();
        host.endpoint()
            .submit_op(
                at,
                node,
                EngineOp::DiscardConfiguration { token: 1 },
                crate::endpoint::Purpose::Silent,
            )
            .unwrap();
        tick(&mut host);
        while host.receive().is_some() {}
        let loop_id = LoopSlotId(SlotId::new(0, 1));
        request(&mut host, Request::CreateLoop { loop_id, strip });
        tick(&mut host);
        assert_eq!(host.output(0)[..16], [1.; 16]);
        let epoch = host.endpoint().graph_epoch.checked_next().unwrap();
        let at = host.endpoint().at();
        host.endpoint()
            .submit_op(
                at,
                node,
                EngineOp::Configure(NodeConfiguration {
                    token: 2,
                    epoch,
                    ..configuration
                }),
                crate::endpoint::Purpose::Silent,
            )
            .unwrap();
        tick(&mut host);
        while host.receive().is_some() {}
        assert_eq!(host.output(0)[..16], [1.; 16]);
        request(&mut host, Request::RemoveLoop { loop_id });
        tick(&mut host);
        assert_eq!(host.output(0)[..16], [0.5; 16]);
    }
}

impl EngineNode {
    pub(crate) fn snapshot_records(
        &self,
        emit: &mut dyn FnMut(shoop_rt_protocol::snapshot::Record),
    ) {
        use shoop_rt_protocol::{
            snapshot::Record, BusControl, BusId, SlotId, StateRecord, StripControl, StripId,
            Telemetry,
        };
        let state = |record| Record::State(record);
        match self {
            Self::StripInput(node) => {
                let id = StripId(SlotId::new(node.slot as u32, node.generation));
                let cells = &node.cells.strips[node.slot];
                for control in [
                    StripControl::InputGain(node.gains.gain.target()),
                    StripControl::InputBalance(node.gains.balance.target()),
                    StripControl::Monitoring(node.manual_monitoring),
                    StripControl::AutomaticMonitoring(node.automatic_enabled),
                ] {
                    emit(state(StateRecord::StripControl { strip: id, control }));
                }
                // The matching output node supplies output controls separately.
                emit(Record::Telemetry(Telemetry::Strip {
                    strip: id,
                    input_peaks: cells.input.snapshot(node.inputs),
                    output_peaks: cells
                        .output
                        .snapshot(cells.snapshot_output_channels.load(Ordering::Relaxed) as usize),
                    midi_in: cells.snapshot_midi_in.swap(false, Ordering::Relaxed),
                    midi_out: cells.snapshot_midi_out.swap(false, Ordering::Relaxed),
                    latest_input_midi: cells.latest_midi(),
                    automatic_monitoring: node.automatic_active,
                }));
            }
            Self::StripOutput(node) => {
                let generation = node.cells.strips[node.slot]
                    .owner_generation
                    .load(Ordering::Relaxed);
                let id = StripId(SlotId::new(node.slot as u32, generation));
                for control in [
                    StripControl::OutputGain(node.gains.gain.target()),
                    StripControl::OutputBalance(node.gains.balance.target()),
                    StripControl::OutputMute(node.mute.target() == 0.),
                ] {
                    emit(state(StateRecord::StripControl { strip: id, control }));
                }
            }
            Self::Bus(node) => {
                let id = BusId(SlotId::new(node.slot as u32, node.generation));
                for control in [
                    BusControl::Gain(node.gains.gain.target()),
                    BusControl::Balance(node.gains.balance.target()),
                    BusControl::Mute(node.mute.target() == 0.),
                ] {
                    emit(state(StateRecord::Bus { bus: id, control }));
                }
                emit(Record::Telemetry(Telemetry::Bus {
                    bus: id,
                    peaks: node.cells.buses[node.slot].snapshot(node.active_channels as usize),
                }));
            }
            _ => {}
        }
    }
}
