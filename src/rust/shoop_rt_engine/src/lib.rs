//! The new engine backend's real-time composition.
//!
//! One [`rt_runtime`] graph runtime holds the loop library's playback and
//! capture nodes next to the engine's own processors: a per-strip input stage
//! (gain, balance, monitoring feed, meters), a per-strip output stage (the sum
//! of its loops and monitored input; gain, balance, mute, meters), a fader per
//! loop and output buses. Strip and loop slots are prepared in pool segments;
//! only occupied slots are in the active graph and loop topology.
//!
//! [`prepare`] returns three owners: the [`ControlEndpoint`], which implements
//! the protocol's [`Endpoint`](shoop_rt_transport::carrier::Endpoint) and runs
//! wherever the host pumps its carrier; the [`Render`] owner, called once per
//! audio callback; and the [`HostStatus`] handle for driver status, xruns and
//! load. Both the endpoint and the render path are bounded and allocation-free
//! after preparation.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[cfg(test)]
mod buffered_tests;

mod bulk;
mod cells;
mod composite;
mod content;
mod history;
mod input;
pub use history::HistoryCapacity;
mod description;
mod endpoint;
mod family;
mod global_midi;
mod graph;
mod jobs;
mod midi_layout;
mod model;
mod monitoring;
mod node;
mod pool;
mod preview;
mod processors;
mod profiling;
mod queue;
mod recorded;
mod render;
mod replacement;
mod resync;
mod ring;
mod stream;
mod timing;
mod topology;

#[cfg(feature = "testing")]
pub mod testing;

pub use cells::HostStatus;
pub use description::{EngineDescription, HostPorts, Segment, MAX_HOST_PORTS};
pub use endpoint::ControlEndpoint;
pub use node::{EngineNode, EngineOp, NodeConfiguration};
pub use processors::{
    HostedProcessor, PreparedProcessor, PreparedProcessors, ProcessorCatalog, ProcessorPhase,
    ProcessorPreset, ProcessorState,
};
pub use render::{Phase, Render, Report, Stopped};
pub use rt_runtime::AudioRenderMode;

use crate::cells::{Cells, Observer};
use crate::endpoint::{Settings, Work, PLANNED};
use crate::graph::{Builder, Shape};
use crate::model::Model;
use crate::node::{BusMixer, LoopFader, NodeSettings, StripInput, StripOutput};
use crate::pool::Pool;
use crate::ring::Ring;
use crate::topology::{Encoder, RECORD_BYTES};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode};
use rt_runtime::graph::PreparedPlan;
use rt_runtime::resource::{CaptureGroupConfig, MidiIndexConfig, ResourceStore};
use rt_runtime::schedule::Execution;
use rt_runtime::{Budgets, Config, Epoch, PlannerId};
use shoop_loops::audio::Smoothing;
use shoop_loops::runtime::{
    Binding, Failure, LoopPlanner, NodePairSettings, Operation, Options, Prepared as LoopParts,
    Reply, Settings as LoopSettings, TopologySettings,
};
use shoop_loops::{compile, Limits as LoopLimits, LoopId, TopologyDescription};
use shoop_rt_protocol::{Direction, DriverStatus, HostPort, HostPortId, HostPortKind, Limits};

pub(crate) type EngineCommand = Command<EngineNode, LoopPlanner, EngineOp>;
pub(crate) type EngineControl = Control<EngineNode, LoopPlanner, EngineOp>;
pub(crate) type EngineExecution = Execution<Operation, Reply, EngineOp, ()>;

/// Topology records per append operation.
const RECORDS_PER_APPEND: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrepareError {
    Runtime(rt_runtime::Error),
    Loops(Failure),
}
impl From<rt_runtime::Error> for PrepareError {
    fn from(error: rt_runtime::Error) -> Self {
        Self::Runtime(error)
    }
}
impl From<Failure> for PrepareError {
    fn from(error: Failure) -> Self {
        Self::Loops(error)
    }
}
impl From<shoop_loops::Error> for PrepareError {
    fn from(error: shoop_loops::Error) -> Self {
        Self::Loops(Failure::Timeline(error))
    }
}

/// Fixed metadata storage for two independent processor-state take revisions.
pub fn recorded_state_storage_bytes(loops: usize) -> Result<usize, rt_runtime::Error> {
    loops
        .checked_mul(core::mem::size_of::<recorded::TakeCells>())
        .ok_or(rt_runtime::Error::Capacity)
}

/// A prepared engine, before its first callback.
pub struct Engine {
    pub endpoint: ControlEndpoint,
    pub render: Render,
    pub host: HostStatus,
}

impl Engine {
    /// An outbox with room for one full telemetry round and the client's credits.
    pub fn outbox(&self) -> shoop_rt_transport::carrier::Outbox {
        let limits = self.endpoint.limits();
        shoop_rt_transport::carrier::Outbox::new(
            2 * usize::from(self.endpoint.settings.max_reliable) + 8,
            limits.strips as usize + 2 * limits.loops as usize + limits.buses as usize + 2,
            usize::from(self.endpoint.settings.max_bulk),
        )
    }
}

fn host_ports(description: &EngineDescription) -> Box<[HostPort]> {
    let host = &description.host;
    let mut ports = Vec::new();
    for (names, kind, direction) in [
        (&host.audio_inputs, HostPortKind::Audio, Direction::Input),
        (&host.audio_outputs, HostPortKind::Audio, Direction::Output),
        (&host.midi_inputs, HostPortKind::Midi, Direction::Input),
        (&host.midi_outputs, HostPortKind::Midi, Direction::Output),
    ] {
        for (index, name) in names.iter().enumerate() {
            ports.push(HostPort {
                id: HostPortId(ports.len() as u32),
                kind,
                direction,
                name: *name,
                preferred: kind == HostPortKind::Audio
                    && direction == Direction::Output
                    && description
                        .master_connections
                        .iter()
                        .any(|&(_, host)| host as usize == index),
            });
        }
    }
    ports.into_boxed_slice()
}

fn runtime_capacities(description: &EngineDescription, graph_frames: usize) -> Config {
    let (strips, loops) = (description.strips(), description.loops());
    let processor_slots = 2
        * (usize::from(description.processors.fx_instances)
            + usize::from(description.processors.synth_instances));
    let nodes = 1 + 2 * strips + 3 * loops + 2 * description.buses as usize + processor_slots;
    let channels = description.strip_channels as usize;
    let master_channels = description.bus_channels as usize;
    let buses = description.buses as usize;
    let counts = [
        description.host.audio_inputs.len(),
        description.host.audio_outputs.len(),
        description.host.midi_inputs.len(),
        description.host.midi_outputs.len(),
    ];
    let [audio_in, audio_out, midi_in, midi_out] = counts;
    let credits = usize::from(description.reliable_credits);
    let pending = description.pending_transitions;
    let slots = pending + 2 * credits + loops * (PLANNED + 3) + 8 * buses + 64;
    let ports = strips * (14 * channels + 9)
        + loops * (8 * channels + 2)
        + 3 * buses * master_channels
        + counts.iter().sum::<usize>()
        + processor_slots * (3 * channels + 3)
        + description.buses as usize
            * (2 * description.bus_channels as usize + description.host.midi_inputs.len());
    let edges = strips
        * (channels * (4 * (audio_in + audio_out) + 2 * buses * master_channels)
            + 2 * midi_in
            + 2 * midi_out)
        + loops * (6 * channels + 3)
        + (channels + 1) * (strips + loops)
        + 2 * buses * master_channels * audio_out
        + processor_slots * (3 * channels + 3)
        + description.buses as usize
            * (2 * description.bus_channels as usize + description.host.midi_inputs.len());
    let page_bytes = description.page_bytes;
    let payload = RECORDS_PER_APPEND.max(loops) * RECORD_BYTES;
    Config {
        sample_rate: description.sample_rate,
        max_frames: graph_frames,
        audio_inputs: audio_in,
        audio_outputs: audio_out,
        midi_inputs: midi_in,
        midi_outputs: midi_out,
        max_nodes: nodes,
        max_ports: ports,
        max_edges: edges.max(1),
        midi_events_per_port: description.midi_events,
        midi_bytes_per_port: description.midi_bytes,
        scheduled_events: slots,
        resource_slots: 20 * loops + 16,
        resource_pages: description.resource_pages,
        page_bytes,
        resource_io_bytes: stream::READ_PAGES * page_bytes,
        resource_io_steps: 0,
        command_slots: slots,
        budgets: Budgets {
            commands: slots,
            events: slots,
            transfer_bytes: payload
                + (2 + stream::READ_PAGES) * page_bytes
                + content::RUNTIME_UPLOAD_BUFFERS
                    * page_bytes.min(shoop_rt_protocol::content::CONTENT_CHUNK_BYTES),
            validation_steps: (3 * nodes + edges + 2 * loops + 64)
                .max(page_bytes.min(shoop_rt_protocol::content::CONTENT_CHUNK_BYTES)),
            publications: slots,
        },
    }
}

fn signal_nodes(
    description: &EngineDescription,
    pool: &Pool,
    settings: &NodeSettings,
) -> Vec<(rt_runtime::NodeId, EngineNode)> {
    let (strips, loops) = (pool.strips(), pool.loops());
    let buses = description.buses as usize;
    let mut nodes = Vec::with_capacity(2 * strips + loops + buses + 1);
    for slot in 0..buses {
        nodes.push((
            pool.bus_input(slot),
            EngineNode::BusInput(BusMixer::new(slot, settings)),
        ));
        nodes.push((
            pool.bus(slot),
            EngineNode::Bus(BusMixer::new(slot, settings)),
        ));
    }
    nodes.push((
        pool.reader(),
        EngineNode::Reader(stream::BulkReader::new(
            settings.cells.clone(),
            description.page_bytes,
        )),
    ));
    for slot in 0..strips {
        nodes.push((
            pool.strip_input(slot),
            EngineNode::StripInput(StripInput::new(slot, settings)),
        ));
        nodes.push((
            pool.strip_output(slot),
            EngineNode::StripOutput(StripOutput::new(slot, settings)),
        ));
    }
    for slot in 0..loops {
        nodes.push((
            pool.fader(slot),
            EngineNode::Fader(LoopFader::new(slot, settings)),
        ));
    }
    nodes
}

/// Prepare an engine for `description`. Allocation happens here only.
pub fn prepare(
    description: &EngineDescription,
    driver: DriverStatus,
) -> Result<Engine, PrepareError> {
    let processors = empty_processors(description)?;
    prepare_with_processors(description, driver, processors)
}

/// Prepare optional retrospective input histories and bounded private edit jobs.
/// Explicit storage limits are installed before the runtime takes ownership.
fn empty_processors(description: &EngineDescription) -> Result<PreparedProcessors, PrepareError> {
    let mut processors = PreparedProcessors::default();
    processors.effective = description
        .processors
        .resolve(
            shoop_rt_protocol::processors::Support::default(),
            description.sample_rate,
        )
        .map_err(|_| rt_runtime::Error::InvalidConfig)?;
    Ok(processors)
}

pub fn prepare_with_shared(
    description: &EngineDescription,
    driver: DriverStatus,
    shared_settings: shoop_loops::runtime::SharedSettings,
) -> Result<Engine, PrepareError> {
    let processors = empty_processors(description)?;
    prepare_with_shared_and_processors(description, driver, shared_settings, processors)
}

pub fn prepare_with_processors(
    description: &EngineDescription,
    driver: DriverStatus,
    processors: PreparedProcessors,
) -> Result<Engine, PrepareError> {
    let prepared = history::Layout::new(description)?;
    let mut expanded = description.clone();
    expanded.resource_pages = prepared.capacity.total_resource_pages;
    prepare_complete(
        &expanded,
        driver,
        prepared.settings(),
        processors,
        description.resource_pages / 4,
    )
}

pub fn prepare_with_shared_and_processors(
    description: &EngineDescription,
    driver: DriverStatus,
    shared_settings: shoop_loops::runtime::SharedSettings,
    processors: PreparedProcessors,
) -> Result<Engine, PrepareError> {
    prepare_complete(
        description,
        driver,
        shared_settings,
        processors,
        description.resource_pages / 4,
    )
}

fn prepare_complete(
    description: &EngineDescription,
    driver: DriverStatus,
    shared_settings: shoop_loops::runtime::SharedSettings,
    processors: PreparedProcessors,
    wet_pages: usize,
) -> Result<Engine, PrepareError> {
    description.validate()?;
    if description.audio_render_mode == rt_runtime::AudioRenderMode::Compact
        && (!description.audio_bench_only || !processors.instances.is_empty())
    {
        return Err(rt_runtime::Error::InvalidConfig.into());
    }
    let policy = description.processors;
    let effective = processors.effective;
    if effective.memory_bytes != u64::from(policy.memory_mb) * (1 << 20)
        || effective.fx_instances > policy.fx_instances
        || effective.synth_instances > policy.synth_instances
        || (policy.builtin_fx == shoop_rt_protocol::ProcessorPolicy::Off
            && effective.fx_instances != 0)
        || (policy.oxisynth == shoop_rt_protocol::ProcessorPolicy::Off
            && effective.synth_instances != 0)
        || (policy.builtin_fx == shoop_rt_protocol::ProcessorPolicy::On
            && effective.fx_instances != policy.fx_instances)
        || (policy.oxisynth == shoop_rt_protocol::ProcessorPolicy::On
            && effective.synth_instances != policy.synth_instances)
        || (effective.fx_instances == 0) != effective.fx_unavailable.is_some()
        || (effective.synth_instances == 0) != effective.synth_unavailable.is_some()
        || (effective.synth_instances == 0 && !processors.presets.is_empty())
        || processors.presets.len() > 512
        || processors
            .presets
            .windows(2)
            .any(|p| (p[0].bank, p[0].program) >= (p[1].bank, p[1].program))
        || (!processors.instances.is_empty() && processors.reserved_bytes == 0)
    {
        return Err(rt_runtime::Error::InvalidConfig.into());
    }
    for instance in &processors.instances {
        let expected = match instance.kind {
            shoop_rt_protocol::ProcessorKind::BuiltInFx => description.strip_channels as usize,
            shoop_rt_protocol::ProcessorKind::OxiSynth => 2,
            shoop_rt_protocol::ProcessorKind::External => {
                return Err(rt_runtime::Error::InvalidConfig.into())
            }
        };
        let ports = instance.backend.ports();
        if ports.audio_inputs != expected
            || ports.audio_outputs != expected
            || ports.midi_inputs != 1
            || ports.midi_outputs != 0
            || expected > description.strip_channels as usize
        {
            return Err(rt_runtime::Error::InvalidShape.into());
        }
    }
    if processors.instances.len() != processors.effective.slots()
        || processors.reserved_bytes > processors.effective.memory_bytes
        || processors
            .instances
            .iter()
            .filter(|p| p.kind == shoop_rt_protocol::ProcessorKind::BuiltInFx)
            .count()
            != 2 * processors.effective.fx_instances as usize
        || processors
            .instances
            .iter()
            .filter(|p| p.kind == shoop_rt_protocol::ProcessorKind::OxiSynth)
            .count()
            != 2 * processors.effective.synth_instances as usize
    {
        return Err(rt_runtime::Error::InvalidConfig.into());
    }
    let pool = if processors.instances.is_empty() {
        Pool::new(&description.segments, description.buses as usize)
    } else {
        Pool::with_processors(
            &description.segments,
            description.buses as usize,
            processors.instances.len(),
        )
    };
    let (strips, loops) = (pool.strips(), pool.loops());
    let channels = description.strip_channels as usize;
    let master_channels = description.bus_channels as usize;
    let buses = description.buses as usize;
    let host = &description.host;
    let counts = [
        host.audio_inputs.len(),
        host.audio_outputs.len(),
        host.midi_inputs.len(),
        host.midi_outputs.len(),
    ];
    let [audio_in, audio_out, midi_in, midi_out] = counts;
    let credits = usize::from(description.reliable_credits);
    let bulk_credits = usize::from(description.bulk_credits);
    let pending = description.pending_transitions;
    let quantum = rt_runtime::Quantum::new(description.quantum)?;
    let config = runtime_capacities(description, quantum.frames()).prepare()?;
    let slots = config.limits().command_slots;
    let midi_events = description.midi_events;
    let midi_bytes = description.midi_bytes;
    let mut resources = ResourceStore::with_capture_groups(
        config,
        quantum,
        (!description.audio_bench_only).then_some(MidiIndexConfig {
            versions: 6 * loops + 8,
            checkpoints_per_version: 64,
            replay_events: 64,
        }),
        CaptureGroupConfig {
            groups: (4 * loops).max(1),
            members_per_group: if description.audio_bench_only { 2 } else { 3 },
            staging_bytes_per_group: quantum.frames() * channels * 8
                + if description.audio_bench_only {
                    0
                } else {
                    12 * midi_events.max(rt_runtime::midi::ResetState::EVENTS)
                        + midi_bytes.max(rt_runtime::midi::ResetState::BYTES)
                },
        },
    )?;
    resources.set_audio_render_mode(description.audio_render_mode);
    let loop_limits = LoopLimits {
        loop_slots: loops.max(1),
        relations: description.timing_relations.max(loops.max(1)),
        program_entries: description.composite_entries,
        program_segments: description.composite_segments,
        pending_requests: pending + 1,
        journal_events: 0,
    };
    let topology = compile(loop_limits, &TopologyDescription::default())?;
    let profile = rt_runtime::profiling::prepare(pool.nodes());
    let (profile_control, profile_recorder, profile_reader) = match profile {
        Some((control, recorder, reader)) => (Some(control), Some(recorder), Some(reader)),
        None => (None, None, None),
    };
    let mut cells = Cells::new(
        strips,
        loops,
        buses,
        stream::RING_PAGES * description.page_bytes,
        driver,
        quantum.frames(),
    );
    if let Some(cells) = alloc::sync::Arc::get_mut(&mut cells) {
        cells.profiling = profile_control;
    }
    let smoothing = match description.smoothing_frames {
        0 => Smoothing::None,
        frames => Smoothing::LinearRestart { frames },
    };
    let planner_id = PlannerId::from_parts(0, 1)?;
    let bindings = (0..loops)
        .map(|slot| {
            Ok(Binding {
                dry_wet: false,
                auxiliary_channels: channels,
                target: LoopId::from_parts(slot as u32, 1)?,
                playback: pool.playback(slot),
                capture: pool.capture(slot),
                channels,
                midi: !description.audio_bench_only,
                gain: 1.,
                smoothing,
                midi_events,
                midi_bytes,
                packet_bytes: midi_bytes,
            })
        })
        .collect::<Result<Vec<_>, shoop_loops::Error>>()?;
    let LoopParts {
        mut planner,
        nodes: loop_nodes,
        client,
    } = LoopPlanner::prepare(
        config,
        quantum,
        topology,
        &mut resources,
        LoopSettings {
            planner: planner_id,
            normal_requests: pending,
            media_slots: 6 * loops + pending + 8,
            export_slots: bulk_credits + 1,
        },
        bindings,
        Vec::new(),
        Options {
            staging: TopologySettings {
                candidates: 1,
                records_per_callback: RECORDS_PER_APPEND,
                validation_steps_per_callback: 16 * (loops + 4) * (loops + 4),
            },
            node_pairs: NodePairSettings {
                candidates: 1,
                sources: 2 * loops + 2,
                source_packets_per_callback: 64,
            },
            observer: Some(Box::new(Observer::new(
                cells.clone(),
                description.audio_render_mode == rt_runtime::AudioRenderMode::Compact,
            ))),
        },
    )?;
    let input_capacity = input::Stage::capacity(&shared_settings);
    if (pool.processors() > 0 || description.external_dry_wet)
        && description.audio_render_mode != rt_runtime::AudioRenderMode::Compact
    {
        planner.prepare_wet_edits(
            &mut resources,
            rt_runtime::resource::shared_audio::Config {
                channels: channels as u32,
                history_frames: 0,
                views: 6 * loops + 8,
                pages: wet_pages,
            },
        )?;
    }
    if !shared_settings.histories.is_empty() || shared_settings.candidates != 0 {
        if shared_settings.histories.iter().any(|h| {
            h.source as usize >= pool.strips()
                || h.audio.is_some_and(|a| a.channels as usize != channels)
        }) {
            return Err(rt_runtime::Error::InvalidConfig.into());
        }
        planner.prepare_shared(&mut resources, shared_settings)?;
    }
    let mut node_midi = midi_layout::Layout::new(description)?;
    if pool.processors() == 0 && !description.external_dry_wet {
        node_midi.playback_merge = Default::default();
        node_midi.global = Default::default();
        node_midi.send = Default::default();
    }
    let settings = NodeSettings {
        external_midi: (description.external_dry_wet && !description.audio_bench_only)
            .then_some((description.midi_events, description.midi_bytes)),
        cells: cells.clone(),
        channels,
        master_channels,
        frames: quantum.frames(),
        ramp: description.ramp_frames,
        midi: node_midi,
    };
    let mut nodes: Vec<PreparedNode<EngineNode, LoopPlanner, EngineOp>> = loop_nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, EngineNode::Loop(node)))
        .collect();
    nodes.extend(
        signal_nodes(description, &pool, &settings)
            .into_iter()
            .map(|(id, node)| PreparedNode::new(id, node)),
    );
    let processor_cells: Box<[_]> = processors
        .instances
        .iter()
        .map(|p| alloc::sync::Arc::new(processors::ProcessorCells::new(p.kind)))
        .collect();
    let processor_catalog = ProcessorCatalog {
        effective: processors.effective,
        reserved_bytes: processors.reserved_bytes,
        kinds: processors.instances.iter().map(|p| p.kind).collect(),
        presets: processors.presets,
    };
    let processor_input = description.processor_midi_input_capacity()?;
    let buffer_limits = rt_buffered::BufferLimits {
        midi_input: rt_buffered::MidiLimits {
            events_per_quantum: processor_input.events,
            bytes_per_quantum: processor_input.bytes,
        },
        midi_output: rt_buffered::MidiLimits {
            events_per_quantum: 0,
            bytes_per_quantum: 0,
        },
        parameters_per_quantum: 64,
    };
    for (slot, processor) in processors.instances.into_iter().enumerate() {
        let mut node = HostedProcessor::new(
            processor.backend,
            processor_cells[slot].clone(),
            quantum,
            buffer_limits,
            description.global_midi_input_capacity()?,
            slot,
            pool.processors(),
        )?;
        node.attach(cells.clone());
        nodes.push(PreparedNode::new(
            pool.processor(slot),
            EngineNode::Processor(node),
        ));
    }
    let (mut control, mut runtime) = GraphRuntime::prepare_for_host(
        config,
        description.max_frames,
        quantum,
        nodes,
        planner,
        resources,
    )?;
    if let Some(recorder) = profile_recorder {
        runtime.set_profiling(recorder);
    }
    if quantum.frames() > 1 {
        runtime.prepare_host_buffering_with_midi_output(settings.midi.tick)?;
    }
    runtime.prepare_graph_staging(1)?;
    control.prepare_submissions()?;
    let mut master_outputs = [model::HostMask::EMPTY; shoop_rt_protocol::MAX_STRIP_CHANNELS];
    for &(channel, host) in &description.master_connections {
        master_outputs[channel as usize].set(host as usize, true);
    }
    let encoder = Encoder::new(loop_limits);
    let image = vec![0; encoder.image_bytes()].into_boxed_slice();
    let work = Work {
        builder: Builder::new(config.limits().max_edges),
        encoder,
        image,
        payloads: (1..=RECORDS_PER_APPEND)
            .map(|records| Some(vec![0; records * RECORD_BYTES].into_boxed_slice()))
            .collect(),
        full: {
            let count =
                (4 * loops + description.composite_entries + description.composite_segments)
                    .div_ceil(RECORDS_PER_APPEND);
            let mut full = Vec::with_capacity(count);
            full.extend(
                (0..count).map(|_| vec![0; RECORDS_PER_APPEND * RECORD_BYTES].into_boxed_slice()),
            );
            full
        },
        plans: [
            Some(PreparedPlan::with_capacity(config)?),
            Some(PreparedPlan::with_capacity(config)?),
        ],
    };
    let mut content_chunk_bytes = description
        .page_bytes
        .min(shoop_rt_protocol::content::CONTENT_CHUNK_BYTES)
        .min(config.limits().budgets.validation_steps)
        .max(1);
    while description.page_bytes % content_chunk_bytes != 0 {
        content_chunk_bytes -= 1;
    }
    let limits = Limits {
        composite_entries: description.composite_entries as u32,
        composite_segments: description.composite_segments as u32,
        timing_relations: loop_limits.relations as u32,
        content_chunk_bytes: content_chunk_bytes as u16,
        strips: strips as u32,
        loops: loops as u32,
        strip_audio_channels: description.strip_channels,
        master_channels: description.master_channels,
        buses: description.buses,
        bus_channels: description.bus_channels,
        host_audio_inputs: audio_in as u16,
        host_audio_outputs: audio_out as u16,
        host_midi_inputs: midi_in as u16,
        host_midi_outputs: midi_out as u16,
        max_frames: description.max_frames as u32,
        quantum: quantum.frames() as u32,
        processing_latency_frames: runtime.buffering_latency() as u32,
        smoothing_limit_frames: description.smoothing_limit_frames,
        reliable_credits: description.reliable_credits,
        bulk_credits: description.bulk_credits,
    };
    let endpoint = ControlEndpoint {
        projected_bus_processors: vec![None; description.buses as usize].into_boxed_slice(),
        processor_projection_cursor: 0,
        projected_processors: vec![None; strips].into_boxed_slice(),
        projected_timing: vec![None; loops].into_boxed_slice(),
        projected_recorded: vec![None; loops].into_boxed_slice(),
        processor_catalog,
        processor_cells,
        profiling: profile_reader,
        profile_named: vec![false; pool.nodes()].into_boxed_slice(),
        profile_names: pool.profiling_names(),
        profile_generation: 0,
        profile_wire: Default::default(),
        profile_catalog: false,
        profile_status: None,
        profile_layout: profiling::Layouts::new(
            &pool,
            config.limits().max_edges,
            description.sample_rate,
            quantum.frames() as u32,
        ),
        control,
        loops: client,
        shape: Shape {
            channels,
            master_channels,
            processor_channels: channels,
        },
        cells: cells.clone(),
        settings: Settings {
            compact: description.audio_render_mode == rt_runtime::AudioRenderMode::Compact,
            audio_only: description.audio_bench_only,
            external_dry_wet: description.external_dry_wet
                && description.audio_render_mode != rt_runtime::AudioRenderMode::Compact,
            ramp: description.ramp_frames,
            lead: description.control_lead_frames,
            smoothing: description.smoothing_frames,
            smoothing_limit: description.smoothing_limit_frames,
            telemetry_interval: description.telemetry_interval_frames,
            channels,
            master_channels,
            host: counts,
            records_per_append: RECORDS_PER_APPEND,
            command_slots: slots,
            max_reliable: description.reliable_credits,
            max_bulk: description.bulk_credits,
        },
        host_ports: host_ports(description),
        model: Model::new(
            strips,
            loops,
            buses,
            description.master_channels,
            master_outputs,
            description.composite_segments,
        ),
        next: Model::new(
            strips,
            loops,
            buses,
            description.master_channels,
            master_outputs,
            description.composite_segments,
        ),
        limits,
        connected: false,
        queue: Ring::new(credits),
        unanswered: 0,
        results: Ring::new(2 * credits + 8),
        pending: vec![None; slots].into_boxed_slice(),
        planned: vec![[None; PLANNED]; loops].into_boxed_slice(),
        job: None,
        batch: vec![
            (
                None,
                model::Change::RemoveStrip {
                    strip: shoop_rt_protocol::StripId(shoop_rt_protocol::SlotId::new(0, 0))
                }
            );
            credits
        ]
        .into_boxed_slice(),
        batch_len: 0,
        touched_strips: vec![0; strips].into_boxed_slice(),
        touched_loops: vec![0; loops].into_boxed_slice(),
        touched_buses: vec![0; buses].into_boxed_slice(),
        blocked_strips: vec![false; strips].into_boxed_slice(),
        blocked_loops: vec![false; loops].into_boxed_slice(),
        blocked_buses: vec![false; buses].into_boxed_slice(),
        blocked_all: false,
        blocked_global: false,
        retired: Ring::new(loops.max(1)),
        fades: vec![pool.bus(0); strips + loops + buses].into_boxed_slice(),
        work,
        graph_epoch: runtime.epoch(),
        topology_epoch: Epoch(0),
        quiet_until: 0,
        resync: None,
        bulk: bulk::Bulk::new(bulk_credits, channels),
        uploads: content::Uploads::new(2 * loops + 2, usize::from(limits.content_chunk_bytes)),
        replacement: replacement::Replacement::new(
            strips,
            loops,
            buses,
            description.composite_segments,
        ),
        family: family::Stage::new(loops),
        input: input::Stage::new(loops, input_capacity),
        composite: composite::Stage::new(description.composite_segments),
        telemetry_frame: None,
        dirty: vec![false; loops].into_boxed_slice(),
        limits_changed: false,
        protocol_error: false,
        preview: None,
        pool,
    };
    let render = Render::new(
        runtime,
        cells.clone(),
        counts,
        description.max_frames,
        description.resource_pages + 128,
    )?;
    Ok(Engine {
        endpoint,
        render,
        host: HostStatus(cells),
    })
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::Ordering;
    use shoop_rt_protocol::{LoopSlotId, Outcome, Request, SlotId, StripId, StripShape};
    use shoop_wasm_test_support::heap::assert_no_heap;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test(no_wasm = "native firmware arena and media-overlap sizing")]
    fn direct_firmware_preparation_preserves_media_overlap_headroom() {
        for (recording_mb, midi_ports, media_mb, arena_mb) in
            [(32, 1, 16, 320), (128, 1, 64, 768), (128, 16, 64, 768)]
        {
            let description = super::EngineDescription {
                history: shoop_rt_protocol::host::InputHistorySettings {
                    seconds: 0,
                    ..Default::default()
                },
                max_frames: 256,
                resource_pages: recording_mb * 1024 * 1024 / 4096,
                host: super::HostPorts::numbered(2, 2, midi_ports),
                midi_bytes: if midi_ports == 1 { 2048 } else { 4096 },
                ..Default::default()
            };
            let (engine, memory) = shoop_wasm_test_support::heap::measure_memory(|| {
                super::prepare(
                    &description,
                    shoop_rt_protocol::DriverStatus {
                        kind: shoop_rt_protocol::DriverKind::Dummy,
                        state: shoop_rt_protocol::DriverRunState::Running,
                        sample_rate: description.sample_rate,
                        buffer_frames: description.max_frames as u32,
                    },
                )
                .unwrap()
            });
            assert!(
                memory.peak < (arena_mb - 3 * media_mb) * 1024 * 1024,
                "recording={recording_mb} MiB, ports={midi_ports}: {memory:?}"
            );
            assert!(!engine
                .endpoint
                .capabilities()
                .contains(shoop_rt_protocol::Capabilities::INPUT_BATCH));
            assert!(!engine
                .endpoint
                .capabilities()
                .contains(shoop_rt_protocol::Capabilities::REPLACING));
            let mut stopped = engine.render.into_stopped();
            stopped.cleanup().unwrap();
        }
    }

    #[shoop_test]
    fn late_control_topology_appends_respect_each_render_callback_budget() {
        let mut host = super::testing::OfflineHost::new(&super::EngineDescription {
            max_frames: 16,
            quantum: 16,
            segments: alloc::vec![super::Segment {
                strips: 1,
                loops: 20,
            }],
            ..super::EngineDescription::default()
        })
        .unwrap();
        host.connect().unwrap();
        fn finish(host: &mut super::testing::OfflineHost, count: usize, late: bool) {
            let mut completed = 0;
            for _ in 0..2_000 {
                assert_no_heap(|| host.process(16)).unwrap();
                loop {
                    if late && completed < count {
                        // The render can advance while a control submission is descheduled.
                        host.endpoint().cells.next_frame.store(0, Ordering::Relaxed);
                    }
                    let Some(message) = assert_no_heap(|| host.receive()) else {
                        break;
                    };
                    if let shoop_rt_protocol::EngineMessage::Result { outcome, .. } = message {
                        assert_eq!(outcome, Outcome::Done);
                        completed += 1;
                    }
                }
                let frame = host.next_frame();
                host.endpoint()
                    .cells
                    .next_frame
                    .store(frame, Ordering::Relaxed);
                if completed == count && host.endpoint().is_idle() {
                    return;
                }
            }
            panic!("topology transaction did not finish: {completed}/{count}");
        }
        finish(&mut host, 0, false);
        let strip = StripId(SlotId::new(0, 1));
        host.request(Request::CreateStrip {
            strip,
            shape: StripShape {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_input: false,
                midi_output: false,
            },
        })
        .unwrap();
        finish(&mut host, 1, false);
        let source = LoopSlotId(SlotId::new(0, 1));
        for index in 0..20 {
            host.request(Request::CreateLoop {
                loop_id: LoopSlotId(SlotId::new(index, 1)),
                strip,
            })
            .unwrap();
        }
        finish(&mut host, 20, false);
        for index in 1..20 {
            host.request(Request::LoopSync {
                loop_id: LoopSlotId(SlotId::new(index, 1)),
                source: Some(source),
            })
            .unwrap();
        }
        finish(&mut host, 19, true);
        for index in 1..20 {
            let id = LoopSlotId(SlotId::new(index, 1));
            assert_eq!(
                host.endpoint().model.find_loop(id).unwrap().sync,
                Some(source)
            );
        }
    }

    #[shoop_test]
    fn offline_minimal_host_resolves_auto_policy_without_implementations() {
        let mut host = super::testing::OfflineHost::new(&super::EngineDescription {
            processors: shoop_rt_protocol::ProcessorSettings::default(),
            ..super::EngineDescription::default()
        })
        .unwrap();
        let catalog = host.endpoint().processor_catalog();
        assert!(catalog.kinds.is_empty());
        assert_eq!(
            catalog.effective.fx_unavailable,
            Some(shoop_rt_protocol::processors::Unavailable::NotCompiled)
        );
        assert_eq!(
            catalog.effective.synth_unavailable,
            Some(shoop_rt_protocol::processors::Unavailable::NotCompiled)
        );
        assert_eq!(catalog.effective.memory_bytes, 256 << 20);
    }

    #[shoop_test]
    fn owners_can_move_to_host_threads() {
        fn send<T: Send>() {}
        fn shared<T: Send + Sync>() {}
        send::<super::ControlEndpoint>();
        send::<super::Render>();
        send::<super::Stopped>();
        shared::<super::HostStatus>();
    }
}
