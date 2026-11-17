#![cfg(all(feature = "fpga-reference", not(target_arch = "wasm32")))]
use shoop_host::fpga_reference;
use shoop_rt_host_native::{DriverConfig, DummyConfig, DummyMode};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command, GraphLoop},
    EngineMessage,
};
use shoop_wasm_test_support::shoop_test;
use std::time::{Duration, Instant};

#[shoop_test]
fn controlled_reference_startup_and_graph_switch_do_not_require_an_implicit_quantum() {
    let started = fpga_reference::start_driver(
        DriverConfig::Dummy(DummyConfig {
            mode: DummyMode::Controlled,
            sample_rate: 48000,
            buffer_frames: 16,
            ..Default::default()
        }),
        16,
        1,
    )
    .unwrap();
    let demand = started.driver.dummy().unwrap().clone();
    let mut client = started.client;
    client.connect().unwrap();
    for _ in 0..20 {
        while client.poll().is_some() {}
    }
    assert_eq!(started.driver.host().status().next_frame, 0);
    client
        .immediate(
            Batch::new(
                0,
                0,
                &[
                    Command::BeginGraph {
                        quantum: 16,
                        loops: 1,
                    },
                    Command::ConfigureLoop(GraphLoop {
                        slot: 0,
                        generation: 1,
                        source: None,
                        input: None,
                        gain: 1.,
                        output: false,
                        repeat_sync: false,
                    }),
                    Command::ActivateGraph,
                ],
            )
            .unwrap(),
        )
        .unwrap();
    for _ in 0..20 {
        while client.poll().is_some() {}
    }
    assert_eq!(started.driver.host().status().next_frame, 0);
    let mut requested = 0;
    let mut complete = false;
    for _ in 0..50 {
        demand.request_frames(16);
        requested += 16;
        assert!(demand.wait_rendered(Duration::from_secs(5)));
        while let Some(message) = client.poll() {
            if let EngineMessage::ImmediateResponse(response) = message {
                assert_eq!(response.header.code(), Code::Applied);
                complete = true;
            }
        }
        if complete {
            break;
        }
    }
    assert!(
        complete,
        "pending switch must yield to independently requested rendering"
    );
    assert_eq!(started.driver.host().status().next_frame, requested);
}
#[shoop_test]
fn reference_can_be_paced_or_run_continuously_without_frame_requests() {
    for mode in [DummyMode::Automatic, DummyMode::Unpaced] {
        let started = fpga_reference::start_driver(
            DriverConfig::Dummy(DummyConfig {
                mode,
                sample_rate: 48000,
                buffer_frames: 16,
                ..Default::default()
            }),
            16,
            1,
        )
        .unwrap();
        let began = Instant::now();
        while started.driver.host().status().next_frame < 64 {
            assert!(began.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        assert_eq!(
            started.driver.host().status().driver.kind,
            shoop_rt_protocol::DriverKind::Dummy
        );
        drop(started);
    }
}

#[cfg(feature = "cpal")]
#[allow(dead_code)]
#[path = "../../shoop_rt_host_native/tests/support/cpal_mock.rs"]
mod cpal_mock;

#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

#[cfg(feature = "cpal")]
#[shoop_test]
fn reference_uses_actual_cpal_input_and_output_without_allocating_in_callbacks() {
    use shoop_audio_drivers as drivers;
    use shoop_rt_protocol::immediate::{TransitionMode, TransitionPlan};
    use shoop_rt_transport::{
        carrier::{InProcess, Outbox},
        client::ProtocolClient,
    };
    let output = cpal_mock::MockDevice::output("Reference out", 2);
    let input = cpal_mock::MockDevice::input("Reference in", 1);
    let devices = cpal_mock::MockHost::new(vec![output.clone(), input.clone()]);
    let started = drivers::start_cpal(
        &devices,
        &drivers::CpalConfig {
            buffer_frames: Some(32),
            sample_rate: Some(48000),
            ..Default::default()
        },
        &fpga_reference::template(16, 1, 64),
        drivers::MidiDevices::default(),
    )
    .unwrap();
    let mut client = ProtocolClient::new(InProcess::new(
        *started
            .endpoint
            .downcast::<fpga_reference::Endpoint>()
            .unwrap(),
        Outbox::new(64, 8, 0),
    ));
    let output = output.stream().unwrap();
    let input = input.stream().unwrap();
    client.connect().unwrap();
    let mut welcomed = false;
    while let Some(message) = client.poll() {
        if let EngineMessage::Welcome { driver, .. } = message {
            assert_eq!(driver.kind, shoop_rt_protocol::DriverKind::Cpal);
            welcomed = true;
        }
    }
    assert!(welcomed);
    let drive = || {
        input.capture(&[0.25; 32]);
        output.render(32)
    };
    let mut send = |revision, commands: &[Command]| {
        client
            .immediate(Batch::new(revision, revision, commands).unwrap())
            .unwrap();
        for _ in 0..100 {
            drive();
            while let Some(message) = client.poll() {
                if let EngineMessage::ImmediateResponse(response) = message {
                    assert_eq!(response.header.code(), Code::Applied);
                    return;
                }
            }
        }
        panic!("CPAL-driven reference command did not finish");
    };
    send(
        0,
        &[
            Command::BeginGraph {
                quantum: 16,
                loops: 2,
            },
            Command::ConfigureLoop(GraphLoop {
                slot: 0,
                generation: 1,
                source: None,
                input: None,
                gain: 1.,
                output: false,
                repeat_sync: false,
            }),
            Command::ConfigureLoop(GraphLoop {
                slot: 1,
                generation: 1,
                source: None,
                input: Some(0),
                gain: 1.,
                output: true,
                repeat_sync: false,
            }),
            Command::ActivateGraph,
        ],
    );
    let change = |mode| {
        Command::Transition(
            1,
            TransitionPlan {
                now: Some(mode),
                first: None,
                second: None,
            },
        )
    };
    send(1, &[change(TransitionMode::Recording)]);
    for _ in 0..8 {
        drive();
    }
    send(1, &[change(TransitionMode::Stopped)]);
    send(1, &[change(TransitionMode::Playing)]);
    for _ in 0..4 {
        drive();
    }
    assert!(drive().iter().all(|v| *v == 0.25));
    send(1, &[Command::LoopGain(1, 0.5)]);
    for _ in 0..4 {
        drive();
    }
    assert!(drive().iter().all(|v| *v == 0.125));
    assert_eq!(output.heap_operations(), 0);
    assert_eq!(input.heap_operations(), 0);
    drop(client);
    let report = started.host.stop();
    assert!(report.stopped.is_ok());
    assert!(report.midi_reset);
}

#[cfg(feature = "jack")]
#[shoop_test]
fn reference_jack_ports_carry_recorded_audio_and_stop_without_midi_reset_waits() {
    use shoop_rt_protocol::immediate::{TransitionMode, TransitionPlan};
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };
    struct Peer {
        source: jack::Port<jack::AudioOut>,
        sink: jack::Port<jack::AudioIn>,
        seen: Arc<AtomicU32>,
    }
    impl jack::ProcessHandler for Peer {
        fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
            self.source.as_mut_slice(scope).fill(0.25);
            if self.sink.as_slice(scope).iter().all(|v| *v == 0.25) {
                self.seen.fetch_add(1, Ordering::Relaxed);
            }
            jack::Control::Continue
        }
    }
    let (peer, _) = match jack::Client::new(
        "reference-driver-peer",
        jack::ClientOptions::NO_START_SERVER,
    ) {
        Ok(v) => v,
        Err(_) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => return,
        Err(e) => panic!("JACK required: {e}"),
    };
    let source = peer
        .register_port("source", jack::AudioOut::default())
        .unwrap();
    let sink = peer
        .register_port("sink", jack::AudioIn::default())
        .unwrap();
    let source_name = source.name().unwrap();
    let sink_name = sink.name().unwrap();
    let seen = Arc::new(AtomicU32::new(0));
    let peer = peer
        .activate_async(
            (),
            Peer {
                source,
                sink,
                seen: seen.clone(),
            },
        )
        .unwrap();
    let started = fpga_reference::start_driver(
        DriverConfig::Jack(shoop_rt_host_native::JackConfig {
            client_name: "reference-driver".into(),
        }),
        16,
        1,
    )
    .unwrap();
    let name = started.driver.host().instance_name();
    peer.as_client()
        .connect_ports_by_name(&source_name, &format!("{name}:audio_in_1"))
        .unwrap();
    peer.as_client()
        .connect_ports_by_name(&format!("{name}:audio_out_1"), &sink_name)
        .unwrap();
    let mut client = started.client;
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut send = |revision, commands: &[Command]| {
        client
            .immediate(Batch::new(revision, revision, commands).unwrap())
            .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            while let Some(m) = client.poll() {
                if let EngineMessage::ImmediateResponse(r) = m {
                    assert_eq!(r.header.code(), Code::Applied);
                    return;
                }
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    send(
        0,
        &[
            Command::BeginGraph {
                quantum: 16,
                loops: 2,
            },
            Command::ConfigureLoop(GraphLoop {
                slot: 0,
                generation: 1,
                source: None,
                input: None,
                gain: 1.,
                output: false,
                repeat_sync: false,
            }),
            Command::ConfigureLoop(GraphLoop {
                slot: 1,
                generation: 1,
                source: None,
                input: Some(0),
                gain: 1.,
                output: true,
                repeat_sync: false,
            }),
            Command::ActivateGraph,
        ],
    );
    let change = |mode| {
        Command::Transition(
            1,
            TransitionPlan {
                now: Some(mode),
                first: None,
                second: None,
            },
        )
    };
    send(1, &[change(TransitionMode::Recording)]);
    std::thread::sleep(Duration::from_millis(40));
    send(1, &[change(TransitionMode::Stopped)]);
    send(1, &[change(TransitionMode::Playing)]);
    let until = Instant::now() + Duration::from_secs(5);
    while seen.load(Ordering::Relaxed) == 0 {
        assert!(
            Instant::now() < until,
            "JACK reference output never played captured input"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(client);
    drop(started.driver);
    peer.deactivate().unwrap();
}
