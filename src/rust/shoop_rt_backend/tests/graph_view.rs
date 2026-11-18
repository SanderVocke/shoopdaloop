mod support;
use shoop_backend::{
    Backend, BackendBusControl, BackendBusRequest, BackendPortRole, BackendTrackControl,
    BackendTrackTopology, DirectTrackRequest, TrackRequest,
};
use shoop_rt_backend::shoop_graph::{ChannelLayout, Graph, GraphView, Owner, SignalType};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn mirror_exports_multichannel_midi_buses_controls_and_all_connections() {
    let mut config = support::description();
    config.strip_channels = 4;
    config.external_dry_wet = true;
    let mut backend = support::stack(&config);
    backend.wait_idle();
    let direct = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "stereo-midi".into(),
            audio_channels: 2,
            midi: true,
            initial_loops: 2,
        })
        .unwrap();
    let external = backend
        .create_track(TrackRequest {
            port_name_base: "external".into(),
            topology: BackendTrackTopology::DryWetExternal {
                dry_audio_channels: 2,
                wet_audio_channels: 3,
                dry_midi: true,
            },
            initial_loops: 1,
        })
        .unwrap();
    let bus = backend
        .create_bus(BackendBusRequest {
            name: "quad".into(),
            channel_count: 4,
            fx: None,
        })
        .unwrap();
    backend.wait_idle();
    backend
        .set_loop_sync_source(direct.loops[1], Some(direct.loops[0]))
        .unwrap();
    backend.set_loop_gain(direct.loops[0], 0.5).unwrap();
    backend
        .set_track_control(direct.track_id, BackendTrackControl::InputGainDb(-3.))
        .unwrap();
    backend
        .set_bus_control(bus.bus_id, BackendBusControl::GainDb(-6.))
        .unwrap();
    let input = direct
        .ports
        .iter()
        .find(|p| p.role == BackendPortRole::AudioInput)
        .unwrap()
        .id;
    let midi = direct
        .ports
        .iter()
        .find(|p| p.role == BackendPortRole::MidiInput)
        .unwrap()
        .id;
    let output = direct
        .ports
        .iter()
        .find(|p| p.role == BackendPortRole::AudioOutput)
        .unwrap()
        .id;
    backend
        .set_port_connected(input, "audio_in_1", true)
        .unwrap();
    backend.set_port_connected(midi, "midi_in_1", true).unwrap();
    backend
        .set_mixer_route(output, bus.channels[3].id, true)
        .unwrap();
    backend.wait_idle();
    let graph = Graph::capture(&backend.graph_view());
    assert_eq!(graph.strips.len(), 2);
    assert_eq!(graph.loops.len(), 3);
    assert_eq!(graph.buses.len(), 2);
    let strip = graph
        .strips
        .iter()
        .find(|s| s.name == "stereo-midi")
        .unwrap();
    assert_eq!(
        strip.layout,
        ChannelLayout::Direct {
            audio: 2,
            midi: true
        }
    );
    assert!((strip.controls.input_gain_db + 3.).abs() < 0.001);
    let processed = graph.strips.iter().find(|s| s.name == "external").unwrap();
    assert_eq!(
        processed.layout,
        ChannelLayout::DryWet {
            dry_audio: 2,
            wet_audio: 3,
            dry_midi: true
        }
    );
    assert_eq!(processed.processor.as_ref().unwrap().type_id, "external");
    assert_eq!(external.ports.len(), 12);
    assert!(graph
        .connections
        .iter()
        .any(|c| c.source.signal == SignalType::Midi));
    let quad = graph.buses.iter().find(|b| b.name == "quad").unwrap();
    assert_eq!(quad.channels, 4);
    assert!((quad.gain_db + 6.).abs() < 0.001);
    assert!(graph
        .connections
        .iter()
        .any(|c| c.destination.owner == Owner::Bus(quad.id) && c.destination.index == 3));
    assert_eq!(
        graph
            .loops
            .iter()
            .filter(|l| l.sync_source.is_some())
            .count(),
        1
    );
    assert_eq!(
        graph.loops.iter().find(|l| l.gain == 0.5).unwrap().strip,
        Some(strip.id)
    );
    assert_eq!(Graph::decode(&graph.encode().unwrap()).unwrap(), graph);
    assert_eq!(
        backend.graph_view().connections().count(),
        graph.connections.len()
    );
}

#[shoop_test]
fn compiler_reads_the_mirror_view_directly_with_unused_midi_ports_present() {
    let mut backend = support::stack(&support::description());
    backend.wait_idle();
    backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "mono".into(),
            audio_channels: 1,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    for (i, port) in shoop_backend::MASTER_BUS_OUTPUT_PORT_IDS
        .into_iter()
        .enumerate()
    {
        backend
            .set_port_connected(port, &format!("audio_out_{}", i + 1), true)
            .unwrap();
    }
    backend.wait_idle();
    let view = backend.graph_view();
    assert!(view.ports().any(|p| p.id.signal == SignalType::Midi));
    let output = view
        .ports()
        .find(|p| {
            p.id.owner == Owner::Host
                && p.id.signal == SignalType::Audio
                && p.id.role == shoop_rt_backend::shoop_graph::PortRole::Output
                && p.channel == 0
        })
        .unwrap();
    assert_ne!(output.id.index, output.channel);
    let program = rt_fpga_compiler::graph::compile(
        &view,
        rt_fpga_compiler::graph::CompileContext {
            quantum: 16,
            limits: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(program.loops.len(), 1);
    assert_eq!(program.resources.output_ports, 2);
    let owned = Graph::capture(&view);
    assert_eq!(
        rt_fpga_compiler::graph::compile(
            &owned,
            rt_fpga_compiler::graph::CompileContext {
                quantum: 16,
                limits: Default::default()
            }
        )
        .unwrap(),
        program
    );
}

#[shoop_test]
fn composite_configuration_and_spans_are_visible_without_runtime_telemetry() {
    use shoop_backend::{
        BackendCompositeKind, BackendCompositeSpan, BackendCompositeTarget, BackendLoopMode,
        BackendPreparedCompositePlan,
    };
    let mut backend = support::stack(&support::description());
    backend.wait_idle();
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "child".into(),
            audio_channels: 1,
            midi: false,
            initial_loops: 2,
        })
        .unwrap();
    backend.wait_idle();
    backend
        .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    backend.clock_mut().render(8);
    backend
        .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
        .unwrap();
    backend.wait_idle();
    let composite = backend.create_prepared_composite().unwrap();
    backend.wait_idle();
    let plan = BackendPreparedCompositePlan {
        kind: BackendCompositeKind::Script,
        sync_source: Some(track.loops[1]),
        unit_frames: 2,
        iterations: 4,
        entries: 1,
        spans: vec![BackendCompositeSpan {
            target: BackendCompositeTarget::Loop(track.loops[0]),
            start: 0,
            end: 4,
            occurrence_start: 0,
            ordinal: 0,
            mode: BackendLoopMode::Playing,
        }],
    };
    backend
        .configure_prepared_composite_loop(composite, &plan)
        .unwrap();
    backend.wait_idle();
    let graph = Graph::capture(&backend.graph_view());
    let parent = graph.loops.iter().find(|l| l.composite.is_some()).unwrap();
    let program = parent.composite.as_ref().unwrap();
    assert_eq!(
        program.kind,
        shoop_rt_backend::shoop_graph::CompositeKind::Explicit
    );
    assert_eq!(program.unit_frames, 2);
    assert_eq!(program.iterations, 4);
    assert_eq!(program.spans.len(), 1);
    let source = graph
        .loops
        .iter()
        .filter(|l| l.composite.is_none())
        .nth(1)
        .unwrap()
        .id;
    assert_eq!(parent.sync_source, Some(source));
    assert_ne!(program.spans[0].target, source);
    assert_eq!(program.spans[0].ordinal, 0);
    assert_eq!(Graph::decode(&graph.encode().unwrap()).unwrap(), graph);
}
