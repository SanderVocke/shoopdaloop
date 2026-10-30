//! The engine's node type: the loop library's playback and capture nodes next
//! to the engine's own strip stages, loop faders and the Master bus.

use crate::cells::{Cells, Peak};
use rt_runtime::buffer::MidiBuffer;
use rt_runtime::graph::PortCounts;
use rt_runtime::midi_state::MidiState;
use rt_runtime::planner::ConsumerAccess;
use rt_runtime::processor::{OperationContext, ProcessBlock, Processor, ProcessorOperation};
use rt_runtime::Error;
use shoop_loops::runtime::{LoopNode, LoopPlanner};
use shoop_rt_protocol::MidiMessage;
use std::sync::Arc;

/// Most injected MIDI messages a strip holds between callbacks.
const INJECTED: usize = 16;

/// Typed operations of the engine's own processors, delivered by the runtime
/// immediately before their frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineOp {
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
}
impl ProcessorOperation for EngineOp {
    type Result = ();
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
    slot: usize,
    cells: Arc<Cells>,
    channels: usize,
    ramp: u32,
    presence: Ramp,
}

/// The Master bus: the sum of routed strips; gain, balance, mute and meters,
/// written to two connection banks.
pub struct MasterBus {
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
    Master(MasterBus),
}

pub(crate) struct NodeSettings {
    pub cells: Arc<Cells>,
    pub channels: usize,
    pub master_channels: usize,
    pub frames: usize,
    pub ramp: u32,
}

impl StripInput {
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        let frames = settings.frames;
        Self {
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
    pub(crate) fn new(slot: usize, settings: &NodeSettings) -> Self {
        let frames = settings.frames;
        Self {
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
            slot,
            cells: settings.cells.clone(),
            channels: settings.channels,
            ramp: settings.ramp,
            presence: Ramp::at(1.),
        }
    }
}
impl MasterBus {
    pub(crate) fn new(settings: &NodeSettings) -> Self {
        Self {
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
        match operation {
            EngineOp::Reset { inputs, .. } => {
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
                    .store(false, std::sync::atomic::Ordering::Relaxed);
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
            EngineOp::Mute(_) | EngineOp::Presence(_) => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
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
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
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
        match operation {
            EngineOp::Reset { outputs, .. } => {
                self.outputs = (outputs as usize).min(self.channels);
                self.bank = Ramp::at(0.);
                self.mute = Ramp::at(1.);
                self.presence = Ramp::at(1.);
                self.gains.reset();
                let cells = &self.cells.strips[self.slot];
                cells.output.clear();
                cells
                    .midi_out
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            EngineOp::Gain(value) => self.gains.gain.set(value, self.ramp),
            EngineOp::Balance(value) => self.gains.balance.set(value, self.ramp),
            EngineOp::Mute(muted) => self.mute.set(f32::from(u8::from(!muted)), self.ramp),
            EngineOp::Presence(on) => self.presence.set(f32::from(u8::from(on)), self.ramp),
            EngineOp::Crossfade => {
                let target = 1. - self.bank.target();
                self.bank.set(target, self.ramp);
            }
            EngineOp::Monitor(_) | EngineOp::Inject(_) => return Err(Error::InvalidParameter),
        }
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
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
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        Ok(())
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
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
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

impl Processor<LoopPlanner, EngineOp> for MasterBus {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: self.channels,
            audio_outputs: 2 * self.channels,
            ..PortCounts::default()
        }
    }
    fn operation(&mut self, operation: EngineOp, _: OperationContext<'_>) -> Result<(), Error> {
        match operation {
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
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let frames = block.frames;
        let channels = self.channels;
        let stereo = channels == 2;
        self.gains.fill(frames, &mut self.mute);
        for i in 0..frames {
            self.weights[i] = self.bank.next();
        }
        for channel in 0..channels {
            {
                let input = block.audio.input(channel).ok_or(Error::InvalidShape)?;
                let gains = self.gains.channel(channel, stereo);
                let mut peak = 0f32;
                for i in 0..frames {
                    let value = input[i] * gains[i];
                    self.scratch[i] = value;
                    peak = peak.max(value.abs());
                }
                self.cells.master.raise(channel, peak);
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

impl Processor<LoopPlanner, EngineOp> for EngineNode {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(node) => node.ports(),
            Self::StripInput(node) => node.ports(),
            Self::StripOutput(node) => node.ports(),
            Self::Fader(node) => node.ports(),
            Self::Master(node) => node.ports(),
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
        match self {
            Self::Loop(_) => Err(Error::InvalidEvent),
            Self::StripInput(node) => node.operation(operation, context),
            Self::StripOutput(node) => node.operation(operation, context),
            Self::Fader(node) => node.operation(operation, context),
            Self::Master(node) => node.operation(operation, context),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Loop(node) => node.process(block),
            Self::StripInput(node) => node.process(block),
            Self::StripOutput(node) => node.process(block),
            Self::Fader(node) => node.process(block),
            Self::Master(node) => node.process(block),
        }
    }
}
