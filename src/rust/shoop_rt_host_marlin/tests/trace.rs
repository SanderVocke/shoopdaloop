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
const Q: usize = 16;
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
            break;
        }
        assert!(Instant::now() < until);
        step(r);
    }
    let mut output = Vec::new();
    for commands in [
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
    ] {
        if !commands.is_empty() {
            c.immediate(Batch::new(1, 1, &commands).unwrap()).unwrap();
        }
        let samples = step(r);
        output.push((samples, c.carrier_mut().endpoint().snapshot()));
        std::thread::sleep(Duration::from_millis(2));
        for response in drain(&mut c) {
            assert_eq!(response.header.code(), Code::Applied);
            for record in response.records() {
                assert!(
                    matches!(record.code(), Code::Applied | Code::Scheduled),
                    "{record:?}"
                );
            }
        }
    }
    c.immediate(graph(1, None)).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        std::thread::sleep(Duration::from_millis(2));
        let rs = drain(&mut c);
        if !rs.is_empty() {
            assert_eq!(rs[0].header.graph(), 2);
            break;
        }
        assert!(Instant::now() < until);
        step(r);
    }
    step(r);
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
