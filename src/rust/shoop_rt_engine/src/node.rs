//! The engine's node type: the loop library's playback and capture nodes next
//! to the engine's own strip stages, loop faders and output buses.

use alloc::boxed::Box;
use alloc::vec;

use crate::cells::{Cells, Peak};
use crate::signal::SignalBlock;
use crate::stream::BulkReader;
use alloc::sync::Arc;
use rt_runtime::buffer::MidiBuffer;
use rt_runtime::graph::PortCounts;
use rt_runtime::midi_state::MidiState;
use rt_runtime::planner::ConsumerAccess;
use rt_runtime::processor::{OperationContext, ProcessBlock, Processor, ProcessorOperation};
use rt_runtime::resource::Cursor;
use rt_runtime::Error;
use shoop_loops::runtime::{LoopNode, LoopPlanner};
use shoop_rt_protocol::MidiMessage;

/// Most injected MIDI messages a strip holds between callbacks.
const INJECTED: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeConfiguration {
    pub token: u64,
    pub epoch: rt_runtime::Epoch,
    pub inputs: u8,
    pub outputs: u8,
    pub bank: u8,
    pub gain: f32,
    pub balance: f32,
    pub monitoring: bool,
    pub muted: bool,
}

/// Typed operations of the engine's own processors, delivered by the runtime
/// immediately before their frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineOp {
    /// Retain values for the next graph epoch without changing the active processor.
    Configure(NodeConfiguration),
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
    configuration: Option<NodeConfiguration>,
    slot: usize,
    cells: Arc<Cells>,
    channels: usize,
    ramp: u32,
    inputs: usize,
    bank: Ramp,
    monitor: Ramp,
    unity: Ramp,
    gains: Gains,
    weights: Box<[f32]>,
    monitoring: Box<[f32]>,
    scratch: Box<[f32]>,
    injected: [Option<MidiMessage>; INJECTED],
    /// Notes sounding through the monitoring feed, released when monitoring stops.
    monitored: Box<MidiState>,
    release: bool,
}

/// A strip's output stage: the sum of its loops and monitored input; output gain,
/// balance, mute and a presence fade, written to two connection banks.
pub struct StripOutput {
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
        self.inputs = (inputs as usize).min(self.channels);
        self.bank = Ramp::at(0.);
        self.monitor = Ramp::at(0.);
        self.gains.reset();
        self.injected = [None; INJECTED];
        *self.monitored = MidiState::default();
        self.release = false;
        let cells = &self.cells.strips[self.slot];
        cells.input.clear();
        cells
            .midi_in
            .store(false, core::sync::atomic::Ordering::Relaxed);
    }
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        let frames = settings.frames;
        Self {
            configuration: None,
            slot,
            cells: settings.cells.clone(),
            channels: settings.channels,
            ramp: settings.ramp,
            inputs: 0,
            bank: Ramp::at(0.),
            monitor: Ramp::at(0.),
            unity: Ramp::at(1.),
            gains: Gains::new(frames),
            weights: vec![0.; frames].into_boxed_slice(),
            monitoring: vec![0.; frames].into_boxed_slice(),
            scratch: vec![0.; frames].into_boxed_slice(),
            injected: [None; INJECTED],
            monitored: Box::default(),
            release: false,
        }
    }
}
impl StripOutput {
    fn reset(&mut self, outputs: u8) {
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
        target.push(offset, frames, packet)?;
    }
    Ok(!source.is_empty())
}

impl Processor<LoopPlanner, EngineOp> for StripInput {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2 * self.channels,
            audio_outputs: 2 * self.channels,
            midi_inputs: 1,
            midi_outputs: 2,
        }
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        self.apply(operation)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.render(&mut SignalBlock::interval(block))
    }
}

impl Processor<LoopPlanner, EngineOp> for StripOutput {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: self.channels,
            audio_outputs: 2 * self.channels,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        self.apply(operation)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.render(&mut SignalBlock::interval(block))
    }
}

impl Processor<LoopPlanner, EngineOp> for LoopFader {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: self.channels,
            audio_outputs: self.channels,
            ..PortCounts::default()
        }
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        self.apply(operation)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.render(&mut SignalBlock::interval(block))
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
        self.render(&mut SignalBlock::interval(block))
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
                node.reset(configuration.inputs);
                node.bank = Ramp::at(f32::from(configuration.bank));
                node.gains.gain = Ramp::at(configuration.gain);
                node.gains.balance = Ramp::at(configuration.balance);
                node.monitor = Ramp::at(f32::from(u8::from(configuration.monitoring)));
            }
            Self::StripOutput(node) => {
                node.reset(configuration.outputs);
                node.bank = Ramp::at(f32::from(configuration.bank));
                node.gains.gain = Ramp::at(configuration.gain);
                node.gains.balance = Ramp::at(configuration.balance);
                node.mute = Ramp::at(f32::from(u8::from(!configuration.muted)));
            }
            Self::Fader(node) => {
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
    fn uses_direct_domain(&self) -> bool {
        true
    }
    fn process_tick(
        &mut self,
        block: &mut rt_runtime::processor::TickProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        self.apply_configuration(block.graph_epoch);
        match self {
            Self::Loop(node) => node.process_tick(block),
            _ => self.render_signal(&mut SignalBlock::integrated(block)),
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
        }
    }
    fn planning(&self) -> ConsumerAccess {
        match self {
            Self::Loop(node) => node.planning(),
            _ => ConsumerAccess::default(),
        }
    }
    fn operation(
        &mut self,
        operation: EngineOp,
        context: OperationContext<'_>,
    ) -> Result<(), Error> {
        self.apply_signal(operation, context.graph_epoch)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        self.apply_configuration(block.graph_epoch);
        match self {
            Self::Loop(node) => node.process(block),
            _ => self.render_signal(&mut SignalBlock::interval(block)),
        }
    }
}

impl StripInput {
    pub(crate) fn render(&mut self, block: &mut SignalBlock<'_, '_>) -> Result<(), Error> {
        let frames = block.frames;
        let channels = self.channels;
        let stereo = self.inputs == 2;
        self.gains.fill(frames, &mut self.unity);
        for i in 0..frames {
            self.weights[i] = self.bank.next();
            self.monitoring[i] = self.monitor.next();
        }
        let cells = &self.cells.strips[self.slot];
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
            let monitor = block
                .audio
                .output(channels + channel)
                .ok_or(Error::InvalidShape)?;
            for i in 0..frames {
                monitor[i] = self.scratch[i] * self.monitoring[i];
            }
        }
        let input = block.midi_inputs.first().ok_or(Error::InvalidShape)?;
        let [capture, monitor] = &mut *block.midi_outputs else {
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
                self.reset(inputs);
            }
            EngineOp::Gain(value) => self.gains.gain.set(value, self.ramp),
            EngineOp::Balance(value) => self.gains.balance.set(value, self.ramp),
            EngineOp::Monitor(on) => {
                self.release |= !on && self.monitor.target() > 0.;
                self.monitor.set(f32::from(u8::from(on)), self.ramp)
            }
            EngineOp::Crossfade => {
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
            | EngineOp::Configure(_)
            | EngineOp::DiscardConfiguration { .. }
            | EngineOp::Presence(_)
            | EngineOp::ReadStart { .. }
            | EngineOp::ReadStop
            | EngineOp::PreviewStart { .. }
            | EngineOp::PreviewStop => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
}

impl StripOutput {
    pub(crate) fn render(&mut self, block: &mut SignalBlock<'_, '_>) -> Result<(), Error> {
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
                let gains = self.gains.channel(channel, stereo);
                let mut peak = 0f32;
                for i in 0..frames {
                    let value = input[i] * gains[i] * self.scales[i];
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
            EngineOp::Monitor(_)
            | EngineOp::Configure(_)
            | EngineOp::DiscardConfiguration { .. }
            | EngineOp::Inject(_)
            | EngineOp::ReadStart { .. }
            | EngineOp::ReadStop
            | EngineOp::PreviewStart { .. }
            | EngineOp::PreviewStop => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
}

impl LoopFader {
    pub(crate) fn render(&mut self, block: &mut SignalBlock<'_, '_>) -> Result<(), Error> {
        let frames = block.frames;
        let peaks: &Peak = &self.cells.loops[self.slot].peaks;
        for channel in 0..self.channels {
            let mut presence = self.presence;
            let (input, output) = block
                .audio
                .pair(channel, channel)
                .ok_or(Error::InvalidShape)?;
            let mut peak = 0f32;
            for i in 0..frames {
                let value = input[i] * presence.next();
                output[i] = value;
                peak = peak.max(value.abs());
            }
            peaks.raise(channel, peak);
            if channel + 1 == self.channels {
                self.presence = presence;
            }
        }
        Ok(())
    }
}

impl LoopFader {
    pub(crate) fn apply(&mut self, operation: EngineOp) -> Result<(), Error> {
        match operation {
            EngineOp::Reset { .. } => {
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
    pub(crate) fn render(&mut self, block: &mut SignalBlock<'_, '_>) -> Result<(), Error> {
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
            Self::Loop(_) => Err(Error::InvalidEvent),
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
    }
    fn render_signal(&mut self, block: &mut SignalBlock<'_, '_>) -> Result<(), Error> {
        match self {
            Self::Loop(_) => Err(Error::InvalidState),
            Self::StripInput(node) => node.render(block),
            Self::StripOutput(node) => node.render(block),
            Self::Fader(node) => node.render(block),
            Self::Bus(node) => node.render(block),
            Self::Reader(node) => node.render(block),
        }
    }
}
impl rt_runtime::processing::Processor<shoop_loops::processing::Bank, EngineOp> for EngineNode {
    fn validate_state(
        &self,
        _: rt_runtime::NodeId,
        _: &shoop_loops::processing::Bank,
    ) -> Result<(), Error> {
        if matches!(self, Self::Loop(_)) {
            Err(Error::InvalidState)
        } else {
            Ok(())
        }
    }
    fn ports(&self) -> PortCounts {
        <Self as Processor<LoopPlanner, EngineOp>>::ports(self)
    }
    fn operation(
        &mut self,
        operation: EngineOp,
        context: rt_runtime::processing::OperationContext<'_>,
        _: &mut shoop_loops::processing::Bank,
    ) -> Result<(), Error> {
        self.apply_signal(operation, context.graph_epoch)
    }
    fn process(
        &mut self,
        block: &mut rt_runtime::processing::Block<'_>,
        _: &mut shoop_loops::processing::Bank,
    ) -> Result<(), Error> {
        self.apply_configuration(block.graph_epoch);
        self.render_signal(&mut SignalBlock::direct(block))
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
            token: 1,
            epoch: epoch.checked_next().unwrap(),
            inputs: 1,
            outputs: 1,
            bank: 1,
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
