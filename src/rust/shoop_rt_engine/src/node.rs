//! The engine's node type: the loop library's playback and capture nodes next
//! to the engine's own strip stages, loop faders and output buses.

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
    ProcessorControl {
        generation: u32,
        control: shoop_rt_protocol::processors::Control,
    },
    ProcessorClaim {
        generation: u32,
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

#[derive(Clone, Copy)]
pub(crate) struct Ramp {
    value: f32,
    target: f32,
    step: f32,
    remaining: u32,
}
impl Ramp {
    pub(crate) fn at(value: f32) -> Self {
        Self {
            value,
            target: value,
            step: 0.,
            remaining: 0,
        }
    }
    pub(crate) fn set(&mut self, target: f32, frames: u32) {
        self.target = target;
        if frames == 0 || self.value == target {
            self.value = target;
            self.remaining = 0;
        } else {
            self.step = (target - self.value) / frames as f32;
            self.remaining = frames;
        }
    }
    pub(crate) fn target(&self) -> f32 {
        self.target
    }
    pub(crate) fn next(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.value = if self.remaining == 0 {
                self.target
            } else {
                self.value + self.step
            };
        }
        self.value
    }
}

/// Legacy stereo balance: negative values attenuate the right channel, positive
/// values the left, linearly.
fn balance_factors(balance: f32) -> [f32; 2] {
    [1. - balance.max(0.), 1. + balance.min(0.)]
}

/// Per-sample gain of each channel from gain and balance ramps, prepared off-render.
struct Gains {
    gain: Ramp,
    balance: Ramp,
    left: Box<[f32]>,
    right: Box<[f32]>,
    plain: Box<[f32]>,
}
impl Gains {
    fn new(frames: usize) -> Self {
        Self {
            gain: Ramp::at(1.),
            balance: Ramp::at(0.),
            left: vec![0.; frames].into_boxed_slice(),
            right: vec![0.; frames].into_boxed_slice(),
            plain: vec![0.; frames].into_boxed_slice(),
        }
    }
    fn reset(&mut self) {
        self.gain = Ramp::at(1.);
        self.balance = Ramp::at(0.);
    }
    /// Fill the per-sample gains, scaled by a further ramp.
    fn fill(&mut self, frames: usize, scale: &mut Ramp) {
        for i in 0..frames {
            let gain = self.gain.next() * scale.next();
            let [left, right] = balance_factors(self.balance.next());
            self.plain[i] = gain;
            self.left[i] = gain * left;
            self.right[i] = gain * right;
        }
    }
    fn channel(&self, channel: usize, stereo: bool) -> &[f32] {
        match (stereo, channel) {
            (true, 0) => &self.left,
            (true, 1) => &self.right,
            _ => &self.plain,
        }
    }
}

/// A strip's input stage: two connection banks of host inputs, input gain and
/// balance, a capture feed for its loops and a monitoring feed.
pub struct StripInput {
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
    unity: Ramp,
    gains: Gains,
    weights: Box<[f32]>,
    monitoring: Box<[f32]>,
    scratch: Box<[f32]>,
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
}

/// The Master bus: the sum of routed strips; gain, balance, mute and meters,
/// written to two connection banks.
pub struct BusMixer {
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
    Reader(BulkReader),
}

pub(crate) struct NodeSettings {
    pub cells: Arc<Cells>,
    pub channels: usize,
    pub master_channels: usize,
    pub frames: usize,
    pub ramp: u32,
}

impl StripInput {
    fn reset(&mut self, inputs: u8) {
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
        self.gains.reset();
        self.injected = [None; INJECTED];
        *self.monitored = MidiState::default();
        *self.live_midi = MidiState::default();
        self.live_midi_active = false;
        self.release = false;
        let cells = &self.cells.strips[self.slot];
        cells.routing_state.store(0, Ordering::Relaxed);
        cells.input.clear();
        cells
            .midi_in
            .store(false, core::sync::atomic::Ordering::Relaxed);
    }
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        let frames = settings.frames;
        Self {
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
            unity: Ramp::at(1.),
            gains: Gains::new(frames),
            weights: vec![0.; frames].into_boxed_slice(),
            monitoring: vec![0.; frames].into_boxed_slice(),
            scratch: vec![0.; frames].into_boxed_slice(),
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
        self.bank = Ramp::at(0.);
        self.mute = Ramp::at(1.);
        self.presence = Ramp::at(1.);
        self.gains.reset();
        let cells = &self.cells.strips[self.slot];
        cells.output.clear();
        cells
            .midi_out
            .store(false, core::sync::atomic::Ordering::Relaxed);
    }
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        let frames = settings.frames;
        Self {
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
        }
    }
}
impl BusMixer {
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        Self {
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
        self.render(block)
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
                node.processor = configuration.processor;
                node.delay.fill(0.);
                node.position = 0;
                node.presence = Ramp::at(1.);
                node.cells.loops[node.slot].peaks.clear();
            }
            Self::Bus(node) => {
                node.active_channels = configuration.outputs;
                node.presence = Ramp::at(1.);
                node.cells.buses[node.slot].clear();
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
    fn render_sources(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let frames = block.frames;
        let channels = self.channels;
        let stereo = self.inputs == 2;
        let activity = block.domain.source_activity(self.slot as u32);
        let processed = self.processor.is_some();
        let forced_off = processed && (activity.rerecording || activity.pre_rerecording);
        if forced_off && self.monitor.target() > 0. {
            self.release = true;
            self.monitor = Ramp::at(0.);
            self.cells.strips[self.slot]
                .forced_monitor_off
                .store(self.generation, core::sync::atomic::Ordering::Relaxed);
        }
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
        let mut demand = live_record || dry_play;
        for i in 0..frames {
            self.weights[i] = self.bank.next();
            self.send_weights[i] = self.send_bank.next();
            self.return_weights[i] = self.return_bank.next();
            self.monitoring[i] = self.monitor.next();
            demand |= self.monitoring[i] > 0.;
            let gate = if dry_play { 1. } else { self.monitoring[i] };
            cells.wet_gate[i].store(gate.to_bits(), core::sync::atomic::Ordering::Relaxed);
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
                let mut peak = 0f32;
                for i in 0..frames {
                    let weight = self.weights[i];
                    let value = (first[i] * (1. - weight) + second[i] * weight) * gains[i];
                    self.scratch[i] = value;
                    peak = peak.max(value.abs());
                }
                if channel < self.inputs {
                    cells.input.raise(channel, peak);
                }
            }
            block
                .audio
                .output(channel)
                .ok_or(Error::InvalidShape)?
                .copy_from_slice(&self.scratch[..frames]);
            for i in 0..frames {
                block
                    .audio
                    .output(channels + channel)
                    .ok_or(Error::InvalidShape)?[i] = self.scratch[i] * self.monitoring[i];
                let playback = block
                    .audio
                    .input(2 * channels + channel)
                    .ok_or(Error::InvalidShape)?[i];
                let live = if !forced_off && (live_record || self.monitoring[i] > 0.) {
                    self.scratch[i]
                } else {
                    0.
                };
                let send = live + playback;
                block
                    .audio
                    .output(2 * channels + channel)
                    .ok_or(Error::InvalidShape)?[i] = send * (1. - self.send_weights[i]);
                block
                    .audio
                    .output(3 * channels + channel)
                    .ok_or(Error::InvalidShape)?[i] = send * self.send_weights[i];
                let first = block
                    .audio
                    .input(3 * channels + channel)
                    .ok_or(Error::InvalidShape)?[i];
                let second = block
                    .audio
                    .input(4 * channels + channel)
                    .ok_or(Error::InvalidShape)?[i];
                block
                    .audio
                    .output(4 * channels + channel)
                    .ok_or(Error::InvalidShape)?[i] =
                    first * (1. - self.return_weights[i]) + second * self.return_weights[i];
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
        if active {
            cells
                .midi_in
                .store(true, core::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }
}

impl StripInput {
    pub(crate) fn apply(&mut self, operation: EngineOp) -> Result<(), Error> {
        match operation {
            EngineOp::Reset { inputs, .. } => {
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
                self.release |= !on && self.monitor.target() > 0.;
                self.monitor.set(f32::from(u8::from(on)), self.ramp)
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
            | EngineOp::ReadStop
            | EngineOp::PreviewStart { .. }
            | EngineOp::PreviewStop
            | EngineOp::ProcessorBind { .. }
            | EngineOp::ProcessorClaim { .. }
            | EngineOp::ProcessorStart { .. }
            | EngineOp::ProcessorSuspend { .. }
            | EngineOp::ProcessorRetire { .. }
            | EngineOp::ProcessorParameter { .. }
            | EngineOp::ProcessorControl { .. } => return Err(Error::InvalidParameter),
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
        for i in 0..frames {
            self.weights[i] = self.bank.next();
            self.scales[i] = self.mute.next();
        }
        self.gains.fill(frames, &mut self.presence);
        let cells = &self.cells.strips[self.slot];
        for channel in 0..channels {
            {
                let input = block.audio.input(channel).ok_or(Error::InvalidShape)?;
                let wet = block
                    .audio
                    .input(channels + channel)
                    .ok_or(Error::InvalidShape)?;
                let gains = self.gains.channel(channel, stereo);
                let mut peak = 0f32;
                for i in 0..frames {
                    let gate = if self.processor.is_some() {
                        if self.processor == Some(shoop_rt_protocol::ProcessorKind::External) {
                            f32::from_bits(
                                cells.wet_gate[i].load(core::sync::atomic::Ordering::Relaxed),
                            )
                        } else {
                            self.wet_gate[i]
                        }
                    } else {
                        0.
                    };
                    let value = (input[i] + wet[i] * gate) * gains[i] * self.scales[i];
                    self.scratch[i] = value;
                    peak = peak.max(value.abs());
                }
                if channel < self.outputs {
                    cells.output.raise(channel, peak);
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
            let output = block.midi_outputs.first_mut().ok_or(Error::InvalidShape)?;
            if copy_midi(input, output, frames)? {
                cells
                    .midi_out
                    .store(true, core::sync::atomic::Ordering::Relaxed);
            }
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
            | EngineOp::ReadStop
            | EngineOp::PreviewStart { .. }
            | EngineOp::PreviewStop
            | EngineOp::ProcessorBind { .. }
            | EngineOp::ProcessorClaim { .. }
            | EngineOp::ProcessorStart { .. }
            | EngineOp::ProcessorSuspend { .. }
            | EngineOp::ProcessorRetire { .. }
            | EngineOp::ProcessorParameter { .. }
            | EngineOp::ProcessorControl { .. } => return Err(Error::InvalidParameter),
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
        for port in 0..2 * self.channels {
            let channel = port % self.channels;
            let auxiliary = port >= self.channels;
            let mut presence = self.presence;
            let (input, output) = block.audio.pair(port, port).ok_or(Error::InvalidShape)?;
            let mut peak = 0f32;
            for i in 0..frames {
                let sample = if delayed && !auxiliary {
                    let index = channel * quantum + (self.position + i) % quantum;
                    let old = self.delay[index];
                    self.delay[index] = input[i];
                    old
                } else {
                    input[i]
                };
                let value = sample * presence.next();
                output[i] = value;
                peak = peak.max(value.abs());
            }
            if auxiliary == dry {
                peaks.raise(channel, peak);
            }
            if port + 1 == 2 * self.channels {
                self.presence = presence;
            }
        }
        self.position = (self.position + frames) % quantum;
        Ok(())
    }
}

impl LoopFader {
    pub(crate) fn apply(&mut self, operation: EngineOp) -> Result<(), Error> {
        match operation {
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
    pub(crate) fn render(
        &mut self,
        block: &mut ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        let frames = block.frames;
        let channels = self.channels;
        let stereo = self.active_channels == 2;
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
        self.gains.fill(frames, &mut self.mute);
        for i in 0..frames {
            self.weights[i] = self.bank.next();
            self.presence_weights[i] = self.presence.next();
        }
        for channel in 0..channels {
            {
                let input = block.audio.input(channel).ok_or(Error::InvalidShape)?;
                let gains = self.gains.channel(channel, stereo);
                let mut peak = 0f32;
                for i in 0..frames {
                    let value =
                        (input[i] + self.preview_samples[i]) * gains[i] * self.presence_weights[i];
                    self.scratch[i] = value;
                    peak = peak.max(value.abs());
                }
                self.cells.buses[self.slot].raise(channel, peak);
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
            EngineOp::Reset { outputs, .. } => {
                self.preview = None;
                self.active_channels = outputs;
                self.bank = Ramp::at(0.);
                self.mute = Ramp::at(1.);
                self.presence = Ramp::at(1.);
                self.gains.reset();
                self.cells.buses[self.slot].clear();
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
            Self::Bus(node) => node.apply(operation),
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
            Self::Bus(node) => node.render(block),
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
