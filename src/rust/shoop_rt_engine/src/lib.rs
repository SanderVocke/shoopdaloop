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

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(test)]
mod buffered_tests;

mod bulk;
mod cells;
mod content;
mod description;
mod endpoint;
mod family;
mod graph;
mod jobs;
mod model;
mod node;
mod pool;
mod preview;
mod queue;
mod render;
mod replacement;
mod resync;
mod ring;
mod stream;
mod topology;

#[cfg(feature = "testing")]
pub mod testing;

pub use cells::HostStatus;
pub use description::{EngineDescription, HostPorts, Segment, MAX_HOST_PORTS};
pub use endpoint::ControlEndpoint;
pub use node::{EngineNode, EngineOp, NodeConfiguration};
pub use render::{Phase, Render, Report, Stopped};

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
            limits.strips as usize + limits.loops as usize + limits.buses as usize + 2,
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

fn runtime_capacities(description: &EngineDescription, pool: &Pool, graph_frames: usize) -> Config {
    let (strips, loops) = (pool.strips(), pool.loops());
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
    let ports = strips * (7 * channels + 5)
        + loops * (4 * channels + 2)
        + 3 * buses * master_channels
        + counts.iter().sum::<usize>();
    let edges = strips
        * (2 * channels * (audio_in + audio_out + buses * master_channels) + midi_in + midi_out)
        + loops * (3 * channels + 2)
        + (channels + 1) * (strips + loops)
        + 2 * buses * master_channels * audio_out;
    let page_bytes = description.page_bytes;
    let payload = RECORDS_PER_APPEND.max(loops) * RECORD_BYTES;
    Config {
        sample_rate: description.sample_rate,
        max_frames: graph_frames,
        audio_inputs: audio_in,
        audio_outputs: audio_out,
        midi_inputs: midi_in,
        midi_outputs: midi_out,
        max_nodes: pool.nodes(),
        max_ports: ports,
        max_edges: edges.max(1),
        midi_events_per_port: description.midi_events,
        midi_bytes_per_port: description.midi_bytes,
        scheduled_events: slots,
        resource_slots: 12 * loops + 16,
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
            validation_steps: (3 * pool.nodes() + edges + 2 * loops + 64)
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
    prepare_with_shared(
        description,
        driver,
        shoop_loops::runtime::SharedSettings::default(),
    )
}

/// Prepare optional retrospective input histories and bounded private edit jobs.
/// This does not enable application grab/replacing mode or add wire commands.
pub fn prepare_with_shared(
    description: &EngineDescription,
    driver: DriverStatus,
    shared_settings: shoop_loops::runtime::SharedSettings,
) -> Result<Engine, PrepareError> {
    description.validate()?;
    let pool = Pool::new(&description.segments, description.buses as usize);
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
    let config = runtime_capacities(description, &pool, quantum.frames()).prepare()?;
    let slots = config.limits().command_slots;
    let midi_events = description.midi_events;
    let midi_bytes = description.midi_bytes;
    let mut resources = ResourceStore::with_capture_groups(
        config,
        quantum,
        Some(MidiIndexConfig {
            versions: 6 * loops + 8,
            checkpoints_per_version: 64,
            replay_events: 64,
        }),
        CaptureGroupConfig {
            groups: (2 * loops).max(1),
            members_per_group: 2,
            staging_bytes_per_group: quantum.frames() * channels * 4
                + 12 * midi_events.max(rt_runtime::midi::ResetState::EVENTS)
                + midi_bytes.max(rt_runtime::midi::ResetState::BYTES),
        },
    )?;
    let loop_limits = LoopLimits {
        loop_slots: loops.max(1),
        relations: loops.max(1),
        program_entries: 1,
        program_segments: 1,
        pending_requests: pending + 1,
        journal_events: 0,
    };
    let topology = compile(loop_limits, &TopologyDescription::default())?;
    let cells = Cells::new(
        strips,
        loops,
        buses,
        stream::RING_PAGES * description.page_bytes,
        driver,
    );
    let smoothing = match description.smoothing_frames {
        0 => Smoothing::None,
        frames => Smoothing::LinearRestart { frames },
    };
    let planner_id = PlannerId::from_parts(0, 1)?;
    let bindings = (0..loops)
        .map(|slot| {
            Ok(Binding {
                target: LoopId::from_parts(slot as u32, 1)?,
                playback: pool.playback(slot),
                capture: pool.capture(slot),
                channels,
                midi: true,
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
            observer: Some(Box::new(Observer(cells.clone()))),
        },
    )?;
    if !shared_settings.histories.is_empty() || shared_settings.candidates != 0 {
        if shared_settings.histories.iter().any(|h| {
            h.source as usize >= pool.strips()
                || h.audio.is_some_and(|a| a.channels as usize != channels)
        }) {
            return Err(rt_runtime::Error::InvalidConfig.into());
        }
        planner.prepare_shared(&mut resources, shared_settings)?;
    }
    let settings = NodeSettings {
        cells: cells.clone(),
        channels,
        master_channels,
        frames: quantum.frames(),
        ramp: description.ramp_frames,
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
    let (mut control, mut runtime) = GraphRuntime::prepare_for_host(
        config,
        description.max_frames,
        quantum,
        nodes,
        planner,
        resources,
    )?;
    if quantum.frames() > 1 {
        runtime.prepare_host_buffering()?;
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
            let count = (3 * loops).div_ceil(RECORDS_PER_APPEND);
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
        control,
        loops: client,
        shape: Shape {
            channels,
            master_channels,
        },
        cells: cells.clone(),
        settings: Settings {
            ramp: description.ramp_frames,
            lead: description.control_lead_frames,
            smoothing: description.smoothing_frames,
            smoothing_limit: description.smoothing_limit_frames,
            telemetry_interval: description.telemetry_interval_frames,
            channels,
            master_channels,
            host: counts,
            records_per_append: RECORDS_PER_APPEND,
            frames: description.max_frames as u32,
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
        ),
        next: Model::new(
            strips,
            loops,
            buses,
            description.master_channels,
            master_outputs,
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
        retired: Ring::new(loops.max(1)),
        fades: vec![pool.bus(0); strips + loops + buses].into_boxed_slice(),
        work,
        graph_epoch: runtime.epoch(),
        topology_epoch: Epoch(0),
        quiet_until: 0,
        resync: None,
        bulk: bulk::Bulk::new(bulk_credits, channels),
        uploads: content::Uploads::new(2 * loops + 2, usize::from(limits.content_chunk_bytes)),
        replacement: replacement::Replacement::new(strips, loops, buses),
        family: family::Stage::new(loops),
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
    use shoop_wasm_test_support::shoop_test;

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
