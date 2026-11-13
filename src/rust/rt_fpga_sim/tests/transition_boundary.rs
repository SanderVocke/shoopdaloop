#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_audio_math::Q824;
use rt_backend_testbench::{production, rtl, spec::*};
use rt_fpga_adapter::{
    looping::{LoopDevice, TpReply},
    protocol,
    transition_service::{Progress, Transfer, TransitionService},
};
use rt_fpga_sim::{Simulator, runtime};
use shoop_wasm_test_support::shoop_test;
use std::path::Path;

fn ready_install(
    owner: &mut TransitionService,
    sim: &mut Simulator<'_>,
    mut transfer: Transfer,
) -> Transfer {
    while transfer.opcode != 4 {
        let reply = sim
            .tp(transfer.opcode, transfer.index, transfer.words)
            .unwrap();
        transfer = match owner.finish(reply).unwrap() {
            Progress::Transfer(next) => next,
            Progress::Reply(reply) => panic!("publication failed: {reply:?}"),
        };
    }
    transfer
}

fn id(slot: u32) -> Identity {
    Identity {
        slot,
        generation: 1,
    }
}
fn scenario() -> Scenario {
    let mut case = Scenario {
        format_version: 1,
        name: "arm-pause-source-chain".into(),
        arithmetic: Arithmetic::Bounded,
        quantum: 32,
        chunk_frames: 32,
        input_ports: 1,
        output_ports: 1,
        request_capacity: 20,
        loops: vec![],
        nodes: vec![],
        connections: vec![],
        media: vec![],
        steps: (0..1024)
            .map(|quantum| Step {
                inputs: vec![(0..32).map(|i| 100003 + quantum * 1013 + i * 37).collect()],
                actions: vec![],
                collect_completed: false,
            })
            .collect(),
    };
    for slot in 0..3 {
        case.loops.push(Loop {
            id: id(slot),
            channels: 1,
            sync_source: Some(id(slot.saturating_sub(1))),
            repeat_sync: true,
            content: None,
        });
        case.nodes.push(Node {
            id: id(2 * slot),
            operation: NodeKind::Playback { target: id(slot) },
        });
        case.nodes.push(Node {
            id: id(2 * slot + 1),
            operation: NodeKind::Capture { target: id(slot) },
        });
        case.connections.push(Connection {
            source: Endpoint {
                node: None,
                port: 0,
            },
            destination: Endpoint {
                node: Some(id(2 * slot + 1)),
                port: 0,
            },
        });
        case.connections.push(Connection {
            source: Endpoint {
                node: Some(id(2 * slot)),
                port: 0,
            },
            destination: Endpoint {
                node: None,
                port: 0,
            },
        });
    }
    let play = Transition::Play {
        position: 0,
        retrigger: true,
    };
    let seek = Transition::Seek {
        position: 32,
        retrigger: true,
    };
    let actions = [
        ("source", 0, 0, Timing::At, play),
        ("follower", 1, 0, Timing::At, play),
        ("last", 2, 0, Timing::At, play),
        ("future-seek", 2, 96, Timing::At, seek),
        (
            "sync-record",
            1,
            32,
            Timing::AfterTicks {
                source: id(0),
                skip: 2,
            },
            Transition::Record,
        ),
        (
            "sync-play",
            1,
            32,
            Timing::AfterTicks {
                source: id(0),
                skip: 3,
            },
            play,
        ),
        ("first-stop", 2, 224, Timing::At, Transition::Stop),
        (
            "cancelled-sync",
            2,
            96,
            Timing::AfterTicks {
                source: id(1),
                skip: 100,
            },
            play,
        ),
        ("absolute-future", 0, 4096, Timing::At, play),
        ("second-stop", 2, 256, Timing::At, Transition::Stop),
        (
            "self-seek",
            0,
            64,
            Timing::AfterTicks {
                source: id(0),
                skip: 4,
            },
            seek,
        ),
        ("superseded-play", 2, 320, Timing::At, play),
        (
            "failed-seek",
            2,
            320,
            Timing::At,
            Transition::Seek {
                position: 1024,
                retrigger: true,
            },
        ),
        ("resume", 2, 640, Timing::At, play),
    ];
    case.steps[0].actions = actions
        .into_iter()
        .map(|(name, target, at, timing, operation)| Action::Transition {
            name: name.into(),
            target: id(target),
            at: at + 256,
            timing,
            operation,
        })
        .collect();
    let mut warmup = vec![];
    for slot in 0..3 {
        warmup.push(Action::Transition {
            name: format!("warmup-record-{slot}"),
            target: id(slot),
            at: 0,
            timing: Timing::At,
            operation: Transition::Record,
        });
        warmup.push(Action::Transition {
            name: format!("warmup-play-{slot}"),
            target: id(slot),
            at: (slot as u64 + 2) * 32,
            timing: Timing::At,
            operation: play,
        });
    }
    warmup.append(&mut case.steps[0].actions);
    case.steps[0].actions = warmup;
    case.nodes.reverse();
    case
}

fn request(action: &Action, sequence: u64) -> shoop_loops::Request {
    let Action::Transition {
        target,
        at,
        timing,
        operation,
        ..
    } = *action
    else {
        panic!("transition fixture")
    };
    let identity =
        |value: Identity| shoop_loops::LoopId::from_parts(value.slot, value.generation).unwrap();
    shoop_loops::Request {
        target: identity(target),
        sequence: rt_runtime::Sequence(sequence),
        at: rt_runtime::Frame(at),
        timing: match timing {
            Timing::At => shoop_loops::Timing::At,
            Timing::AfterTicks { source, skip } => shoop_loops::Timing::AfterTicks {
                source: identity(source),
                skip,
            },
        },
        transition: match operation {
            Transition::Stop => shoop_loops::Transition::Stop,
            Transition::Record => shoop_loops::Transition::Record {
                monitor_previous: false,
            },
            Transition::Play {
                position,
                retrigger,
            } => shoop_loops::Transition::Play {
                position,
                retrigger,
            },
            Transition::Seek {
                position,
                retrigger,
            } => shoop_loops::Transition::Seek {
                position,
                retrigger,
            },
        },
    }
}

#[shoop_test]
fn admitted_actions_and_audio_survive_1024_quanta_without_arm_service() {
    exercise_service_pause(false, false);
}
#[shoop_test]
fn a_held_install_reply_cannot_stall_1024_quanta_of_admitted_audio() {
    exercise_service_pause(true, false);
}
#[shoop_test]
fn a_held_settlement_reply_does_not_gate_the_fabric_boundary() {
    let mut case = scenario();
    case.steps.truncate(8);
    let expected = production::run::<Q824>(&case).unwrap();
    let image = rtl::compile(&case).unwrap();
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-boundary");
    let rt = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&rt, None, true).unwrap();
    sim.load(&image).unwrap();
    let mut owner = TransitionService::new(&image.transition).unwrap();
    for (index, action) in case.steps[0].actions.iter().enumerate() {
        let words = protocol::kernel_transition(request(action, index as u64 + 1)).unwrap();
        owner
            .exchange(4, 0, words, |transfer| {
                sim.tp(transfer.opcode, transfer.index, transfer.words)
            })
            .unwrap();
    }
    let held = sim.hold_command(16, 0, [0; 20]).unwrap();
    assert_eq!(held.0, 0);
    sim.cycle();
    for (step, expected) in case.steps.iter().zip(&expected.quanta) {
        assert_eq!(
            Samples::Q824(sim.run(&image, &step.inputs).unwrap()),
            expected.outputs
        );
        assert_eq!(sim.held_response(), Some(held));
    }
    assert_eq!(sim.frame(), 256);
    assert_eq!(sim.consume_response().unwrap(), held);
}
#[shoop_test]
fn a_partly_staged_membership_keeps_the_old_view_running_for_1024_quanta() {
    exercise_service_pause(false, true);
}
fn exercise_service_pause(hold_install_reply: bool, partial: bool) {
    let case = scenario();
    let expected = production::run::<Q824>(&case).unwrap();
    let image = rtl::compile(&case).unwrap();
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-boundary");
    std::fs::create_dir_all(&cache).unwrap();
    let rt = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&rt, None, true).unwrap();
    sim.load(&image).unwrap();
    let mut owner = TransitionService::new(&image.transition).unwrap();
    let mut identities = vec![];
    let mut held = None;
    for (index, action) in case.steps[0].actions.iter().enumerate() {
        let words = protocol::kernel_transition(request(action, index as u64 + 1)).unwrap();
        let reply = if hold_install_reply && index + 1 == case.steps[0].actions.len() {
            let transfer = owner.begin(4, 0, words).unwrap();
            let transfer = ready_install(&mut owner, &mut sim, transfer);
            let (error, words) = sim
                .hold_command(transfer.opcode, transfer.index, transfer.words)
                .unwrap();
            let reply = TpReply { error, words };
            held = Some(reply);
            reply
        } else {
            owner
                .exchange(4, 0, words, |transfer| {
                    sim.tp(transfer.opcode, transfer.index, transfer.words)
                })
                .unwrap()
        };
        assert_eq!(reply.error, 0);
        identities.push((reply.words[0], reply.words[1]));
    }
    let mut staged = None;
    if partial {
        let mut transfer = owner.begin(22, 0, [0; 20]).unwrap();
        for _ in 0..2 {
            let reply = sim
                .tp(transfer.opcode, transfer.index, transfer.words)
                .unwrap();
            transfer = match owner.finish(reply).unwrap() {
                Progress::Transfer(next) => next,
                other => panic!("incomplete publication ended early: {other:?}"),
            };
        }
        staged = Some(transfer);
    }
    let before = sim.cycles;
    for (step, expected) in case.steps.iter().zip(&expected.quanta) {
        assert_eq!(
            Samples::Q824(sim.run(&image, &step.inputs).unwrap()),
            expected.outputs,
            "frame {}",
            expected.frame
        );
        if let Some(reply) = held {
            assert_eq!(sim.held_response(), Some((reply.error, reply.words)));
        }
    }
    if let Some(reply) = held {
        assert_eq!(sim.consume_response().unwrap(), (reply.error, reply.words));
        owner.finish(reply).unwrap();
    }
    assert!(sim.cycles > before);
    assert_eq!(sim.frame(), 1024 * 32);
    if let Some(mut transfer) = staged {
        loop {
            let reply = sim
                .tp(transfer.opcode, transfer.index, transfer.words)
                .unwrap();
            match owner.finish(reply).unwrap() {
                Progress::Transfer(next) => transfer = next,
                Progress::Reply(reply) => {
                    assert_eq!(reply.error, 0);
                    break;
                }
            }
        }
    }
    let status = sim.command(0, 0, [0; 20]).unwrap();
    let events = &expected.quanta.last().unwrap().events;
    assert_eq!(protocol::get(&status, 192, 32) as usize, events.len());
    for (index, event) in events.iter().enumerate() {
        let raw = sim.command(15, index as u32, [0; 20]).unwrap();
        assert_eq!(protocol::get(&raw, 0, 64), event.frame);
        assert_eq!(protocol::get(&raw, 64, 32), u64::from(event.target.slot));
        assert_eq!(
            protocol::get(&raw, 96, 32),
            u64::from(event.target.generation)
        );
        assert_eq!(
            protocol::get(&raw, 254, 2),
            match event.kind {
                EventKind::Wrap => 1,
                EventKind::Pulse => 2,
                _ => panic!("unexpected final transition"),
            }
        );
    }
    for state in &expected.quanta.last().unwrap().states {
        let raw = sim.command(12, state.id.slot, [0; 20]).unwrap();
        assert_eq!(protocol::get(&raw, 1, 32), u64::from(state.id.generation));
        for (offset, value) in [
            (64, state.content_length),
            (128, state.position),
            (192, state.capture_frames),
            (256, state.cycles),
            (320, state.content_revision),
            (384, state.capture_revision),
        ] {
            assert_eq!(
                protocol::get(&raw, offset, 64),
                value,
                "loop {} field {offset}",
                state.id.slot
            );
        }
        assert_eq!(
            ["stopped", "playing", "recording"][protocol::get(&raw, 33, 2) as usize],
            state.mode
        );
        assert_eq!(protocol::get(&raw, 35, 1) != 0, state.repeat_sync);
        assert_eq!(
            (protocol::get(&raw, 36, 1) != 0).then(|| protocol::get(&raw, 512, 64)),
            state.last_pulse
        );
    }
    let results: Vec<_> = expected.quanta.iter().flat_map(|q| &q.outcomes).collect();
    assert_eq!(results.len(), identities.len());
    let extra = protocol::kernel_transition(request(&case.steps[0].actions[0], 999)).unwrap();
    assert_eq!(owner.begin(4, 0, extra), Err(2));
    for (index, ((slot, generation), action)) in
        identities.iter().zip(&case.steps[0].actions).enumerate()
    {
        let mut words = [0; 20];
        words[0] = *generation;
        let reply = owner
            .exchange(6, *slot, words, |transfer| {
                sim.tp(transfer.opcode, transfer.index, transfer.words)
            })
            .unwrap();
        assert_eq!(reply.error, 0);
        let completed = protocol::completion(&reply.words, *slot).unwrap().unwrap();
        let Action::Transition { name, at, .. } = action else {
            unreachable!()
        };
        let result = results.iter().find(|result| &result.name == name).unwrap();
        assert_eq!(completed.id.generation(), *generation);
        assert_eq!(completed.sequence.0, index as u64 + 1);
        assert_eq!(completed.requested.0, result.requested);
        assert_eq!(completed.effective.0, at.div_ceil(32) * 32);
        let (frame, outcome) = match completed.outcome {
            shoop_loops::Outcome::Applied { frame } => (frame, "applied"),
            shoop_loops::Outcome::Superseded { frame } => (frame, "superseded"),
            shoop_loops::Outcome::Cancelled { frame } => (frame, "cancelled"),
            shoop_loops::Outcome::Failed {
                frame,
                reason: shoop_loops::Error::InvalidState,
            } => (frame, "failed:invalid_state"),
            shoop_loops::Outcome::Failed {
                frame,
                reason: shoop_loops::Error::InvalidFrame,
            } => (frame, "failed:invalid_frame"),
            other => panic!("unexpected outcome {other:?}"),
        };
        assert_eq!(
            (frame.0, outcome),
            (result.terminal, result.outcome.as_str()),
            "{name}"
        );
    }
    let reused = owner.begin(4, 0, extra).unwrap();
    let reused = ready_install(&mut owner, &mut sim, reused);
    assert_eq!((reused.index, protocol::get(&reused.words, 1, 32)), (0, 2));
    for binding in &image.loop_views {
        let media = expected
            .media
            .iter()
            .find(|media| {
                media.name == format!("loop:{}:{}", binding.id.slot, binding.id.generation)
            })
            .unwrap();
        let samples = binding
            .views
            .iter()
            .map(|&view| sim.read_view(u64::from(view), media.frames as u32).unwrap())
            .collect();
        assert_eq!(Samples::Q824(samples), media.samples, "{}", media.name);
    }
}

#[shoop_test]
fn prepared_install_uses_the_actual_device_frame_after_service_delay() {
    let case = scenario();
    let image = rtl::compile(&case).unwrap();
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-boundary");
    let rt = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&rt, None, true).unwrap();
    sim.load(&image).unwrap();
    let mut owner = TransitionService::new(&image.transition).unwrap();
    let action = Action::Transition {
        name: "late".into(),
        target: id(0),
        at: 0,
        timing: Timing::At,
        operation: Transition::Record,
    };
    let transfer = owner
        .begin(
            4,
            0,
            protocol::kernel_transition(request(&action, 1)).unwrap(),
        )
        .unwrap();
    let transfer = ready_install(&mut owner, &mut sim, transfer);
    for step in &case.steps[..3] {
        assert_eq!(sim.run(&image, &step.inputs).unwrap(), vec![vec![0; 32]]);
    }
    assert_eq!(sim.frame(), 96);
    let reply = sim
        .tp(transfer.opcode, transfer.index, transfer.words)
        .unwrap();
    assert!(matches!(
        owner.finish(reply).unwrap(),
        Progress::Reply(TpReply { error: 0, .. })
    ));
    sim.run(&image, &case.steps[3].inputs).unwrap();
    let observed = owner
        .exchange(13, 0, [0; 20], |transfer| {
            sim.tp(transfer.opcode, transfer.index, transfer.words)
        })
        .unwrap()
        .words;
    let mut steps = vec![
        serde_json::json!({"collect_completed":false,"collect":[],"cancel":[],"commands":[]});
        4
    ];
    steps[3]["commands"] = serde_json::json!([{"target":id(0),"sequence":1,"at":0,"timing":{"kind":"at"},"operation":{"kind":"record"}}]);
    let source = serde_json::json!({"format_version":1,"name":"late-install","plan":{"description":image.transition.description,"limits":image.transition.limits},"steps":steps});
    let expected = rt_backend_testbench::transition_fixture::run_transition_oracle(
        serde_json::from_value(source).unwrap(),
    )
    .unwrap();
    let retained = expected.results.last().unwrap().requests[0]
        .as_ref()
        .unwrap();
    assert_eq!(protocol::get(&observed, 448, 64), retained.effective);
    assert_eq!(retained.effective, 96);
    assert_eq!(protocol::get(&observed, 576, 64), 96);
    assert_eq!(protocol::get(&observed, 192, 64), 0);
    assert_eq!(protocol::get(&observed, 37, 3), 1);
}
