use crate::{control, dsp, CaptureTap, Config, Prepared, PreparedRoute, Render};
use rt_runtime::{
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::{CaptureGroupConfig, MidiIndexConfig, ResourceStore},
    Budgets, Error, NodeId, PlannerId,
};
use shoop_loop_engine::{
    audio::Smoothing,
    compile,
    runtime::{Binding, Engine, Failure, Settings},
    Limits, LoopId, LoopKind, LoopSpec, TopologyDescription,
};

pub fn prepare(
    config: Config,
    presets: Vec<dsp::Preset>,
    initial: dsp::PresetId,
) -> Result<Prepared, Failure> {
    if config.max_frames == 0 || config.max_frames > 64 || config.export_slots == 0 {
        return Err(Error::InvalidConfig.into());
    }
    let mut dsp_config = config.dsp;
    dsp_config.max_frames = config.max_frames;
    let prepared_dsp = dsp::prepare(dsp_config, presets, initial)?;
    let runtime = rt_runtime::Config {
        max_frames: config.max_frames,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        max_nodes: 4,
        max_ports: 24,
        max_edges: 16,
        command_slots: 16,
        scheduled_events: 32,
        midi_events_per_port: 2048,
        midi_bytes_per_port: 8192,
        resource_slots: 32,
        resource_pages: config.resource_pages,
        page_bytes: 256,
        budgets: Budgets {
            commands: 16,
            events: 32,
            subranges: 0,
            validation_steps: 128,
            transfer_bytes: 1024,
            publications: 16,
        },
        ..rt_runtime::Config::default()
    }
    .prepare()?;
    let resources = ResourceStore::with_capture_groups(
        runtime,
        Some(MidiIndexConfig {
            versions: 8,
            checkpoints_per_version: 32,
            replay_events: 128,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 32768,
        },
    )?;
    let target = LoopId::from_parts(0, 1)?;
    let playback = NodeId::from_parts(0, 1)?;
    let capture = NodeId::from_parts(1, 1)?;
    let effect = NodeId::from_parts(2, 1)?;
    let topology = compile(
        Limits {
            loop_slots: 1,
            pending_requests: 9,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: target,
                kind: LoopKind::Primitive,
                length: 0,
                sync_source: None,
            }],
            ..TopologyDescription::default()
        },
    )?;
    let initial_graph = graph(config.capture, playback, effect, capture);
    let (controller, mut engine) = Engine::prepare(
        runtime,
        topology,
        resources,
        Settings {
            planner: PlannerId::from_parts(0, 1)?,
            normal_requests: 8,
            media_slots: 8,
            export_slots: config.export_slots,
        },
        vec![Binding {
            target,
            playback,
            capture,
            channels: 2,
            midi: true,
            gain: 1.,
            smoothing: Smoothing::None,
            midi_events: 2048,
            midi_bytes: 512,
            packet_bytes: 512,
        }],
        Vec::new(),
        vec![(effect, prepared_dsp.processor)],
        initial_graph,
    )?;
    engine.prepare_graph_staging(1)?;
    let candidate =
        controller.compile_graph(&graph(CaptureTap::Live, playback, effect, capture))?;
    let (client, ingress) =
        control::channel(controller, target, prepared_dsp.client, prepared_dsp.meters);
    let route = PreparedRoute {
        plan: candidate,
        identity: client.identity(),
    };
    let render = Render::new(config, engine, ingress, prepared_dsp.render)?;
    Ok(Prepared {
        client,
        render,
        route,
        target,
    })
}

fn graph(tap: CaptureTap, playback: NodeId, effect: NodeId, capture: NodeId) -> GraphDescription {
    let mut graph = GraphDescription {
        nodes: vec![playback, effect, capture],
        connections: Vec::new(),
    };
    for channel in 0..2 {
        let processed = Endpoint::node(effect, PortType::Audio, channel);
        let live = Endpoint::host(PortType::Audio, channel);
        let captured = match tap {
            CaptureTap::PostDsp => processed,
            CaptureTap::Live => live,
            CaptureTap::DryWet if channel == 0 => live,
            CaptureTap::DryWet => processed,
        };
        graph.connections.extend([
            Connection {
                source: live,
                destination: Endpoint::node(effect, PortType::Audio, channel),
            },
            Connection {
                source: Endpoint::node(playback, PortType::Audio, channel),
                destination: Endpoint::node(effect, PortType::Audio, channel + 2),
            },
            Connection {
                source: processed,
                destination: Endpoint::host(PortType::Audio, channel),
            },
            Connection {
                source: captured,
                destination: Endpoint::node(capture, PortType::Audio, channel),
            },
        ]);
    }
    let live = Endpoint::host(PortType::Midi, 0);
    let processed = Endpoint::node(effect, PortType::Midi, 0);
    graph.connections.extend([
        Connection {
            source: live,
            destination: Endpoint::node(effect, PortType::Midi, 0),
        },
        Connection {
            source: Endpoint::node(playback, PortType::Midi, 0),
            destination: Endpoint::node(effect, PortType::Midi, 1),
        },
        Connection {
            source: processed,
            destination: Endpoint::host(PortType::Midi, 0),
        },
        Connection {
            source: if tap == CaptureTap::Live {
                live
            } else {
                processed
            },
            destination: Endpoint::node(capture, PortType::Midi, 0),
        },
    ]);
    graph
}
