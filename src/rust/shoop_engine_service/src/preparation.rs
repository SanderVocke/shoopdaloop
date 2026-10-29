use crate::{
    control, dsp, CaptureTap, Config, InitialContent, Prepared, PreparedRoute, Render, ServiceNode,
};
use rt_runtime::{
    executor::{GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::{
        encode_midi_record, CaptureGroupConfig, Description, Format, MidiIndexConfig,
        ResourceStore, UploadId,
    },
    Budgets, Error, Frame, NodeId, PlannerId, Revision,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{Binding, Failure, LoopPlanner, Options, Prepared as LoopParts, Settings},
    Limits, LoopId, LoopKind, LoopSpec, TopologyDescription,
};

pub fn prepare(
    config: Config,
    presets: Vec<dsp::Preset>,
    initial: dsp::PresetId,
) -> Result<Prepared, Failure> {
    prepare_inner(config, presets, initial, None)
}

/// Construct initial immutable stereo/indexed-MIDI media entirely off-render.
pub fn prepare_with_content(
    config: Config,
    presets: Vec<dsp::Preset>,
    initial: dsp::PresetId,
    content: InitialContent,
) -> Result<Prepared, Failure> {
    prepare_inner(config, presets, initial, Some(content))
}

fn prepare_inner(
    config: Config,
    presets: Vec<dsp::Preset>,
    initial: dsp::PresetId,
    content: Option<InitialContent>,
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
    let mut resources = ResourceStore::with_capture_groups(
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
    let imports = content
        .map(|content| import(&mut resources, target, content))
        .transpose()?
        .into_iter()
        .collect::<Vec<_>>();
    let length = imports.first().map_or(0, |input| input.description.frames);
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
                length,
                sync_source: None,
            }],
            ..TopologyDescription::default()
        },
    )?;
    let initial_graph = graph(config.capture, playback, effect, capture);
    let LoopParts {
        planner,
        nodes,
        client,
    } = LoopPlanner::prepare(
        runtime,
        topology,
        &mut resources,
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
        imports,
        Options::default(),
    )?;
    let nodes = nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, ServiceNode::Loop(node)))
        .chain([PreparedNode::new(
            effect,
            ServiceNode::Dsp(prepared_dsp.processor),
        )])
        .collect();
    let (mut control, mut engine) = GraphRuntime::prepare_with_initial_graph(
        runtime,
        nodes,
        planner,
        resources,
        &initial_graph,
        &client.bindings_for(&initial_graph),
    )?;
    control.prepare_submissions()?;
    engine.prepare_graph_staging(1)?;
    let candidate = client.compile_graph(
        engine.catalog(),
        &graph(CaptureTap::Live, playback, effect, capture),
    )?;
    let (client, ingress) = control::channel(
        control,
        client,
        target,
        prepared_dsp.client,
        prepared_dsp.meters,
    );
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

fn import(
    resources: &mut ResourceStore,
    target: LoopId,
    content: InitialContent,
) -> Result<shoop_loops::media::Input, Error> {
    use shoop_loops::media;
    let frames = content.audio[0].len();
    if frames == 0 || content.audio[1].len() != frames {
        return Err(Error::InvalidShape);
    }
    let audio_len = frames.checked_mul(8).ok_or(Error::Capacity)?;
    let midi_len = content.midi.iter().try_fold(0usize, |n, packet| {
        if packet.bytes.len() > 512 {
            return Err(Error::Capacity);
        }
        n.checked_add(12)
            .and_then(|n| n.checked_add(packet.bytes.len()))
            .ok_or(Error::Capacity)
    })?;
    if audio_len > resources.config().resource_bytes()
        || midi_len > resources.config().resource_bytes()
    {
        return Err(Error::Capacity);
    }
    let frames = u64::try_from(frames).map_err(|_| Error::Capacity)?;
    let mut audio = Vec::with_capacity(audio_len);
    for frame in 0..content.audio[0].len() {
        for channel in &content.audio {
            audio.extend_from_slice(&channel[frame].to_le_bytes());
        }
    }
    let mut midi = vec![0; midi_len];
    let mut offset = 0;
    for packet in &content.midi {
        offset += encode_midi_record(packet.frame, &packet.bytes, &mut midi[offset..])?;
    }
    let mut sources = [None; 2];
    for (index, (format, bytes)) in [
        (Format::Audio { channels: 2 }, audio.as_slice()),
        (Format::Midi, midi.as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        let resource = resources.create()?;
        let description = Description {
            format,
            frames,
            bytes: bytes.len(),
        };
        let upload = if format == Format::Midi {
            resources.begin_indexed_midi_upload(
                resource,
                Revision(0),
                description,
                &content.midi_initial,
            )?
        } else {
            resources.begin_upload(resource, Revision(0), description)?
        };
        write_initial(resources, upload, bytes)?;
        resources.commit(upload)?;
        resources.begin_callback();
        let snapshot = resources.snapshot(resource, Frame(0))?;
        let read = resources.read_index(snapshot.id)?;
        sources[index] = Some(media::Source {
            snapshot,
            index: read,
        });
        // Immutable leases own the versions; no mutable startup handle is retained.
        resources.remove(resource)?;
    }
    Ok(media::Input {
        description: media::Description {
            owner: target,
            revision: Revision(0),
            frames,
            channels: 2,
            midi: true,
        },
        audio: sources[0],
        midi: sources[1],
    })
}

fn write_initial(
    resources: &mut ResourceStore,
    upload: UploadId,
    bytes: &[u8],
) -> Result<(), Error> {
    for (index, chunk) in bytes.chunks(32).enumerate() {
        // Preparation-only progress, before any engine callback or admission.
        resources.begin_callback();
        resources.write_upload(upload, index * 32, chunk)?;
    }
    resources.finish_upload(upload)
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
