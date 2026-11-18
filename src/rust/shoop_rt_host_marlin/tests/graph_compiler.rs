#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
#[path = "../../rt_fpga_compiler/tests/support/mod.rs"]
mod support;
use rt_fpga_compiler::shoop_graph::{Graph, Owner, PortRole};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    host::Renderer,
};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command, TransitionMode, TransitionPlan},
    DriverKind, DriverRunState, DriverStatus, EngineMessage,
};
use shoop_rt_transport::{
    carrier::{Endpoint, InProcess, Outbox},
    client::ProtocolClient,
};
use shoop_wasm_test_support::shoop_test;
use std::time::{Duration, Instant};
use support::*;
const Q: usize = 16;
fn step(r: &mut impl Renderer) -> [f32; 2] {
    let mut a = [0.; Q];
    let mut b = a;
    r.process(
        AudioBlock {
            frame: r.next_frame(),
            frames: Q,
            inputs: &[&[0.25; Q]],
            outputs: &mut [&mut a, &mut b],
        },
        MidiBlock {
            inputs: &[],
            outputs: &mut [],
        },
    )
    .unwrap();
    assert!(a.iter().all(|&s| s == a[0]) && b.iter().all(|&s| s == b[0]));
    [a[0], b[0]]
}
fn response<E: Endpoint>(c: &mut ProtocolClient<InProcess<E>>, r: &mut impl Renderer) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        while let Some(m) = c.poll() {
            if let EngineMessage::ImmediateResponse(response) = m {
                assert_eq!(response.header.code(), Code::Applied);
                return;
            }
        }
        assert!(Instant::now() < deadline, "graph command stalled");
        step(r);
    }
}
fn upload<E: Endpoint>(
    c: &mut ProtocolClient<InProcess<E>>,
    r: &mut impl Renderer,
    g: &Graph,
    revision: u64,
) {
    for batch in shoop_rt_protocol::graph::upload(g, Q as u32, revision).unwrap() {
        c.immediate(batch).unwrap();
        response(c, r);
    }
}
fn stereo_graph() -> Graph {
    let mut graph = session(2);
    for s in &mut graph.strips {
        s.controls.input_monitoring = false;
    }
    let first = graph
        .ports
        .iter()
        .find(|p| p.id.owner == Owner::Host && p.id.role == PortRole::Input && p.channel == 0)
        .unwrap()
        .id;
    let second = graph
        .ports
        .iter()
        .find(|p| p.id.owner == Owner::Host && p.id.role == PortRole::Input && p.channel == 1)
        .unwrap()
        .id;
    for edge in &mut graph.connections {
        if edge.source == second {
            edge.source = first;
        }
    }
    graph.ports.retain(|p| p.id != second);
    graph
}

fn run<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) -> Vec<[f32; 2]> {
    let mut c = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    c.connect().unwrap();
    while c.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops[0].balance = 0.5;
    upload(&mut c, &mut renderer, &graph, 0);
    for mode in [TransitionMode::Recording, TransitionMode::Playing] {
        let commands = (100..103)
            .map(|slot| {
                Command::Transition(
                    slot,
                    TransitionPlan {
                        now: Some(mode),
                        first: None,
                        second: None,
                    },
                )
            })
            .collect::<Vec<_>>();
        c.immediate(Batch::new(1, 1, &commands).unwrap()).unwrap();
        response(&mut c, &mut renderer);
        for _ in 0..8 {
            step(&mut renderer);
            while c.poll().is_some() {}
        }
    }
    let first = step(&mut renderer);
    assert_eq!(first, [0.375, 0.4375]);
    graph.buses[0].muted = true;
    upload(&mut c, &mut renderer, &graph, 1);
    for _ in 0..4 {
        step(&mut renderer);
        while c.poll().is_some() {}
    }
    let next = step(&mut renderer);
    assert_eq!(next, [0.25, 0.25]);
    vec![first, next]
}
#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn complete_stereo_graphs_record_play_and_retain_media_on_both_backends() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    let p = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
    let expected = run(p.endpoint, p.renderer);
    let p = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
    assert_eq!(run(p.endpoint, p.renderer), expected);
}

#[shoop_test(no_wasm = "actual Marlin simulation")]
fn default_rtl_profile_runs_a_routed_64_loop_stereo_graph() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    let prepared = shoop_rt_host_marlin::prepare(Q as u32, driver, 256, Q).unwrap();
    let mut renderer = prepared.renderer;
    let mut client = ProtocolClient::new(InProcess::new(prepared.endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    let template = graph.loops[0].clone();
    graph.loops = (100..164)
        .map(|slot| rt_fpga_compiler::shoop_graph::LoopView {
            id: id(slot),
            ..template.clone()
        })
        .collect();
    upload(&mut client, &mut renderer, &graph, 0);
    assert_eq!(step(&mut renderer), [0., 0.]);
}
