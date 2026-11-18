mod support;
use rt_fpga_compiler::{
    graph::{compile, CompileContext},
    StateProgramTarget,
};
use rt_fpga_reference::{
    compiler::RustTarget, fixed::Fixed, jobs::JobProcessor, pipeline::Pipeline, storage::MediaStore,
};
use shoop_backend::{
    Backend, BackendBusControl, BackendBusRequest, BackendPortRole, BackendTrackControl,
    DirectTrackRequest,
};
use shoop_rt_backend::shoop_graph::{Graph, GraphView};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn complete_mirror_graph_compilation_matches_native_gain_balance_monitoring_and_bus_routing() {
    let mut backend = support::stack(&support::description());
    backend.wait_idle();
    let a = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "a".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 2,
        })
        .unwrap();
    let b = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "b".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    let aux = backend
        .create_bus(BackendBusRequest {
            name: "aux".into(),
            channel_count: 2,
            fx: None,
        })
        .unwrap();
    backend.wait_idle();
    for track in [&a, &b] {
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        for (i, port) in track
            .ports
            .iter()
            .filter(|p| p.role == BackendPortRole::AudioInput)
            .enumerate()
        {
            backend
                .set_port_connected(port.id, &format!("audio_in_{}", i + 1), true)
                .unwrap();
        }
    }
    backend
        .set_track_control(a.track_id, BackendTrackControl::InputGainDb(-6.))
        .unwrap();
    backend
        .set_track_control(a.track_id, BackendTrackControl::InputBalance(-0.25))
        .unwrap();
    backend
        .set_track_control(a.track_id, BackendTrackControl::OutputGainDb(-3.))
        .unwrap();
    backend
        .set_track_control(a.track_id, BackendTrackControl::OutputBalance(0.5))
        .unwrap();
    backend
        .set_bus_control(aux.bus_id, BackendBusControl::GainDb(-3.))
        .unwrap();
    backend
        .set_bus_control(aux.bus_id, BackendBusControl::Balance(-0.5))
        .unwrap();
    for (i, port) in a
        .ports
        .iter()
        .filter(|p| p.role == BackendPortRole::AudioOutput)
        .enumerate()
    {
        backend
            .set_mixer_route(port.id, aux.channels[1 - i].id, true)
            .unwrap();
    }
    for (i, port) in b
        .ports
        .iter()
        .filter(|p| p.role == BackendPortRole::AudioOutput)
        .enumerate()
    {
        backend
            .set_mixer_route(port.id, shoop_backend::MASTER_BUS_CHANNEL_IDS[i], true)
            .unwrap();
    }
    for (i, channel) in aux.channels.iter().enumerate() {
        backend
            .set_port_connected(
                channel.output_port_id,
                &format!("audio_out_{}", i + 1),
                true,
            )
            .unwrap();
    }
    for (i, port) in shoop_backend::MASTER_BUS_OUTPUT_PORT_IDS
        .into_iter()
        .enumerate()
    {
        backend
            .set_port_connected(port, &format!("audio_out_{}", i + 1), true)
            .unwrap();
    }
    backend.wait_idle();
    let graph = Graph::capture(&backend.graph_view());
    assert_eq!(graph.loops().count(), 3);
    assert_eq!(graph.buses().count(), 2);
    let logical = compile(
        &graph,
        CompileContext {
            quantum: 16,
            limits: Default::default(),
        },
    )
    .unwrap();
    let mut media = MediaStore::new(1).unwrap();
    let views = (0..6)
        .map(|_| media.create_recording().unwrap().0 as u64)
        .collect::<Vec<_>>();
    let compiled = RustTarget::new(&views).build(&logical).unwrap();
    let r = compiled.resources;
    let mut pipeline = Pipeline::prepare(
        compiled.plan,
        compiled.program,
        compiled.memory,
        r.queue_capacity as usize,
    )
    .unwrap();
    let mut jobs =
        JobProcessor::with_history_slots(16, r.buffers as usize, r.history_slots as usize);
    for input in [[0.25, 0.5], [-0.5, 0.125]] {
        for (ch, value) in input.iter().enumerate() {
            backend.clock_mut().offline.input_mut(ch).fill(*value);
        }
        backend.clock_mut().offline.process(16).unwrap();
        let a = vec![Fixed::from_f32(input[0]); 16];
        let b = vec![Fixed::from_f32(input[1]); 16];
        let mut left = vec![Fixed::ZERO; 16];
        let mut right = left.clone();
        pipeline
            .process(
                &mut jobs,
                &mut media,
                &[&a, &b],
                &mut [&mut left, &mut right],
            )
            .unwrap();
        for (ch, actual) in [&left, &right].into_iter().enumerate() {
            let expected = backend.clock_mut().offline.output(ch);
            for (actual, expected) in actual.iter().zip(expected) {
                assert!(
                    (actual.to_f32() - expected).abs() < 0.000002,
                    "channel={ch} actual={actual:?} expected={expected}"
                );
            }
        }
    }
}
