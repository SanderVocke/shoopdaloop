//! The new engine backend's real-time composition.
//!
//! One [`rt_runtime`] graph runtime holds the loop library's playback and
//! capture nodes next to the engine's own processors: a per-strip input stage
//! (gain, balance, monitoring feed, meters), a per-strip output stage (the sum
//! of its loops and monitored input; gain, balance, mute, meters), a fader per
//! loop and the Master bus. Strip and loop slots are prepared in pool segments;
//! only occupied slots are in the active graph and loop topology.
//!
//! [`prepare`] returns three owners: the [`ControlEndpoint`], which implements
//! the protocol's [`Endpoint`](shoop_rt_protocol::carrier::Endpoint) and runs
//! wherever the host pumps its carrier; the [`Render`] owner, called once per
//! audio callback; and the [`HostStatus`] handle for driver status, xruns and
//! load. Both the endpoint and the render path are bounded and allocation-free
//! after preparation.
#![forbid(unsafe_code)]

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod bulk;
mod cells;
mod description;
mod endpoint;
mod graph;
mod jobs;
mod model;
mod node;
mod pool;
mod render;
mod resync;
mod ring;
mod topology;

#[cfg(feature = "testing")]
pub mod testing;

pub use cells::HostStatus;
pub use description::{EngineDescription, HostPorts, Segment, MAX_HOST_PORTS};
pub use endpoint::ControlEndpoint;
pub use node::{EngineNode, EngineOp};
pub use render::{Phase, Render, Report, Stopped};

use crate::cells::{Cells, Observer};
use crate::endpoint::{Settings, Work, PLANNED};
use crate::graph::{Builder, Shape};
use crate::model::Model;
use crate::node::{LoopFader, MasterBus, NodeSettings, StripInput, StripOutput};
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
const RECORDS_PER_APPEND: usize = 16;

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
    pub fn outbox(&self) -> shoop_rt_protocol::carrier::Outbox {
        let limits = self.endpoint.limits();
        shoop_rt_protocol::carrier::Outbox::new(
            2 * usize::from(self.endpoint.settings.max_reliable) + 8,
            limits.strips as usize + limits.loops as usize + 2,
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

/// Prepare an engine for `description`. Allocation happens here only.
pub fn prepare(
    description: &EngineDescription,
    driver: DriverStatus,
) -> Result<Engine, PrepareError> {
    description.validate()?;
    let pool = Pool::new(&description.segments);
    let (strips, loops) = (pool.strips(), pool.loops());
    let channels = description.strip_channels as usize;
    let master_channels = description.master_channels as usize;
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
    let slots = pending + 2 * credits + loops * (PLANNED + 3) + 64;
    let ports = strips * (7 * channels + 5)
        + loops * (4 * channels + 2)
        + 3 * master_channels
        + counts.iter().sum::<usize>();
    let edges = strips
        * (2 * channels * (audio_in + audio_out + master_channels) + midi_in + midi_out)
        + loops * (3 * channels + 2)
        + (channels + 1) * (strips + loops)
        + 2 * master_channels * audio_out;
    let page_bytes = description.page_bytes;
    let payload = RECORDS_PER_APPEND * RECORD_BYTES;
    let config = Config {
        sample_rate: description.sample_rate,
        max_frames: description.max_frames,
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
        resource_io_bytes: 0,
        resource_io_steps: 0,
        command_slots: slots,
        budgets: Budgets {
            commands: slots,
            events: slots,
            subranges: 0,
            transfer_bytes: payload + 2 * page_bytes,
            validation_steps: 3 * pool.nodes() + edges + 2 * loops + 64,
            publications: slots,
        },
    }
    .prepare()?;
    let midi_events = description.midi_events;
    let midi_bytes = description.midi_bytes;
    let mut resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 6 * loops + 8,
            checkpoints_per_version: 64,
            replay_events: 64,
        }),
        CaptureGroupConfig {
            groups: (2 * loops).max(1),
            members_per_group: 2,
            staging_bytes_per_group: description.max_frames * channels * 4
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
    let cells = Cells::new(strips, loops, driver);
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
        planner,
        nodes: loop_nodes,
        client,
    } = LoopPlanner::prepare(
        config,
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
                sources: 0,
                source_packets_per_callback: 0,
            },
            observer: Some(Box::new(Observer(cells.clone()))),
        },
    )?;
    let settings = NodeSettings {
        cells: cells.clone(),
        channels,
        master_channels,
        frames: description.max_frames,
        ramp: description.ramp_frames,
    };
    let mut nodes: Vec<PreparedNode<EngineNode, LoopPlanner, EngineOp>> = loop_nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, EngineNode::Loop(node)))
        .collect();
    nodes.push(PreparedNode::new(
        pool.master(),
        EngineNode::Master(MasterBus::new(&settings)),
    ));
    for slot in 0..strips {
        nodes.push(PreparedNode::new(
            pool.strip_input(slot),
            EngineNode::StripInput(StripInput::new(slot, &settings)),
        ));
        nodes.push(PreparedNode::new(
            pool.strip_output(slot),
            EngineNode::StripOutput(StripOutput::new(slot, &settings)),
        ));
    }
    for slot in 0..loops {
        nodes.push(PreparedNode::new(
            pool.fader(slot),
            EngineNode::Fader(LoopFader::new(slot, &settings)),
        ));
    }
    let (mut control, mut runtime) =
        GraphRuntime::prepare_with_planner_and_resources(config, nodes, planner, resources)?;
    runtime.prepare_graph_staging(1)?;
    control.prepare_submissions()?;
    let planner = runtime
        .catalog()
        .planner()
        .ok_or(rt_runtime::Error::InvalidState)?;
    let mut master_outputs = [0u64; shoop_rt_protocol::MAX_STRIP_CHANNELS];
    for &(channel, host) in &description.master_connections {
        master_outputs[channel as usize] |= 1 << host;
    }
    let encoder = Encoder::new(loop_limits);
    let image = vec![0; encoder.image_bytes()].into_boxed_slice();
    let work = Work {
        builder: Builder::new(config.limits().max_edges, loops),
        encoder,
        image,
        payloads: (1..=RECORDS_PER_APPEND)
            .map(|records| Some(vec![0; records * RECORD_BYTES].into_boxed_slice()))
            .collect(),
        plans: [
            Some(PreparedPlan::with_capacity(config)?),
            Some(PreparedPlan::with_capacity(config)?),
        ],
    };
    let limits = Limits {
        strips: strips as u32,
        loops: loops as u32,
        strip_audio_channels: description.strip_channels,
        master_channels: description.master_channels,
        host_audio_inputs: audio_in as u16,
        host_audio_outputs: audio_out as u16,
        host_midi_inputs: midi_in as u16,
        host_midi_outputs: midi_out as u16,
        max_frames: description.max_frames as u32,
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
        planner,
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
            command_slots: slots,
            max_reliable: description.reliable_credits,
            max_bulk: description.bulk_credits,
        },
        host_ports: host_ports(description),
        model: Model::new(strips, loops, master_outputs),
        next: Model::new(strips, loops, master_outputs),
        limits,
        connected: false,
        queue: Ring::new(credits),
        unanswered: 0,
        results: Ring::new(2 * credits + 8),
        pending: vec![None; slots].into_boxed_slice(),
        planned: vec![[None; PLANNED]; loops].into_boxed_slice(),
        job: None,
        work,
        graph_epoch: runtime.epoch(),
        topology_epoch: Epoch(0),
        quiet_until: 0,
        resync: None,
        bulk: bulk::Bulk::new(bulk_credits, page_bytes, channels),
        telemetry_frame: None,
        dirty: vec![false; loops].into_boxed_slice(),
        limits_changed: false,
        protocol_error: false,
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
