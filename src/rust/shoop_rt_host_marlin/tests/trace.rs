#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    host::Renderer,
};
use shoop_rt_protocol::{
    immediate::{
        Batch, Code, Command, GraphLoop, PlannedTransition, TransitionMode, TransitionPlan,
    },
    DriverKind, DriverRunState, DriverStatus, EngineMessage,
};
use shoop_rt_transport::{
    carrier::{Endpoint, InProcess, Outbox},
    client::ProtocolClient,
};
use shoop_wasm_test_support::shoop_test;
use std::time::{Duration, Instant};
const Q: usize = 32;
fn driver() -> DriverStatus {
    DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 48000,
        buffer_frames: Q as u32,
    }
}
fn step(r: &mut impl Renderer) -> [f32; Q] {
    let mut a = [0.; Q];
    let mut b = [0.; Q];
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
    assert_eq!(a, b);
    a
}
fn graph(rev: u64, source: Option<u32>) -> Batch {
    Batch::new(
        rev,
        rev,
        &[
            Command::BeginGraph {
                quantum: Q as u32,
                loops: 2,
            },
            Command::ConfigureLoop(GraphLoop {
                slot: 0,
                generation: 1,
                source: None,
                input: Some(0),
                gain: 1.,
                output: false,
                repeat_sync: false,
            }),
            Command::ConfigureLoop(GraphLoop {
                slot: 1,
                generation: 1,
                source,
                input: Some(0),
                gain: 1.,
                output: true,
                repeat_sync: false,
            }),
            Command::ActivateGraph,
        ],
    )
    .unwrap()
}
fn now(slot: u32, mode: TransitionMode) -> Command {
    Command::Transition(
        slot,
        TransitionPlan {
            now: Some(mode),
            first: None,
            second: None,
        },
    )
}
fn drain<E: Endpoint>(
    c: &mut ProtocolClient<InProcess<E>>,
) -> Vec<shoop_rt_protocol::immediate::Response> {
    let mut responses = Vec::new();
    while let Some(m) = c.poll() {
        if let EngineMessage::ImmediateResponse(r) = m {
            responses.push(r);
        }
    }
    responses
}
trait Snap: Endpoint {
    fn snapshot(&self) -> Vec<shoop_rt_protocol::snapshot::Record>;
}
impl Snap for shoop_rt_host_reference::Endpoint {
    fn snapshot(&self) -> Vec<shoop_rt_protocol::snapshot::Record> {
        let mut bytes = vec![0; self.snapshot_capacity()];
        let (_, len) = self.read_snapshot(&mut bytes).unwrap();
        shoop_rt_protocol::snapshot::Records(&bytes[shoop_rt_protocol::snapshot::HEADER..len])
            .map(Result::unwrap)
            .collect()
    }
}
impl Snap for shoop_rt_host_marlin::Endpoint {
    fn snapshot(&self) -> Vec<shoop_rt_protocol::snapshot::Record> {
        let mut bytes = vec![0; self.snapshot_capacity()];
        let (_, len) = self.read_snapshot(&mut bytes).unwrap();
        shoop_rt_protocol::snapshot::Records(&bytes[shoop_rt_protocol::snapshot::HEADER..len])
            .map(Result::unwrap)
            .collect()
    }
}
fn trace<E: Snap>(
    endpoint: E,
    r: &mut impl Renderer,
) -> Vec<([f32; Q], Vec<shoop_rt_protocol::snapshot::Record>)> {
    let mut c = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(64, 8, 0)));
    c.connect().unwrap();
    drain(&mut c);
    c.immediate(graph(0, Some(0))).unwrap();
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        std::thread::sleep(Duration::from_millis(2));
        let rs = drain(&mut c);
        if !rs.is_empty() {
            assert_eq!(rs[0].header.code(), Code::Applied);
            assert_eq!(rs[0].header.graph(), 1);
            break;
        }
        assert!(Instant::now() < until);
        step(r);
    }
    let mut output = Vec::new();
    for (index, commands) in [
        vec![
            now(0, TransitionMode::Recording),
            now(1, TransitionMode::Recording),
        ],
        vec![],
        vec![
            now(0, TransitionMode::Playing),
            now(1, TransitionMode::Playing),
        ],
        vec![],
        vec![Command::LoopGain(1, 0.5)],
        vec![],
        vec![Command::Transition(
            1,
            TransitionPlan {
                now: None,
                first: Some(PlannedTransition {
                    mode: TransitionMode::Stopped,
                    skip: 0,
                }),
                second: Some(PlannedTransition {
                    mode: TransitionMode::Playing,
                    skip: 1,
                }),
            },
        )],
        vec![],
        vec![],
        vec![],
        vec![],
    ]
    .into_iter()
    .enumerate()
    {
        if !commands.is_empty() {
            c.immediate(Batch::new(1, 1, &commands).unwrap()).unwrap();
        }
        let samples = step(r);
        output.push((samples, c.carrier_mut().endpoint().snapshot()));
        std::thread::sleep(Duration::from_millis(2));
        for response in drain(&mut c) {
            assert_eq!(response.header.code(), Code::Applied);
            for record in response.records() {
                let expected = if matches!(record.command(),Command::Transition(_,plan) if plan.first.is_some()||plan.second.is_some())
                {
                    Code::Scheduled
                } else {
                    Code::Applied
                };
                assert_eq!(record.code(), expected, "{record:?}");
            }
        }
        if index == 0 || index == 2 {
            // Exercise recording and playback across an allocation block boundary.
            for _ in 0..1024 / Q {
                let samples = step(r);
                output.push((samples, c.carrier_mut().endpoint().snapshot()));
            }
        }
    }
    c.observe_snapshots(true).unwrap();
    c.immediate(graph(1, None)).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        std::thread::sleep(Duration::from_millis(2));
        let mut published = false;
        while let Some(message) = c.poll() {
            if matches!(
                message,
                EngineMessage::SnapshotConfiguration { graph: 2, .. }
            ) {
                published = true;
            }
        }
        if published {
            break;
        }
        assert!(Instant::now() < until);
        step(r);
    }
    c.immediate(Batch::new(2, 2, &[now(1, TransitionMode::Stopped)]).unwrap())
        .unwrap();
    step(r);
    assert!(c
        .carrier_mut()
        .endpoint()
        .snapshot()
        .iter()
        .any(|r| matches!(
            r,
            shoop_rt_protocol::snapshot::Record::Telemetry(shoop_rt_protocol::Telemetry::Loop {
                mode: shoop_rt_protocol::LoopMode::Stopped,
                ..
            })
        )));
    std::thread::sleep(Duration::from_millis(2));
    let responses = drain(&mut c);
    assert_eq!(responses.len(), 2);
    for response in responses {
        assert_eq!(response.header.graph(), 2);
        assert_eq!(response.header.code(), Code::Applied);
        for record in response.records() {
            assert_eq!(record.code(), Code::Applied);
        }
    }
    for command in [
        now(1, TransitionMode::Stopped),
        Command::Transition(
            1,
            TransitionPlan {
                now: None,
                first: Some(PlannedTransition {
                    mode: TransitionMode::Recording,
                    skip: 0,
                }),
                second: None,
            },
        ),
    ] {
        c.immediate(Batch::new(2, 2, &[command]).unwrap()).unwrap();
        let mut complete = false;
        for _ in 0..50 {
            step(r);
            std::thread::sleep(Duration::from_millis(2));
            for response in drain(&mut c) {
                assert_eq!(response.records()[0].code(), Code::Applied);
                complete = true;
            }
            if complete {
                break;
            }
        }
        assert!(complete);
    }
    assert!(c
        .carrier_mut()
        .endpoint()
        .snapshot()
        .iter()
        .any(|r| matches!(
            r,
            shoop_rt_protocol::snapshot::Record::Telemetry(shoop_rt_protocol::Telemetry::Loop {
                mode: shoop_rt_protocol::LoopMode::Recording,
                ..
            })
        )));
    output
}
#[shoop_test(no_wasm = "native Verilator model")]
fn actual_rtl_matches_reference_record_play_gain_plans_and_live_loading() {
    let reference = shoop_rt_host_reference::prepare(Q as u32, driver(), 1).unwrap();
    let mut rust = reference.renderer;
    let expected = trace(reference.endpoint, &mut rust);
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver(), 1, Q).unwrap();
    let mut renderer = rtl.renderer;
    let actual = trace(rtl.endpoint, &mut renderer);
    assert_eq!(actual, expected);
    renderer.into_inner().stop().unwrap();
}

#[shoop_test(no_wasm = "64-loop native RTL and reference comparison")]
fn sixty_four_loops_record_and_play_with_the_same_samples() {
    use shoop_rt_protocol::immediate::COMMANDS;

    fn render<E: Endpoint>(endpoint: E, renderer: &mut impl Renderer) -> Vec<[f32; Q]> {
        let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(64, 8, 0)));
        client.connect().unwrap();
        drain(&mut client);
        let mut graph = vec![Command::BeginGraph {
            quantum: Q as u32,
            loops: 64,
        }];
        graph.extend((0..64).map(|slot| {
            Command::ConfigureLoop(GraphLoop {
                slot,
                generation: 1,
                source: None,
                input: Some(0),
                gain: 1. / 64.,
                output: true,
                repeat_sync: false,
            })
        }));
        graph.push(Command::ActivateGraph);
        for commands in graph.chunks(COMMANDS) {
            client
                .immediate(Batch::new(0, 0, commands).unwrap())
                .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            step(renderer);
            let replies = drain(&mut client);
            for reply in &replies {
                assert_eq!(reply.header.code(), Code::Applied);
                assert!(reply.records().iter().all(|r| r.code() == Code::Applied));
            }
            if replies.iter().any(|r| {
                r.records()
                    .iter()
                    .any(|r| matches!(r.command(), Command::ActivateGraph))
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
        }

        let mut output = Vec::new();
        for mode in [TransitionMode::Recording, TransitionMode::Playing] {
            let commands: Vec<_> = (0..64).map(|slot| now(slot, mode)).collect();
            for commands in commands.chunks(COMMANDS) {
                client
                    .immediate(Batch::new(1, 1, commands).unwrap())
                    .unwrap();
            }
            let mut acknowledged = 0;
            for _ in 0..8 {
                let samples = step(renderer);
                if mode == TransitionMode::Playing && acknowledged == 64 {
                    output.push(samples);
                }
                for reply in drain(&mut client) {
                    assert_eq!(reply.header.code(), Code::Applied);
                    for record in reply.records() {
                        assert_eq!(record.code(), Code::Applied);
                        acknowledged += 1;
                    }
                }
            }
            assert_eq!(acknowledged, 64);
        }
        assert!(output.iter().all(|samples| samples == &[0.25; Q]));
        output
    }

    let reference = shoop_rt_host_reference::prepare(Q as u32, driver(), 1).unwrap();
    let mut reference_renderer = reference.renderer;
    let expected = render(reference.endpoint, &mut reference_renderer);
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver(), 1, Q).unwrap();
    let mut rtl_renderer = rtl.renderer;
    let actual = render(rtl.endpoint, &mut rtl_renderer);
    assert!(actual.len() >= 4 && expected.len() >= 4);
    assert_eq!(&actual[..4], &expected[..4]);
    rtl_renderer.into_inner().stop().unwrap();
}
#[shoop_test(no_wasm = "native owner thread")]
fn initial_snapshot_and_zero_demand_switch_and_stop_make_progress() {
    let rtl = shoop_rt_host_marlin::prepare(Q as u32, driver(), 1, Q).unwrap();
    let mut renderer = rtl.renderer;
    let mut bytes = vec![0; rtl.endpoint.snapshot_capacity()];
    let (_, len) = rtl.endpoint.read_snapshot(&mut bytes).unwrap();
    assert_eq!(
        shoop_rt_protocol::snapshot::Header::read(&bytes[..len])
            .unwrap()
            .frame,
        0
    );
    let mut c = ProtocolClient::new(InProcess::new(rtl.endpoint, Outbox::new(64, 8, 0)));
    c.connect().unwrap();
    drain(&mut c);
    c.immediate(graph(0, None)).unwrap();
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(1));
        assert!(drain(&mut c).is_empty());
        assert_eq!(renderer.next_frame().0, 0);
    }
    for _ in 0..5 {
        step(&mut renderer);
        std::thread::sleep(Duration::from_millis(2));
        let rs = drain(&mut c);
        if !rs.is_empty() {
            assert_eq!(rs[0].header.code(), Code::Applied);
            renderer.into_inner().stop().unwrap();
            return;
        }
    }
    panic!("pending switch prevented independent render demand");
}

#[shoop_test(no_wasm = "native serialized owner and bounded queues")]
fn command_only_turns_stale_groups_and_full_completions_do_not_need_audio_or_block_stop() {
    use shoop_rt_protocol::{ClientMessage, Lane};
    use shoop_rt_transport::carrier::Endpoint as _;
    let prepared = shoop_rt_host_marlin::prepare(Q as u32, driver(), 1, Q).unwrap();
    let mut render = prepared.renderer;
    let mut c = ProtocolClient::new(InProcess::new(prepared.endpoint, Outbox::new(64, 8, 0)));
    c.connect().unwrap();
    drain(&mut c);
    c.immediate(graph(0, Some(0))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        std::thread::sleep(Duration::from_millis(1));
        step(&mut render);
        if !drain(&mut c).is_empty() {
            break;
        }
        assert!(Instant::now() < deadline);
    }
    let frame = render.next_frame();
    c.immediate(Batch::new(1, 1, &[Command::LoopGain(1, 0.125)]).unwrap())
        .unwrap();
    loop {
        let rs = drain(&mut c);
        if !rs.is_empty() {
            assert_eq!(rs[0].records()[0].code(), Code::Applied);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(render.next_frame(), frame);
    c.immediate(Batch::new(0, 0, &[Command::LoopGain(1, 1.)]).unwrap())
        .unwrap();
    loop {
        let rs = drain(&mut c);
        if !rs.is_empty() {
            assert_eq!(rs[0].header.code(), Code::StaleConfiguration);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let endpoint = c.carrier_mut().endpoint_mut();
    let batch = ClientMessage::Immediate(Batch::new(1, 1, &[Command::BusMute(0, false)]).unwrap());
    let mut no_drain = Outbox::new(0, 0, 0);
    assert_eq!(no_drain.available(Lane::Reliable), 0);
    let mut accepted = 0;
    for _ in 0..1000 {
        if endpoint.try_deliver(batch, &mut no_drain).is_err() {
            break;
        }
        accepted += 1;
    }
    assert!(
        accepted > 0 && accepted < 1000,
        "bounded completion credits must refuse admission"
    );
    for _ in 0..10 {
        endpoint.poll(&mut no_drain);
    }
    assert_eq!(render.next_frame(), frame);
    render.into_inner().stop().unwrap();
}
