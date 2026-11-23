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
const Q: usize = 32;
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
fn response<E: Endpoint>(c: &mut ProtocolClient<InProcess<E>>, r: &mut impl Renderer) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        while let Some(m) = c.poll() {
            if let EngineMessage::ImmediateResponse(response) = m {
                assert_eq!(response.header.code(), Code::Applied);
                for record in response.records() {
                    assert_eq!(record.code(), Code::Applied, "{:?}", record.command());
                }
                return response.header.frame();
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
) -> u64 {
    let mut frame = 0;
    for batch in shoop_rt_protocol::graph::upload(g, Q as u32, revision).unwrap() {
        c.immediate(batch).unwrap();
        frame = response(c, r);
    }
    frame
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

fn composite_round_trip<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 8, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut graph = stereo_graph();
    graph.loops.truncate(1);
    let mut parent = graph.loops[0].clone();
    parent.id = id(120);
    parent.strip = None;
    parent.composite = Some(CompositeView {
        kind: CompositeKind::Explicit,
        version: 1,
        unit_frames: Q as u64,
        iterations: 8,
        sample_rate: 48000,
        quantum: Q as u32,
        spans: vec![
            CompositeSpan {
                target: id(100),
                start: 0,
                end: 2,
                occurrence_start: 0,
                ordinal: 0,
                mode: LoopMode::Recording,
            },
            CompositeSpan {
                target: id(100),
                start: 2,
                end: 8,
                occurrence_start: 2,
                ordinal: 1,
                mode: LoopMode::Playing,
            },
        ]
        .into(),
    });
    graph.loops.push(parent);
    upload(&mut client, &mut renderer, &graph, 0);
    let transition = |slot, mode| {
        Command::Transition(
            slot,
            TransitionPlan {
                now: Some(mode),
                first: None,
                second: None,
            },
        )
    };
    client
        .immediate(Batch::new(1, 1, &[transition(120, TransitionMode::Playing)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert!((0..8).any(|_| step(&mut renderer) == [0.125, 0.125]));
    client
        .immediate(Batch::new(1, 1, &[transition(120, TransitionMode::Stopped)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0., 0.]);

    let plan = graph.loops[1].composite.as_mut().unwrap();
    plan.kind = CompositeKind::Regular;
    plan.version = 2;
    plan.iterations = 16;
    plan.spans = vec![CompositeSpan {
        target: id(100),
        start: 0,
        end: 16,
        occurrence_start: 0,
        ordinal: 0,
        mode: LoopMode::Playing,
    }]
    .into();
    upload(&mut client, &mut renderer, &graph, 1);
    client
        .immediate(Batch::new(2, 2, &[transition(120, TransitionMode::Playing)]).unwrap())
        .unwrap();
    let start = response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);

    let plan = graph.loops[1].composite.as_mut().unwrap();
    plan.version = 3;
    let span = &mut plan.spans.to_mut()[0];
    span.start = 1;
    span.occurrence_start = 1;
    let switched = upload(&mut client, &mut renderer, &graph, 2);
    assert!(switched > start);
    assert_eq!((switched - start) % (16 * Q as u64), 0);
    let mut silent = 0;
    for _ in 0..16 {
        let samples = step(&mut renderer);
        if samples == [0., 0.] {
            silent += 1;
        } else {
            assert_eq!(samples, [0.125, 0.125]);
        }
    }
    assert_eq!(silent, 1);

    client
        .immediate(Batch::new(3, 3, &[transition(120, TransitionMode::Stopped)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0., 0.]);
    graph.loops.pop();
    upload(&mut client, &mut renderer, &graph, 3);
    client
        .immediate(Batch::new(4, 4, &[transition(100, TransitionMode::Playing)]).unwrap())
        .unwrap();
    response(&mut client, &mut renderer);
    assert_eq!(step(&mut renderer), [0.125, 0.125]);
}

#[shoop_test(no_wasm = "actual Marlin simulation and reference renderer")]
fn prepared_composite_hosts_record_and_publish_stopped_and_restart_edits() {
    let driver = DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    };
    let rust = shoop_rt_host_reference::prepare(Q as u32, driver, 1).unwrap();
    composite_round_trip(rust.endpoint, rust.renderer);
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver, 1, Q).unwrap();
    composite_round_trip(rtl.endpoint, rtl.renderer);
}
