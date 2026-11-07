use rt_runtime::{
    graph::{Connection, Endpoint, GraphDescription, PortType, PreparedPlan},
    midi_state::MidiState,
    resource::{
        encode_midi_record, CaptureGroupConfig, Description, Format, MidiIndexConfig,
        ResourceStore, UploadId,
    },
    Budgets, Error, Frame, NodeId, PlannerId, Quantum, Revision,
};
use shoop_example_dsp as dsp;
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{Binding, Failure, Settings},
    Limits, LoopId, LoopKind, LoopSpec, TopologyDescription,
};
use shoop_loops_test_support::{LoopControl, LoopRuntime};

pub type Control = LoopControl<dsp::Dsp, dsp::Operation>;
pub type Runtime = LoopRuntime<dsp::Dsp, dsp::Operation>;
#[derive(Clone, Copy, PartialEq)]
pub enum Tap {
    PostDsp,
    Live,
    DryWet,
}
pub struct MidiPacket {
    pub frame: u64,
    pub bytes: Vec<u8>,
}
pub struct Content {
    pub audio: [Vec<f32>; 2],
    pub midi_initial: MidiState,
    pub midi: Vec<MidiPacket>,
}
pub struct Setup {
    pub control: Control,
    pub runtime: Runtime,
    pub route: PreparedPlan,
    pub target: LoopId,
    pub meters: dsp::Meters,
}
pub fn effect() -> NodeId {
    NodeId::from_parts(2, 1).unwrap()
}

pub fn assemble(
    pages: usize,
    tap: Tap,
    presets: Vec<dsp::Preset>,
    initial: dsp::PresetId,
    content: Option<Content>,
) -> Result<Setup, Failure> {
    let quantum = Quantum::new(1)?;
    let config = rt_runtime::Config {
        max_frames: 64,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        max_nodes: 4,
        max_ports: 24,
        max_edges: 16,
        command_slots: 32,
        scheduled_events: 32,
        midi_events_per_port: 2048,
        midi_bytes_per_port: 8192,
        resource_slots: 32,
        resource_pages: pages,
        page_bytes: 256,
        budgets: Budgets {
            commands: 32,
            events: 32,
            validation_steps: 128,
            transfer_bytes: 1024,
            publications: 16,
        },
        ..rt_runtime::Config::default()
    }
    .prepare()?;
    let prepared = dsp::prepare(dsp::Config::default(), presets, initial)?;
    let mut resources = ResourceStore::with_capture_groups(
        config,
        quantum,
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
    let imports: Vec<_> = content
        .map(|c| import(&mut resources, target, c))
        .transpose()?
        .into_iter()
        .collect();
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
    let (control, mut runtime) = LoopRuntime::prepare(
        config,
        topology,
        resources,
        Settings {
            planner: PlannerId::from_parts(0, 1)?,
            normal_requests: 8,
            media_slots: 8,
            export_slots: 4,
        },
        vec![Binding {
            dry_wet: false,
            auxiliary_channels: 0,
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
        vec![(effect(), prepared.processor)],
        graph(tap, playback, effect(), capture),
    )?;
    runtime.prepare_graph_staging(1)?;
    let route = control.compile_graph(&graph(Tap::Live, playback, effect(), capture))?;
    Ok(Setup {
        control,
        runtime,
        route,
        target,
        meters: prepared.meters,
    })
}

fn import(
    resources: &mut ResourceStore,
    target: LoopId,
    content: Content,
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
        auxiliary: None,
        description: media::Description {
            auxiliary_channels: 0,
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

pub fn graph(tap: Tap, playback: NodeId, effect: NodeId, capture: NodeId) -> GraphDescription {
    let mut graph = GraphDescription {
        nodes: vec![playback, effect, capture],
        connections: Vec::new(),
    };
    for channel in 0..2 {
        let processed = Endpoint::node(effect, PortType::Audio, channel);
        let live = Endpoint::host(PortType::Audio, channel);
        let captured = match tap {
            Tap::PostDsp => processed,
            Tap::Live => live,
            Tap::DryWet if channel == 0 => live,
            Tap::DryWet => processed,
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
            source: if tap == Tap::Live { live } else { processed },
            destination: Endpoint::node(capture, PortType::Midi, 0),
        },
    ]);
    graph
}

pub fn cleanup(stopped: &mut shoop_loops_test_support::StoppedLoops) {
    stopped.cleanup().unwrap();
    let limits = stopped.resources().config().limits();
    for _ in 0..limits.resource_pages + limits.resource_slots {
        stopped.resources().begin_callback();
        stopped.resources().reclaim();
    }
}
