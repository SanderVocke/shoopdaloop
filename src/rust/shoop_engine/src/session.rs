//! Owns the engine's entities and runs them in dependency order each cycle.
//!
//! Ports, loops and channels live in arenas and refer to each other by index. The
//! processing schedule is recomputed only when the topology changes, tracked by a
//! request/applied id pair: mutations bump the request, and [`Session::apply_graph_changes`]
//! brings the applied id up to it.
//!
//! the audio callback. Here recomputation is an explicit call, because the thread
//! boundary only exists once a driver does; [`Session::process`] refuses to run a
//! stale graph rather than silently using one.
//!
//! Audio only for now: MIDI channels are not yet routed through the session.

#[cfg(feature = "lv2")]
use crate::carla_processor::CarlaProcessor;
use shoop_plugin_protocol::MAX_MIDI_EVENTS_PER_BLOCK;
use std::collections::HashMap;
use std::sync::Arc;

use crate::audio_channel::PreparedAudioChannelData;
use crate::audio_midi_loop::AudioMidiLoop;
use crate::basic_loop::SyncSourceState;
use crate::channel_mode::{loop_mode_to_channel_process_flags, ChannelMode, ProcessFlags};
use crate::composite_plan::{CompiledCompositePlan, LoopIdentity, LoopTargetKind};
use crate::composite_timeline::{
    AcceptedTimelineControl, BoundaryIntent, BoundaryIntentOrigin, BoundaryTargetAction,
    CompositeBoundaryTimeline, CompositeTimelineBuildError, CompositeTimelineControlError,
    CompositeTimelineFault,
};
use crate::dummy_midi_port::DummyMidiPort;
use crate::dummy_port::DummyAudioPort;
use crate::external_audio_port::ExternalAudioPort;
use crate::external_midi_port::ExternalMidiPort;
use crate::graph::{processing_order, GraphError, NodeIdx, NodeSpec};
use crate::graph_build::{
    ChannelDesc, GraphDesc, LoopDesc, LoopIdx, NodeMap, PortDesc, PortIdx, ProcessorDesc,
};
use crate::internal_audio_port::InternalAudioPort;
use crate::loop_mode::LoopMode;
use crate::midi_state::MAX_DIFF_MESSAGES;
use crate::midi_storage::MidiStorageElem;
use crate::profiling::{Profiler, ProfilingReport, ProfilingReportItem, Stage};
use crate::state_mirror::{
    AudioChannelStateMirror, AudioPortStateMirror, LoopStateMirror, MidiChannelStateMirror,
    MidiPortStateMirror,
};
use crate::tiny_synth_fx::TinySynthFxProcessor;

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
/// `n_recursive_0_procs` but increments on every recursion, not only zero-length
/// ones, so it bounds total sub-blocks the same way.
const MAX_SUB_BLOCKS: u32 = 16;
pub const MAX_AUDIO_RINGBUFFER_ADOPTIONS: usize = 64;
pub const MAX_AUDIO_RINGBUFFER_ADOPTION_CHANNELS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioRingbufferAdoption {
    pub loop_idx: usize,
    pub reverse_start_cycle: Option<i32>,
    pub cycles_length: Option<i32>,
    pub go_to_cycle: Option<i32>,
    pub go_to_mode: LoopMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioRingbufferAdoptionChannelShape {
    pub loop_idx: usize,
    pub channel_idx: usize,
    pub chunk_size: usize,
    pub capacity: usize,
}

#[derive(Debug, Clone)]
pub struct AudioRingbufferAdoptionShape {
    channels: [Option<AudioRingbufferAdoptionChannelShape>; MAX_AUDIO_RINGBUFFER_ADOPTION_CHANNELS],
    n_channels: usize,
}

impl AudioRingbufferAdoptionShape {
    pub fn channels(&self) -> impl Iterator<Item = AudioRingbufferAdoptionChannelShape> + '_ {
        self.channels[..self.n_channels].iter().flatten().copied()
    }
}

#[derive(Debug)]
pub struct PreparedAudioRingbufferAdoptionChannel {
    pub loop_idx: usize,
    pub channel_idx: usize,
    pub data: PreparedAudioChannelData,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("graph could not be scheduled: {0}")]
    Graph(#[from] GraphError),
    #[error("no such port: {0}")]
    NoSuchPort(usize),
    #[error("no such loop: {0}")]
    NoSuchLoop(usize),
    #[error("no such processor")]
    NoSuchProcessor,
    #[error("loop {0} cannot be its own sync source")]
    SelfSync(usize),
    #[error("no channel at index {0}, or it is not of the expected kind")]
    NoSuchChannel(usize),
    #[error("audio ringbuffer adoption exceeds its bounded request or destination capacity")]
    AudioRingbufferAdoptionCapacity,
    #[error("the composite timeline references stale or missing primitive slot {0}")]
    StaleCompositeTarget(u32),
    #[error("the composite/session propagation topology is invalid: {0}")]
    CompositeTimeline(#[from] CompositeTimelineBuildError),
    #[error("the prepared composite timeline does not match the current primitive topology")]
    StaleCompositeTopology,
    #[error("the prepared composite timeline has no version or is not newer than version {0}")]
    StaleCompositeVersion(u64),
    #[error("running replacement exceeds bounded restart or retirement capacity")]
    CompositeReplacementRequiresRuntimeTransfer,
}

#[derive(Debug)]
pub struct RejectedCompositeTimeline {
    pub error: SessionError,
    pub timeline: CompositeBoundaryTimeline,
}

#[derive(Debug)]
pub struct ReclaimedCompositeTimeline {
    timeline: CompositeBoundaryTimeline,
}

impl ReclaimedCompositeTimeline {
    pub fn n_composites(&self) -> usize {
        self.timeline.n_composites()
    }
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
    /// Tombstones keep stale stable identities from being redirected.
    loop_live: Vec<bool>,
    /// Composite scheduling and same-sample resolution on the loop-group timeline.
    composite_timeline: CompositeBoundaryTimeline,
    /// Stable queue-order tie-break assigned when callback-start commands are accepted.
    composite_acceptance_sequence: u64,
    /// Most recent globally prepared timeline revision accepted by the callback.
    composite_timeline_version: u64,

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
    /// Primitive events and natural intents gathered at a settled boundary.
    boundary_triggers: Vec<LoopIdentity>,
    boundary_delivered_triggers: Vec<LoopIdentity>,
    boundary_natural_intents: Vec<BoundaryIntent>,
    /// Stable, callback-owned processors and their explicit graph routes.
    processors: Vec<ProcessorRoute>,
    /// Cycles that hit [`MAX_SUB_BLOCKS`] without finishing.
    n_stuck_cycles: u32,
    /// Cycles run against a schedule older than the current topology.
    ///
    /// Not an error: a stale cycle runs the last-applied schedule rather than being
    /// refused, so audio keeps flowing while the next schedule is built. The counter
    /// exists because the alternative -- staying quiet about it -- is what made a
    /// permanently stale graph look like ordinary silence.
    n_stale_cycles: u32,
    /// Sub-blocks used by the most recent cycle, across all loop steps.
    ///
    /// A performance signal as much as a correctness one: every extra sub-block is
    /// another pass over every loop in the step.
    n_sub_blocks_last_cycle: u32,
    /// Existing deterministic stage profiler, enabled only for explicit tracing sessions.
    profiler: Profiler,
}

#[derive(Debug)]
enum ProcessorBackend {
    External,
    Test2x2x1,
    Tiny(TinySynthFxProcessor),
    #[cfg(feature = "lv2")]
    Carla(Box<dyn CarlaProcessor>),
}

#[derive(Debug)]
struct ProcessorRoute {
    title: String,
    backend: ProcessorBackend,
    active: bool,
    audio_inputs: Vec<usize>,
    audio_outputs: Vec<usize>,
    midi_inputs: Vec<usize>,
    midi_staging: Vec<Vec<MidiStorageElem>>,
}

/// A topology snapshot: everything [`build_schedule`] needs, borrowing nothing.
///
/// Produced by [`Session::describe_topology`] and consumed by [`build_schedule`], so the
/// expensive part of a rebuild can run on a thread that does not hold the session.
#[derive(Debug)]
pub struct Topology {
    graph: GraphDesc,
    /// MIDI channel indices per loop, in channel order.
    midi_by_loop: Vec<Vec<usize>>,
    n_loops: usize,
    /// `graph_request_id` when this was taken, so an install can say what it covers.
    graph_id: u64,
}

/// A schedule built for one topology, ready to be installed.
///
/// Holds every allocation a rebuild needs, grown to size here so that
/// [`Session::install_schedule`] is nothing but moves. Also what an install hands back, so
/// the memory it displaced is freed by the installer's choice of thread rather than
/// wherever the swap happened to occur.
#[derive(Debug)]
pub struct PreparedSchedule {
    specs: Vec<NodeSpec>,
    node_map: NodeMap,
    schedule: Vec<Vec<NodeIdx>>,
    node_actions: Vec<NodeAction>,
    midi_mappings_by_loop: Vec<Vec<usize>>,
    /// Per-MIDI-channel scratch, pre-reserved so no cycle grows one.
    midi_in_scratch: Vec<Vec<MidiStorageElem>>,
    midi_out_scratch: Vec<Vec<MidiStorageElem>>,
    /// Loop-step and boundary scratch, likewise sized here rather than on first use.
    loop_group: Vec<usize>,
    boundary_triggers: Vec<LoopIdentity>,
    boundary_delivered_triggers: Vec<LoopIdentity>,
    boundary_natural_intents: Vec<BoundaryIntent>,
    /// Topology generation this covers.
    for_graph_id: u64,
}

/// Builds a schedule from a topology. The expensive half of a rebuild, and pure.
///
/// Lowers the description to nodes, topologically sorts them, resolves what each node does,
/// and grows every buffer a cycle will need. Nothing here touches a [`Session`], which is
/// the point: it can run on any thread, at any time, while audio keeps flowing against the
/// schedule already installed.
#[tracing::instrument(name = "engine.graph.build_schedule", skip_all)]
pub fn build_schedule(topology: Topology) -> Result<PreparedSchedule, SessionError> {
    let Topology {
        graph,
        midi_by_loop,
        n_loops,
        graph_id,
    } = topology;

    let (specs, node_map) = graph.build();
    let schedule = processing_order(&specs)?;

    let mut node_actions = vec![NodeAction::None; specs.len()];
    for (i, &n) in node_map.port_prepare.iter().enumerate() {
        node_actions[n.0] = NodeAction::PortPrepare(i);
    }
    for (i, &n) in node_map.port_process.iter().enumerate() {
        node_actions[n.0] = NodeAction::PortProcess(i);
    }
    for (i, &n) in node_map.processor_process.iter().enumerate() {
        node_actions[n.0] = NodeAction::ProcessorProcess(i);
    }
    for (i, &n) in node_map.loop_process.iter().enumerate() {
        node_actions[n.0] = NodeAction::LoopProcess(i);
    }
    for (i, &n) in node_map.channel_prepare.iter().enumerate() {
        node_actions[n.0] = NodeAction::ChannelPrepare(i);
    }
    for (i, &n) in node_map.channel_process.iter().enumerate() {
        node_actions[n.0] = NodeAction::ChannelProcess(i);
    }

    // Scratch sized for the widest loop, so a cycle neither searches nor grows a buffer.
    // Room inside each buffer as well as for the buffers themselves: a cycle pushing its
    // first message into a zero-capacity vector would allocate on the audio thread, and a
    // playback interruption may emit targeted note-offs.
    let widest = midi_by_loop.iter().map(|v| v.len()).max().unwrap_or(0);
    let mut midi_in_scratch: Vec<Vec<MidiStorageElem>> = Vec::with_capacity(widest);
    let mut midi_out_scratch: Vec<Vec<MidiStorageElem>> = Vec::with_capacity(widest);
    for _ in 0..widest {
        midi_in_scratch.push(Vec::with_capacity(MIDI_SCRATCH_CAPACITY));
        midi_out_scratch.push(Vec::with_capacity(MIDI_OUT_SCRATCH_CAPACITY));
    }

    Ok(PreparedSchedule {
        specs,
        node_map,
        schedule,
        node_actions,
        midi_mappings_by_loop: midi_by_loop,
        midi_in_scratch,
        midi_out_scratch,
        loop_group: Vec::with_capacity(n_loops),
        boundary_triggers: Vec::with_capacity(n_loops),
        boundary_delivered_triggers: Vec::with_capacity(n_loops),
        boundary_natural_intents: Vec::with_capacity(n_loops),
        for_graph_id: graph_id,
    })
}

/// Both cross a thread boundary on the way to an install, so this has to hold. Checked here
/// rather than discovered when the scheduler is wired up.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Topology>();
    assert_send::<PreparedSchedule>();
};

/// What a scheduled node does when its step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeAction {
    PortPrepare(usize),
    PortProcess(usize),
    ProcessorProcess(usize),
    LoopProcess(usize),
    ChannelPrepare(usize),
    ChannelProcess(usize),
    /// A node with no work, only ordering.
    None,
}

fn adoption_window(
    request: &AudioRingbufferAdoption,
    cycle_len: u32,
    sync_pos: u32,
    data_len: usize,
) -> (usize, usize, usize) {
    let cycles = request.cycles_length.unwrap_or(1).max(1) as u32;
    let go_cycle = request.go_to_cycle.unwrap_or(0).max(0) as u32;
    let wanted_len = if cycle_len > 0 {
        if request.reverse_start_cycle == Some(0) {
            sync_pos
        } else if request.go_to_mode == LoopMode::Recording {
            go_cycle.saturating_mul(cycle_len).saturating_add(sync_pos)
        } else {
            cycles.saturating_mul(cycle_len)
        }
    } else {
        data_len.min(u32::MAX as usize) as u32
    } as usize;
    let end = if cycle_len > 0 {
        if let Some(reverse_start_cycle) = request.reverse_start_cycle {
            if reverse_start_cycle == 0 {
                data_len
            } else {
                let cycles_before_current =
                    (reverse_start_cycle.max(0) as u32).saturating_sub(cycles);
                let offset =
                    sync_pos.saturating_add(cycles_before_current.saturating_mul(cycle_len));
                data_len.saturating_sub(offset as usize)
            }
        } else if request.go_to_mode == LoopMode::Recording {
            data_len
        } else {
            let offset = sync_pos.saturating_add(go_cycle.saturating_mul(cycle_len));
            data_len.saturating_sub(offset as usize)
        }
    } else {
        data_len
    };
    (wanted_len, end.saturating_sub(wanted_len), end)
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

    pub fn profiling_report(&self) -> ProfilingReport {
        ProfilingReport {
            items: Stage::ALL
                .iter()
                .map(|stage| {
                    let report = self.profiler.report(*stage);
                    ProfilingReportItem {
                        key: stage.name().to_string(),
                        n_samples: report.calls as f32,
                        average: if report.calls == 0 {
                            0.0
                        } else {
                            report.last_ns as f32 / report.calls as f32
                        },
                        worst: report.worst_ns as f32,
                        most_recent: report.last_ns as f32,
                    }
                })
                .collect(),
        }
    }

    /// Where a channel sits: which loop, which kind, and which index within that loop.
    ///
    /// The arena index is what a connection and a handle both hold, but the channel itself
    /// lives inside its loop, so anything going from one to the other needs this. Published
    /// state does exactly that, once per channel per cycle.
    pub fn channel_mapping(&self, idx: usize) -> Option<&ChannelMapping> {
        self.channels.get(idx)
    }

    pub fn audio_channel(&self, idx: usize) -> Option<&crate::audio_channel::AudioChannel> {
        let mapping = self.channels.get(idx)?;
        (mapping.kind == ChannelKind::Audio)
            .then(|| {
                self.loops
                    .get(mapping.loop_idx)?
                    .audio_channel(mapping.channel_idx)
            })
            .flatten()
    }

    pub fn audio_channel_mut(
        &mut self,
        idx: usize,
    ) -> Option<&mut crate::audio_channel::AudioChannel> {
        let mapping = self.channels.get(idx)?;
        let (loop_idx, channel_idx) = (mapping.loop_idx, mapping.channel_idx);
        (mapping.kind == ChannelKind::Audio)
            .then(|| self.loops.get_mut(loop_idx)?.audio_channel_mut(channel_idx))
            .flatten()
    }

    pub fn midi_channel(&self, idx: usize) -> Option<&crate::midi_channel::MidiChannel> {
        let mapping = self.channels.get(idx)?;
        (mapping.kind == ChannelKind::Midi)
            .then(|| {
                self.loops
                    .get(mapping.loop_idx)?
                    .midi_channel(mapping.channel_idx)
            })
            .flatten()
    }

    pub fn midi_channel_mut(
        &mut self,
        idx: usize,
    ) -> Option<&mut crate::midi_channel::MidiChannel> {
        let mapping = self.channels.get(idx)?;
        let (loop_idx, channel_idx) = (mapping.loop_idx, mapping.channel_idx);
        (mapping.kind == ChannelKind::Midi)
            .then(|| self.loops.get_mut(loop_idx)?.midi_channel_mut(channel_idx))
            .flatten()
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

    pub fn loop_identity(&self, idx: usize) -> Option<LoopIdentity> {
        self.loop_live
            .get(idx)
            .copied()
            .unwrap_or(false)
            .then_some(LoopIdentity {
                slot: idx as u32,
                generation: 1,
                kind: LoopTargetKind::Basic,
            })
    }

    pub fn composite_timeline(&self) -> &CompositeBoundaryTimeline {
        &self.composite_timeline
    }

    pub fn composite_timeline_mut(&mut self) -> &mut CompositeBoundaryTimeline {
        &mut self.composite_timeline
    }

    pub fn composite_timeline_version(&self) -> u64 {
        self.composite_timeline_version
    }

    pub fn accept_composite_transition(
        &mut self,
        source: LoopIdentity,
        mode: LoopMode,
        delay: u32,
    ) -> Result<u64, CompositeTimelineControlError> {
        let acceptance_sequence = self.composite_acceptance_sequence;
        self.composite_timeline
            .request_transition(source, mode, delay)?;
        self.publish_composite_anticipated_transitions();
        self.composite_acceptance_sequence = self.composite_acceptance_sequence.saturating_add(1);
        Ok(acceptance_sequence)
    }

    pub fn accept_composite_immediate_transition(
        &mut self,
        source: LoopIdentity,
        mode: LoopMode,
        iteration: i64,
    ) -> Result<u64, CompositeTimelineControlError> {
        let acceptance_sequence = self.composite_acceptance_sequence;
        self.composite_timeline.queue_immediate_transition(
            source,
            mode,
            iteration,
            acceptance_sequence,
        )?;
        self.apply_composite_controls_now()?;
        self.publish_composite_anticipated_transitions();
        self.composite_acceptance_sequence = self.composite_acceptance_sequence.saturating_add(1);
        Ok(acceptance_sequence)
    }

    fn apply_composite_controls_now(&mut self) -> Result<(), CompositeTimelineControlError> {
        let Session {
            composite_timeline,
            loops,
            loop_live,
            ..
        } = self;
        composite_timeline.align_sync_positions(|identity| {
            if identity.kind != LoopTargetKind::Basic
                || identity.generation != 1
                || !loop_live
                    .get(identity.slot as usize)
                    .copied()
                    .unwrap_or(false)
            {
                return None;
            }
            loops
                .get(identity.slot as usize)
                .map(|loop_| u64::from(loop_.position()))
        });
        let trace = composite_timeline
            .resolve_boundary(&[], &[], |identity| {
                identity.kind == LoopTargetKind::Basic
                    && identity.generation == 1
                    && loop_live
                        .get(identity.slot as usize)
                        .copied()
                        .unwrap_or(false)
            })
            .map_err(|_| CompositeTimelineControlError::BoundaryFault)?;
        for entry in trace {
            if entry.target.kind != LoopTargetKind::Basic
                || entry.target.generation != 1
                || !loop_live
                    .get(entry.target.slot as usize)
                    .copied()
                    .unwrap_or(false)
            {
                continue;
            }
            let Some(loop_) = loops.get_mut(entry.target.slot as usize) else {
                continue;
            };
            match entry.action {
                BoundaryTargetAction::Stop => loop_.set_mode(LoopMode::Stopped),
                BoundaryTargetAction::SetMode {
                    mode,
                    offset_samples,
                    retrigger,
                } => {
                    let mode_changed = loop_.mode() != mode;
                    loop_.set_mode(mode);
                    if retrigger || mode_changed {
                        let offset = u32::try_from(offset_samples).unwrap_or(u32::MAX);
                        if matches!(mode, LoopMode::Recording | LoopMode::RecordingDryIntoWet) {
                            loop_.set_length(offset);
                            loop_.set_position(0);
                        } else {
                            loop_.set_position(offset.min(loop_.length()));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn accept_composite_play_after_record(
        &mut self,
        source: LoopIdentity,
        enabled: bool,
    ) -> Result<u64, CompositeTimelineControlError> {
        let acceptance_sequence = self.composite_acceptance_sequence;
        self.composite_timeline
            .set_play_after_record(source, enabled)?;
        self.publish_composite_states();
        self.composite_acceptance_sequence = self.composite_acceptance_sequence.saturating_add(1);
        Ok(acceptance_sequence)
    }

    pub fn reclaim_composite_plans(
        &mut self,
        storage: Vec<CompiledCompositePlan>,
    ) -> Vec<CompiledCompositePlan> {
        self.composite_timeline.reclaim_retired_plans(storage)
    }

    pub fn accept_composite_fault_reset(&mut self) -> u64 {
        let acceptance_sequence = self.composite_acceptance_sequence;
        self.composite_timeline.reset_fault();
        self.composite_acceptance_sequence = self.composite_acceptance_sequence.saturating_add(1);
        acceptance_sequence
    }

    pub fn accept_composite_control(
        &mut self,
        target: LoopIdentity,
        action: BoundaryTargetAction,
        at_sample: Option<u64>,
    ) -> Result<u64, CompositeTimelineControlError> {
        let acceptance_sequence = self.composite_acceptance_sequence;
        let control = AcceptedTimelineControl {
            at_sample: at_sample.unwrap_or_else(|| self.composite_timeline.sample_clock()),
            target,
            action,
            acceptance_sequence,
        };
        self.composite_timeline.queue_control(control)?;
        self.publish_composite_anticipated_transitions();
        self.composite_acceptance_sequence = self.composite_acceptance_sequence.saturating_add(1);
        Ok(acceptance_sequence)
    }

    fn publish_composite_anticipated_transitions(&self) {
        for (index, loop_) in self.loops.iter().enumerate() {
            let transition = loop_.first_planned_transition().or_else(|| {
                self.loop_identity(index)
                    .and_then(|identity| self.composite_timeline.anticipated_transition(identity))
            });
            loop_.publish_state_with_transition(transition);
        }
        self.publish_composite_states();
    }

    fn publish_composite_states(&self) {
        let timeline = &self.composite_timeline;
        let loop_live = &self.loop_live;
        timeline.publish_state_mirrors(|identity| match identity.kind {
            LoopTargetKind::Basic => {
                identity.generation == 1
                    && loop_live
                        .get(identity.slot as usize)
                        .copied()
                        .unwrap_or(false)
            }
            LoopTargetKind::Composite => timeline.is_current_composite(identity),
        });
    }

    pub fn install_composite_timeline(
        &mut self,
        timeline: CompositeBoundaryTimeline,
    ) -> Result<CompositeBoundaryTimeline, SessionError> {
        self.validate_composite_targets(&timeline)?;
        if !timeline.is_empty() {
            timeline.validate_primitive_sync_sources(&self.sync_sources)?;
        }
        self.composite_timeline.mark_mirrors_removed_by(&timeline);
        let previous = std::mem::replace(&mut self.composite_timeline, timeline);
        self.publish_composite_states();
        Ok(previous)
    }

    pub fn install_prepared_composite_timeline(
        &mut self,
        timeline: CompositeBoundaryTimeline,
    ) -> Result<ReclaimedCompositeTimeline, RejectedCompositeTimeline> {
        let result = self.validate_composite_targets(&timeline).and_then(|_| {
            if !timeline.matches_prepared_primitive_sync_sources(&self.sync_sources) {
                return Err(SessionError::StaleCompositeTopology);
            }
            match timeline.prepared_version() {
                Some(version) if version > self.composite_timeline_version => Ok(version),
                _ => Err(SessionError::StaleCompositeVersion(
                    self.composite_timeline_version,
                )),
            }
        });
        let version = match result {
            Ok(version) => version,
            Err(error) => return Err(RejectedCompositeTimeline { error, timeline }),
        };
        if self
            .composite_timeline
            .replacement_requires_runtime_transfer()
        {
            if self
                .composite_timeline
                .can_queue_runtime_preserving_replacement(&timeline)
            {
                let reclaimed = self
                    .composite_timeline
                    .queue_runtime_preserving_replacement(timeline);
                self.composite_timeline_version = version;
                self.publish_composite_states();
                return Ok(ReclaimedCompositeTimeline {
                    timeline: reclaimed,
                });
            }
            if !self
                .composite_timeline
                .can_restart_with_changed_topology(&timeline)
            {
                return Err(RejectedCompositeTimeline {
                    error: SessionError::CompositeReplacementRequiresRuntimeTransfer,
                    timeline,
                });
            }
            for identity in self.composite_timeline.active_primitive_children() {
                if let Some(loop_) = self.loops.get_mut(identity.slot as usize) {
                    loop_.set_mode(LoopMode::Stopped);
                }
            }
            let mut previous = std::mem::replace(&mut self.composite_timeline, timeline);
            previous.mark_mirrors_removed_by(&self.composite_timeline);
            self.composite_timeline.prepare_changed_topology_restart(
                &mut previous,
                &mut self.composite_acceptance_sequence,
            );
            self.composite_timeline_version = version;
            self.publish_composite_states();
            Ok(ReclaimedCompositeTimeline { timeline: previous })
        } else {
            let mut previous = std::mem::replace(&mut self.composite_timeline, timeline);
            previous.mark_mirrors_removed_by(&self.composite_timeline);
            self.composite_timeline
                .prepare_stopped_replacement(&mut previous);
            self.composite_timeline_version = version;
            self.publish_composite_states();
            Ok(ReclaimedCompositeTimeline { timeline: previous })
        }
    }

    fn validate_composite_targets(
        &self,
        timeline: &CompositeBoundaryTimeline,
    ) -> Result<(), SessionError> {
        if let Some(identity) = timeline.first_invalid_primitive(|identity| {
            identity.kind == LoopTargetKind::Basic
                && identity.generation == 1
                && self
                    .loop_live
                    .get(identity.slot as usize)
                    .copied()
                    .unwrap_or(false)
        }) {
            Err(SessionError::StaleCompositeTarget(identity.slot))
        } else {
            Ok(())
        }
    }

    /// True when the schedule matches the current topology.
    pub fn graph_up_to_date(&self) -> bool {
        self.graph_request_id == self.graph_applied_id
    }

    pub fn graph_request_id(&self) -> u64 {
        self.graph_request_id
    }

    pub fn graph_applied_id(&self) -> u64 {
        self.graph_applied_id
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

    pub fn add_audio_port_with_state(
        &mut self,
        mut port: Port,
        state: Arc<AudioPortStateMirror>,
    ) -> Result<usize, SessionError> {
        port.audio_mut()
            .ok_or(SessionError::NoSuchPort(self.ports.len()))?
            .set_state_mirror(state);
        Ok(self.add_port(port))
    }

    pub fn add_midi_port_with_state(
        &mut self,
        mut port: Port,
        state: Arc<MidiPortStateMirror>,
    ) -> Result<usize, SessionError> {
        port.midi_mut()
            .ok_or(SessionError::NoSuchPort(self.ports.len()))?
            .set_state_mirror(state);
        Ok(self.add_port(port))
    }

    pub fn create_loop(&mut self) -> usize {
        self.create_loop_with_state(Arc::new(LoopStateMirror::default()))
    }

    pub fn create_loop_with_state(&mut self, state: Arc<LoopStateMirror>) -> usize {
        self.loops.push(AudioMidiLoop::with_state_mirror(state));
        self.sync_sources.push(None);
        self.sync_snapshots.push(None);
        self.loop_live.push(true);
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
        self.add_audio_channel_with_state(
            loop_idx,
            chunk_size,
            mode,
            Arc::new(AudioChannelStateMirror::default()),
        )
    }

    pub fn add_audio_channel_with_state(
        &mut self,
        loop_idx: usize,
        chunk_size: usize,
        mode: ChannelMode,
        state: Arc<AudioChannelStateMirror>,
    ) -> Result<usize, SessionError> {
        self.add_audio_channel_with_state_and_snapshots(loop_idx, chunk_size, mode, state, None)
    }

    pub fn add_audio_channel_with_bounded_capacity(
        &mut self,
        loop_idx: usize,
        chunk_size: usize,
        capacity: usize,
        mode: ChannelMode,
    ) -> Result<usize, SessionError> {
        let loop_ = self
            .loops
            .get_mut(loop_idx)
            .ok_or(SessionError::NoSuchLoop(loop_idx))?;
        let channel_idx = loop_.add_audio_channel_with_bounded_capacity(chunk_size, capacity, mode);
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

    pub fn add_audio_channel_with_bounded_capacity_unprepared(
        &mut self,
        loop_idx: usize,
        chunk_size: usize,
        capacity: usize,
        mode: ChannelMode,
    ) -> Result<usize, SessionError> {
        let loop_ = self
            .loops
            .get_mut(loop_idx)
            .ok_or(SessionError::NoSuchLoop(loop_idx))?;
        let channel_idx =
            loop_.add_audio_channel_with_bounded_capacity_unprepared(chunk_size, capacity, mode);
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

    pub fn add_audio_channel_with_state_and_snapshots(
        &mut self,
        loop_idx: usize,
        chunk_size: usize,
        mode: ChannelMode,
        state: Arc<AudioChannelStateMirror>,
        snapshots: Option<crate::content_snapshot::AudioProcessSnapshotWriter>,
    ) -> Result<usize, SessionError> {
        let l = self
            .loops
            .get_mut(loop_idx)
            .ok_or(SessionError::NoSuchLoop(loop_idx))?;
        let channel_idx =
            l.add_audio_channel_with_state_and_snapshots(chunk_size, mode, state, snapshots);
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
        self.add_midi_channel_with_state(
            loop_idx,
            capacity_elems,
            mode,
            Arc::new(MidiChannelStateMirror::default()),
        )
    }

    pub fn add_midi_channel_with_state(
        &mut self,
        loop_idx: usize,
        capacity_elems: usize,
        mode: ChannelMode,
        state: Arc<MidiChannelStateMirror>,
    ) -> Result<usize, SessionError> {
        self.add_midi_channel_with_state_and_snapshots(loop_idx, capacity_elems, mode, state, None)
    }

    pub fn add_midi_channel_with_state_and_snapshots(
        &mut self,
        loop_idx: usize,
        capacity_elems: usize,
        mode: ChannelMode,
        state: Arc<MidiChannelStateMirror>,
        snapshots: Option<crate::content_snapshot::MidiProcessSnapshotWriter>,
    ) -> Result<usize, SessionError> {
        let l = self
            .loops
            .get_mut(loop_idx)
            .ok_or(SessionError::NoSuchLoop(loop_idx))?;
        let channel_idx =
            l.add_midi_channel_with_state_and_snapshots(capacity_elems, mode, state, snapshots);
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

    pub fn disconnect_channel_port(
        &mut self,
        channel: usize,
        port: usize,
    ) -> Result<(), SessionError> {
        let mapping = self
            .channels
            .get_mut(channel)
            .ok_or(SessionError::NoSuchChannel(channel))?;
        let mut changed = false;
        if mapping.input_port == Some(port) {
            mapping.input_port = None;
            changed = true;
        }
        if mapping.output_port == Some(port) {
            mapping.output_port = None;
            changed = true;
        }
        if changed {
            self.note_graph_change();
        }
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
        self.loop_live[loop_idx] = false;
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
        for processor in &mut self.processors {
            processor
                .audio_inputs
                .retain(|&candidate| candidate != port);
            processor
                .audio_outputs
                .retain(|&candidate| candidate != port);
            if let Some(index) = processor
                .midi_inputs
                .iter()
                .position(|&candidate| candidate == port)
            {
                processor.midi_inputs.remove(index);
                processor.midi_staging.remove(index);
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
        let previous = self.sync_sources[loop_idx];
        self.sync_sources[loop_idx] = source;
        if !self.composite_timeline.is_empty() {
            if let Err(error) = self
                .composite_timeline
                .validate_primitive_sync_sources(&self.sync_sources)
            {
                self.sync_sources[loop_idx] = previous;
                return Err(error.into());
            }
        }
        // A loop with no sync source transitions immediately rather than waiting
        // for a trigger, so this changes behaviour and not just wiring.
        self.loops[loop_idx].set_sync_source(source.map(|_| SyncSourceState::default()));
        Ok(())
    }

    pub fn sync_source_of(&self, loop_idx: usize) -> Option<usize> {
        self.sync_sources.get(loop_idx).copied().flatten()
    }

    pub fn set_test_fx_active(&mut self, title: impl Into<String>, active: bool) {
        let title = title.into();
        if let Some(route) = self
            .processors
            .iter_mut()
            .find(|route| route.title == title)
        {
            route.active = active;
        } else {
            self.processors.push(ProcessorRoute {
                title,
                backend: ProcessorBackend::Test2x2x1,
                active,
                audio_inputs: Vec::new(),
                audio_outputs: Vec::new(),
                midi_inputs: Vec::new(),
                midi_staging: Vec::new(),
            });
            self.note_graph_change();
        }
    }

    pub fn set_external_processor(&mut self, title: impl Into<String>) {
        let title = title.into();
        if self.processors.iter().any(|route| route.title == title) {
            return;
        }
        self.processors.push(ProcessorRoute {
            title,
            backend: ProcessorBackend::External,
            active: true,
            audio_inputs: Vec::new(),
            audio_outputs: Vec::new(),
            midi_inputs: Vec::new(),
            midi_staging: Vec::new(),
        });
        self.note_graph_change();
    }

    pub fn set_processor_ports(
        &mut self,
        title: &str,
        audio_inputs: Vec<usize>,
        audio_outputs: Vec<usize>,
        midi_inputs: Vec<usize>,
    ) -> Result<(), SessionError> {
        for &port in audio_inputs.iter().chain(&audio_outputs) {
            if !self.ports.get(port).is_some_and(Port::is_audio) {
                return Err(SessionError::NoSuchPort(port));
            }
        }
        for &port in &midi_inputs {
            if !self
                .ports
                .get(port)
                .is_some_and(|port| port.midi().is_some())
            {
                return Err(SessionError::NoSuchPort(port));
            }
        }
        let route = self
            .processors
            .iter_mut()
            .find(|route| route.title == title)
            .ok_or(SessionError::NoSuchProcessor)?;
        route.audio_inputs = audio_inputs;
        route.audio_outputs = audio_outputs;
        route.midi_inputs = midi_inputs;
        route.midi_staging.clear();
        route.midi_staging.resize_with(route.midi_inputs.len(), || {
            Vec::with_capacity(MAX_MIDI_EVENTS_PER_BLOCK)
        });
        self.note_graph_change();
        Ok(())
    }

    pub fn set_tiny_synth_fx_processor(
        &mut self,
        title: impl Into<String>,
        processor: TinySynthFxProcessor,
    ) -> Option<TinySynthFxProcessor> {
        let title = title.into();
        let displaced = if let Some(route) = self
            .processors
            .iter_mut()
            .find(|route| route.title == title)
        {
            match std::mem::replace(&mut route.backend, ProcessorBackend::Tiny(processor)) {
                ProcessorBackend::Tiny(previous) => Some(previous),
                _ => None,
            }
        } else {
            self.processors.push(ProcessorRoute {
                title,
                backend: ProcessorBackend::Tiny(processor),
                active: false,
                audio_inputs: Vec::new(),
                audio_outputs: Vec::new(),
                midi_inputs: Vec::new(),
                midi_staging: Vec::new(),
            });
            None
        };
        self.note_graph_change();
        displaced
    }

    pub fn set_tiny_synth_fx_active(&mut self, title: &str, active: bool) {
        if let Some(route) = self
            .processors
            .iter_mut()
            .find(|route| route.title == title)
        {
            route.active = active;
        }
    }

    pub fn tiny_synth_fx_processor_mut(
        &mut self,
        title: &str,
    ) -> Option<&mut TinySynthFxProcessor> {
        self.processors
            .iter_mut()
            .find(|route| route.title == title)
            .and_then(|route| match &mut route.backend {
                ProcessorBackend::Tiny(processor) => Some(processor),
                _ => None,
            })
    }

    #[cfg(feature = "lv2")]
    pub fn set_carla_fx_host(&mut self, title: impl Into<String>, host: Box<dyn CarlaProcessor>) {
        let title = title.into();
        if let Some(route) = self
            .processors
            .iter_mut()
            .find(|route| route.title == title)
        {
            route.backend = ProcessorBackend::Carla(host);
        } else {
            self.processors.push(ProcessorRoute {
                title,
                backend: ProcessorBackend::Carla(host),
                active: true,
                audio_inputs: Vec::new(),
                audio_outputs: Vec::new(),
                midi_inputs: Vec::new(),
                midi_staging: Vec::new(),
            });
        }
        self.note_graph_change();
    }

    pub fn describe_audio_ringbuffer_adoption(
        &mut self,
        requests: &[AudioRingbufferAdoption],
    ) -> Result<AudioRingbufferAdoptionShape, SessionError> {
        if requests.len() > MAX_AUDIO_RINGBUFFER_ADOPTIONS {
            return Err(SessionError::AudioRingbufferAdoptionCapacity);
        }
        self.refresh_sync_snapshots();
        let mut shape = AudioRingbufferAdoptionShape {
            channels: [None; MAX_AUDIO_RINGBUFFER_ADOPTION_CHANNELS],
            n_channels: 0,
        };
        for (index, request) in requests.iter().enumerate() {
            if request.loop_idx >= self.loops.len() {
                return Err(SessionError::NoSuchLoop(request.loop_idx));
            }
            if requests[..index]
                .iter()
                .any(|previous| previous.loop_idx == request.loop_idx)
            {
                return Err(SessionError::AudioRingbufferAdoptionCapacity);
            }
            let sync = self.loops[request.loop_idx].sync_source();
            let cycle_len = sync.map(|state| state.length).unwrap_or(0);
            let sync_pos = sync.map(|state| state.position).unwrap_or(0);
            for mapping in self.channels.iter().filter(|mapping| {
                mapping.loop_idx == request.loop_idx && mapping.kind == ChannelKind::Audio
            }) {
                if shape.n_channels >= MAX_AUDIO_RINGBUFFER_ADOPTION_CHANNELS {
                    return Err(SessionError::AudioRingbufferAdoptionCapacity);
                }
                let ring_capacity = mapping
                    .input_port
                    .and_then(|port| self.ports.get(port))
                    .and_then(Port::audio)
                    .map(|port| port.ringbuffer_capacity())
                    .unwrap_or(0);
                let wanted = adoption_window(request, cycle_len, sync_pos, ring_capacity).0;
                let channel = self.loops[request.loop_idx]
                    .audio_channel(mapping.channel_idx)
                    .ok_or(SessionError::NoSuchChannel(mapping.channel_idx))?;
                shape.channels[shape.n_channels] = Some(AudioRingbufferAdoptionChannelShape {
                    loop_idx: request.loop_idx,
                    channel_idx: mapping.channel_idx,
                    chunk_size: channel.chunk_size(),
                    capacity: ring_capacity.max(wanted),
                });
                shape.n_channels += 1;
            }
        }
        Ok(shape)
    }

    pub fn prepare_audio_ringbuffers_prepared(
        &mut self,
        requests: &[AudioRingbufferAdoption],
        prepared: &mut [PreparedAudioRingbufferAdoptionChannel],
    ) -> Result<(), SessionError> {
        let shape = self.describe_audio_ringbuffer_adoption(requests)?;
        if prepared.len() != shape.n_channels {
            return Err(SessionError::AudioRingbufferAdoptionCapacity);
        }
        for (slot, expected) in prepared.iter_mut().zip(shape.channels()) {
            if slot.loop_idx != expected.loop_idx
                || slot.channel_idx != expected.channel_idx
                || slot.data.capacity() < expected.capacity
            {
                return Err(SessionError::AudioRingbufferAdoptionCapacity);
            }
            let request = requests
                .iter()
                .find(|request| request.loop_idx == slot.loop_idx)
                .expect("prepared adoption target was described");
            let sync = self.loops[request.loop_idx].sync_source();
            let cycle_len = sync.map(|state| state.length).unwrap_or(0);
            let sync_pos = sync.map(|state| state.position).unwrap_or(0);
            let mapping = self
                .channels
                .iter()
                .find(|mapping| {
                    mapping.loop_idx == slot.loop_idx
                        && mapping.kind == ChannelKind::Audio
                        && mapping.channel_idx == slot.channel_idx
                })
                .expect("prepared adoption channel was described");
            let source = mapping
                .input_port
                .and_then(|port| self.ports.get(port))
                .and_then(Port::audio);
            let data_len = source.map(|port| port.ringbuffer_n_samples()).unwrap_or(0);
            let (wanted, start, end) = adoption_window(request, cycle_len, sync_pos, data_len);
            slot.data.begin_load(wanted);
            let mut offset = 0;
            if let Some(source) = source {
                source.visit_ringbuffer_range(start, end, |samples| {
                    slot.data.write(offset, samples);
                    offset += samples.len();
                });
            }
        }
        Ok(())
    }

    pub fn commit_audio_ringbuffers_prepared_with_snapshots(
        &mut self,
        requests: &[AudioRingbufferAdoption],
        prepared: &mut [PreparedAudioRingbufferAdoptionChannel],
        snapshots: &[crate::content_snapshot::PreparedAudioSnapshot],
    ) -> Result<(), SessionError> {
        if prepared.len() != snapshots.len() {
            return Err(SessionError::AudioRingbufferAdoptionCapacity);
        }
        for (slot, snapshot) in prepared.iter_mut().zip(snapshots.iter().copied()) {
            self.loops[slot.loop_idx]
                .audio_channel_mut(slot.channel_idx)
                .ok_or(SessionError::NoSuchChannel(slot.channel_idx))?
                .commit_prepared_data_and_snapshot(&mut slot.data, snapshot);
        }
        self.apply_audio_ringbuffer_adoption_states(requests);
        Ok(())
    }

    pub fn adopt_audio_ringbuffers_prepared(
        &mut self,
        requests: &[AudioRingbufferAdoption],
        prepared: &mut [PreparedAudioRingbufferAdoptionChannel],
    ) -> Result<(), SessionError> {
        self.adopt_audio_ringbuffers_prepared_inner(requests, prepared, None)
    }

    /// The application backend variant also returns preallocated copies for control-thread
    /// state-mirror publication. Filling the copies is bounded and allocation-free.
    pub fn adopt_audio_ringbuffers_prepared_with_copies(
        &mut self,
        requests: &[AudioRingbufferAdoption],
        prepared: &mut [PreparedAudioRingbufferAdoptionChannel],
        copies: &mut [Vec<f32>],
    ) -> Result<(), SessionError> {
        self.adopt_audio_ringbuffers_prepared_inner(requests, prepared, Some(copies))
    }

    fn adopt_audio_ringbuffers_prepared_inner(
        &mut self,
        requests: &[AudioRingbufferAdoption],
        prepared: &mut [PreparedAudioRingbufferAdoptionChannel],
        mut copies: Option<&mut [Vec<f32>]>,
    ) -> Result<(), SessionError> {
        if let Some(copies) = copies.as_ref() {
            if copies.len() != prepared.len()
                || copies
                    .iter()
                    .zip(prepared.iter())
                    .any(|(copy, slot)| copy.capacity() < slot.data.capacity())
            {
                return Err(SessionError::AudioRingbufferAdoptionCapacity);
            }
        }
        self.prepare_audio_ringbuffers_prepared(requests, prepared)?;
        if let Some(copies) = copies.as_deref_mut() {
            for (slot, copy) in prepared.iter().zip(copies.iter_mut()) {
                slot.data.copy_to_preallocated(copy);
            }
        }

        for slot in prepared {
            self.loops[slot.loop_idx]
                .audio_channel_mut(slot.channel_idx)
                .expect("prepared adoption channel was validated")
                .commit_prepared_data(&mut slot.data);
        }
        self.apply_audio_ringbuffer_adoption_states(requests);
        Ok(())
    }

    fn apply_audio_ringbuffer_adoption_states(&mut self, requests: &[AudioRingbufferAdoption]) {
        for request in requests {
            let sync = self.loops[request.loop_idx].sync_source();
            let cycle_len = sync.map(|state| state.length).unwrap_or(0);
            let sync_pos = sync.map(|state| state.position).unwrap_or(0);
            let data_len = self
                .channels
                .iter()
                .filter(|mapping| {
                    mapping.loop_idx == request.loop_idx && mapping.kind == ChannelKind::Audio
                })
                .filter_map(|mapping| {
                    self.loops[request.loop_idx]
                        .audio_channel(mapping.channel_idx)
                        .map(|channel| channel.length())
                })
                .max()
                .unwrap_or(0);
            let adopted_len = data_len.min(u32::MAX as usize) as u32;
            let go_cycle = request.go_to_cycle.unwrap_or(0).max(0) as u32;
            let loop_ = &mut self.loops[request.loop_idx];
            match request.go_to_mode {
                LoopMode::Recording => {
                    loop_.set_mode(LoopMode::Recording);
                    loop_.set_length(adopted_len);
                }
                LoopMode::Unknown => {
                    loop_.set_length(adopted_len);
                    loop_.set_mode(LoopMode::Stopped);
                }
                mode => {
                    loop_.set_length(adopted_len);
                    loop_.set_mode(mode);
                    if cycle_len > 0 {
                        loop_.set_position(
                            go_cycle.saturating_mul(cycle_len).saturating_add(sync_pos),
                        );
                    }
                }
            }
        }
    }

    /// Retroactively fills loops' audio channels from their input ports' rolling
    /// layers and commits all requested post-grab states in one bounded transaction.
    pub fn adopt_audio_ringbuffers(
        &mut self,
        requests: &[AudioRingbufferAdoption],
    ) -> Result<(), SessionError> {
        if requests.len() > MAX_AUDIO_RINGBUFFER_ADOPTIONS {
            return Err(SessionError::AudioRingbufferAdoptionCapacity);
        }
        for (index, request) in requests.iter().enumerate() {
            if request.loop_idx >= self.loops.len() {
                return Err(SessionError::NoSuchLoop(request.loop_idx));
            }
            if requests[..index]
                .iter()
                .any(|previous| previous.loop_idx == request.loop_idx)
            {
                return Err(SessionError::AudioRingbufferAdoptionCapacity);
            }
        }

        self.refresh_sync_snapshots();
        for request in requests {
            let sync = self.loops[request.loop_idx].sync_source();
            let cycle_len = sync.map(|state| state.length).unwrap_or(0);
            let sync_pos = sync.map(|state| state.position).unwrap_or(0);
            for mapping in self.channels.iter().filter(|mapping| {
                mapping.loop_idx == request.loop_idx && mapping.kind == ChannelKind::Audio
            }) {
                let data_len = mapping
                    .input_port
                    .and_then(|port| self.ports.get(port))
                    .and_then(Port::audio)
                    .map(|port| port.ringbuffer_n_samples())
                    .unwrap_or(0);
                let wanted_len = adoption_window(request, cycle_len, sync_pos, data_len).0;
                let channel = self.loops[request.loop_idx]
                    .audio_channel(mapping.channel_idx)
                    .ok_or(SessionError::NoSuchChannel(mapping.channel_idx))?;
                if !channel.can_load_without_allocation(wanted_len) {
                    return Err(SessionError::AudioRingbufferAdoptionCapacity);
                }
            }
        }

        let channels = &self.channels;
        let ports = &self.ports;
        let loops = &mut self.loops;
        for request in requests {
            let sync = loops[request.loop_idx].sync_source();
            let cycle_len = sync.map(|state| state.length).unwrap_or(0);
            let sync_pos = sync.map(|state| state.position).unwrap_or(0);
            let go_cycle = request.go_to_cycle.unwrap_or(0).max(0) as u32;
            let mut adopted_len = 0usize;

            for mapping in channels.iter().filter(|mapping| {
                mapping.loop_idx == request.loop_idx && mapping.kind == ChannelKind::Audio
            }) {
                let source = mapping
                    .input_port
                    .and_then(|port| ports.get(port))
                    .and_then(Port::audio);
                let data_len = source.map(|port| port.ringbuffer_n_samples()).unwrap_or(0);
                let (wanted_len, start, end) =
                    adoption_window(request, cycle_len, sync_pos, data_len);
                adopted_len = adopted_len.max(wanted_len);
                let channel = loops[request.loop_idx]
                    .audio_channel_mut(mapping.channel_idx)
                    .expect("adoption mappings were validated");
                channel.begin_bounded_load(wanted_len);
                let mut destination_offset = 0;
                if let Some(source) = source {
                    source.visit_ringbuffer_range(start, end, |samples| {
                        channel.write_bounded_load(destination_offset, samples);
                        destination_offset += samples.len();
                    });
                }
                channel.finish_bounded_load();
            }

            let loop_ = &mut loops[request.loop_idx];
            let adopted_len = adopted_len.min(u32::MAX as usize) as u32;
            match request.go_to_mode {
                LoopMode::Recording => {
                    loop_.set_mode(LoopMode::Recording);
                    loop_.set_length(adopted_len);
                }
                LoopMode::Unknown => {
                    loop_.set_length(adopted_len);
                    loop_.set_mode(LoopMode::Stopped);
                }
                mode => {
                    loop_.set_length(adopted_len);
                    loop_.set_mode(mode);
                    if cycle_len > 0 {
                        loop_.set_position(
                            go_cycle.saturating_mul(cycle_len).saturating_add(sync_pos),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub fn adopt_audio_ringbuffers_for_loop(
        &mut self,
        loop_idx: usize,
        reverse_start_cycle: Option<i32>,
        cycles_length: Option<i32>,
        go_to_cycle: Option<i32>,
        go_to_mode: LoopMode,
    ) -> Result<(), SessionError> {
        self.adopt_audio_ringbuffers(&[AudioRingbufferAdoption {
            loop_idx,
            reverse_start_cycle,
            cycles_length,
            go_to_cycle,
            go_to_mode,
        }])
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
            // callback. That is what makes sync work: all loops advance to the
            // same position before any trigger is resolved, so a dependent sees
            // its source's trigger in the same sub-block.
            processors: self
                .processors
                .iter()
                .map(|processor| ProcessorDesc {
                    name: processor.title.clone(),
                    input_ports: processor
                        .audio_inputs
                        .iter()
                        .chain(&processor.midi_inputs)
                        .copied()
                        .map(PortIdx)
                        .collect(),
                    output_ports: processor
                        .audio_outputs
                        .iter()
                        .copied()
                        .map(PortIdx)
                        .collect(),
                })
                .collect(),
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

    /// Everything a schedule is built from, detached from the session.
    ///
    /// Taken under whatever lock guards the session, then handed to [`build_schedule`] with
    /// no lock held: this half needs `&self` and is cheap, that half needs nothing and is
    /// expensive. Splitting them is what keeps the topological sort out of the critical
    /// section the audio thread contends for every cycle.
    pub fn describe_topology(&self) -> Topology {
        // Per-loop MIDI channel order, so a cycle never has to search for it.
        let mut midi_by_loop: Vec<Vec<usize>> = vec![Vec::new(); self.loops.len()];
        for (i, m) in self.channels.iter().enumerate() {
            if m.kind == ChannelKind::Midi {
                midi_by_loop[m.loop_idx].push(i);
            }
        }
        for v in midi_by_loop.iter_mut() {
            v.sort_by_key(|&i| self.channels[i].channel_idx);
        }

        Topology {
            graph: self.describe(),
            midi_by_loop,
            n_loops: self.loops.len(),
            graph_id: self.graph_request_id,
        }
    }

    /// Installs a prebuilt schedule, handing back the one it displaced.
    ///
    /// Returned rather than dropped, because freeing is as forbidden on the audio thread as
    /// allocating: whoever installs decides where the old schedule dies. Today that is the
    /// scheduler thread, which drops it immediately.
    ///
    /// Safe to install a schedule built from an older topology, which is the whole reason the
    /// build can happen without the lock. Nothing this schedule indexes can have gone away:
    /// every removal in this module is a tombstone -- `remove_port`, `remove_loop` and
    /// `remove_channel` disconnect and disable but never shrink an arena, precisely so live
    /// indices keep pointing at the same object. A topology change made during the build is
    /// therefore missing from the schedule but not misdescribed by it, and `graph_applied_id`
    /// below records that so the next rebuild is armed.
    pub fn install_schedule(&mut self, mut prepared: PreparedSchedule) -> PreparedSchedule {
        // From what the build actually saw, never from the current request id: a change that
        // landed while the schedule was being built cannot be in it, and claiming otherwise
        // would leave the session stale while reporting itself current -- which presents as
        // routing that silently never updates.
        let covered = prepared.for_graph_id;
        prepared.for_graph_id = self.graph_applied_id;

        // Schedule installation is the control-path opportunity to size optional FX scratch.
        // Doing it here keeps the first realtime cycle from discovering the effect's buffer.
        for port in &mut self.ports {
            if let Some(audio) = port.audio_mut() {
                audio.reserve_processing(self.buffer_size as usize);
            }
            if let Some(external) = port.as_external_mut() {
                let _ = external.buffer(self.buffer_size as usize);
            }
        }
        self.scratch.resize(self.buffer_size as usize, 0.0);
        self.out_scratch.resize(self.buffer_size as usize, 0.0);

        std::mem::swap(&mut self.specs, &mut prepared.specs);
        std::mem::swap(&mut self.node_map, &mut prepared.node_map);
        std::mem::swap(&mut self.schedule, &mut prepared.schedule);
        std::mem::swap(&mut self.node_actions, &mut prepared.node_actions);
        std::mem::swap(
            &mut self.midi_mappings_by_loop,
            &mut prepared.midi_mappings_by_loop,
        );
        std::mem::swap(&mut self.midi_in_scratch, &mut prepared.midi_in_scratch);
        std::mem::swap(&mut self.midi_out_scratch, &mut prepared.midi_out_scratch);
        std::mem::swap(&mut self.loop_group, &mut prepared.loop_group);
        std::mem::swap(&mut self.boundary_triggers, &mut prepared.boundary_triggers);
        std::mem::swap(
            &mut self.boundary_delivered_triggers,
            &mut prepared.boundary_delivered_triggers,
        );
        std::mem::swap(
            &mut self.boundary_natural_intents,
            &mut prepared.boundary_natural_intents,
        );

        self.graph_applied_id = covered;
        prepared
    }

    /// Recomputes the schedule for the current topology, in one step.
    ///
    /// The composition of [`Self::describe_topology`], [`build_schedule`] and
    /// [`Self::install_schedule`], for a caller holding the session exclusively and with no
    /// reason to let go of it: construction, tests, and anything not competing with an audio
    /// thread. A caller that *is* competing should run the three separately.
    pub fn apply_graph_changes(&mut self) -> Result<(), SessionError> {
        let displaced = self.install_schedule(build_schedule(self.describe_topology())?);
        drop(displaced);
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
    /// `process_loops` did and is still owed.
    ///
    /// Infallible on purpose. It used to return `Err(GraphOutOfDate)`, which every caller
    /// discarded with `let _ =` -- so a permanently stale graph silenced the session with
    /// nothing reported anywhere. There is now no error to drop: a stale cycle runs and is
    /// counted in [`Self::n_stale_cycles`].
    pub fn process(&mut self, n_frames: usize) {
        self.profiler
            .set_enabled(shoop_tracing::is_tracing_requested());
        let _session_span = shoop_tracing::realtime_span!("engine.rt.session", value = n_frames);
        // A stale graph runs the last-applied schedule rather than refusing the cycle.
        //
        // Refusing meant a single un-applied connection silenced the whole session until
        // someone noticed, and it made deferring a reschedule impossible: any delay
        // between a topology change and its apply was a gap of dropped cycles.
        //
        // Running the old schedule is sound because the arenas are append-only --
        // `remove_port`, `remove_channel` and `remove_loop` clear a slot but never shrink
        // `ports`/`channels`/`loops` -- so indices captured by an older schedule stay
        // valid. Routing is read live from `port_connections` in `propagate_port`, so a
        // disconnect still takes effect on the next cycle; only nodes added since the last
        // apply are missing, and those are genuinely not wired up yet.
        if !self.graph_up_to_date() {
            self.n_stale_cycles = self.n_stale_cycles.saturating_add(1);
        }
        self.n_sub_blocks_last_cycle = 0;
        {
            let _span = shoop_tracing::realtime_span_detail!("engine.rt.composites.begin");
            self.composite_timeline.begin_callback();
        }
        let steps = std::mem::take(&mut self.schedule);
        for step in &steps {
            // Loops in one step are co-processed, so they are gathered and
            // advanced together rather than one after another.
            self.loop_group.clear();
            for node in step {
                match self.node_actions[node.0] {
                    NodeAction::PortPrepare(i) => {
                        let _span = shoop_tracing::realtime_span_detail!(
                            "engine.rt.ports.prepare",
                            value = i
                        );
                        let began = self.profiler.begin();
                        self.ports[i].prepare(n_frames);
                        self.profiler.finish(Stage::PortPrepare, began);
                    }
                    NodeAction::PortProcess(i) => {
                        let _span = shoop_tracing::realtime_span_detail!(
                            "engine.rt.ports.process",
                            value = i
                        );
                        let began = self.profiler.begin();
                        self.ports[i].process(n_frames);
                        {
                            let _routing_span = shoop_tracing::realtime_span_detail!(
                                "engine.rt.routing.external",
                                value = i
                            );
                            self.propagate_port(i, n_frames);
                        }
                        self.profiler.finish(Stage::PortProcess, began);
                    }
                    NodeAction::ProcessorProcess(i) => {
                        let began = self.profiler.begin();
                        self.process_processor(i, n_frames);
                        self.profiler.finish(Stage::ProcessorProcess, began);
                    }
                    NodeAction::LoopProcess(i) => self.loop_group.push(i),
                    NodeAction::ChannelPrepare(i) => {
                        let _span = shoop_tracing::realtime_span_detail!(
                            "engine.rt.channels.prepare",
                            value = i
                        );
                        let began = self.profiler.begin();
                        self.channel_prepare(i, n_frames);
                        self.profiler.finish(Stage::ChannelPrepare, began);
                    }
                    NodeAction::ChannelProcess(i) => {
                        let _span = shoop_tracing::realtime_span_detail!(
                            "engine.rt.channels.process",
                            value = i
                        );
                        let began = self.profiler.begin();
                        self.channel_finalize(i, n_frames);
                        self.profiler.finish(Stage::ChannelProcess, began);
                    }
                    NodeAction::None => {}
                }
            }
            if !self.loop_group.is_empty() {
                {
                    let _span = shoop_tracing::realtime_span!(
                        "engine.rt.loops",
                        value = self.loop_group.len()
                    );
                    let began = self.profiler.begin();
                    self.process_loop_group(n_frames);
                    self.profiler.finish(Stage::LoopProcess, began);
                }
                {
                    let _span = shoop_tracing::realtime_span_detail!("engine.rt.midi.playback");
                    self.synth_prerecorded_midi_playback(n_frames);
                }
            }
        }
        {
            let _span = shoop_tracing::realtime_span!("engine.rt.state_publication");
            self.publish_composite_anticipated_transitions();
        }
        self.profiler.end_cycle();
        self.schedule = steps;
    }

    /// Runs one processor after its routed inputs and before its output ports.
    fn process_processor(&mut self, processor_index: usize, n_frames: usize) {
        let _span =
            shoop_tracing::realtime_span!("engine.rt.fx.processor", value = processor_index);
        let mut processors = std::mem::take(&mut self.processors);
        let Some(route) = processors.get_mut(processor_index) else {
            self.processors = processors;
            return;
        };

        for (staging, &port_index) in route.midi_staging.iter_mut().zip(&route.midi_inputs) {
            staging.clear();
            staging.extend(
                self.ports[port_index]
                    .midi_events()
                    .iter()
                    .take(MAX_MIDI_EVENTS_PER_BLOCK)
                    .copied(),
            );
            if let Some(port) = self.ports[port_index].as_external_midi() {
                let remaining = MAX_MIDI_EVENTS_PER_BLOCK.saturating_sub(staging.len());
                staging.extend(port.outgoing().iter().take(remaining).copied());
            }
        }

        if route.active {
            match &mut route.backend {
                ProcessorBackend::External => {}
                ProcessorBackend::Test2x2x1 => {
                    for output_index in 0..route.audio_outputs.len() {
                        self.scratch[..n_frames].fill(0.0);
                        if let Some(&input) = route.audio_inputs.get(output_index) {
                            let input = self.ports[input].buffer(n_frames);
                            for (output, input) in self.scratch[..n_frames].iter_mut().zip(input) {
                                *output = *input * 0.5;
                            }
                        }
                        for event in route.midi_staging.iter().flatten() {
                            let time = event.time as usize;
                            if time < n_frames
                                && event.data().len() >= 3
                                && (event.data()[0] & 0xf0) == 0x90
                                && event.data()[2] > 0
                            {
                                self.scratch[time] += event.data()[2] as f32 / 255.0;
                            }
                        }
                        self.ports[route.audio_outputs[output_index]]
                            .buffer(n_frames)
                            .copy_from_slice(&self.scratch[..n_frames]);
                    }
                }
                ProcessorBackend::Tiny(processor) => {
                    if n_frames <= processor.max_frames() {
                        for input_index in 0..processor.logical_channel_count() {
                            let Some(destination) = processor.plane_mut(input_index, n_frames)
                            else {
                                continue;
                            };
                            if let Some(&port_index) = route.audio_inputs.get(input_index) {
                                destination
                                    .copy_from_slice(self.ports[port_index].buffer(n_frames));
                            } else {
                                destination.fill(0.0);
                            }
                        }
                        processor.clear_silent_plane(n_frames);
                        let events = route.midi_staging.first().map(Vec::as_slice).unwrap_or(&[]);
                        processor.process(n_frames, events);
                        for (output_index, &port_index) in route.audio_outputs.iter().enumerate() {
                            let Some(source) = processor.plane(output_index, n_frames) else {
                                continue;
                            };
                            self.ports[port_index]
                                .buffer(n_frames)
                                .copy_from_slice(source);
                        }
                    }
                }
                #[cfg(feature = "lv2")]
                ProcessorBackend::Carla(host) => {
                    if host.is_active() {
                        for (input_index, &port_index) in route.audio_inputs.iter().enumerate() {
                            if let Some(destination) = host.audio_input_mut(input_index) {
                                destination[..n_frames]
                                    .copy_from_slice(self.ports[port_index].buffer(n_frames));
                            }
                        }
                        for (midi_index, staging) in route.midi_staging.iter().enumerate() {
                            let mut events = [(0_u32, &[][..]); MAX_MIDI_EVENTS_PER_BLOCK];
                            for (destination, event) in events.iter_mut().zip(staging) {
                                *destination = (
                                    event.time.min(n_frames.saturating_sub(1) as u32),
                                    event.data(),
                                );
                            }
                            let _ = host.set_midi_input_events(
                                midi_index,
                                &events[..staging.len().min(MAX_MIDI_EVENTS_PER_BLOCK)],
                            );
                        }
                        let _ = host.process(n_frames);
                        for (output_index, &port_index) in route.audio_outputs.iter().enumerate() {
                            if let Some(source) = host.audio_output(output_index) {
                                self.ports[port_index]
                                    .buffer(n_frames)
                                    .copy_from_slice(&source[..n_frames]);
                            }
                        }
                    }
                }
            }
        }
        self.processors = processors;
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
                if !loop_mode_to_channel_process_flags(l.mode(), ch.mode())
                    .contains(ProcessFlags::PLAYBACK)
                {
                    continue;
                }
                if ch.contents_were_loaded() {
                    continue;
                }
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
            .is_some_and(|audio| audio.passthrough_muted())
        {
            return;
        }

        if self.ports[from].midi().is_some() {
            let source_muted = self.ports[from]
                .midi()
                .is_some_and(|midi| midi.passthrough_muted() || midi.muted());
            let cleanup = self.ports[from]
                .midi_mut()
                .and_then(|midi| midi.take_passthrough_cleanup());
            let conns = std::mem::take(&mut self.port_connections);
            if let Some(targets) = conns.get(&from) {
                if let Some(messages) = cleanup.as_ref() {
                    for &to in targets {
                        if to == from || to >= self.ports.len() {
                            continue;
                        }
                        for message in messages {
                            self.ports[to].write_midi(*message);
                        }
                    }
                }
                if !source_muted && !targets.is_empty() {
                    let events = self.ports[from].midi_events().to_vec();
                    let mut delivered = false;
                    for &to in targets {
                        if to == from || to >= self.ports.len() {
                            continue;
                        }
                        if self.ports[to].midi().is_some_and(|midi| midi.muted()) {
                            continue;
                        }
                        for message in &events {
                            self.ports[to].write_midi(*message);
                        }
                        delivered = true;
                    }
                    if delivered {
                        if let Some(midi) = self.ports[from].midi_mut() {
                            midi.record_passthrough(&events);
                        }
                    }
                }
            }
            if let Some(messages) = cleanup {
                if let Some(midi) = self.ports[from].midi_mut() {
                    midi.finish_passthrough_cleanup(messages);
                }
            }
            self.port_connections = conns;
            return;
        }

        let conns = std::mem::take(&mut self.port_connections);
        if let Some(targets) = conns.get(&from) {
            if !targets.is_empty() {
                if self.scratch.len() < n_frames {
                    self.scratch.resize(n_frames, 0.0);
                }
                {
                    let src = self.ports[from].buffer(n_frames);
                    let n = n_frames.min(src.len());
                    self.scratch[..n].copy_from_slice(&src[..n]);
                    for sample in &mut self.scratch[n..n_frames] {
                        *sample = 0.0;
                    }
                }
                for &to in targets {
                    if to == from || to >= self.ports.len() {
                        continue;
                    }
                    let dst = self.ports[to].buffer(n_frames);
                    let n = n_frames.min(dst.len());
                    for (destination, sample) in dst[..n].iter_mut().zip(&self.scratch[..n]) {
                        *destination += *sample;
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
    /// loop still needs splitting when its end falls inside the buffer.
    fn process_loop_group(&mut self, n_frames: usize) {
        let _composite_span = shoop_tracing::realtime_span_detail!("engine.rt.composites.timeline");
        if self.composite_timeline.fault().fault != CompositeTimelineFault::None {
            return;
        }
        let mut remaining = n_frames;
        let mut sub_blocks = 0u32;

        while remaining > 0 {
            sub_blocks += 1;
            self.n_sub_blocks_last_cycle += 1;
            if sub_blocks > MAX_SUB_BLOCKS {
                self.n_stuck_cycles = self.n_stuck_cycles.saturating_add(1);
                self.composite_timeline.latch_sub_block_overflow();
                return;
            }

            self.refresh_sync_snapshots();

            let mut until = remaining;
            for gi in 0..self.loop_group.len() {
                let li = self.loop_group[gi];
                self.loops[li].resync_poi();
                if let Some(poi) = self.loops[li].next_poi() {
                    until = until.min(poi as usize);
                }
            }
            if let Some(control_poi) = self.composite_timeline.next_control_poi(remaining) {
                until = until.min(control_poi);
            }

            if until > 0 {
                for gi in 0..self.loop_group.len() {
                    self.advance_loop(self.loop_group[gi], until);
                }
                self.composite_timeline.advance_clock(until);
            }
            for gi in 0..self.loop_group.len() {
                self.loops[self.loop_group[gi]].handle_poi();
            }

            self.boundary_delivered_triggers.clear();
            let mut event_waves = 0usize;
            let mut first_wave = true;
            loop {
                let mut sync_waves = 0usize;
                loop {
                    self.refresh_sync_snapshots();
                    let mut changed = false;
                    for gi in 0..self.loop_group.len() {
                        let loop_idx = self.loop_group[gi];
                        let was_triggering =
                            self.loops[loop_idx].as_sync_source_state().triggering_now;
                        self.loops[loop_idx].handle_sync();
                        changed |= !was_triggering
                            && self.loops[loop_idx].as_sync_source_state().triggering_now;
                    }
                    if !changed {
                        break;
                    }
                    sync_waves += 1;
                    if sync_waves >= self.composite_timeline.max_event_waves() {
                        self.composite_timeline.latch_event_wave_overflow();
                        return;
                    }
                }

                self.boundary_triggers.clear();
                self.boundary_natural_intents.clear();
                for gi in 0..self.loop_group.len() {
                    let loop_idx = self.loop_group[gi];
                    let state = self.loops[loop_idx].as_sync_source_state();
                    if !state.triggering_now || !self.loop_live[loop_idx] {
                        continue;
                    }
                    let identity = LoopIdentity {
                        slot: loop_idx as u32,
                        generation: 1,
                        kind: LoopTargetKind::Basic,
                    };
                    if self.boundary_delivered_triggers.contains(&identity) {
                        continue;
                    }
                    self.boundary_triggers.push(identity);
                    self.boundary_delivered_triggers.push(identity);
                    self.boundary_natural_intents.push(BoundaryIntent {
                        target: identity,
                        action: BoundaryTargetAction::SetMode {
                            mode: state.mode,
                            offset_samples: u64::from(state.position),
                            retrigger: false,
                        },
                        origin: BoundaryIntentOrigin::Natural { source: identity },
                    });
                }

                if !first_wave && self.boundary_triggers.is_empty() {
                    break;
                }
                first_wave = false;
                event_waves += 1;
                if event_waves > self.composite_timeline.max_event_waves() {
                    self.composite_timeline.latch_event_wave_overflow();
                    return;
                }

                {
                    let Session {
                        composite_timeline,
                        loops,
                        loop_live,
                        boundary_triggers,
                        boundary_delivered_triggers,
                        boundary_natural_intents,
                        ..
                    } = self;
                    composite_timeline.align_sync_positions(|identity| {
                        if identity.kind != LoopTargetKind::Basic
                            || identity.generation != 1
                            || !loop_live
                                .get(identity.slot as usize)
                                .copied()
                                .unwrap_or(false)
                        {
                            return None;
                        }
                        loops
                            .get(identity.slot as usize)
                            .map(|loop_| u64::from(loop_.position()))
                    });
                    let trace = match composite_timeline.resolve_boundary(
                        boundary_triggers,
                        boundary_natural_intents,
                        |identity| {
                            identity.kind == LoopTargetKind::Basic
                                && identity.generation == 1
                                && loop_live
                                    .get(identity.slot as usize)
                                    .copied()
                                    .unwrap_or(false)
                        },
                    ) {
                        Ok(trace) => trace,
                        Err(_) => return,
                    };
                    for entry in trace {
                        if entry.target.kind != LoopTargetKind::Basic
                            || entry.target.generation != 1
                            || !loop_live
                                .get(entry.target.slot as usize)
                                .copied()
                                .unwrap_or(false)
                        {
                            continue;
                        }
                        let Some(loop_) = loops.get_mut(entry.target.slot as usize) else {
                            continue;
                        };
                        match entry.action {
                            BoundaryTargetAction::Stop => loop_.set_mode(LoopMode::Stopped),
                            BoundaryTargetAction::SetMode {
                                mode,
                                offset_samples,
                                retrigger,
                            } => {
                                let mode_changed = loop_.mode() != mode;
                                loop_.set_mode(mode);
                                if retrigger || mode_changed {
                                    let offset = u32::try_from(offset_samples).unwrap_or(u32::MAX);
                                    if matches!(
                                        mode,
                                        LoopMode::Recording | LoopMode::RecordingDryIntoWet
                                    ) {
                                        loop_.set_length(offset);
                                        loop_.set_position(0);
                                    } else {
                                        loop_.set_position(offset.min(loop_.length()));
                                    }
                                }
                                if loop_.as_sync_source_state().triggering_now {
                                    if !boundary_delivered_triggers.contains(&entry.target) {
                                        boundary_delivered_triggers.push(entry.target);
                                    }
                                }
                            }
                        }
                    }
                }
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

    /// Cycles that ran against a schedule older than the current topology.
    ///
    /// A few of these after a topology change are expected -- that is the reschedule
    /// window. A count that keeps climbing means nothing is applying the graph.
    pub fn n_stale_cycles(&self) -> u32 {
        self.n_stale_cycles
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
                    crate::realtime_allow_alloc_once!(
                        "AudioChannel temporary mirrored data publication",
                        || ch.finalize_process(&self.scratch[..n_frames], out)
                    );
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
        s.process(4);
    }

    #[test]
    fn adding_entities_invalidates_the_graph() {
        let mut s = Session::default();
        s.add_port(internal("p", 4));
        check!(!s.graph_up_to_date());
        // Stale is not fatal: the cycle runs the last-applied schedule and is counted.
        s.process(4);
        check!(s.n_stale_cycles() == 1);

        let_assert!(Ok(()) = s.apply_graph_changes());
        check!(s.graph_up_to_date());
        s.process(4);
        check!(s.n_stale_cycles() == 1);
    }

    /// The property that lets a stale cycle run safely: existing work keeps happening.
    #[test]
    fn a_stale_graph_keeps_running_the_previous_schedule() {
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

        s.process(4);
        check!(s.port_mut(output).unwrap().buffer(4).to_vec() == vec![1.0; 4]);

        // Dirty the graph without applying, as adding a track does.
        let _later = s.add_port(internal("added-later", 4));
        check!(!s.graph_up_to_date());

        s.process(4);

        // Still playing. Before this change the cycle was refused outright and the
        // session fell silent for good.
        check!(s.port_mut(output).unwrap().buffer(4).to_vec() == vec![1.0; 4]);
        check!(s.n_stale_cycles() == 1);
    }

    /// The reason the build can run without holding the session, in one assertion.
    ///
    /// `install_schedule` takes `graph_applied_id` from what the *build* saw, not from the
    /// session's current request id. Take that from the session instead and this is what
    /// breaks: the change made below is absent from the schedule, yet the session reports
    /// itself current, so nothing ever arms another rebuild and the new port stays unrouted
    /// forever -- silence with no stale-cycle count to point at it.
    #[test]
    fn a_change_arriving_during_a_build_leaves_the_graph_stale() {
        let mut s = Session::default();
        let_assert!(Ok(()) = s.apply_graph_changes());

        let topology = s.describe_topology();
        // After the description was taken, so no schedule built from it can contain it.
        let _added = s.add_port(internal("added-mid-build", 4));

        let_assert!(Ok(prepared) = build_schedule(topology));
        let displaced = s.install_schedule(prepared);
        drop(displaced);

        check!(
            !s.graph_up_to_date(),
            "a schedule that predates the change must not mark the graph current"
        );
        // And the follow-up rebuild does bring it current.
        let_assert!(Ok(()) = s.apply_graph_changes());
        check!(s.graph_up_to_date());
    }

    #[test]
    fn installing_a_schedule_built_from_the_current_topology_marks_it_current() {
        let mut s = Session::default();
        s.add_port(internal("p", 4));
        check!(!s.graph_up_to_date());

        let_assert!(Ok(prepared) = build_schedule(s.describe_topology()));
        drop(s.install_schedule(prepared));

        check!(s.graph_up_to_date());
    }

    /// A schedule built before a removal must still be safe to install and run.
    ///
    /// This is what the split rests on: between describing and installing, anything may have
    /// happened to the session. It is safe only because removals are tombstones -- they
    /// disconnect and disable but never shrink an arena -- so every index the older schedule
    /// holds still names the same object. Were any removal to compact its arena, this would
    /// index past the end or, worse, drive the wrong loop.
    #[test]
    fn a_schedule_built_before_a_removal_still_installs_and_runs() {
        let mut s = Session::default();
        let output = s.add_port(internal("out", 4));
        let doomed = s.add_port(internal("doomed", 4));
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

        // Describe first, then tear things out behind the build's back.
        let topology = s.describe_topology();
        let_assert!(Ok(()) = s.remove_port(doomed));
        let_assert!(Ok(()) = s.remove_loop(l));

        let_assert!(Ok(prepared) = build_schedule(topology));
        drop(s.install_schedule(prepared));

        // Runs against a schedule describing entities that have since been disabled, and
        // neither panics nor produces anything from the loop that was removed.
        s.process(4);
        check!(s.port_mut(output).unwrap().buffer(4).to_vec() == vec![0.0; 4]);
    }

    /// What a stale cycle does and does not pick up.
    ///
    /// A channel already in the schedule reads its `output_port` live, so it starts feeding
    /// a newly assigned port straight away. What waits for the reschedule is the *port's*
    /// own node -- its `prepare` (which clears the buffer each cycle) and its `process`
    /// (gain, muting, metering). So the routing lands early and the port-level processing
    /// lands on apply; neither state is broken, which is what makes deferral safe.
    #[test]
    fn a_port_added_mid_stream_is_fed_but_not_yet_processed() {
        let mut s = Session::default();
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.apply_graph_changes());
        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[2.0, 2.0, 2.0, 2.0]);
        s.loop_mut(l).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));

        let late = s.add_port(internal("late", 4));
        let_assert!(Ok(()) = s.connect_channel_output(c, late));
        s.port_mut(late).unwrap().audio_mut().unwrap().set_gain(0.5);

        s.process(4);
        // Fed immediately, but ungained: the port's own process node has not run.
        check!(s.port_mut(late).unwrap().buffer(4).to_vec() == vec![2.0; 4]);
        check!(s.n_stale_cycles() == 1);

        let_assert!(Ok(()) = s.apply_graph_changes());
        s.process(4);
        // Now the port is scheduled: cleared at the top of the cycle, gained at the end.
        check!(s.port_mut(late).unwrap().buffer(4).to_vec() == vec![1.0; 4]);
        check!(s.n_stale_cycles() == 1);
    }

    /// Pass-through routing is read live, so a disconnect does not wait for a reschedule.
    #[test]
    fn a_disconnect_takes_effect_before_the_graph_is_reapplied() {
        let mut s = Session::default();
        let from = s.add_port(internal("from", 4));
        let to = s.add_port(internal("to", 4));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_output(c, from));
        let_assert!(Ok(()) = s.connect_ports_internal(from, to));
        let_assert!(Ok(()) = s.apply_graph_changes());
        s.loop_mut(l)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[1.0, 1.0, 1.0, 1.0]);
        s.loop_mut(l).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));

        s.process(4);
        check!(s.port_mut(to).unwrap().buffer(4).to_vec() == vec![1.0; 4]);

        // Drop the pass-through wiring but do not reschedule.
        s.port_connections.remove(&from);
        s.note_graph_change();
        check!(!s.graph_up_to_date());

        s.process(4);
        check!(s.port_mut(to).unwrap().buffer(4).to_vec() == vec![0.0; 4]);
    }

    /// Indices held by an older schedule must stay valid, which is why removal keeps slots.
    #[test]
    fn removal_never_shrinks_the_arenas() {
        let mut s = Session::default();
        let p = s.add_port(internal("p", 4));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_audio_channel(l, 4, ChannelMode::Direct));
        let (ports, loops, channels) = (s.n_ports(), s.n_loops(), s.n_channels());

        let_assert!(Ok(()) = s.remove_channel(c, ChannelKind::Audio));
        let_assert!(Ok(()) = s.remove_port(p));
        let_assert!(Ok(()) = s.remove_loop(l));

        check!(s.n_ports() == ports);
        check!(s.n_loops() == loops);
        check!(s.n_channels() == channels);
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
        s.process(4);

        check!(s.loop_(l).unwrap().length() == 4);
        check!(s.loop_(l).unwrap().audio_channel(0).unwrap().data() == vec![1.0, 2.0, 3.0, 4.0]);

        // Now play it back and capture what leaves the output port.
        s.port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        s.process(4);

        let got = s
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .dequeue_data(4);
        let_assert!(Ok(samples) = got);
        check!(samples == vec![1.0, 2.0, 3.0, 4.0]);
    }

    fn test_processor_session(monitored: bool) -> (Session, usize, usize) {
        let mut session = Session::default();
        session.set_buffer_size(4);
        let input = session.add_port(dummy(10, "dry-input", PortDirection::Input));
        let send = session.add_port(internal("test:audio_in_0", 4));
        let return_ = session.add_port(internal("test:audio_out_0", 4));
        let output = session.add_port(dummy(11, "wet-output", PortDirection::Output));
        session.connect_ports_internal(input, send).unwrap();
        session.connect_ports_internal(return_, output).unwrap();
        session
            .port_mut(return_)
            .unwrap()
            .audio_mut()
            .unwrap()
            .set_passthrough_muted(!monitored);
        let loop_ = session.create_loop();
        let dry = session
            .add_audio_channel(loop_, 4, ChannelMode::Dry)
            .unwrap();
        let wet = session
            .add_audio_channel(loop_, 4, ChannelMode::Wet)
            .unwrap();
        session.connect_channel_input(dry, input).unwrap();
        session.connect_channel_output(dry, send).unwrap();
        session.connect_channel_input(wet, return_).unwrap();
        session.connect_channel_output(wet, output).unwrap();
        session.set_test_fx_active("test", true);
        session
            .set_processor_ports("test", vec![send], vec![return_], vec![])
            .unwrap();
        session.apply_graph_changes().unwrap();
        (session, loop_, output)
    }

    #[test]
    fn external_processor_node_records_the_staged_return_it_monitors() {
        let mut session = Session::default();
        session.set_buffer_size(4);
        let dry_source = session.add_port(dummy(40, "dry-source", PortDirection::Input));
        let dry_send = session.add_port(dummy(41, "dry-send", PortDirection::Output));
        let wet_return = session.add_port(Port::External(ExternalAudioPort::new(
            "wet-return",
            PortDirection::Input,
            4,
        )));
        let wet_output = session.add_port(dummy(42, "wet-output", PortDirection::Output));
        session
            .connect_ports_internal(dry_source, dry_send)
            .unwrap();
        session
            .connect_ports_internal(wet_return, wet_output)
            .unwrap();
        let loop_ = session.create_loop();
        let wet = session
            .add_audio_channel(loop_, 4, ChannelMode::Wet)
            .unwrap();
        session.connect_channel_input(wet, wet_return).unwrap();
        session.connect_channel_output(wet, wet_output).unwrap();
        session.set_external_processor("external");
        session
            .set_processor_ports("external", vec![dry_send], vec![wet_return], vec![])
            .unwrap();
        session.apply_graph_changes().unwrap();
        session
            .port_mut(dry_source)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[1.0, 2.0, 3.0, 4.0]);
        session
            .port_mut(wet_return)
            .unwrap()
            .as_external_mut()
            .unwrap()
            .stage_input(&[5.0, 6.0, 7.0, 8.0]);
        session
            .port_mut(wet_output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        session.set_loop_mode(loop_, LoopMode::Recording).unwrap();
        session.process(4);
        check!(
            session
                .loop_(loop_)
                .unwrap()
                .audio_channel(0)
                .unwrap()
                .data()
                == vec![5.0, 6.0, 7.0, 8.0]
        );
        let monitored = session
            .port_mut(wet_output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .dequeue_data(4)
            .unwrap();
        check!(monitored == vec![5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn test_processor_mode_matrix_routes_exact_wet_samples() {
        let (mut session, loop_, output) = test_processor_session(true);
        session
            .port_mut(0)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[2.0, 4.0, 6.0, 8.0]);
        session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        session.set_loop_mode(loop_, LoopMode::Recording).unwrap();
        session.process(4);
        check!(
            session
                .loop_(loop_)
                .unwrap()
                .audio_channel(0)
                .unwrap()
                .data()
                == vec![2.0, 4.0, 6.0, 8.0]
        );
        check!(
            session
                .loop_(loop_)
                .unwrap()
                .audio_channel(1)
                .unwrap()
                .data()
                == vec![1.0, 2.0, 3.0, 4.0]
        );
        let monitored = session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .dequeue_data(4)
            .unwrap();
        check!(monitored == vec![1.0, 2.0, 3.0, 4.0]);

        session.set_test_fx_active("test", false);
        session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        session.set_loop_mode(loop_, LoopMode::Playing).unwrap();
        session.process(4);
        let wet_playback = session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .dequeue_data(4)
            .unwrap();
        check!(wet_playback == vec![1.0, 2.0, 3.0, 4.0]);

        session.set_test_fx_active("test", true);
        session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        session
            .set_loop_mode(loop_, LoopMode::PlayingDryThroughWet)
            .unwrap();
        session.process(4);
        let dry_through_wet = session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .dequeue_data(4)
            .unwrap();
        check!(dry_through_wet == vec![1.0, 2.0, 3.0, 4.0]);

        session
            .loop_mut(loop_)
            .unwrap()
            .audio_channel_mut(1)
            .unwrap()
            .load_data(&[9.0, 9.0, 9.0, 9.0]);
        session
            .set_loop_mode(loop_, LoopMode::RecordingDryIntoWet)
            .unwrap();
        session.process(4);
        check!(
            session
                .loop_(loop_)
                .unwrap()
                .audio_channel(1)
                .unwrap()
                .data()
                == vec![1.0, 2.0, 3.0, 4.0]
        );
    }

    #[test]
    fn test_processor_maps_timestamped_midi_to_both_outputs() {
        let mut session = Session::default();
        session.set_buffer_size(4);
        let midi = session.add_port(dummy_midi(20, "test:midi_in_0", PortDirection::Input));
        let left = session.add_port(internal("test:audio_out_0", 4));
        let right = session.add_port(internal("test:audio_out_1", 4));
        session.set_test_fx_active("test", true);
        session
            .set_processor_ports("test", vec![], vec![left, right], vec![midi])
            .unwrap();
        session.apply_graph_changes().unwrap();
        session
            .port_mut(midi)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(2, &[0x90, 60, 127]);
        session.process(4);
        for output in [left, right] {
            check!(session.port_mut(output).unwrap().buffer(4) == [0.0, 0.0, 127.0 / 255.0, 0.0]);
        }
    }

    #[test]
    fn parallel_test_processors_do_not_share_routes() {
        let mut session = Session::default();
        session.set_buffer_size(4);
        let source_a = session.add_port(dummy(30, "source-a", PortDirection::Input));
        let source_b = session.add_port(dummy(31, "source-b", PortDirection::Input));
        let input_a = session.add_port(internal("a:audio_in_0", 4));
        let input_b = session.add_port(internal("b:audio_in_0", 4));
        let output_a = session.add_port(internal("a:audio_out_0", 4));
        let output_b = session.add_port(internal("b:audio_out_0", 4));
        session.connect_ports_internal(source_a, input_a).unwrap();
        session.connect_ports_internal(source_b, input_b).unwrap();
        session.set_test_fx_active("a", true);
        session.set_test_fx_active("b", true);
        session
            .set_processor_ports("a", vec![input_a], vec![output_a], vec![])
            .unwrap();
        session
            .set_processor_ports("b", vec![input_b], vec![output_b], vec![])
            .unwrap();
        session.apply_graph_changes().unwrap();
        session
            .port_mut(source_a)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[2.0; 4]);
        session
            .port_mut(source_b)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[10.0; 4]);
        session.process(4);
        check!(session.port_mut(output_a).unwrap().buffer(4) == [1.0; 4]);
        check!(session.port_mut(output_b).unwrap().buffer(4) == [5.0; 4]);
    }

    #[test]
    fn test_processor_unmonitored_recording_captures_wet_without_output() {
        let (mut session, loop_, output) = test_processor_session(false);
        session
            .port_mut(0)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[2.0, 4.0, 6.0, 8.0]);
        session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        session.set_loop_mode(loop_, LoopMode::Recording).unwrap();
        session.process(4);
        check!(
            session
                .loop_(loop_)
                .unwrap()
                .audio_channel(1)
                .unwrap()
                .data()
                == vec![1.0, 2.0, 3.0, 4.0]
        );
        let output = session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .dequeue_data(4)
            .unwrap();
        check!(output == vec![0.0; 4]);
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
        s.process(4);
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
        s.process(4);

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
        s.process(4);

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
        s.process(4);

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
        s.process(4);

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
        s.process(4);
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
        s.process(4);
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
        s.process(4);
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
        s.process(4);

        check!(s.loop_(l).unwrap().midi_channel(0).unwrap().n_events() == 2);

        // Play it back and capture what leaves the output port.
        s.loop_mut(l).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let d = s.port_mut(output).unwrap().as_dummy_midi_mut().unwrap();
        let_assert!(Ok(()) = d.request_data(4));
        s.process(4);

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
    fn replacing_midi_through_a_session_overwrites_loaded_events() {
        use crate::midi;
        let mut s = Session::default();
        let input = s.add_port(dummy_midi(1, "min", PortDirection::Input));
        let l = s.create_loop();
        let_assert!(Ok(c) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.connect_channel_input(c, input));
        let_assert!(Ok(()) = s.apply_graph_changes());
        s.loop_mut(l)
            .unwrap()
            .midi_channel_mut(0)
            .unwrap()
            .set_contents(
                &[
                    MidiStorageElem::new(1, &midi::note_on(0, 60, 100)).unwrap(),
                    MidiStorageElem::new(2, &midi::note_off(0, 60, 0)).unwrap(),
                ],
                4,
                None,
            );
        s.loop_mut(l).unwrap().set_length(4);
        s.port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(0, &midi::note_on(0, 64, 100));
        s.port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(3, &midi::note_off(0, 64, 0));

        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Replacing));
        s.process(4);

        let contents = s.loop_(l).unwrap().midi_channel(0).unwrap().contents();
        check!(contents.len() == 2);
        check!(contents[0].time == 0);
        check!(contents[0].data() == midi::note_on(0, 64, 100).as_slice());
        check!(contents[1].time == 3);
        check!(contents[1].data() == midi::note_off(0, 64, 0).as_slice());
    }

    #[test]
    fn muting_midi_passthrough_cleans_forwarded_notes_exactly_once() {
        use crate::midi;
        let mut s = Session::default();
        let source = s.add_port(dummy_midi(1, "source", PortDirection::Input));
        let target = s.add_port(dummy_midi(2, "target", PortDirection::Output));
        let_assert!(Ok(()) = s.connect_ports_internal(source, target));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.port_mut(source)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(0, &midi::note_on(0, 60, 100));
        s.port_mut(target)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .request_data(1)
            .unwrap();
        s.process(1);
        let started = s
            .port_mut(target)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .take_written_requested_msgs();
        check!(started.len() == 1);
        check!(started[0].data() == midi::note_on(0, 60, 100).as_slice());

        s.port_mut(source)
            .unwrap()
            .midi_mut()
            .unwrap()
            .set_passthrough_muted(true);
        s.port_mut(source)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(0, &midi::note_on(0, 61, 100));
        s.port_mut(target)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .request_data(1)
            .unwrap();
        s.process(1);
        let cleanup = s
            .port_mut(target)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .take_written_requested_msgs();
        check!(cleanup.len() == 1);
        check!(cleanup[0].data() == midi::note_off(0, 60, 0).as_slice());

        s.port_mut(target)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .request_data(1)
            .unwrap();
        s.process(1);
        check!(s
            .port_mut(target)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .take_written_requested_msgs()
            .is_empty());
    }

    #[test]
    fn a_midi_channel_without_ports_still_advances_the_loop() {
        let mut s = Session::default();
        let l = s.create_loop();
        let_assert!(Ok(_) = s.add_midi_channel(l, 64, ChannelMode::Direct));
        let_assert!(Ok(()) = s.apply_graph_changes());
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
        s.process(4);
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
        s.process(4);

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
        s.process(4);

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
        s.process(4);

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
        s.process(4);
        // A second cycle with no new arrivals must not re-record the first one.
        s.process(4);
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
        s.process(4);

        // Play twice over a length of 8, so the message sounds exactly once.
        s.loop_mut(l).unwrap().set_length(8);
        let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
        let d = s.port_mut(output).unwrap().as_dummy_midi_mut().unwrap();
        let_assert!(Ok(()) = d.request_data(8));
        s.process(4);
        s.process(4);

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

        s.process(4);
        check!(s.port_mut(output).unwrap().buffer(4).to_vec() == vec![1.0, 2.0, 3.0, 4.0]);
        check!(s.position_of(l) == Some(4));
        // Nothing to split: the wrap is beyond this buffer.
        check!(s.n_sub_blocks_last_cycle() == 1);

        // Frames 5 and 6 play, then the loop wraps and frames 1 and 2 play again.
        s.process(4);
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
        s.process(6);
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

        s.process(6);
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
        s.process(4);
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

        s.process(4);
        s.process(4);
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
        let_assert!(Ok(()) = s.set_loop_sync_source(a, Some(b)));
        let_assert!(Ok(()) = s.set_loop_sync_source(b, Some(a)));
        let_assert!(Ok(()) = s.apply_graph_changes());

        s.loop_mut(a).unwrap().set_length(4);
        s.loop_mut(b).unwrap().set_length(4);
        let_assert!(Ok(()) = s.set_loop_mode(a, LoopMode::Playing));
        let_assert!(Ok(()) = s.set_loop_mode(b, LoopMode::Playing));
        s.process(4);
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

    #[cfg(feature = "lv2")]
    #[test]
    fn inactive_carla_fx_chain_bypasses_processing_and_tails() {
        let _exclusive = crate::lv2_carla::lock_carla_test();
        let Ok(host) =
            crate::lv2_carla::CarlaLv2Host::instantiate(crate::FXChainType::CarlaRack, 48_000, 64)
        else {
            eprintln!(
                "skipping Carla inactive routing test; Carla Rack is not installed in LV2_PATH"
            );
            return;
        };
        let host: Box<dyn CarlaProcessor> = Box::new(host);
        let mut s = Session::default();
        s.set_sample_rate(48_000);
        s.set_buffer_size(64);
        s.set_carla_fx_host("carla", host);
        let source = s.add_port(dummy(75, "source", PortDirection::Input));
        let audio_in = s.add_port(internal("carla:audio_in_0", 64));
        let fx_out = s.add_port(internal("carla:audio_out_0", 64));
        s.connect_ports_internal(source, audio_in).unwrap();
        s.set_processor_ports("carla", vec![audio_in], vec![fx_out], vec![])
            .unwrap();
        s.port_mut(source)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&vec![1.0; 64]);
        s.apply_graph_changes().unwrap();
        s.process(64);

        let wet = s.port_mut(fx_out).unwrap().buffer(64).to_vec();
        assert!(wet.iter().all(|s| *s == 0.0));
    }

    #[cfg(feature = "lv2")]
    #[test]
    fn fake_carla_processor_node_records_its_wet_output() {
        let mut host =
            crate::carla_processor::FakeCarlaProcessor::new(crate::FXChainType::CarlaRack, 1, 4);
        host.set_active(true);
        let mut session = Session::default();
        session.set_buffer_size(4);
        session.set_carla_fx_host("carla", Box::new(host));
        let source = session.add_port(dummy(77, "source", PortDirection::Input));
        let input = session.add_port(internal("carla:audio_in_0", 4));
        let output = session.add_port(internal("carla:audio_out_0", 4));
        session.connect_ports_internal(source, input).unwrap();
        session
            .set_processor_ports("carla", vec![input], vec![output], vec![])
            .unwrap();
        let loop_ = session.create_loop();
        let wet = session
            .add_audio_channel(loop_, 4, ChannelMode::Wet)
            .unwrap();
        session.connect_channel_input(wet, output).unwrap();
        session
            .port_mut(source)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&[1.0, 2.0, 3.0, 4.0]);
        session.set_loop_mode(loop_, LoopMode::Recording).unwrap();
        session.apply_graph_changes().unwrap();
        session.process(4);
        assert_eq!(
            session
                .loop_(loop_)
                .unwrap()
                .audio_channel(0)
                .unwrap()
                .data(),
            vec![1.0, 2.0, 3.0, 4.0]
        );
    }

    #[cfg(feature = "lv2")]
    #[test]
    fn bridged_carla_session_path_has_no_allocation_or_mutex() {
        let fake = crate::carla_processor::FakeCarlaProcessor::new(
            crate::FXChainType::CarlaRack,
            2,
            shoop_plugin_protocol::MAX_BLOCK_FRAMES,
        );
        let (control, endpoint) =
            crate::carla_processor::spawn_processor_bridge(Box::new(fake), 1_000, 100).unwrap();
        control.set_active(true);
        let mut s = Session::default();
        s.set_sample_rate(1_000);
        s.set_buffer_size(100);
        s.set_carla_fx_host("carla", endpoint);
        let source = s.add_port(dummy(78, "source", PortDirection::Input));
        let audio_in = s.add_port(internal("carla:audio_in_0", 100));
        let fx_out = s.add_port(internal("carla:audio_out_0", 100));
        let midi_in = s.add_port(dummy_midi(79, "carla:midi_in_0", PortDirection::Input));
        s.connect_ports_internal(source, audio_in).unwrap();
        s.set_processor_ports("carla", vec![audio_in], vec![fx_out], vec![midi_in])
            .unwrap();
        s.port_mut(source)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&vec![0.25; 200]);
        s.apply_graph_changes().unwrap();
        s.process(100);

        crate::realtime_lock_guard::set_enabled(true);
        assert_no_alloc::assert_no_alloc(|| {
            crate::realtime_lock_guard::forbid_locks_if_enabled(|| {
                s.process(100);
            });
        });
        crate::realtime_lock_guard::set_enabled(false);
    }

    #[cfg(feature = "lv2")]
    #[test]
    fn carla_fx_chain_audio_route_runs_from_session_ports_to_wet_output() {
        let _exclusive = crate::lv2_carla::lock_carla_test();
        let Ok(mut host) =
            crate::lv2_carla::CarlaLv2Host::instantiate(crate::FXChainType::CarlaRack, 48_000, 64)
        else {
            eprintln!("skipping Carla routing test; Carla Rack is not installed in LV2_PATH");
            return;
        };
        host.set_active(true);
        let host: Box<dyn CarlaProcessor> = Box::new(host);
        let mut s = Session::default();
        s.set_sample_rate(48_000);
        s.set_buffer_size(64);
        s.set_carla_fx_host("carla", host);
        let source = s.add_port(dummy(76, "source", PortDirection::Input));
        let audio_in = s.add_port(internal("carla:audio_in_0", 64));
        let fx_out = s.add_port(internal("carla:audio_out_0", 64));
        let midi_in = s.add_port(dummy_midi(77, "carla:midi_in_0", PortDirection::Input));
        s.connect_ports_internal(source, audio_in).unwrap();
        s.set_processor_ports("carla", vec![audio_in], vec![fx_out], vec![midi_in])
            .unwrap();
        let mut impulse = vec![0.0; 64];
        impulse[0] = 1.0;
        s.port_mut(source)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&impulse);
        assert!(s
            .port_mut(midi_in)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(3, &[0x90, 60, 100]));
        s.apply_graph_changes().unwrap();
        s.process(64);

        let wet = s.port_mut(fx_out).unwrap().buffer(64).to_vec();
        assert_eq!(wet.len(), 64);
        assert!(wet.iter().all(|s| s.is_finite()));
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
        s.process(2);
        s.process(2);

        check!(s.loop_(l).unwrap().length() == 4);
        check!(s.loop_(l).unwrap().audio_channel(0).unwrap().data() == vec![1.0, 2.0, 3.0, 4.0]);
    }
}
