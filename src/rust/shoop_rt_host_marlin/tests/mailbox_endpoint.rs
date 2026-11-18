#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]

#[path = "../../rt_fpga_rtl/tests/support/host_axi.rs"]
mod host_axi;
#[path = "../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;

use host_axi::{build_pipeline, pipeline_contract, Axi};
use mono::{session, MonoLoop};
use rt_fpga_compiler::{shoop_graph::Id, ProgramLimits};
use rt_fpga_rtl::{
    host::{registers::RESULT_OK, Client, Error, Mailbox, Request},
    pipeline::Geometry,
    sim::Sim,
};
use shoop_rt_host_reference::{fpga, program_limits};
use shoop_rt_protocol::{
    immediate::{
        Batch, Code, Command, PlannedTransition, Response, TransitionMode, TransitionPlan,
    },
    DriverKind, DriverRunState, DriverStatus, EngineMessage,
};
use shoop_rt_transport::{
    carrier::{InProcess, Outbox},
    client::ProtocolClient,
};
use shoop_wasm_test_support::shoop_test;
use std::cell::RefCell;

const Q: usize = 32;

fn limit(attempts: usize) -> impl FnMut() -> bool {
    let mut left = attempts;
    move || {
        left = left.saturating_sub(1);
        left != 0
    }
}

fn sample(client: &mut Client<Axi<'_, '_>>, request: Request) -> u32 {
    let response = client
        .exchange(request, limit(10_000))
        .expect("sample exchange");
    assert_eq!(response.code, RESULT_OK);
    response.data[0]
}

fn quantum(sim: &RefCell<Sim<'_>>, client: &mut Client<Axi<'_, '_>>, input: i32) -> [[i32; Q]; 2] {
    for index in 0..Q {
        sample(client, Request::input_sample(index as u32, input));
    }
    {
        let mut sim = sim.borrow_mut();
        sim.model.set("quantum_valid", 1);
        sim.until(200, "accept paced run", |model| {
            model.get("quantum_ready") != 0
        });
        sim.idle(1);
        sim.model.set("quantum_valid", 0);
        sim.until(100_000, "complete paced run", |model| {
            model.get("quantum_done") != 0 || model.get("fault") != 0
        });
        assert_eq!(sim.model.get("fault"), 0);
    }
    core::array::from_fn(|port| {
        core::array::from_fn(|i| {
            sample(client, Request::output_sample((port * Q + i) as u32)) as i32
        })
    })
}

#[shoop_test(no_wasm = "Uses the real AXI mailbox and paced media pipeline in Marlin")]
fn mailbox_endpoint_schedules_sync_and_replays_through_live_graph_changes() {
    let limits = ProgramLimits {
        quantum: Q as u32,
        ..program_limits()
    };
    let geometry = Geometry::new([limits.state_values.into(), 4096, Q as u64], 32);
    let compiled = build_pipeline(&geometry, limits, 16_666);
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    sim.model.set_all(&[("reset", 1), ("epoch", 7)]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.idle(2);
    let sim = RefCell::new(sim);
    let mailbox = Mailbox::open(Axi(&sim), pipeline_contract()).unwrap();
    let mut samples = Client::new(Mailbox::open(Axi(&sim), pipeline_contract()).unwrap());
    let driver = DriverStatus {
        kind: DriverKind::Simulated,
        state: DriverRunState::Starting,
        sample_rate: 48_000,
        buffer_frames: Q as u32,
    };
    let (endpoint, mut owner) = fpga::prepare(mailbox, driver).unwrap();
    owner.initialize(limit(100_000)).unwrap();
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 32, 0)));
    client.connect().unwrap();
    while client.poll().is_some() {}
    let mut commit = |batch: Batch| -> Response {
        client.immediate(batch).unwrap();
        for _ in 0..300 {
            while let Some(message) = client.poll() {
                if let EngineMessage::ImmediateResponse(response) = message {
                    return response;
                }
            }
            owner.step(limit(30_000)).unwrap();
        }
        panic!("FPGA endpoint response stalled");
    };
    let anchor = MonoLoop {
        id: Id {
            slot: 0,
            generation: 1,
        },
        source: None,
        input: true,
        output: false,
        gain: 1.,
        repeat_sync: false,
    };
    let voice = MonoLoop {
        id: Id {
            slot: 1,
            generation: 1,
        },
        source: Some(anchor.id.slot),
        output: true,
        ..anchor
    };
    for batch in shoop_rt_protocol::graph::upload(&session(&[anchor, voice]), Q as u32, 0).unwrap()
    {
        assert_eq!(commit(batch).header.code(), Code::Applied);
    }
    fn transition(id: u32, mode: TransitionMode) -> Command {
        Command::Transition(
            id,
            TransitionPlan {
                now: Some(mode),
                ..Default::default()
            },
        )
    }
    let moved = commit(Batch::new(1, 1, &[transition(0, TransitionMode::Recording)]).unwrap());
    assert_eq!(
        (moved.header.frame(), moved.records()[0].code()),
        (0, Code::Applied)
    );
    for _ in 0..3 {
        assert_eq!(quantum(&sim, &mut samples, 0x100000), [[0; Q]; 2]);
    }
    let scheduled = commit(
        Batch::new(
            1,
            1,
            &[
                transition(0, TransitionMode::Playing),
                Command::Transition(
                    1,
                    TransitionPlan {
                        now: None,
                        first: Some(PlannedTransition {
                            mode: TransitionMode::Recording,
                            skip: 0,
                        }),
                        second: Some(PlannedTransition {
                            mode: TransitionMode::Stopped,
                            skip: 2,
                        }),
                    },
                ),
            ],
        )
        .unwrap(),
    );
    assert_eq!(scheduled.header.frame(), (3 * Q) as u64);
    assert_eq!(scheduled.records()[1].code(), Code::Scheduled);
    for _ in 0..12 {
        assert_eq!(quantum(&sim, &mut samples, 0x200000), [[0; Q]; 2]);
    }
    let extra = MonoLoop {
        id: Id {
            slot: 2,
            generation: 1,
        },
        source: None,
        input: false,
        output: false,
        ..anchor
    };
    for batch in
        shoop_rt_protocol::graph::upload(&session(&[anchor, voice, extra]), Q as u32, 1).unwrap()
    {
        assert_eq!(commit(batch).header.code(), Code::Applied);
    }
    let playing = commit(
        Batch::new(
            2,
            2,
            &[
                Command::LoopGain(1, 0.5),
                transition(1, TransitionMode::Playing),
            ],
        )
        .unwrap(),
    );
    assert_eq!(playing.header.frame(), (15 * Q) as u64);
    assert_eq!(playing.records()[1].code(), Code::Applied);
    for _ in 0..3 {
        assert_eq!(quantum(&sim, &mut samples, 0), [[0x100000; Q]; 2]);
    }

    let many: Vec<_> = (16..80)
        .map(|slot| MonoLoop {
            id: Id {
                slot,
                generation: 1,
            },
            output: true,
            ..anchor
        })
        .collect();
    for batch in shoop_rt_protocol::graph::upload(&session(&many), Q as u32, 2).unwrap() {
        assert_eq!(commit(batch).header.code(), Code::Applied);
    }
    for mode in [TransitionMode::Recording, TransitionMode::Playing] {
        for group in many.chunks(8) {
            let commands: Vec<_> = group
                .iter()
                .map(|voice| transition(voice.id.slot, mode))
                .collect();
            let reply = commit(Batch::new(3, 3, &commands).unwrap());
            assert_eq!(reply.header.code(), Code::Applied);
            assert!(reply.records().iter().all(|r| r.code() == Code::Applied));
        }
        let expected = if mode == TransitionMode::Recording {
            0
        } else {
            64 * 0x10000
        };
        for _ in 0..4 {
            assert_eq!(quantum(&sim, &mut samples, 0x10000), [[expected; Q]; 2]);
        }
    }
    let metrics = samples.metrics().unwrap();
    println!("Paced FPGA test metrics {metrics:?}");
    assert_eq!(metrics.completed_quanta, 26);
    assert!(metrics.peak_quantum_cycles > 0 && metrics.peak_quantum_cycles < 100_000_000 / 1_500);
    assert_eq!(metrics.audio_xruns, 0);
    assert!(sample(&mut samples, Request::storage_usage()) >= 64);
    sim.borrow_mut().model.set("audio_xruns", 3);
    assert_eq!(samples.metrics().unwrap().audio_xruns, 3);
    owner.stop(limit(30_000)).unwrap();
    assert_eq!(sim.borrow().model.get("audio_enable"), 0);
    for gain in 0..12 {
        let batch = Batch::new(3, 3, &[Command::LoopGain(16, gain as f32 / 16.)]).unwrap();
        client.immediate(batch).unwrap();
    }
    samples.abort().unwrap();
    sim.borrow_mut().idle(2);
    assert_eq!(
        samples.start(Request::capture_snapshot(0)),
        Err(Error::Fault)
    );
    assert!(owner.step(limit(30_000)).is_err());
    owner.fail();
    let mut responses = 0;
    let mut disconnected = false;
    while let Some(message) = client.poll() {
        match message {
            EngineMessage::ImmediateResponse(reply) => {
                assert!(!disconnected);
                assert_eq!(reply.header.code(), Code::CannotApply);
                assert_eq!(
                    reply.records()[0].command(),
                    Command::LoopGain(16, responses as f32 / 16.),
                );
                responses += 1;
            }
            EngineMessage::Disconnected(reason) => {
                assert_eq!(reason, shoop_rt_protocol::DisconnectReason::Fault);
                disconnected = true;
            }
            _ => (),
        }
    }
    assert_eq!(responses, 12);
    assert!(disconnected);
}
