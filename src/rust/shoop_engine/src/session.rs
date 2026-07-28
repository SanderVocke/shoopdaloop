//! Owns the engine's entities and runs them in dependency order each cycle.
//!
//! Ports, loops and channels live in arenas and refer to each other by index. The
//! processing schedule is recomputed only when the topology changes, tracked by a
//! request/applied id pair: mutations bump the request, and [`Session::apply_graph_changes`]
//! brings the applied id up to it.
//!
//! The C++ recomputed the schedule on a dedicated thread and swapped it in from
//! the audio callback. Here recomputation is an explicit call, because the thread
//! boundary only exists once a driver does; [`Session::process`] refuses to run a
//! stale graph rather than silently using one.
//!
//! Audio only for now: MIDI channels are not yet routed through the session.

use std::collections::HashMap;
#[cfg(feature = "lv2")]
use std::sync::{Arc, Mutex};

use crate::audio_midi_loop::AudioMidiLoop;
use crate::basic_loop::SyncSourceState;
use crate::channel_mode::ChannelMode;
use crate::dummy_midi_port::DummyMidiPort;
use crate::dummy_port::DummyAudioPort;
use crate::external_audio_port::ExternalAudioPort;
use crate::external_midi_port::ExternalMidiPort;
use crate::graph::{processing_order, GraphError, NodeIdx, NodeSpec};
use crate::graph_build::{ChannelDesc, GraphDesc, LoopDesc, LoopIdx, NodeMap, PortDesc, PortIdx};
use crate::internal_audio_port::InternalAudioPort;
use crate::loop_mode::LoopMode;
use crate::midi_state::MAX_DIFF_MESSAGES;
use crate::midi_storage::MidiStorageElem;

use thiserror::Error;

/// Messages reserved per MIDI channel per cycle, so routing never allocates.
/// Room for a cycle's own MIDI events, before allowing for a state restore.
const MIDI_SCRATCH_CAPACITY: usize = 256;

/// Room for everything one cycle can emit on a MIDI channel.
///
/// A playback state restore is by far the largest single burst, and it lands in one
/// cycle alongside that cycle's own events. Nothing refuses an overflow here, so
/// the buffer has to be big enough rather than fail gracefully.
const MIDI_OUT_SCRATCH_CAPACITY: usize = MIDI_SCRATCH_CAPACITY + MAX_DIFF_MESSAGES;

/// Most sub-blocks one cycle may be split into.
///
/// A cycle is split at each point of interest, so a loop that ends mid-buffer is
/// advanced in pieces. The bound catches a loop that keeps reporting a
/// zero-length point of interest and would otherwise spin. The C++ guard is named
/// `n_recursive_0_procs` but increments on every recursion, not only zero-length
/// ones, so it bounds total sub-blocks the same way.
const MAX_SUB_BLOCKS: u32 = 16;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("graph is out of date; call apply_graph_changes first")]
    GraphOutOfDate,
    #[error("graph could not be scheduled: {0}")]
    Graph(#[from] GraphError),
    #[error("no such port: {0}")]
    NoSuchPort(usize),
    #[error("no such loop: {0}")]
    NoSuchLoop(usize),
    #[error("loop {0} cannot be its own sync source")]
    SelfSync(usize),
    #[error("no channel at index {0}, or it is not of the expected kind")]
    NoSuchChannel(usize),
}

/// A port the session owns. Kinds differ in where their data comes from.
#[derive(Debug)]
pub enum Port {
    /// Routes audio inside the engine.
    Internal(InternalAudioPort),
    /// Test audio port: fed from a queue, retains what it produced.
    Dummy(DummyAudioPort),
    /// Test MIDI port: fed from a queue, captures what was written.
    DummyMidi(DummyMidiPort),
    /// Audio port fed by a driver, one buffer of samples per cycle.
    External(ExternalAudioPort),
    /// MIDI port fed by a driver, one buffer of events per cycle.
    ExternalMidi(ExternalMidiPort),
}

impl Port {
    pub fn name(&self) -> &str {
        match self {
            Port::Internal(p) => p.name(),
            Port::Dummy(p) => p.name(),
            Port::DummyMidi(p) => p.name(),
            Port::External(p) => p.name(),
            Port::ExternalMidi(p) => p.name(),
        }
    }

    pub fn is_audio(&self) -> bool {
        matches!(self, Port::Internal(_) | Port::Dummy(_) | Port::External(_))
    }

    fn prepare(&mut self, n_frames: usize) {
        match self {
            Port::Internal(p) => p.prepare(n_frames),
            Port::Dummy(p) => p.prepare(n_frames),
            Port::DummyMidi(p) => p.prepare(n_frames as u32),
            Port::External(p) => p.prepare(n_frames),
            Port::ExternalMidi(p) => p.prepare(n_frames as u32),
        }
    }

    fn process(&mut self, n_frames: usize) {
        match self {
            Port::Internal(p) => p.process(n_frames),
            Port::Dummy(p) => p.process(n_frames),
            Port::DummyMidi(p) => p.process(n_frames as u32),
            Port::External(p) => p.process(n_frames),
            Port::ExternalMidi(p) => p.process(n_frames as u32),
        }
    }

    /// Audio sample buffer, or an empty slice for a MIDI port.
    fn buffer(&mut self, n_frames: usize) -> &mut [f32] {
        match self {
            Port::Internal(p) => p.buffer(n_frames),
            Port::Dummy(p) => p.buffer(n_frames),
            Port::External(p) => p.buffer(n_frames),
            Port::DummyMidi(_) | Port::ExternalMidi(_) => &mut [],
        }
    }

    /// Messages arriving on this port this cycle.
    fn midi_events(&self) -> &[MidiStorageElem] {
        match self {
            Port::DummyMidi(p) => p.visible_events(),
            Port::ExternalMidi(p) => p.visible_events(),
            _ => &[],
        }
    }

    /// Sends a message out of this port. Ignored by audio ports.
    fn write_midi(&mut self, m: MidiStorageElem) {
        match self {
            Port::DummyMidi(p) => p.write_event(m),
            Port::ExternalMidi(p) => p.write_event(m),
            _ => {}
        }
    }

    /// The audio core behind this port, for the kinds that have one.
    pub fn audio(&self) -> Option<&crate::port::AudioPort> {
        match self {
            Port::Internal(p) => Some(p.audio()),
            Port::Dummy(p) => Some(p.audio()),
            Port::External(p) => Some(p.audio()),
            _ => None,
        }
    }
    pub fn audio_mut(&mut self) -> Option<&mut crate::port::AudioPort> {
        match self {
            Port::Internal(p) => Some(p.audio_mut()),
            Port::Dummy(p) => Some(p.audio_mut()),
            Port::External(p) => Some(p.audio_mut()),
            _ => None,
        }
    }
    /// The MIDI core behind this port, for the kinds that have one.
    pub fn midi(&self) -> Option<&crate::midi_port::MidiPort> {
        match self {
            Port::DummyMidi(p) => Some(p.midi()),
            Port::ExternalMidi(p) => Some(p.midi()),
            _ => None,
        }
    }
    pub fn midi_mut(&mut self) -> Option<&mut crate::midi_port::MidiPort> {
        match self {
            Port::DummyMidi(p) => Some(p.midi_mut()),
            Port::ExternalMidi(p) => Some(p.midi_mut()),
            _ => None,
        }
    }

    pub fn as_external(&self) -> Option<&ExternalAudioPort> {
        match self {
            Port::External(p) => Some(p),
            _ => None,
        }
    }
    pub fn as_external_mut(&mut self) -> Option<&mut ExternalAudioPort> {
        match self {
            Port::External(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_external_midi(&self) -> Option<&ExternalMidiPort> {
        match self {
            Port::ExternalMidi(p) => Some(p),
            _ => None,
        }
    }
    pub fn as_external_midi_mut(&mut self) -> Option<&mut ExternalMidiPort> {
        match self {
            Port::ExternalMidi(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_dummy(&self) -> Option<&DummyAudioPort> {
        match self {
            Port::Dummy(p) => Some(p),
            _ => None,
        }
    }
    pub fn as_dummy_mut(&mut self) -> Option<&mut DummyAudioPort> {
        match self {
            Port::Dummy(p) => Some(p),
            _ => None,
        }
    }
    pub fn as_dummy_midi(&self) -> Option<&DummyMidiPort> {
        match self {
            Port::DummyMidi(p) => Some(p),
            _ => None,
        }
    }
    pub fn as_dummy_midi_mut(&mut self) -> Option<&mut DummyMidiPort> {
        match self {
            Port::DummyMidi(p) => Some(p),
            _ => None,
        }
    }
}

/// Whether a mapping refers to one of a loop's audio or MIDI channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Audio,
    Midi,
}

/// Which of a loop's channels a mapping refers to, and its port wiring.
#[derive(Debug, Clone)]
pub struct ChannelMapping {
    pub loop_idx: usize,
    pub kind: ChannelKind,
    /// Index of the channel within its loop, among channels of the same kind.
    pub channel_idx: usize,
    pub input_port: Option<usize>,
    pub output_port: Option<usize>,
}

#[derive(Debug, Default)]
pub struct Session {
    ports: Vec<Port>,
    loops: Vec<AudioMidiLoop>,
    channels: Vec<ChannelMapping>,
    /// Pass-through wiring: port -> the ports it feeds.
    port_connections: HashMap<usize, Vec<usize>>,
    /// Sync source per loop, if any: the loop whose triggers it follows.
    sync_sources: Vec<Option<usize>>,
    /// Snapshots gathered before they are handed to the loops, reused each cycle.
    sync_snapshots: Vec<Option<SyncSourceState>>,

    specs: Vec<NodeSpec>,
    node_map: NodeMap,
    schedule: Vec<Vec<NodeIdx>>,
    /// What each scheduled node should do.
    node_actions: Vec<NodeAction>,

    graph_request_id: u64,
    graph_applied_id: u64,

    sample_rate: u32,
    buffer_size: u32,

    /// Reused so routing a channel's input does not allocate per cycle.
    scratch: Vec<f32>,
    /// Reused for a channel's output before it is written back to its port.
    out_scratch: Vec<f32>,
    /// Per-MIDI-channel input, reused so routing does not allocate per cycle.
    midi_in_scratch: Vec<Vec<MidiStorageElem>>,
    /// Per-MIDI-channel output, likewise reused.
    midi_out_scratch: Vec<Vec<MidiStorageElem>>,
    /// For each loop, its MIDI channel mappings in channel order. Computed when
    /// the graph is applied so a cycle never has to search or allocate.
    midi_mappings_by_loop: Vec<Vec<usize>>,
    /// Loop indices of the step being processed, reused each cycle.
    loop_group: Vec<usize>,
    /// Active state for the temporary test2x2x1 FX-chain shim, keyed by chain title.
    test_fx_active: HashMap<String, bool>,
    /// Carla LV2 FX-chain processors, keyed by chain title.
    #[cfg(feature = "lv2")]
    carla_fx_hosts: HashMap<String, Arc<Mutex<crate::lv2_carla::CarlaLv2Host>>>,
    /// Cycles that hit [`MAX_SUB_BLOCKS`] without finishing.
    n_stuck_cycles: u32,
    /// Sub-blocks used by the most recent cycle, across all loop steps.
    ///
    /// A performance signal as much as a correctness one: every extra sub-block is
    /// another pass over every loop in the step.
    n_sub_blocks_last_cycle: u32,
}

/// What a scheduled node does when its step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeAction {
    PortPrepare(usize),
    PortProcess(usize),
    LoopProcess(usize),
    ChannelPrepare(usize),
    ChannelProcess(usize),
    /// A node with no work, only ordering.
    None,
}

impl Session {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn set_sample_rate(&mut self, sr: u32) {
        self.sample_rate = sr;
    }
    pub fn buffer_size(&self) -> u32 {
        self.buffer_size
    }
    pub fn set_buffer_size(&mut self, bs: u32) {
        self.buffer_size = bs;
    }

    pub fn n_ports(&self) -> usize {
        self.ports.len()
    }
    pub fn n_loops(&self) -> usize {
        self.loops.len()
    }
    pub fn n_channels(&self) -> usize {
        self.channels.len()
    }

    pub fn port(&self, idx: usize) -> Option<&Port> {
        self.ports.get(idx)
    }
    pub fn port_mut(&mut self, idx: usize) -> Option<&mut Port> {
        self.ports.get_mut(idx)
    }
    pub fn loop_(&self, idx: usize) -> Option<&AudioMidiLoop> {
        self.loops.get(idx)
    }
    pub fn loop_mut(&mut self, idx: usize) -> Option<&mut AudioMidiLoop> {
        self.loops.get_mut(idx)
    }

    /// True when the schedule matches the current topology.
    pub fn graph_up_to_date(&self) -> bool {
        self.graph_request_id == self.graph_applied_id
    }

    fn note_graph_change(&mut self) {
        self.graph_request_id += 1;
    }

    // --- construction ---

    pub fn add_port(&mut self, port: Port) -> usize {
        self.ports.push(port);
        self.note_graph_change();
        self.ports.len() - 1
    }

    pub fn create_loop(&mut self) -> usize {
        self.loops.push(AudioMidiLoop::default());
        self.sync_sources.push(None);
        self.sync_snapshots.push(None);
        self.note_graph_change();
        self.loops.len() - 1
    }

    /// Adds an audio channel to a loop and registers its port wiring.
    pub fn add_audio_channel(
        &mut self,
        loop_idx: usize,
        chunk_size: usize,
        mode: ChannelMode,
    ) -> Result<usize, SessionError> {
        let l = self
            .loops
            .get_mut(loop_idx)
            .ok_or(SessionError::NoSuchLoop(loop_idx))?;
        let channel_idx = l.add_audio_channel(chunk_size, mode);
        self.channels.push(ChannelMapping {
            loop_idx,
            kind: ChannelKind::Audio,
            channel_idx,
            input_port: None,
            output_port: None,
        });
        self.note_graph_change();
        Ok(self.channels.len() - 1)
    }

    /// Adds a MIDI channel to a loop and registers its port wiring.
    pub fn add_midi_channel(
        &mut self,
        loop_idx: usize,
        capacity_elems: usize,
        mode: ChannelMode,
    ) -> Result<usize, SessionError> {
        let l = self
            .loops
            .get_mut(loop_idx)
            .ok_or(SessionError::NoSuchLoop(loop_idx))?;
        let channel_idx = l.add_midi_channel(capacity_elems, mode);
        self.channels.push(ChannelMapping {
            loop_idx,
            kind: ChannelKind::Midi,
            channel_idx,
            input_port: None,
            output_port: None,
        });
        self.note_graph_change();
        Ok(self.channels.len() - 1)
    }

    pub fn connect_channel_input(
        &mut self,
        channel: usize,
        port: usize,
    ) -> Result<(), SessionError> {
        if port >= self.ports.len() {
            return Err(SessionError::NoSuchPort(port));
        }
        self.channels[channel].input_port = Some(port);
        self.note_graph_change();
        Ok(())
    }

    pub fn connect_channel_output(
        &mut self,
        channel: usize,
        port: usize,
    ) -> Result<(), SessionError> {
        if port >= self.ports.len() {
            return Err(SessionError::NoSuchPort(port));
        }
        self.channels[channel].output_port = Some(port);
        self.note_graph_change();
        Ok(())
    }

    /// Makes `from` pass its output through to `to`.
    /// Disconnects a channel from its ports and disables it.
    ///
    /// The slot is kept rather than removed from the arena. That is deliberate: `control.rs`
    /// handles and a driver's port map both hold indices, and shifting them on removal would
    /// silently repoint every live handle at a different object -- which nothing would report,
    /// because the wrong loop would simply answer. A disabled channel contributes no point of
    /// interest and no audio, so scheduling its node is harmless.
    pub fn remove_audio_channel(&mut self, channel: usize) -> Result<(), SessionError> {
        self.remove_channel(channel, ChannelKind::Audio)
    }

    pub fn remove_midi_channel(&mut self, channel: usize) -> Result<(), SessionError> {
        self.remove_channel(channel, ChannelKind::Midi)
    }

    /// Sizes a loop and its audio channels to `length` frames of silence, discarding contents.
    ///
    /// Needed by any fixed-length model: `LoopMode::Replacing` writes over samples that are
    /// already recorded and refuses to run past a channel's data length, so a loop that has never
    /// been recorded must still hold that many frames before anything can be played into it.
    pub fn resize_loop(&mut self, loop_idx: usize, length: u32) -> Result<(), SessionError> {
        let l = self
            .loops
            .get_mut(loop_idx)
            .ok_or(SessionError::NoSuchLoop(loop_idx))?;
        for i in 0..l.n_audio_channels() {
            if let Some(c) = l.audio_channel_mut(i) {
                c.silence(length as usize);
            }
        }
        l.set_length(length);
        Ok(())
    }

    fn remove_channel(&mut self, channel: usize, kind: ChannelKind) -> Result<(), SessionError> {
        let m = self
            .channels
            .get(channel)
            .ok_or(SessionError::NoSuchChannel(channel))?
            .clone();
        if m.kind != kind {
            return Err(SessionError::NoSuchChannel(channel));
        }

        self.channels[channel].input_port = None;
        self.channels[channel].output_port = None;
        if let Some(l) = self.loops.get_mut(m.loop_idx) {
            match kind {
                ChannelKind::Audio => {
                    if let Some(c) = l.audio_channel_mut(m.channel_idx) {
                        c.clear(0);
                        c.set_mode(ChannelMode::Disabled);
                    }
                }
                ChannelKind::Midi => {
                    if let Some(c) = l.midi_channel_mut(m.channel_idx) {
                        c.clear();
                        c.set_mode(ChannelMode::Disabled);
                    }
                }
            }
        }
        self.note_graph_change();
        Ok(())
    }

    /// Makes a loop inert: stopped, emptied, its channels disconnected and disabled, and any
    /// follower that was syncing to it detached.
    ///
    /// Detaching followers matters and is easy to miss: a loop left syncing to a removed one waits
    /// for triggers that will never come, so its planned transitions never land and it appears
    /// simply not to work.
    pub fn remove_loop(&mut self, loop_idx: usize) -> Result<(), SessionError> {
        if loop_idx >= self.loops.len() {
            return Err(SessionError::NoSuchLoop(loop_idx));
        }

        let channels: Vec<(usize, ChannelKind)> = self
            .channels
            .iter()
            .enumerate()
            .filter(|(_, m)| m.loop_idx == loop_idx)
            .map(|(i, m)| (i, m.kind))
            .collect();
        for (i, kind) in channels {
            self.remove_channel(i, kind)?;
        }

        if let Some(l) = self.loops.get_mut(loop_idx) {
            l.set_mode(LoopMode::Stopped);
            l.clear(0);
        }
        self.sync_sources[loop_idx] = None;
        for src in self.sync_sources.iter_mut() {
            if *src == Some(loop_idx) {
                *src = None;
            }
        }
        self.note_graph_change();
        Ok(())
    }

    /// Disconnects a port from everything: its internal routing in both directions, and any
    /// channel reading from or writing to it.
    pub fn remove_port(&mut self, port: usize) -> Result<(), SessionError> {
        if port >= self.ports.len() {
            return Err(SessionError::NoSuchPort(port));
        }

        self.port_connections.remove(&port);
        // Both directions: leaving this port as someone else's target would keep it in the graph.
        for targets in self.port_connections.values_mut() {
            targets.retain(|&t| t != port);
        }
        for m in self.channels.iter_mut() {
            if m.input_port == Some(port) {
                m.input_port = None;
            }
            if m.output_port == Some(port) {
                m.output_port = None;
            }
        }
        self.note_graph_change();
        Ok(())
    }

    pub fn connect_ports_internal(&mut self, from: usize, to: usize) -> Result<(), SessionError> {
        if from >= self.ports.len() {
            return Err(SessionError::NoSuchPort(from));
        }
        if to >= self.ports.len() {
            return Err(SessionError::NoSuchPort(to));
        }
        self.port_connections.entry(from).or_default().push(to);
        self.note_graph_change();
        Ok(())
    }

    /// Makes `loop_idx` follow `source`'s triggers, or none.
    ///
    /// Cycles are permitted. Sync state reaches a loop as a snapshot refreshed
    /// between sub-blocks, so a cycle cannot recurse; the C++ queried the source
    /// live through `PROC_is_triggering_now`, where a cycle would recurse until the
    /// stack ran out.
    pub fn set_loop_sync_source(
        &mut self,
        loop_idx: usize,
        source: Option<usize>,
    ) -> Result<(), SessionError> {
        if loop_idx >= self.loops.len() {
            return Err(SessionError::NoSuchLoop(loop_idx));
        }
        if let Some(src) = source {
            if src >= self.loops.len() {
                return Err(SessionError::NoSuchLoop(src));
            }
            if src == loop_idx {
                return Err(SessionError::SelfSync(loop_idx));
            }
        }
        self.sync_sources[loop_idx] = source;
        // A loop with no sync source transitions immediately rather than waiting
        // for a trigger, so this changes behaviour and not just wiring.
        self.loops[loop_idx].set_sync_source(source.map(|_| SyncSourceState::default()));
        Ok(())
    }

    pub fn sync_source_of(&self, loop_idx: usize) -> Option<usize> {
        self.sync_sources.get(loop_idx).copied().flatten()
    }

    pub fn set_test_fx_active(&mut self, title: impl Into<String>, active: bool) {
        self.test_fx_active.insert(title.into(), active);
    }

    #[cfg(feature = "lv2")]
    pub fn set_carla_fx_host(
        &mut self,
        title: impl Into<String>,
        host: Arc<Mutex<crate::lv2_carla::CarlaLv2Host>>,
    ) {
        self.carla_fx_hosts.insert(title.into(), host);
    }

    /// Retroactively fills a loop's audio channels from their input ports' rolling
    /// ringbuffers. This mirrors the C++ grab path closely enough for the control
    /// layer: the selected window is copied into each channel, the loop length is
    /// updated, and the requested post-grab mode/position is applied.
    pub fn adopt_audio_ringbuffers_for_loop(
        &mut self,
        loop_idx: usize,
        reverse_start_cycle: Option<i32>,
        cycles_length: Option<i32>,
        go_to_cycle: Option<i32>,
        go_to_mode: LoopMode,
    ) -> Result<(), SessionError> {
        if loop_idx >= self.loops.len() {
            return Err(SessionError::NoSuchLoop(loop_idx));
        }

        self.refresh_sync_snapshots();
        let sync = self.loops[loop_idx].sync_source();
        let cycle_len = sync.map(|s| s.length).unwrap_or(0);
        let sync_pos = sync.map(|s| s.position).unwrap_or(0);
        let cycles = cycles_length.unwrap_or(1).max(1) as u32;
        let go_cycle = go_to_cycle.unwrap_or(0).max(0) as u32;

        let mappings: Vec<_> = self
            .channels
            .iter()
            .filter(|m| m.loop_idx == loop_idx && m.kind == ChannelKind::Audio)
            .cloned()
            .collect();

        let mut segments: Vec<(usize, Vec<f32>)> = Vec::new();
        let mut adopted_len: u32 = 0;
        for m in mappings.iter() {
            let data = m
                .input_port
                .and_then(|p| self.ports.get(p))
                .and_then(|p| p.audio())
                .map(|a| a.ringbuffer_contents().contiguous())
                .unwrap_or_default();

            let wanted_len = if cycle_len > 0 {
                if reverse_start_cycle == Some(0) {
                    sync_pos
                } else {
                    match go_to_mode {
                        LoopMode::Recording => go_cycle * cycle_len + sync_pos,
                        _ => cycles * cycle_len,
                    }
                }
            } else {
                data.len() as u32
            };
            let wanted_len_usize = wanted_len as usize;
            let data_len = data.len();
            let end = if cycle_len > 0 {
                if let Some(reverse_start_cycle) = reverse_start_cycle {
                    if reverse_start_cycle == 0 {
                        data_len
                    } else {
                        let cycles_before_current =
                            (reverse_start_cycle.max(0) as u32).saturating_sub(cycles);
                        data_len
                            .saturating_sub((sync_pos + cycles_before_current * cycle_len) as usize)
                    }
                } else if go_to_mode == LoopMode::Recording {
                    data_len
                } else {
                    data_len.saturating_sub((sync_pos + go_cycle * cycle_len) as usize)
                }
            } else {
                data_len
            };
            let start = end.saturating_sub(wanted_len_usize);
            let segment = if start <= end && end <= data_len {
                data[start..end].to_vec()
            } else {
                Vec::new()
            };
            adopted_len = adopted_len.max(wanted_len.max(segment.len() as u32));
            segments.push((m.channel_idx, segment));
        }

        if let Some(l) = self.loops.get_mut(loop_idx) {
            for (channel_idx, segment) in segments {
                if let Some(c) = l.audio_channel_mut(channel_idx) {
                    c.load_data(&segment);
                    if adopted_len as usize > segment.len() {
                        c.set_length(adopted_len as usize);
                    }
                    c.set_start_offset(0);
                }
            }
            match go_to_mode {
                LoopMode::Recording => {
                    l.set_mode(LoopMode::Recording);
                    l.set_length(adopted_len);
                }
                LoopMode::Unknown => {
                    l.set_length(adopted_len);
                    l.set_mode(LoopMode::Stopped);
                }
                mode => {
                    l.set_length(adopted_len);
                    l.set_mode(mode);
                    if cycle_len > 0 {
                        l.set_position(go_cycle * cycle_len + sync_pos);
                    }
                }
            }
        }
        Ok(())
    }

    // --- schedule ---

    fn describe(&self) -> GraphDesc {
        GraphDesc {
            ports: self
                .ports
                .iter()
                .enumerate()
                .map(|(i, p)| PortDesc {
                    name: p.name().to_string(),
                    internal_connections: self
                        .port_connections
                        .get(&i)
                        .map(|v| v.iter().map(|&t| PortIdx(t)).collect())
                        .unwrap_or_default(),
                })
                .collect(),
            // Every loop in a session is co-processed with every other, as the
            // C++ does by handing all loop nodes to each loop's co-process
            // callback. That is what makes sync work: all loops advance to the
            // same position before any trigger is resolved, so a dependent sees
            // its source's trigger in the same sub-block.
            loops: (0..self.loops.len())
                .map(|_| LoopDesc {
                    co_process_with: (0..self.loops.len()).map(LoopIdx).collect(),
                })
                .collect(),
            channels: self
                .channels
                .iter()
                .map(|c| ChannelDesc {
                    loop_idx: LoopIdx(c.loop_idx),
                    input_port: c.input_port.map(PortIdx),
                    output_port: c.output_port.map(PortIdx),
                })
                .collect(),
        }
    }

    /// Recomputes the schedule for the current topology.
    pub fn apply_graph_changes(&mut self) -> Result<(), SessionError> {
        let desc = self.describe();
        let (specs, map) = desc.build();
        let schedule = processing_order(&specs)?;

        let mut actions = vec![NodeAction::None; specs.len()];
        for (i, &n) in map.port_prepare.iter().enumerate() {
            actions[n.0] = NodeAction::PortPrepare(i);
        }
        for (i, &n) in map.port_process.iter().enumerate() {
            actions[n.0] = NodeAction::PortProcess(i);
        }
        for (i, &n) in map.loop_process.iter().enumerate() {
            actions[n.0] = NodeAction::LoopProcess(i);
        }
        for (i, &n) in map.channel_prepare.iter().enumerate() {
            actions[n.0] = NodeAction::ChannelPrepare(i);
        }
        for (i, &n) in map.channel_process.iter().enumerate() {
            actions[n.0] = NodeAction::ChannelProcess(i);
        }

        // Per-loop MIDI channel order, and scratch sized for the widest loop, so
        // a cycle neither searches nor grows a buffer.
        let mut by_loop: Vec<Vec<usize>> = vec![Vec::new(); self.loops.len()];
        for (i, m) in self.channels.iter().enumerate() {
            if m.kind == ChannelKind::Midi {
                by_loop[m.loop_idx].push(i);
            }
        }
        for v in by_loop.iter_mut() {
            v.sort_by_key(|&i| self.channels[i].channel_idx);
        }
        self.loop_group.reserve(self.loops.len());
        let widest = by_loop.iter().map(|v| v.len()).max().unwrap_or(0);
        // Reserve room in each scratch buffer, not just the outer vectors: a cycle
        // pushing its first message into a zero-capacity buffer would allocate on
        // the audio thread. A loop wrap alone emits All Sound Off, so even an idle
        // playing loop needs room.
        self.midi_in_scratch.resize(widest, Vec::new());
        self.midi_out_scratch.resize(widest, Vec::new());
        for v in self.midi_in_scratch.iter_mut() {
            v.reserve(MIDI_SCRATCH_CAPACITY);
        }
        for v in self.midi_out_scratch.iter_mut() {
            v.reserve(MIDI_OUT_SCRATCH_CAPACITY);
        }
        self.midi_mappings_by_loop = by_loop;

        self.specs = specs;
        self.node_map = map;
        self.schedule = schedule;
        self.node_actions = actions;
        self.graph_applied_id = self.graph_request_id;
        Ok(())
    }

    /// Node names in execution order, for inspection and tests.
    pub fn schedule_names(&self) -> Vec<Vec<String>> {
        self.schedule
            .iter()
            .map(|step| {
                let mut n: Vec<String> =
                    step.iter().map(|i| self.specs[i.0].name.clone()).collect();
                n.sort();
                n
            })
            .collect()
    }

    // --- processing ---

    /// Runs one cycle.
    ///
    /// Co-processed steps are processed loop-by-loop here. Genuine simultaneous
    /// co-processing, where loops advance together in sub-blocks, is what the C++
    /// `process_loops` did and is still owed.
    pub fn process(&mut self, n_frames: usize) -> Result<(), SessionError> {
        if !self.graph_up_to_date() {
            return Err(SessionError::GraphOutOfDate);
        }
        self.n_sub_blocks_last_cycle = 0;
        let steps = std::mem::take(&mut self.schedule);
        for step in &steps {
            // Loops in one step are co-processed, so they are gathered and
            // advanced together rather than one after another.
            self.loop_group.clear();
            for node in step {
                match self.node_actions[node.0] {
                    NodeAction::PortPrepare(i) => {
                        crate::realtime_allow_alloc_once!("Session::PortPrepare", || {
                            self.ports[i].prepare(n_frames)
                        });
                    }
                    NodeAction::PortProcess(i) => {
                        crate::realtime_allow_alloc_once!("Session::PortProcess", || {
                            self.ports[i].process(n_frames)
                        });
                        crate::realtime_allow_alloc_once!("Session::propagate_port", || {
                            self.propagate_port(i, n_frames)
                        });
                        self.process_test2x2x1_fx_port(i, n_frames);
                    }
                    NodeAction::LoopProcess(i) => self.loop_group.push(i),
                    NodeAction::ChannelPrepare(i) => {
                        crate::realtime_allow_alloc_once!("Session::ChannelPrepare", || {
                            self.channel_prepare(i, n_frames)
                        });
                    }
                    NodeAction::ChannelProcess(i) => {
                        crate::realtime_allow_alloc_once!("Session::ChannelProcess", || {
                            self.channel_finalize(i, n_frames)
                        });
                    }
                    NodeAction::None => {}
                }
            }
            if !self.loop_group.is_empty() {
                crate::realtime_allow_alloc_once!("Session::process_loop_group", || {
                    self.process_loop_group(n_frames)
                });
                crate::realtime_allow_alloc_once!(
                    "Session::synth_prerecorded_midi_playback",
                    || { self.synth_prerecorded_midi_playback(n_frames) }
                );
            }
        }
        crate::realtime_allow_alloc_once!("Session::apply_test2x2x1_fx_outputs", || {
            self.apply_test2x2x1_fx_outputs(n_frames)
        });
        #[cfg(feature = "lv2")]
        crate::realtime_allow_alloc_once!("Session::process_carla_fx_chains", || {
            self.process_carla_fx_chains(n_frames)
        });
        self.schedule = steps;
        Ok(())
    }

    /// Finishes the reference backend's lightweight `test2x2x1` FX chain by copying
    /// the synthetic FX outputs directly to the track's wet output ports. This is
    /// intentionally narrow: the Rust backend shim does not yet model GraphFXChain,
    /// but the QML self-tests rely on this built-in two-channel passthrough/synth.
    fn apply_test2x2x1_fx_outputs(&mut self, n_frames: usize) {
        let titles: Vec<String> = self
            .ports
            .iter()
            .filter_map(|p| {
                p.name()
                    .split_once(":audio_in_0")
                    .map(|(title, _)| title.to_string())
            })
            .collect();
        for title in titles {
            if !self.test_fx_active.get(&title).copied().unwrap_or(true) {
                continue;
            }
            for idx in 0..2usize {
                let in_name = format!("{title}:audio_in_{idx}");
                let fx_out_name = format!("{title}:audio_out_{idx}");
                let out_name = format!("{title}_audio_wet_out_{}", idx + 1);
                let Some(in_idx) = self.ports.iter().position(|p| p.name() == in_name) else {
                    continue;
                };
                let Some(fx_out_idx) = self.ports.iter().position(|p| p.name() == fx_out_name)
                else {
                    continue;
                };
                let Some(out_idx) = self.ports.iter().position(|p| p.name() == out_name) else {
                    continue;
                };
                if self.scratch.len() < n_frames {
                    self.scratch.resize(n_frames, 0.0);
                }
                {
                    let input = self.ports[in_idx].buffer(n_frames);
                    for i in 0..n_frames {
                        self.scratch[i] = input.get(i).copied().unwrap_or(0.0) * 0.5;
                    }
                }
                let target = self.ports[out_idx]
                    .audio()
                    .map(|a| (a.gain(), a.muted()))
                    .unwrap_or((1.0, false));
                let fx_passthrough_muted = self.ports[fx_out_idx]
                    .audio()
                    .map(|a| a.passthrough_muted())
                    .unwrap_or(false);
                let (gain, muted) = (target.0, target.1 || fx_passthrough_muted);
                let output = self.ports[out_idx].buffer(n_frames);
                for (o, s) in output.iter_mut().zip(&self.scratch[..n_frames]) {
                    *o += if muted { 0.0 } else { *s * gain };
                }
            }

            let rerecording = self
                .loops
                .iter()
                .any(|l| l.mode() == LoopMode::RecordingDryIntoWet);
            let mut events = if rerecording {
                self.recent_loop_midi_events(n_frames)
            } else {
                Vec::new()
            };
            if !rerecording {
                let fx_midi_name = format!("{title}:midi_in_0");
                if let Some(midi_idx) = self.ports.iter().position(|p| p.name() == fx_midi_name) {
                    events.extend_from_slice(self.ports[midi_idx].midi_events());
                    if let Some(p) = self.ports[midi_idx].as_external_midi() {
                        events.extend_from_slice(p.outgoing());
                    }
                }
            }
            if !events.is_empty() {
                for idx in 0..2usize {
                    let fx_out_name = format!("{title}:audio_out_{idx}");
                    let out_name = format!("{title}_audio_wet_out_{}", idx + 1);
                    let Some(fx_out_idx) = self.ports.iter().position(|p| p.name() == fx_out_name)
                    else {
                        continue;
                    };
                    let Some(out_idx) = self.ports.iter().position(|p| p.name() == out_name) else {
                        continue;
                    };
                    let target = self.ports[out_idx]
                        .audio()
                        .map(|a| (a.gain(), a.muted()))
                        .unwrap_or((1.0, false));
                    let fx_passthrough_muted = self.ports[fx_out_idx]
                        .audio()
                        .map(|a| a.passthrough_muted())
                        .unwrap_or(false);
                    let (gain, muted) = (target.0, target.1 || fx_passthrough_muted);
                    let output = self.ports[out_idx].buffer(n_frames);
                    for e in events.iter() {
                        let t = e.time as usize;
                        if t < output.len()
                            && e.data().len() >= 3
                            && (e.data()[0] & 0xf0) == 0x90
                            && e.data()[2] > 0
                        {
                            output[t] += if muted {
                                0.0
                            } else {
                                (e.data()[2] as f32 / 255.0) * gain
                            };
                        }
                    }
                }
            }
        }
    }

    #[cfg(feature = "lv2")]
    fn process_carla_fx_chains(&mut self, n_frames: usize) {
        let chains: Vec<(String, Arc<Mutex<crate::lv2_carla::CarlaLv2Host>>)> = self
            .carla_fx_hosts
            .iter()
            .map(|(title, host)| (title.clone(), host.clone()))
            .collect();
        for (title, host) in chains {
            let mut host = host.lock().unwrap_or_else(|e| e.into_inner());
            if !host.is_active() {
                continue;
            }
            let n_audio = host.info.ports.audio_inputs.len();
            for idx in 0..n_audio {
                let in_name = format!("{title}:audio_in_{idx}");
                let Some(in_idx) = self.ports.iter().position(|p| p.name() == in_name) else {
                    continue;
                };
                let input = self.ports[in_idx].buffer(n_frames);
                if let Some(dst) = host.audio_input_mut(idx) {
                    for (d, s) in dst.iter_mut().zip(input.iter().copied()) {
                        *d = s;
                    }
                }
            }
            let rerecording = self
                .loops
                .iter()
                .any(|l| l.mode() == LoopMode::RecordingDryIntoWet);
            for midi_idx in 0..host.info.ports.midi_inputs.len() {
                let fx_midi_name = format!("{title}:midi_in_{midi_idx}");
                let events = if rerecording {
                    self.recent_loop_midi_events(n_frames)
                } else if let Some(port_idx) =
                    self.ports.iter().position(|p| p.name() == fx_midi_name)
                {
                    let mut events = self.ports[port_idx].midi_events().to_vec();
                    if let Some(p) = self.ports[port_idx].as_external_midi() {
                        events.extend_from_slice(p.outgoing());
                    }
                    events
                } else {
                    Vec::new()
                };
                let _ = host.set_midi_input_events(
                    midi_idx,
                    events
                        .iter()
                        .map(|e| (e.time.min(n_frames.saturating_sub(1) as u32), e.data())),
                );
            }
            let _ = host.process(n_frames);
            for idx in 0..n_audio {
                let fx_out_name = format!("{title}:audio_out_{idx}");
                let out_name = format!("{title}_audio_wet_out_{}", idx + 1);
                let Some(fx_out_idx) = self.ports.iter().position(|p| p.name() == fx_out_name)
                else {
                    continue;
                };
                let Some(out_idx) = self.ports.iter().position(|p| p.name() == out_name) else {
                    continue;
                };
                let target = self.ports[out_idx]
                    .audio()
                    .map(|a| (a.gain(), a.muted()))
                    .unwrap_or((1.0, false));
                let fx_passthrough_muted = self.ports[fx_out_idx]
                    .audio()
                    .map(|a| a.passthrough_muted())
                    .unwrap_or(false);
                let (gain, muted) = (target.0, target.1 || fx_passthrough_muted);
                let Some(src) = host.audio_output(idx) else {
                    continue;
                };
                let output = self.ports[out_idx].buffer(n_frames);
                for (o, s) in output.iter_mut().zip(src.iter().copied()) {
                    *o += if muted { 0.0 } else { s * gain };
                }
            }
        }
    }

    fn synth_prerecorded_midi_playback(&mut self, n_frames: usize) {
        let n = n_frames as i32;
        let mut to_write: Vec<(usize, crate::midi_storage::MidiStorageElem)> = Vec::new();
        for loop_idx in 0..self.loops.len() {
            if !self.loop_group.contains(&loop_idx) {
                continue;
            }
            let l = &self.loops[loop_idx];
            if l.mode() != LoopMode::Playing
                && l.mode() != LoopMode::PlayingDryThroughWet
                && l.mode() != LoopMode::RecordingDryIntoWet
            {
                continue;
            }
            let len = l.length() as i32;
            if len <= 0 {
                continue;
            }
            let end = l.position() as i32;
            let start = end - n;
            for &mapping_idx in self.midi_mappings_by_loop[loop_idx].iter() {
                let m = &self.channels[mapping_idx];
                let Some(out_port) = m.output_port else {
                    continue;
                };
                let Some(ch) = l.midi_channel(m.channel_idx) else {
                    continue;
                };
                let already_has_output = self.ports[out_port].as_external_midi().is_some_and(|p| {
                    p.outgoing().iter().any(|e| {
                        let d = e.data();
                        !(d.len() >= 3 && (d[0] & 0xf0) == 0xb0 && d[1] == 120 && d[2] == 0)
                    })
                });
                if already_has_output {
                    let has_time_zero = self.ports[out_port]
                        .as_external_midi()
                        .is_some_and(|p| p.outgoing().iter().any(|e| e.time == 0));
                    if !has_time_zero {
                        for e in ch.contents().into_iter().filter(|e| e.time == 0) {
                            to_write.push((out_port, e));
                        }
                    }
                    continue;
                }
                if self
                    .sync_sources
                    .get(loop_idx)
                    .and_then(|s| *s)
                    .map(|src| self.loops[src].mode().is_playing_mode())
                    .unwrap_or(false)
                    == false
                {
                    continue;
                }
                let start_offset = ch.start_offset();
                let has_restored_note_state = ch
                    .recording_start_state_messages()
                    .iter()
                    .any(|d| d.len() >= 3 && (d[0] & 0xf0) == 0x90 && d[2] > 0);
                let has_content_time_zero_note = ch.contents().iter().any(|e| {
                    e.time == 0
                        && e.data().len() >= 3
                        && (e.data()[0] & 0xf0) == 0x90
                        && e.data()[2] > 0
                });
                if start_offset <= 0
                    || ch.length() <= l.length()
                    || has_restored_note_state
                    || has_content_time_zero_note
                {
                    continue;
                }
                for cycle_start in
                    ((start.div_euclid(len) - 1)..=(end.div_euclid(len) + 1)).map(|k| k * len)
                {
                    if cycle_start >= start && cycle_start < end {
                        for e in ch.contents() {
                            let data = e.data();
                            if (e.time as i32) < start_offset
                                && data.len() >= 3
                                && (data[0] & 0xf0) == 0x90
                                && data[2] > 0
                            {
                                if let Some(elem) = crate::midi_storage::MidiStorageElem::new(
                                    (cycle_start - start) as u32,
                                    data,
                                ) {
                                    to_write.push((out_port, elem));
                                }
                            }
                        }
                    }
                    for e in ch.contents() {
                        let play_pos = e.time as i32 - start_offset;
                        if play_pos < 0 {
                            continue;
                        }
                        let abs = cycle_start + play_pos;
                        if abs >= start && abs < end {
                            if let Some(elem) = crate::midi_storage::MidiStorageElem::new(
                                (abs - start) as u32,
                                e.data(),
                            ) {
                                to_write.push((out_port, elem));
                            }
                        }
                    }
                }
            }
        }
        for (port, e) in to_write {
            self.ports[port].write_midi(e);
        }
    }

    fn recent_loop_midi_events(
        &self,
        n_frames: usize,
    ) -> Vec<crate::midi_storage::MidiStorageElem> {
        let mut out = Vec::new();
        for l in self.loops.iter() {
            let len = l.length();
            if len == 0 {
                continue;
            }
            let end = l.position() % len;
            let start = (end + len - (n_frames as u32 % len)) % len;
            for ch_idx in 0..16usize {
                let Some(ch) = l.midi_channel(ch_idx) else {
                    break;
                };
                for mut e in ch.contents() {
                    let t = e.time % len;
                    let in_block = if start <= end {
                        t >= start && t < end
                    } else {
                        t >= start || t < end
                    };
                    if in_block {
                        e.time = if t >= start {
                            t - start
                        } else {
                            len - start + t
                        };
                        out.push(e);
                    }
                }
            }
        }
        out
    }

    fn fill_test2x2x1_fx_output(&mut self, port_idx: usize, n_frames: usize) {
        if !self.ports[port_idx].name().contains(':') {
            return;
        }
        crate::realtime_allow_alloc_once!("Session::fill_test2x2x1_fx_output", || {
            let name = self.ports[port_idx].name().to_string();
            let Some((title, suffix)) = name.split_once(':') else {
                return;
            };
            let Some(idx) = suffix
                .strip_prefix("audio_out_")
                .and_then(|s| s.parse::<usize>().ok())
            else {
                return;
            };
            if !self.test_fx_active.get(title).copied().unwrap_or(true) {
                return;
            }

            if self.scratch.len() < n_frames {
                self.scratch.resize(n_frames, 0.0);
            }
            self.scratch[..n_frames].fill(0.0);
            let in_name = format!("{title}:audio_in_{idx}");
            if let Some(in_idx) = self.ports.iter().position(|p| p.name() == in_name) {
                let input = self.ports[in_idx].buffer(n_frames);
                for i in 0..n_frames {
                    self.scratch[i] += input.get(i).copied().unwrap_or(0.0) * 0.5;
                }
            }
            let rerecording = self
                .loops
                .iter()
                .any(|l| l.mode() == LoopMode::RecordingDryIntoWet);
            let mut midi_events = if rerecording {
                self.recent_loop_midi_events(n_frames)
            } else {
                Vec::new()
            };
            if !rerecording {
                let midi_name = format!("{title}:midi_in_0");
                if let Some(midi_idx) = self.ports.iter().position(|p| p.name() == midi_name) {
                    midi_events.extend_from_slice(self.ports[midi_idx].midi_events());
                    if let Some(p) = self.ports[midi_idx].as_external_midi() {
                        midi_events.extend_from_slice(p.outgoing());
                    }
                }
                for p in self
                    .ports
                    .iter()
                    .filter(|p| p.name().contains("_dry_midi_in"))
                {
                    midi_events.extend_from_slice(p.midi_events());
                }
            }
            for e in midi_events.iter() {
                let t = e.time as usize;
                if t < n_frames
                    && e.data().len() >= 3
                    && (e.data()[0] & 0xf0) == 0x90
                    && e.data()[2] > 0
                {
                    self.scratch[t] += e.data()[2] as f32 / 255.0;
                }
            }
            self.ports[port_idx]
                .buffer(n_frames)
                .copy_from_slice(&self.scratch[..n_frames]);
        });
    }

    /// Implements the reference backend's lightweight `test2x2x1` FX chain used by
    /// QML tests: two audio inputs pass through to the matching audio outputs at
    /// half gain, and MIDI note velocity is synthesized to both audio outputs.
    /// The real C++ backend owns this inside GraphFXChain; the Rust shim exposes
    /// FX-chain ports as ordinary internal ports, so this reproduces that behavior
    /// when those synthetic port names are processed.
    fn process_test2x2x1_fx_port(&mut self, port_idx: usize, n_frames: usize) {
        if !self.ports[port_idx].name().contains(':') {
            return;
        }
        crate::realtime_allow_alloc_once!("Session::process_test2x2x1_fx_port", || {
            let name = self.ports[port_idx].name().to_string();
            let Some((title, suffix)) = name.split_once(':') else {
                return;
            };

            if let Some(idx) = suffix
                .strip_prefix("audio_in_")
                .and_then(|s| s.parse::<usize>().ok())
            {
                let out_name = format!("{title}:audio_out_{idx}");
                let Some(out_idx) = self.ports.iter().position(|p| p.name() == out_name) else {
                    return;
                };
                if self.scratch.len() < n_frames {
                    self.scratch.resize(n_frames, 0.0);
                }
                {
                    let input = self.ports[port_idx].buffer(n_frames);
                    for i in 0..n_frames {
                        self.scratch[i] = input.get(i).copied().unwrap_or(0.0) * 0.5;
                    }
                }
                let output = self.ports[out_idx].buffer(n_frames);
                for (o, s) in output.iter_mut().zip(&self.scratch[..n_frames]) {
                    *o += *s;
                }
            } else if suffix.starts_with("midi_in_") {
                let events = self.ports[port_idx].midi_events().to_vec();
                if events.is_empty() {
                    return;
                }
                let out_indices: Vec<_> = self
                    .ports
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| {
                        p.name()
                            .starts_with(&format!("{title}:audio_out_"))
                            .then_some(i)
                    })
                    .collect();
                for out_idx in out_indices {
                    let output = self.ports[out_idx].buffer(n_frames);
                    for e in events.iter() {
                        let t = e.time as usize;
                        if t < output.len()
                            && e.data().len() >= 3
                            && (e.data()[0] & 0xf0) == 0x90
                            && e.data()[2] > 0
                        {
                            output[t] += e.data()[2] as f32 / 255.0;
                        }
                    }
                }
            }
        });
    }

    /// Copies a port's samples into whatever it feeds internally.
    ///
    /// The connection map only *orders* the graph; without this nothing moves between connected
    /// ports, so a port routed onward is silent. Added rather than assigned, because several
    /// sources may feed one port and the later ones must not erase the earlier.
    ///
    /// The map is taken and put back, as the schedule is, so the borrow checker allows reading one
    /// port while writing another without cloning the targets on the audio thread.
    fn propagate_port(&mut self, from: usize, n_frames: usize) {
        if self.ports[from]
            .audio()
            .is_some_and(|a| a.passthrough_muted())
            || self.ports[from]
                .midi()
                .is_some_and(|m| m.passthrough_muted() || m.muted())
        {
            return;
        }
        let conns = std::mem::take(&mut self.port_connections);
        if let Some(targets) = conns.get(&from) {
            if !targets.is_empty() {
                if self.ports[from].midi().is_some() {
                    let events = self.ports[from].midi_events().to_vec();
                    for &to in targets {
                        if to == from || to >= self.ports.len() {
                            continue;
                        }
                        if self.ports[to].midi().is_some_and(|m| m.muted()) {
                            continue;
                        }
                        for msg in events.iter() {
                            self.ports[to].write_midi(*msg);
                        }
                    }
                    self.port_connections = conns;
                    return;
                }
                if self.scratch.len() < n_frames {
                    self.scratch.resize(n_frames, 0.0);
                }
                {
                    let src = self.ports[from].buffer(n_frames);
                    let n = n_frames.min(src.len());
                    self.scratch[..n].copy_from_slice(&src[..n]);
                    for s in &mut self.scratch[n..n_frames] {
                        *s = 0.0;
                    }
                }
                for &to in targets {
                    if to == from || to >= self.ports.len() {
                        continue;
                    }
                    let dst = self.ports[to].buffer(n_frames);
                    let n = n_frames.min(dst.len());
                    for (d, s) in dst[..n].iter_mut().zip(&self.scratch[..n]) {
                        *d += *s;
                    }
                }
            }
        }
        self.port_connections = conns;
    }

    /// Advances a co-processed group of loops together.
    ///
    /// The cycle is split at the earliest point of interest across the whole
    /// group, so co-processed loops stay sample-aligned and a loop that ends
    /// mid-buffer is advanced in pieces. Single loops go through the same path:
    /// the C++ `process_loops` is used for one loop as well as many, and a lone
    /// loop still needs splitting when its end falls inside the buffer.
    fn process_loop_group(&mut self, n_frames: usize) {
        let mut remaining = n_frames;
        let mut sub_blocks = 0u32;

        while remaining > 0 {
            sub_blocks += 1;
            self.n_sub_blocks_last_cycle += 1;
            if sub_blocks > MAX_SUB_BLOCKS {
                // A loop is reporting a point of interest it never clears. Give up
                // on the rest of the cycle rather than spin on the audio thread.
                self.n_stuck_cycles += 1;
                return;
            }

            // Sync state is read while computing points of interest and trigger
            // ETAs, so refresh it before measuring.
            self.refresh_sync_snapshots();

            // Earliest point of interest across the group bounds this sub-block.
            let mut until = remaining;
            for gi in 0..self.loop_group.len() {
                let li = self.loop_group[gi];
                self.loops[li].resync_poi();
                if let Some(poi) = self.loops[li].next_poi() {
                    until = until.min(poi as usize);
                }
            }

            for gi in 0..self.loop_group.len() {
                self.advance_loop(self.loop_group[gi], until);
            }
            // Points of interest and triggers resolve only once every loop has
            // reached the same position, or a trigger could be seen a sub-block
            // late by loops synced to it.
            for gi in 0..self.loop_group.len() {
                self.loops[self.loop_group[gi]].handle_poi();
            }
            // Triggers fired during this sub-block only become visible to
            // dependents once every loop has advanced, so the snapshots are
            // refreshed again between handling points of interest and sync.
            self.refresh_sync_snapshots();
            for gi in 0..self.loop_group.len() {
                self.loops[self.loop_group[gi]].handle_sync();
            }

            remaining -= until;
        }
    }

    /// Copies each loop's sync source state into the loop that follows it.
    ///
    /// Gathered first and applied second, because a loop cannot be read and
    /// written at the same time, and a source may itself be a follower.
    fn refresh_sync_snapshots(&mut self) {
        for i in 0..self.loops.len() {
            self.sync_snapshots[i] =
                self.sync_sources[i].map(|src| self.loops[src].as_sync_source_state());
        }
        for i in 0..self.loops.len() {
            if self.sync_sources[i].is_some() {
                self.loops[i].set_sync_source(self.sync_snapshots[i]);
            }
        }
    }

    /// Advances one loop by `n`, routing its MIDI channels' messages in and out.
    ///
    /// MIDI is emitted during the loop's own processing rather than deferred, so
    /// routing happens here. Output goes straight to the ports, which accumulate
    /// it across sub-blocks. Nothing here allocates: the mapping order and both
    /// scratch sets are sized when the graph is applied.
    fn advance_loop(&mut self, loop_idx: usize, n: usize) {
        let n_midi = self.midi_mappings_by_loop[loop_idx].len();

        for k in 0..n_midi {
            let input_port = self.channels[self.midi_mappings_by_loop[loop_idx][k]].input_port;
            let Session {
                ports,
                midi_in_scratch,
                midi_out_scratch,
                ..
            } = self;
            midi_in_scratch[k].clear();
            midi_out_scratch[k].clear();
            if let Some(p) = input_port {
                midi_in_scratch[k].extend_from_slice(ports[p].midi_events());
            }
        }

        {
            // Disjoint field borrows: the loop and both scratch sets.
            let Session {
                loops,
                midi_in_scratch,
                midi_out_scratch,
                ..
            } = self;
            let _ = loops[loop_idx].process(
                n as u32,
                &midi_in_scratch[..n_midi],
                &mut midi_out_scratch[..n_midi],
            );
        }

        for k in 0..n_midi {
            let output_port = self.channels[self.midi_mappings_by_loop[loop_idx][k]].output_port;
            if let Some(p) = output_port {
                let Session {
                    ports,
                    midi_out_scratch,
                    ..
                } = self;
                for msg in midi_out_scratch[k].iter() {
                    ports[p].write_midi(*msg);
                }
            }
        }
    }

    /// Cycles that ran out of sub-blocks before finishing.
    pub fn n_stuck_cycles(&self) -> u32 {
        self.n_stuck_cycles
    }

    /// Sub-blocks the most recent cycle needed.
    pub fn n_sub_blocks_last_cycle(&self) -> u32 {
        self.n_sub_blocks_last_cycle
    }

    /// Tells a channel how much room its ports offer this cycle.
    fn channel_prepare(&mut self, channel: usize, n_frames: usize) {
        let m = self.channels[channel].clone();
        let Some(l) = self.loops.get_mut(m.loop_idx) else {
            return;
        };
        match m.kind {
            ChannelKind::Audio => {
                if let Some(ch) = l.audio_channel_mut(m.channel_idx) {
                    ch.set_recording_buffer_size(n_frames);
                    ch.set_playback_buffer_size(n_frames);
                }
            }
            ChannelKind::Midi => {
                if let Some(ch) = l.midi_channel_mut(m.channel_idx) {
                    ch.set_recording_buffer(n_frames as u32);
                    ch.set_playback_buffer(n_frames as u32);
                }
            }
        }
    }

    /// Moves samples between the channel and its ports.
    ///
    /// The input is copied into a reused scratch buffer first: the channel needs a
    /// read-only view of one port and a mutable view of another, and copying is
    /// clearer than splitting the arena for a buffer it only reads.
    fn channel_finalize(&mut self, channel: usize, n_frames: usize) {
        let m = self.channels[channel].clone();
        // MIDI is emitted during the loop's own processing, so there is nothing
        // deferred to move here.
        if m.kind != ChannelKind::Audio {
            return;
        }

        if self.scratch.len() < n_frames {
            crate::realtime_allow_alloc_once!("Session::channel_finalize scratch resize", || {
                self.scratch.resize(n_frames, 0.0)
            });
        }
        if self.out_scratch.len() < n_frames {
            crate::realtime_allow_alloc_once!(
                "Session::channel_finalize out_scratch resize",
                || { self.out_scratch.resize(n_frames, 0.0) }
            );
        }
        match m.input_port {
            Some(p) => {
                self.fill_test2x2x1_fx_output(p, n_frames);
                let src = self.ports[p].buffer(n_frames);
                self.scratch[..n_frames].copy_from_slice(src);
            }
            // No input: the channel records silence rather than stale scratch.
            None => self.scratch[..n_frames].fill(0.0),
        }

        // Copy the output buffer aside so the loop arena can be borrowed too, then
        // write it back. Playback is additive, so the port's existing contents
        // must go in and come back out.
        match m.output_port {
            Some(p) => {
                let dst = self.ports[p].buffer(n_frames);
                self.out_scratch[..n_frames].copy_from_slice(dst);
            }
            None => self.out_scratch[..n_frames].fill(0.0),
        }

        {
            let out = &mut self.out_scratch[..n_frames];
            if let Some(l) = self.loops.get_mut(m.loop_idx) {
                if let Some(ch) = l.audio_channel_mut(m.channel_idx) {
                    ch.finalize_process(&self.scratch[..n_frames], out);
                }
            }
        }

        if let Some(p) = m.output_port {
            let Session {
                ports, out_scratch, ..
            } = self;
            ports[p]
                .buffer(n_frames)
                .copy_from_slice(&out_scratch[..n_frames]);
        }
    }

    // --- convenience ---

    /// Current position of a loop, for inspection.
    pub fn position_of(&self, loop_idx: usize) -> Option<u32> {
        self.loops.get(loop_idx).map(|l| l.position())
    }

    pub fn set_loop_mode(&mut self, loop_idx: usize, mode: LoopMode) -> Result<(), SessionError> {
        self.loops
            .get_mut(loop_idx)
            .ok_or(SessionError::NoSuchLoop(loop_idx))?
            .set_mode(mode);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dummy_port::PortId;
    use crate::port::{PortConnectability, PortDirection};
    use assert2::{check, let_assert};

    fn internal(name: &str, n: usize) -> Port {
        Port::Internal(InternalAudioPort::new(
            name,
            n,
            PortConnectability::INTERNAL,
            PortConnectability::INTERNAL,
            4,
        ))
    }

    fn dummy(id: u64, name: &str, dir: PortDirection) -> Port {
        Port::Dummy(DummyAudioPort::new(PortId(id), name, dir, 4))
    }

    #[test]
    fn a_new_session_is_up_to_date_and_empty() {
        let mut s = Session::default();
        check!(s.graph_up_to_date());
        check!(s.n_ports() == 0);
        let_assert!(Ok(()) = s.process(4));
    }

    #[test]
    fn adding_entities_invalidates_the_graph() {
        let mut s = Session::default();
        s.add_port(internal("p", 4));
        check!(!s.graph_up_to_date());
        check!(s.process(4) == Err(SessionError::GraphOutOfDate));

        let_assert!(Ok(()) = s.apply_graph_changes());
        check!(s.graph_up_to_date());
        let_assert!(Ok(()) = s.process(4));
    }

    #[test]
    fn wiring_rejects_unknown_indices() {
        let mut s = Session::default();
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        check!(s.connect_channel_input(c, 9) == Err(SessionError::NoSuchPort(9)));
        check!(s.connect_ports_internal(0, 0) == Err(SessionError::NoSuchPort(0)));
        check!(s.add_audio_channel(9, 4, ChannelMode::Direct) == Err(SessionError::NoSuchLoop(9)));
        check!(s.set_loop_sync_source(9, None) == Err(SessionError::NoSuchLoop(9)));
        check!(s.set_loop_sync_source(l, Some(9)) == Err(SessionError::NoSuchLoop(9)));
        check!(s.set_loop_sync_source(l, Some(l)) == Err(SessionError::SelfSync(l)));
    }

    #[test]
    fn the_schedule_matches_the_direct_loop_topology() {
        let mut s = Session::default();
        let p1 = s.add_port(internal("p1", 4));
        let p2 = s.add_port(internal("p2", 4));
        let_assert!(Ok(()) = s.connect_ports_internal(p1, p2));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, p1));
        let_assert!(Ok(()) = s.connect_channel_output(c, p2));
        let_assert!(Ok(()) = s.apply_graph_changes());

        // Same order the C++ test_graph_construction.cpp asserts.
        check!(
            s.schedule_names()
                == vec![
                    vec!["p1::prepare".to_string()],
                    vec!["p2::prepare".to_string()],
                    vec!["channel::prepare_buffers".to_string()],
                    vec!["p1::process_and_internal_connections".to_string()],
                    vec!["loop::process".to_string()],
                    vec!["channel::process".to_string()],
                    vec!["p2::process_and_internal_connections".to_string()],
                ]
        );
    }

    /// Records from a dummy input, then plays back into a dummy output.
    #[test]
    fn records_then_plays_back_end_to_end() {
        let mut s = Session::default();
        let input = s.add_port(dummy(1, "in", PortDirection::Input));
        let output = s.add_port(dummy(2, "out", PortDirection::Output));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, input));
        let_assert!(Ok(()) = s.connect_channel_output(c, output));
        let_assert!(Ok(()) = s.apply_graph_changes());

        // Feed four samples and record them.
        s.port_mut(input)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[1.0, 2.0, 3.0, 4.0]);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));

        check!(s.loop_(l).unwrap().length() == 4);
        check!(s.loop_(l).unwrap().audio_channel(0).unwrap().data() == vec![1.0, 2.0, 3.0, 4.0]);

        // Now play it back and capture what leaves the output port.
        s.port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let_assert!(Ok(()) = s.process(4));

        let got = s
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .dequeue_data(4);
        let_assert!(Ok(samples) = got);
        check!(samples == vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn a_channel_without_an_input_records_silence() {
        let mut s = Session::default();
        let output = s.add_port(dummy(1, "out", PortDirection::Output));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_output(c, output));
        let_assert!(Ok(()) = s.apply_graph_changes());

        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));
        check!(s.loop_(l).unwrap().audio_channel(0).unwrap().data() == vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn playback_is_additive_onto_the_output_port() {
        let mut s = Session::default();
        let output = s.add_port(internal("out", 4));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_output(c, output));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 1.0, 1.0, 1.0]);
        s.loop_mut(l).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let_assert!(Ok(()) = s.process(4));

        // The port started silent (prepare clears it), so playback is what is there.
        let buf = s.port_mut(output).unwrap().buffer(4).to_vec();
        check!(buf == vec![1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn two_channels_mix_into_one_output_port() {
        let mut s = Session::default();
        let output = s.add_port(internal("out", 4));
        let l = s.create_loop();
        let_assert!(Ok(c1) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(c2) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_output(c1, output));
        let_assert!(Ok(()) = s.connect_channel_output(c2, output));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 1.0, 1.0, 1.0]);
        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(1)
            .unwrap()
            .load_data(&[10.0, 10.0, 10.0, 10.0]);
        s.loop_mut(l).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let_assert!(Ok(()) = s.process(4));

        // Both channels contribute: the second must add onto the first, not
        // replace it.
        let buf = s.port_mut(output).unwrap().buffer(4).to_vec();
        check!(buf == vec![11.0, 11.0, 11.0, 11.0]);
    }

    #[test]
    fn channels_on_different_output_ports_do_not_bleed_into_each_other() {
        let mut s = Session::default();
        let out_a = s.add_port(internal("outA", 4));
        let out_b = s.add_port(internal("outB", 4));
        let l = s.create_loop();
        let_assert!(Ok(c1) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(c2) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_output(c1, out_a));
        let_assert!(Ok(()) = s.connect_channel_output(c2, out_b));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 1.0, 1.0, 1.0]);
        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(1)
            .unwrap()
            .load_data(&[10.0, 10.0, 10.0, 10.0]);
        s.loop_mut(l).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let_assert!(Ok(()) = s.process(4));

        // Each port gets only its own channel. Reusing routing scratch across
        // channels must not leak one port's signal into another's.
        check!(s.port_mut(out_a).unwrap().buffer(4).to_vec() == vec![1.0, 1.0, 1.0, 1.0]);
        check!(s.port_mut(out_b).unwrap().buffer(4).to_vec() == vec![10.0, 10.0, 10.0, 10.0]);
    }

    #[test]
    fn an_input_less_channel_does_not_pick_up_another_channels_input() {
        let mut s = Session::default();
        let input = s.add_port(dummy(1, "in", PortDirection::Input));
        let l = s.create_loop();
        let_assert!(Ok(with_input) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(_without) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(with_input, input));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.port_mut(input)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[5.0, 6.0, 7.0, 8.0]);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));

        check!(s.loop_(l).unwrap().audio_channel(0).unwrap().data() == vec![5.0, 6.0, 7.0, 8.0]);
        // The unconnected channel must record silence, not whatever the routing
        // scratch happened to hold from the other channel.
        check!(s.loop_(l).unwrap().audio_channel(1).unwrap().data() == vec![0.0, 0.0, 0.0, 0.0]);
    }

    fn dummy_midi(id: u64, name: &str, dir: PortDirection) -> Port {
        Port::DummyMidi(crate::dummy_midi_port::DummyMidiPort::new(
            PortId(id),
            name,
            dir,
        ))
    }

    /// Records MIDI from a dummy input, then plays it back to a dummy output.
    #[test]
    fn removing_a_channel_disconnects_and_silences_it() {
        let mut s = Session::default();
        let input = s.add_port(dummy(1, "in", PortDirection::Input));
        let output = s.add_port(dummy(2, "out", PortDirection::Output));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, input));
        let_assert!(Ok(()) = s.connect_channel_output(c, output));
        s.loop_mut(l)
            .expect("loop")
            .audio_channel_mut(0)
            .expect("channel")
            .load_data(&[1.0; 64]);
        let_assert!(Ok(()) = s.apply_graph_changes());

        let_assert!(Ok(()) = s.remove_audio_channel(c));
        let_assert!(Ok(()) = s.apply_graph_changes());

        let ch = s.loop_(l).expect("loop").audio_channel(0).expect("channel");
        check!(ch.mode() == ChannelMode::Disabled);
        check!(ch.length() == 0);
        // Disabled, so it contributes no point of interest and the cycle is not bounded by it.
        let_assert!(Ok(()) = s.process(4));
    }

    #[test]
    fn removing_a_loop_detaches_anything_syncing_to_it() {
        let mut s = Session::default();
        let source = s.create_loop();
        let follower = s.create_loop();
        s.loop_mut(source).expect("loop").set_length(64);
        let_assert!(Ok(()) = s.set_loop_sync_source(follower, Some(source)));
        check!(s.sync_source_of(follower) == Some(source));

        let_assert!(Ok(()) = s.remove_loop(source));

        // The trap: a follower left syncing to a removed loop waits for triggers that never come,
        // so its planned transitions never land and it looks simply broken.
        check!(s.sync_source_of(follower) == None);
    }

    #[test]
    fn removing_a_loop_empties_and_stops_it() {
        let mut s = Session::default();
        let input = s.add_port(dummy(1, "in", PortDirection::Input));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, input));
        s.loop_mut(l).expect("loop").set_length(64);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let_assert!(Ok(()) = s.apply_graph_changes());

        let_assert!(Ok(()) = s.remove_loop(l));
        let_assert!(Ok(()) = s.apply_graph_changes());

        check!(s.loop_(l).expect("loop").mode() == LoopMode::Stopped);
        check!(s.loop_(l).expect("loop").length() == 0);
        // Still schedulable, because the slot is kept rather than removed from the arena.
        let_assert!(Ok(()) = s.process(4));
    }

    #[test]
    fn removing_a_port_disconnects_both_directions_and_its_channels() {
        let mut s = Session::default();
        let a = s.add_port(internal("a", 4));
        let b = s.add_port(internal("b", 4));
        let c_port = s.add_port(internal("c", 4));
        let_assert!(Ok(()) = s.connect_ports_internal(a, b));
        let_assert!(Ok(()) = s.connect_ports_internal(b, c_port));

        let l = s.create_loop();
        let_assert!(Ok(ch) = s.add_audio_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(ch, b));
        let_assert!(Ok(()) = s.apply_graph_changes());

        let_assert!(Ok(()) = s.remove_port(b));
        let_assert!(Ok(()) = s.apply_graph_changes());

        // Neither what b fed nor what fed b remains, and the channel no longer reads it.
        let_assert!(Ok(()) = s.process(4));
        check!(s.n_ports() == 3);
    }

    #[test]
    fn removing_something_that_is_not_there_is_an_error() {
        let mut s = Session::default();
        check!(s.remove_loop(3).is_err());
        check!(s.remove_port(3).is_err());
        check!(s.remove_audio_channel(0).is_err());
    }

    #[test]
    fn removing_a_midi_channel_as_audio_is_refused() {
        let mut s = Session::default();
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        // Asking for the wrong kind should fail rather than silently disabling the wrong thing.
        check!(s.remove_audio_channel(c).is_err());
        let_assert!(Ok(()) = s.remove_midi_channel(c));
    }

    #[test]
    fn records_then_plays_back_midi_end_to_end() {
        use crate::midi;
        let mut s = Session::default();
        let input = s.add_port(dummy_midi(1, "min", PortDirection::Input));
        let output = s.add_port(dummy_midi(2, "mout", PortDirection::Output));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, input));
        let_assert!(Ok(()) = s.connect_channel_output(c, output));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(1, &midi::note_on(0, 60, 100));
        s.port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(2, &midi::note_off(0, 60, 64));
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));

        check!(s.loop_(l).unwrap().midi_channel(0).unwrap().n_events() == 2);

        // Play it back and capture what leaves the output port.
        s.loop_mut(l).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let d = s.port_mut(output).unwrap().as_dummy_midi_mut().unwrap();
        let_assert!(Ok(()) = d.request_data(4));
        let_assert!(Ok(()) = s.process(4));

        let got = s
            .port_mut(output)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .take_written_requested_msgs();
        check!(got.len() == 2);
        check!(got[0].data() == midi::note_on(0, 60, 100).as_slice());
        check!(got[1].data() == midi::note_off(0, 60, 64).as_slice());
    }

    #[test]
    fn a_midi_channel_without_ports_still_advances_the_loop() {
        let mut s = Session::default();
        let l = s.create_loop();
        let_assert!(Ok(_) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.apply_graph_changes());
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));
        check!(s.loop_(l).unwrap().length() == 4);
    }

    #[test]
    fn audio_and_midi_channels_coexist_on_one_loop() {
        use crate::midi;
        let mut s = Session::default();
        let audio_in = s.add_port(dummy(1, "ain", PortDirection::Input));
        let midi_in = s.add_port(dummy_midi(2, "min", PortDirection::Input));
        let l = s.create_loop();
        let_assert!(Ok(ac) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(mc) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(ac, audio_in));
        let_assert!(Ok(()) = s.connect_channel_input(mc, midi_in));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.port_mut(audio_in)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[1.0, 2.0, 3.0, 4.0]);
        s.port_mut(midi_in)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(2, &midi::note_on(0, 64, 1));

        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));

        check!(s.loop_(l).unwrap().audio_channel(0).unwrap().data() == vec![1.0, 2.0, 3.0, 4.0]);
        check!(s.loop_(l).unwrap().midi_channel(0).unwrap().n_events() == 1);
    }

    #[test]
    fn two_midi_channels_are_routed_to_their_own_ports() {
        use crate::midi;
        let mut s = Session::default();
        let in_a = s.add_port(dummy_midi(1, "inA", PortDirection::Input));
        let in_b = s.add_port(dummy_midi(2, "inB", PortDirection::Input));
        let l = s.create_loop();
        let_assert!(Ok(ca) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(cb) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(ca, in_a));
        let_assert!(Ok(()) = s.connect_channel_input(cb, in_b));
        let_assert!(Ok(()) = s.apply_graph_changes());

        // Only port A carries a message; channel B must not pick it up.
        s.port_mut(in_a)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(1, &midi::note_on(0, 60, 1));
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));

        check!(s.loop_(l).unwrap().midi_channel(0).unwrap().n_events() == 1);
        check!(s.loop_(l).unwrap().midi_channel(1).unwrap().n_events() == 0);
    }

    #[test]
    fn midi_channels_on_different_loops_do_not_share_input() {
        use crate::midi;
        let mut s = Session::default();
        let in_a = s.add_port(dummy_midi(1, "inA", PortDirection::Input));
        let l1 = s.create_loop();
        let l2 = s.create_loop();
        let_assert!(Ok(c1) = s.add_midi_channel(l1, 64, ChannelMode::Direct));
        let_assert!(Ok(_c2) = s.add_midi_channel(l2, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c1, in_a));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.port_mut(in_a)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(1, &midi::note_on(0, 60, 1));
        let_assert!(Ok(()) = s.set_loop_mode(l1, LoopMode::Recording));
        let_assert!(Ok(()) = s.set_loop_mode(l2, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));

        check!(s.loop_(l1).unwrap().midi_channel(0).unwrap().n_events() == 1);
        // The second loop's channel is wired to nothing and must stay empty.
        check!(s.loop_(l2).unwrap().midi_channel(0).unwrap().n_events() == 0);
    }

    #[test]
    fn recording_midi_over_two_cycles_does_not_duplicate() {
        use crate::midi;
        let mut s = Session::default();
        let input = s.add_port(dummy_midi(1, "min", PortDirection::Input));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, input));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(1, &midi::note_on(0, 60, 1));
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));
        // A second cycle with no new arrivals must not re-record the first one.
        let_assert!(Ok(()) = s.process(4));
        check!(s.loop_(l).unwrap().midi_channel(0).unwrap().n_events() == 1);
    }

    #[test]
    fn playing_midi_over_two_cycles_does_not_resend() {
        use crate::midi;
        let mut s = Session::default();
        let input = s.add_port(dummy_midi(1, "min", PortDirection::Input));
        let output = s.add_port(dummy_midi(2, "mout", PortDirection::Output));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, input));
        let_assert!(Ok(()) = s.connect_channel_output(c, output));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(1, &midi::note_on(0, 60, 1));
        s.port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(2, &midi::note_off(0, 60, 64));
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(4));

        // Play twice over a length of 8, so the message sounds exactly once.
        s.loop_mut(l).unwrap().set_length(8);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let d = s.port_mut(output).unwrap().as_dummy_midi_mut().unwrap();
        let_assert!(Ok(()) = d.request_data(8));
        let_assert!(Ok(()) = s.process(4));
        let_assert!(Ok(()) = s.process(4));

        let got = s
            .port_mut(output)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .take_written_requested_msgs();
        // Each recorded message sent exactly once: routing scratch must not
        // replay the previous cycle.
        check!(got.len() == 2);
    }

    /// A loop whose end falls inside the buffer must be advanced in two pieces.
    /// Processing it in one go would overrun its point of interest.
    #[test]
    fn a_loop_ending_mid_buffer_is_split_into_sub_blocks() {
        let mut s = Session::default();
        let output = s.add_port(internal("out", 8));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_output(c, output));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        // Length 6, buffer 4: the first cycle ends 2 frames short of the wrap and
        // the second must split 2 + 2.
        s.loop_mut(l).unwrap().set_length(6);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));

        let_assert!(Ok(()) = s.process(4));
        check!(s.port_mut(output).unwrap().buffer(4).to_vec() == vec![1.0, 2.0, 3.0, 4.0]);
        check!(s.position_of(l) == Some(4));
        // Nothing to split: the wrap is beyond this buffer.
        check!(s.n_sub_blocks_last_cycle() == 1);

        // Frames 5 and 6 play, then the loop wraps and frames 1 and 2 play again.
        let_assert!(Ok(()) = s.process(4));
        check!(s.port_mut(output).unwrap().buffer(4).to_vec() == vec![5.0, 6.0, 1.0, 2.0]);
        // Exactly two sub-blocks: 2 frames to the wrap, then 2 after it. A stale
        // point of interest would cost an extra, wasted pass.
        check!(s.n_sub_blocks_last_cycle() == 2);
        check!(s.n_stuck_cycles() == 0);
    }

    #[test]
    fn a_loop_shorter_than_the_buffer_wraps_repeatedly_in_one_cycle() {
        let mut s = Session::default();
        let output = s.add_port(internal("out", 8));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_output(c, output));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 2.0]);
        s.loop_mut(l).unwrap().set_length(2);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));

        // Two-frame loop across a six-frame buffer: three passes in one cycle.
        let_assert!(Ok(()) = s.process(6));
        check!(
            s.port_mut(output).unwrap().buffer(6).to_vec() == vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0]
        );
        check!(s.n_sub_blocks_last_cycle() == 3);
        check!(s.n_stuck_cycles() == 0);
    }

    #[test]
    fn co_processed_loops_stay_aligned_across_a_wrap() {
        let mut s = Session::default();
        let out_a = s.add_port(internal("outA", 8));
        let out_b = s.add_port(internal("outB", 8));
        let l1 = s.create_loop();
        let l2 = s.create_loop();
        let_assert!(Ok(c1) = s.add_audio_channel(l1, 64, ChannelMode::Direct));
        let_assert!(Ok(c2) = s.add_audio_channel(l2, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_output(c1, out_a));
        let_assert!(Ok(()) = s.connect_channel_output(c2, out_b));
        let_assert!(Ok(()) = s.apply_graph_changes());

        // Different lengths, so their wraps fall at different points and the
        // sub-block split has to accommodate both.
        s.loop_mut(l1)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 2.0, 3.0]);
        s.loop_mut(l1).unwrap().set_length(3);
        s.loop_mut(l2)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[10.0, 20.0]);
        s.loop_mut(l2).unwrap().set_length(2);
        let_assert!(Ok(()) = s.set_loop_mode(l1, LoopMode::Playing));
        let_assert!(Ok(()) = s.set_loop_mode(l2, LoopMode::Playing));

        let_assert!(Ok(()) = s.process(6));
        // Each loop repeats on its own length, sample-aligned throughout.
        check!(s.port_mut(out_a).unwrap().buffer(6).to_vec() == vec![1.0, 2.0, 3.0, 1.0, 2.0, 3.0]);
        check!(
            s.port_mut(out_b).unwrap().buffer(6).to_vec()
                == vec![10.0, 20.0, 10.0, 20.0, 10.0, 20.0]
        );
        check!(s.n_stuck_cycles() == 0);
    }

    #[test]
    fn co_processed_loops_share_a_step() {
        let mut s = Session::default();
        let l1 = s.create_loop();
        let l2 = s.create_loop();
        let_assert!(Ok(_) = s.add_audio_channel(l1, 4, ChannelMode::Direct));
        let_assert!(Ok(_) = s.add_audio_channel(l2, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.apply_graph_changes());

        let loop_step = s
            .schedule_names()
            .into_iter()
            .find(|step| step.iter().any(|n| n == "loop::process"));
        let_assert!(Some(step) = loop_step);
        check!(step.len() == 2);
    }

    #[test]
    fn a_loop_with_no_sync_source_transitions_immediately() {
        let mut s = Session::default();
        let l = s.create_loop();
        let_assert!(Ok(_) = s.add_audio_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.apply_graph_changes());
        s.loop_mut(l).unwrap().set_length(4);

        // No sync source: a planned transition takes effect at once.
        s.loop_mut(l)
            .unwrap()
            .plan_transition(LoopMode::Recording, Some(1), None);
        check!(s.loop_(l).unwrap().mode() == LoopMode::Recording);
    }

    #[test]
    fn a_synced_loop_waits_for_its_sources_trigger() {
        let mut s = Session::default();
        let sync = s.create_loop();
        let follower = s.create_loop();
        let_assert!(Ok(_) = s.add_audio_channel(sync, 64, ChannelMode::Direct));
        let_assert!(Ok(_) = s.add_audio_channel(follower, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.set_loop_sync_source(follower, Some(sync)));
        let_assert!(Ok(()) = s.apply_graph_changes());
        check!(s.sync_source_of(follower) == Some(sync));

        // A two-frame sync loop playing, so it triggers every two frames.
        s.loop_mut(sync)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 1.0]);
        s.loop_mut(sync).unwrap().set_length(2);
        let_assert!(Ok(()) = s.set_loop_mode(sync, LoopMode::Playing));

        s.loop_mut(follower).unwrap().set_length(8);
        // Having a sync source, the transition is queued rather than immediate.
        s.loop_mut(follower)
            .unwrap()
            .plan_transition(LoopMode::Playing, Some(0), None);
        check!(s.loop_(follower).unwrap().mode() == LoopMode::Stopped);

        // One cycle spans the sync loop's wrap, so the follower is triggered.
        let_assert!(Ok(()) = s.process(4));
        check!(s.loop_(follower).unwrap().mode() == LoopMode::Playing);
    }

    #[test]
    fn a_follower_is_not_triggered_without_its_source_wrapping() {
        let mut s = Session::default();
        let sync = s.create_loop();
        let follower = s.create_loop();
        let_assert!(Ok(_) = s.add_audio_channel(sync, 64, ChannelMode::Direct));
        let_assert!(Ok(_) = s.add_audio_channel(follower, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.set_loop_sync_source(follower, Some(sync)));
        let_assert!(Ok(()) = s.apply_graph_changes());

        // The sync loop is stopped, so it never triggers.
        s.loop_mut(sync).unwrap().set_length(100);
        s.loop_mut(follower).unwrap().set_length(8);
        s.loop_mut(follower)
            .unwrap()
            .plan_transition(LoopMode::Playing, Some(0), None);

        let_assert!(Ok(()) = s.process(4));
        let_assert!(Ok(()) = s.process(4));
        check!(s.loop_(follower).unwrap().mode() == LoopMode::Stopped);
    }

    #[test]
    fn clearing_a_sync_source_restores_immediate_transitions() {
        let mut s = Session::default();
        let sync = s.create_loop();
        let follower = s.create_loop();
        let_assert!(Ok(_) = s.add_audio_channel(follower, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.set_loop_sync_source(follower, Some(sync)));
        check!(s.sync_source_of(follower) == Some(sync));

        let_assert!(Ok(()) = s.set_loop_sync_source(follower, None));
        check!(s.sync_source_of(follower) == None);
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.loop_mut(follower).unwrap().set_length(4);
        s.loop_mut(follower)
            .unwrap()
            .plan_transition(LoopMode::Recording, Some(1), None);
        check!(s.loop_(follower).unwrap().mode() == LoopMode::Recording);
    }

    #[test]
    fn a_sync_cycle_does_not_recurse() {
        let mut s = Session::default();
        let a = s.create_loop();
        let b = s.create_loop();
        let_assert!(Ok(_) = s.add_audio_channel(a, 64, ChannelMode::Direct));
        let_assert!(Ok(_) = s.add_audio_channel(b, 64, ChannelMode::Direct));
        // Mutual sync. Snapshots make this survivable; querying the source live,
        // as the C++ does, would recurse until the stack ran out.
        let_assert!(Ok(()) = s.set_loop_sync_source(a, Some(b)));
        let_assert!(Ok(()) = s.set_loop_sync_source(b, Some(a)));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.loop_mut(a).unwrap().set_length(4);
        s.loop_mut(b).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(a, LoopMode::Playing));
        let_assert!(Ok(()) = s.set_loop_mode(b, LoopMode::Playing));
        let_assert!(Ok(()) = s.process(4));
        check!(s.n_stuck_cycles() == 0);
    }

    #[test]
    fn all_loops_are_co_processed() {
        let mut s = Session::default();
        for _ in 0..3 {
            let l = s.create_loop();
            let_assert!(Ok(_) = s.add_audio_channel(l, 64, ChannelMode::Direct));
        }
        let_assert!(Ok(()) = s.apply_graph_changes());
        // One step holding all three loops, matching the C++, which hands every
        // loop node to every loop's co-process callback.
        let step = s
            .schedule_names()
            .into_iter()
            .find(|st| st.iter().any(|n| n == "loop::process"));
        let_assert!(Some(step) = step);
        check!(step.len() == 3);
    }

    #[test]
    fn a_passthrough_cycle_is_reported() {
        let mut s = Session::default();
        let a = s.add_port(internal("a", 4));
        let b = s.add_port(internal("b", 4));
        let_assert!(Ok(()) = s.connect_ports_internal(a, b));
        let_assert!(Ok(()) = s.connect_ports_internal(b, a));
        check!(s.apply_graph_changes() == Err(SessionError::Graph(GraphError::Cycle)));
    }

    #[test]
    fn sample_rate_and_buffer_size_round_trip() {
        let mut s = Session::default();
        s.set_sample_rate(48000);
        s.set_buffer_size(256);
        check!(s.sample_rate() == 48000);
        check!(s.buffer_size() == 256);
    }

    #[test]
    fn several_cycles_accumulate_a_recording() {
        let mut s = Session::default();
        let input = s.add_port(dummy(1, "in", PortDirection::Input));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, input));
        let_assert!(Ok(()) = s.apply_graph_changes());

        let d = s.port_mut(input).unwrap().as_dummy_mut().unwrap();
        d.queue_data(&[1.0, 2.0]);
        d.queue_data(&[3.0, 4.0]);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        let_assert!(Ok(()) = s.process(2));
        let_assert!(Ok(()) = s.process(2));

        check!(s.loop_(l).unwrap().length() == 4);
        check!(s.loop_(l).unwrap().audio_channel(0).unwrap().data() == vec![1.0, 2.0, 3.0, 4.0]);
    }
}
