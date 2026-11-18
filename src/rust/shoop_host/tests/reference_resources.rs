#![cfg(all(feature = "fpga-reference", not(target_arch = "wasm32")))]
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    host::Renderer,
};
use shoop_rt_protocol::{
    immediate::{
        Batch, Code, Command, GraphLoop, Response, TransitionMode, TransitionPlan, COMMANDS,
    },
    DriverKind, DriverRunState, DriverStatus, EngineMessage,
};
use shoop_rt_transport::{
    carrier::{Endpoint, InProcess, Outbox},
    client::ProtocolClient,
};
use shoop_wasm_test_support::shoop_test;
use std::time::{Duration, Instant};
const Q: usize = 16;
fn driver() -> DriverStatus {
    DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    }
}
fn quantum(renderer: &mut impl Renderer) {
    let mut left = [0.; Q];
    let mut right = [0.; Q];
    renderer
        .process(
            AudioBlock {
                frame: renderer.next_frame(),
                frames: Q,
                inputs: &[&[0.; Q]],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
    assert_eq!(left, [0.; Q]);
    assert_eq!(right, left);
}
fn receive<E: Endpoint>(
    client: &mut ProtocolClient<InProcess<E>>,
    renderer: &mut impl Renderer,
) -> Response {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        while let Some(message) = client.poll() {
            if let EngineMessage::ImmediateResponse(response) = message {
                return response;
            }
        }
        assert!(Instant::now() < deadline, "graph admission stalled");
        quantum(renderer);
    }
}
fn upload<E: Endpoint>(
    client: &mut ProtocolClient<InProcess<E>>,
    renderer: &mut impl Renderer,
    revision: u64,
    slots: std::ops::Range<u32>,
    output: bool,
    self_sync: bool,
) -> Response {
    let mut commands = vec![Command::BeginGraph {
        quantum: Q as u32,
        loops: slots.len() as u32,
    }];
    commands.extend(slots.map(|slot| {
        Command::ConfigureLoop(GraphLoop {
            slot,
            generation: 1,
            source: self_sync.then_some(slot),
            input: Some(0),
            gain: 1.,
            output,
            repeat_sync: false,
        })
    }));
    commands.push(Command::ActivateGraph);
    let mut terminal = None;
    for chunk in commands.chunks(COMMANDS) {
        client
            .immediate(Batch::new(revision, revision, chunk).unwrap())
            .unwrap();
        let response = receive(client, renderer);
        if chunk.contains(&Command::ActivateGraph) {
            terminal = Some(response);
        } else {
            assert_eq!(response.header.code(), Code::Applied);
        }
    }
    terminal.unwrap()
}
fn admission<E: Endpoint>(endpoint: E, mut renderer: impl Renderer) -> Vec<(Code, u64)> {
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(4, 4, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut trace = Vec::new();
    // Exact instruction/job/buffer fit, sparse logical identities, and >32 upload records.
    let first = upload(&mut client, &mut renderer, 0, 1000..1031, false, false);
    assert_eq!(first.header.code(), Code::Applied);
    let revision = first.header.graph();
    trace.push((first.header.code(), revision));
    // Start every recording together: this exercises the worst-case queue requirement.
    let commands = (1000..1031)
        .map(|slot| {
            Command::Transition(
                slot,
                TransitionPlan {
                    now: Some(TransitionMode::Recording),
                    first: None,
                    second: None,
                },
            )
        })
        .collect::<Vec<_>>();
    client
        .immediate(Batch::new(revision, revision, &commands).unwrap())
        .unwrap();
    let recording = receive(&mut client, &mut renderer);
    assert!(recording
        .records()
        .iter()
        .all(|r| r.code() == Code::Applied));
    quantum(&mut renderer);
    // Fits by itself, but both loops and values of the active graph remain owned.
    let refused = upload(
        &mut client,
        &mut renderer,
        revision,
        2000..2002,
        false,
        false,
    );
    assert_eq!(refused.header.code(), Code::LoopCapacity);
    trace.push((refused.header.code(), refused.header.graph()));
    // An invalid upload never publishes a partial graph.
    client
        .immediate(
            Batch::new(
                revision,
                revision,
                &[
                    Command::BeginGraph {
                        quantum: Q as u32,
                        loops: 2,
                    },
                    Command::ActivateGraph,
                ],
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        receive(&mut client, &mut renderer).header.code(),
        Code::CannotApply
    );
    // All 31 source changes and their switch still form one ordered command group.
    // Retained state needs no extra slots after the failed admission.
    let retained = upload(
        &mut client,
        &mut renderer,
        revision,
        1000..1031,
        false,
        true,
    );
    assert_eq!(retained.header.code(), Code::Applied);
    trace.push((retained.header.code(), retained.header.graph()));
    let small = upload(
        &mut client,
        &mut renderer,
        retained.header.graph(),
        2000..2001,
        false,
        false,
    );
    assert_eq!(small.header.code(), Code::Applied);
    let revision = small.header.graph();
    trace.push((small.header.code(), revision));
    // 21 routed loops require 65 audio buffers in a 64-buffer target.
    let large = upload(
        &mut client,
        &mut renderer,
        revision,
        3000..3021,
        true,
        false,
    );
    assert_eq!(large.header.code(), Code::BufferCapacity);
    trace.push((large.header.code(), large.header.graph()));
    let fitting = upload(
        &mut client,
        &mut renderer,
        revision,
        3000..3020,
        true,
        false,
    );
    assert_eq!(fitting.header.code(), Code::Applied);
    trace.push((fitting.header.code(), fitting.header.graph()));
    quantum(&mut renderer);
    trace
}
#[shoop_test]
fn reference_admits_by_compiler_resources_and_live_loader_occupancy() {
    let prepared = shoop_rt_host_reference::prepare(Q as u32, driver(), 1).unwrap();
    admission(prepared.endpoint, prepared.renderer);
}
#[cfg(feature = "rtl-sim")]
#[shoop_test(no_wasm = "actual Marlin model and reference comparison")]
fn marlin_and_reference_make_the_same_resource_admission_decisions() {
    let prepared = shoop_rt_host_reference::prepare(Q as u32, driver(), 1).unwrap();
    let expected = admission(prepared.endpoint, prepared.renderer);
    let prepared = shoop_rt_host_marlin::prepare(Q as u32, driver(), 1, Q).unwrap();
    assert_eq!(admission(prepared.endpoint, prepared.renderer), expected);
}
