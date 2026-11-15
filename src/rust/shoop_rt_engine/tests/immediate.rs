#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod support;
use shoop_rt_protocol::{
    immediate::{Batch, Command, TransitionMode, FIFO_SINGLETONS, STAGED_BATCHES},
    snapshot::{Header, Record, Records, HEADER},
    *,
};
use shoop_rt_transport::carrier::CarrierError;
use shoop_rt_transport::client::ClientError;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn fixture() -> (Harness, (u64, u64)) {
    fixture_for(&description())
}
fn fixture_for(description: &shoop_rt_engine::EngineDescription) -> (Harness, (u64, u64)) {
    let mut h = Harness::new(description);
    h.done(Request::CreateStrip {
        strip: strip(0, 1),
        shape: shape(2, 2, false),
    });
    h.done(Request::CreateLoop {
        loop_id: loop_id(0, 1),
        strip: strip(0, 1),
    });
    h.host.client.observe_snapshots(true).unwrap();
    h.host
        .client
        .configure_snapshots(snapshot::SnapshotDelivery::Pull)
        .unwrap();
    h.host.client.request_snapshot(0).unwrap();
    h.run(16, 16);
    let config = h
        .lifecycle
        .iter()
        .rev()
        .find_map(|m| match m {
            EngineMessage::SnapshotConfiguration { graph, topology } => Some((*graph, *topology)),
            _ => None,
        })
        .unwrap();
    (h, config)
}

fn configuration(h: &mut Harness) -> (u64, u64) {
    h.run(1, 1);
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    let header = Header::read(&bytes[..len]).unwrap();
    (header.graph, header.topology)
}

#[shoop_test]
fn configuration_versions_cover_loop_strip_and_bus_address_reuse() {
    let (mut h, mut previous) = fixture();
    for request in [
        Request::RemoveLoop {
            loop_id: loop_id(0, 1),
        },
        Request::CreateLoop {
            loop_id: loop_id(0, 2),
            strip: strip(0, 1),
        },
        Request::RemoveLoop {
            loop_id: loop_id(0, 2),
        },
        Request::RemoveStrip { strip: strip(0, 1) },
        Request::CreateStrip {
            strip: strip(0, 2),
            shape: shape(2, 2, false),
        },
        Request::CreateBus {
            bus: BusId(SlotId::new(1, 1)),
            channels: 2,
        },
        Request::RemoveBus {
            bus: BusId(SlotId::new(1, 1)),
        },
        Request::CreateBus {
            bus: BusId(SlotId::new(1, 2)),
            channels: 2,
        },
    ] {
        h.done(request);
        let next = configuration(&mut h);
        assert_ne!(
            previous, next,
            "identity change must activate a new version: {request:?}"
        );
        previous = next;
    }
    let remove = h
        .host
        .request(Request::RemoveBus {
            bus: BusId(SlotId::new(1, 2)),
        })
        .unwrap();
    let create = h
        .host
        .request(Request::CreateBus {
            bus: BusId(SlotId::new(1, 3)),
            channels: 2,
        })
        .unwrap();
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let mut reused = false;
    for _ in 0..256 {
        h.run(1, 1);
        let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
        let header = Header::read(&bytes[..len]).unwrap();
        for record in Records(&bytes[HEADER..len]).map(Result::unwrap) {
            if let Record::State(StateRecord::Bus { bus, .. }) = record {
                if bus == BusId(SlotId::new(1, 3)) {
                    assert_ne!((header.graph, header.topology), previous);
                    reused = true;
                }
            }
        }
        if h.results.contains_key(&create) && h.idle() {
            break;
        }
    }
    assert!(reused);
    assert_eq!(h.results.get(&remove), Some(&Outcome::Done));
    assert_eq!(h.results.get(&create), Some(&Outcome::Done));
}

#[shoop_test]
fn partial_callbacks_return_results_before_the_next_snapshot() {
    let mut d = description();
    d.quantum = 16;
    let (mut h, c) = fixture_for(&d);
    let clock = h.host.endpoint().command_clock();
    let frame = clock.frame().0;
    h.lifecycle.clear();
    send(&mut h, c, &[Command::LoopGain(0, 0.25)]);
    assert_no_heap(|| h.host.process(1)).unwrap();
    assert_eq!(clock.frame().0, frame);
    h.pump();
    let responses: Vec<_> = h
        .lifecycle
        .iter()
        .filter_map(|message| match message {
            EngineMessage::ImmediateResponse(response) => Some(*response),
            _ => None,
        })
        .collect();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].header.frame(), frame);
    assert_eq!(responses[0].records()[0].code(), immediate::Code::Applied);
    assert_eq!(gain(&mut h), 1.);
    h.run(15, 1);
    assert_eq!(gain(&mut h), 0.25);
    assert_eq!(clock.frame().0, frame + 16);
}

#[shoop_test]
fn optional_deadlines_use_processing_frames_and_return_every_result_in_order() {
    use immediate::Code;
    let (mut h, c) = fixture();
    let clock = h.host.endpoint().command_clock();
    let frame = clock.frame().0;
    assert!(frame > 0);
    h.lifecycle.clear();
    let groups = [
        Batch::new(
            c.0,
            c.1,
            &[
                Command::LoopGain(0, 0.25),
                Command::LoopGain(u32::MAX, 0.9),
                Command::LoopBalance(0, 2.0),
                Command::LoopGain(0, 0.75),
            ],
        )
        .unwrap()
        .with_deadline(frame),
        Batch::new(c.0, c.1, &[Command::LoopGain(0, 0.9)])
            .unwrap()
            .with_deadline(frame - 1),
        Batch::new(c.0, c.1, &[Command::LoopBalance(0, 0.5)])
            .unwrap()
            .with_deadline(frame + 100),
        Batch::new(c.0.wrapping_add(1), c.1, &[Command::LoopGain(0, 0.1)]).unwrap(),
    ];
    for group in groups {
        assert_no_heap(|| h.host.client.immediate(group)).unwrap();
    }
    assert_eq!(h.host.client.reliable_in_flight(), 0);
    assert_no_heap(|| h.host.process(1)).unwrap();
    assert_eq!(clock.frame().0, frame + 1);
    h.pump();
    let responses: Vec<_> = h
        .lifecycle
        .iter()
        .filter_map(|message| match message {
            EngineMessage::ImmediateResponse(response) => Some(*response),
            _ => None,
        })
        .collect();
    assert_eq!(responses.len(), groups.len());
    for (response, submitted) in responses.iter().zip(groups) {
        assert_eq!(response.header.frame(), frame);
        assert_eq!(response.header.deadline(), submitted.deadline);
        assert!(response
            .records()
            .iter()
            .map(|record| record.command())
            .eq(submitted.commands().iter().copied()));
        assert!(response
            .records()
            .iter()
            .all(|record| record.code() != Code::Unprocessed));
    }
    assert_eq!(
        responses[0]
            .records()
            .iter()
            .map(|r| r.code())
            .collect::<Vec<_>>(),
        [
            Code::Applied,
            Code::TargetUnavailable,
            Code::InvalidValue,
            Code::Applied
        ]
    );
    assert_eq!(responses[1].header.code(), Code::DeadlineMissed);
    assert_eq!(responses[1].records()[0].code(), Code::DeadlineMissed);
    assert_eq!(
        responses[2].records()[0].code(),
        Code::Applied,
        "future expiry never delays execution"
    );
    assert_eq!(responses[3].header.code(), Code::StaleConfiguration);
    assert_eq!(gain(&mut h), 0.75);
    let mut fresh = Harness::new(&description());
    assert_eq!(fresh.host.endpoint().command_clock().frame().0, 0);
    assert_eq!(
        clock.frame().0,
        frame + 1,
        "a new engine owns a fresh clock"
    );
}

#[shoop_test]
fn commands_run_in_order_and_failures_do_not_suppress_later_effects() {
    let (mut h, c) = fixture();
    let l = loop_id(0, 1);
    send(
        &mut h,
        c,
        &[
            Command::LoopGain(l.0.index, 0.25),
            Command::LoopGain(99, 0.9),
            Command::LoopBalance(l.0.index, 2.),
            Command::LoopGain(l.0.index, f32::NAN),
            Command::Transition(l.0.index, TransitionMode::Stopped),
            Command::Transition(l.0.index, TransitionMode::Playing),
            Command::LoopGain(l.0.index, 0.75),
        ],
    );
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.75);
    assert!(h.telemetry.iter().any(|(_, t)| matches!(t,
        Telemetry::Immediate(d) if d.rt_refused == 4 && d.last_rt == 4)));
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test(no_wasm = "native callback timing smoke")]
fn full_callback_scalar_allowance_is_heap_free() {
    use std::time::Instant;
    let mut d = description();
    d.quantum = 16;
    let (mut h, c) = fixture_for(&d);
    let commands = [Command::LoopGain(0, 0.5); 32];
    let mut samples = Vec::new();
    for _ in 0..128 {
        for _ in 0..7 {
            send(&mut h, c, &commands);
        }
        send(&mut h, c, &commands[..24]);
        let start = Instant::now();
        assert_no_heap(|| h.host.run(16, |_, _| 0.)).unwrap();
        samples.push(start.elapsed().as_nanos());
        h.pump();
    }
    samples.sort_unstable();
    eprintln!(
        "248 scalar commands + callback/snapshot, ns: median={}, max={}",
        samples[64], samples[127]
    );
    assert_eq!(gain(&mut h), 0.5);
}
fn gain(h: &mut Harness) -> f32 {
    h.lifecycle.clear();
    h.host.client.request_snapshot(0).unwrap();
    h.pump();
    let mut bytes = Vec::new();
    for m in &h.lifecycle {
        if let EngineMessage::SnapshotReply(c) = m {
            assert_eq!(bytes.len(), c.offset as usize);
            bytes.extend_from_slice(&c.bytes[..usize::from(c.len)]);
        }
    }
    let header = Header::read(&bytes).unwrap();
    assert!(header.records > 0);
    Records(&bytes[HEADER..])
        .find_map(|r| match r.unwrap() {
            Record::LoopLevels { gain, .. } => Some(gain),
            _ => None,
        })
        .unwrap()
}
fn send(h: &mut Harness, config: (u64, u64), commands: &[Command]) {
    assert_no_heap(|| {
        h.host
            .client
            .immediate(Batch::new(config.0, config.1, commands).unwrap())
    })
    .unwrap();
}

#[shoop_test]
fn invalid_target_fails_individually_and_both_paths_keep_arrival_order() {
    let (mut h, c) = fixture();
    h.done(Request::ReplacementBegin { id: 123 });
    send(&mut h, c, &[Command::LoopGain(0, 0.2)]);
    h.run(16, 16);
    assert_eq!(
        gain(&mut h),
        0.2,
        "staging a replacement does not lock live controls"
    );
    h.done(Request::ReplacementCancel { id: 123 });
    send(
        &mut h,
        c,
        &[Command::LoopGain(0, 0.25), Command::OutputGain(99, 0.5)],
    );
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.25);
    assert!(h
        .telemetry
        .iter()
        .any(|(_, t)| matches!(t,Telemetry::Immediate(d) if d.rt_refused == 1 && d.last_rt == 3)));
    send(&mut h, c, &[Command::LoopGain(0, 0.25)]);
    let legacy = h.send(Request::LoopGain {
        loop_id: loop_id(0, 1),
        gain: 0.75,
    });
    send(&mut h, c, &[Command::LoopGain(0, 0.5)]);
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.25);
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.75);
    assert_eq!(h.results.get(&legacy), Some(&Outcome::Done));
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.5);
    send(&mut h, (c.0 + 1, c.1), &[Command::LoopGain(0, 0.75)]);
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.5);
    use shoop_rt_transport::carrier::{Endpoint, Outbox};
    let mut out = Outbox::new(1, 1, 1);
    out.push(EngineMessage::SnapshotReady { sequence: 0 })
        .unwrap();
    assert_no_heap(|| {
        h.host.endpoint().deliver(
            ClientMessage::Request {
                correlation: u64::MAX,
                request: Request::LoopGain {
                    loop_id: loop_id(0, 1),
                    gain: 0.75,
                },
            },
            &mut out,
        )
    });
    assert_eq!(
        out.pop(),
        Some(EngineMessage::SnapshotReady { sequence: 0 })
    );
    assert_no_heap(|| h.host.endpoint().poll(&mut out));
    assert_no_heap(|| h.host.process(16)).unwrap();
    assert_no_heap(|| h.host.endpoint().poll(&mut out));
    assert_eq!(
        out.pop(),
        Some(EngineMessage::Result {
            correlation: u64::MAX,
            outcome: Outcome::Done
        })
    );
}

#[shoop_test]
fn graph_edits_wait_for_groups_preserve_scalars_and_reject_old_configuration() {
    let (mut h, c) = fixture();
    let commands = [
        Command::LoopGain(0, 0.25),
        Command::LoopBalance(0, -0.5),
        Command::OutputGain(0, 0.5),
        Command::OutputMute(0, true),
        Command::BusGain(0, 0.75),
        Command::BusMute(0, true),
    ];
    send(&mut h, c, &commands);
    let edit = h.send(Request::CreateLoop {
        loop_id: loop_id(1, 1),
        strip: strip(0, 1),
    });
    send(&mut h, c, &[Command::LoopGain(0, 0.9)]);
    h.run(1024, 16);
    assert_eq!(h.results.get(&edit), Some(&Outcome::Done));
    assert_eq!(gain(&mut h), 0.25);
    assert!(h.telemetry.iter().any(|(_, t)| matches!(t,
        Telemetry::Immediate(d) if d.rt_refused == 1 && d.last_rt == 2)));
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    let header = Header::read(&bytes[..len]).unwrap();
    assert_ne!((header.graph, header.topology), c);
    let records: Vec<_> = Records(&bytes[HEADER..len]).map(Result::unwrap).collect();
    for record in [
        Record::LoopLevels {
            id: loop_id(0, 1),
            gain: 0.25,
            balance: -0.5,
            repeat_sync: false,
        },
        Record::State(StateRecord::StripControl {
            strip: strip(0, 1),
            control: StripControl::OutputGain(0.5),
        }),
        Record::State(StateRecord::StripControl {
            strip: strip(0, 1),
            control: StripControl::OutputMute(true),
        }),
        Record::State(StateRecord::Bus {
            bus: BusId::MASTER,
            control: BusControl::Gain(0.75),
        }),
        Record::State(StateRecord::Bus {
            bus: BusId::MASTER,
            control: BusControl::Mute(true),
        }),
    ] {
        assert!(records.contains(&record), "{record:?}");
    }
    send(
        &mut h,
        (header.graph, header.topology),
        &[Command::LoopGain(0, 0.5)],
    );
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.5);
    h.done(Request::RemoveLoop {
        loop_id: loop_id(1, 1),
    });
}

#[shoop_test]
fn recording_and_media_edits_resume_between_immediate_groups() {
    let (mut h, _) = fixture();
    let l = loop_id(0, 1);
    h.done(Request::Connect {
        port: port(strip(0, 1), PortKind::AudioIn, 0),
        host: audio_in(0),
        connected: true,
    });
    h.input = Box::new(|_, _| 1.);
    h.run(16, 16);
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    let header = Header::read(&bytes[..len]).unwrap();
    let c = (header.graph, header.topology);
    send(&mut h, c, &[Command::LoopGain(l.0.index, 0.25)]);
    h.done(Request::LoopTransition {
        loop_id: l,
        mode: LoopMode::Recording,
        delay: None,
    });
    h.run(8, 1);
    send(
        &mut h,
        c,
        &[
            Command::Transition(l.0.index, TransitionMode::Playing),
            Command::LoopGain(l.0.index, 0.9),
        ],
    );
    h.run(16, 16);
    assert_eq!(
        gain(&mut h),
        0.9,
        "the recording start fails individually while the gain still changes"
    );
    send(&mut h, c, &[Command::LoopGain(l.0.index, 0.5)]);
    let stop = h.send(Request::LoopTransition {
        loop_id: l,
        mode: LoopMode::Stopped,
        delay: None,
    });
    send(
        &mut h,
        c,
        &[Command::Transition(l.0.index, TransitionMode::Playing)],
    );
    h.run(128, 16);
    assert_eq!(h.results.get(&stop), Some(&Outcome::Done));
    assert_eq!(gain(&mut h), 0.5);
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    assert!(Records(&bytes[HEADER..len]).any(|r| matches!(r,
        Ok(Record::Telemetry(Telemetry::Loop { loop_id, mode: LoopMode::Playing, .. })) if loop_id == l)),
        "{:?}", Records(&bytes[HEADER..len]).collect::<Vec<_>>());
    let (frames, audio, _) = h.read(l, BulkSelector::AudioFrames);
    assert!(frames >= 8);
    assert!(audio.chunks_exact(2).all(|frame| frame == [1., 0.]));
    h.done(Request::LoopTransition {
        loop_id: l,
        mode: LoopMode::Stopped,
        delay: None,
    });
    h.done(Request::LoopClear { loop_id: l });
    send(&mut h, c, &[Command::LoopGain(l.0.index, 0.75)]);
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.75);
}

#[shoop_test]
fn prepared_replacement_can_commit_after_live_groups_without_retargeting_them() {
    let (mut h, c) = fixture();
    h.done(Request::ReplacementBegin { id: 7 });
    h.done(Request::ReplacementEdit {
        id: 7,
        edit: ReplacementEdit::Strip {
            strip: strip(0, 2),
            shape: shape(2, 2, false),
            controls: StripPreset {
                input_gain: 1.,
                input_balance: 0.,
                monitoring: false,
                automatic_monitoring: false,
                output_gain: 0.75,
                output_balance: 0.,
                output_mute: false,
            },
        },
    });
    h.done(Request::ReplacementEdit {
        id: 7,
        edit: ReplacementEdit::Loop {
            loop_id: loop_id(0, 2),
            strip: strip(0, 2),
            sync: None,
            frames: 0,
            dry_audio: None,
            recorded_state: None,
            audio: None,
            midi: None,
            gain: 0.6,
            balance: 0.,
        },
    });
    h.done(Request::ReplacementPrepare { id: 7 });
    assert_eq!(
        configuration(&mut h),
        c,
        "candidate preparation preserves the active version"
    );
    let mut prepared = vec![0; h.host.endpoint().snapshot_capacity()];
    let (_, len) = h.host.endpoint().read_snapshot(&mut prepared).unwrap();
    let identities: Vec<_> = Records(&prepared[HEADER..len])
        .map(Result::unwrap)
        .collect();
    assert!(identities
        .iter()
        .any(|r| matches!(r, Record::LoopLevels { id, .. } if *id == loop_id(0, 1))));
    assert!(identities.iter().any(|r| matches!(r, Record::State(StateRecord::StripControl { strip: id, .. }) if *id == strip(0, 1))));
    assert!(!identities.iter().any(|r| matches!(r, Record::State(StateRecord::StripControl { strip: id, .. }) if *id == strip(0, 2))));
    send(
        &mut h,
        c,
        &[Command::LoopGain(0, 0.25), Command::OutputGain(0, 0.5)],
    );
    let commit = h.send(Request::ReplacementCommit { id: 7 });
    send(&mut h, c, &[Command::LoopGain(0, 0.9)]);
    h.run(1024, 16);
    assert_eq!(h.results.get(&commit), Some(&Outcome::Done));
    assert_eq!(
        gain(&mut h),
        0.6,
        "replacement's explicit settings become authoritative"
    );
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    let header = Header::read(&bytes[..len]).unwrap();
    let next = (header.graph, header.topology);
    assert_ne!(c, next);
    assert!(Records(&bytes[HEADER..len]).any(|r| r
        == Ok(Record::State(StateRecord::StripControl {
            strip: strip(0, 2),
            control: StripControl::OutputGain(0.75)
        }))));
    send(
        &mut h,
        next,
        &[Command::LoopGain(99, 0.8), Command::OutputMute(0, true)],
    );
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.6);
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    assert!(Records(&bytes[HEADER..len]).any(|r| r
        == Ok(Record::State(StateRecord::StripControl {
            strip: strip(0, 2),
            control: StripControl::OutputMute(true)
        }))));
    send(&mut h, next, &[Command::LoopGain(0, 0.8)]);
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.8);
    h.done(Request::RemoveLoop {
        loop_id: loop_id(0, 2),
    });
}

#[shoop_test]
fn mixed_scalars_publish_a_single_consistent_snapshot() {
    let (mut h, c) = fixture();
    let s = strip(0, 1);
    let l = loop_id(0, 1);
    let b = BusId::MASTER;
    send(
        &mut h,
        c,
        &[
            Command::LoopGain(l.0.index, 0.25),
            Command::LoopBalance(l.0.index, -0.5),
            Command::InputGain(s.0.index, 0.5),
            Command::InputBalance(s.0.index, 0.25),
            Command::OutputGain(s.0.index, 0.75),
            Command::OutputBalance(s.0.index, -0.25),
            Command::OutputMute(s.0.index, true),
            Command::BusGain(b.0.index, 0.5),
            Command::BusBalance(b.0.index, 0.25),
            Command::BusMute(b.0.index, true),
        ],
    );
    h.run(16, 16);
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    let records: Vec<_> = Records(&bytes[HEADER..len]).map(Result::unwrap).collect();
    assert!(records.contains(&Record::LoopLevels {
        id: l,
        gain: 0.25,
        balance: -0.5,
        repeat_sync: false
    }));
    for control in [
        StripControl::InputGain(0.5),
        StripControl::InputBalance(0.25),
        StripControl::OutputGain(0.75),
        StripControl::OutputBalance(-0.25),
        StripControl::OutputMute(true),
    ] {
        assert!(records.contains(&Record::State(StateRecord::StripControl {
            strip: s,
            control
        })));
    }
    for control in [
        BusControl::Gain(0.5),
        BusControl::Balance(0.25),
        BusControl::Mute(true),
    ] {
        assert!(records.contains(&Record::State(StateRecord::Bus { bus: b, control })));
    }
}

#[shoop_test]
fn bounded_staging_preserves_order_and_disconnect_does_not_replay_unsent_work() {
    let (mut h, c) = fixture();
    let admitted = FIFO_SINGLETONS + STAGED_BATCHES;
    for i in 0..admitted {
        send(&mut h, c, &[Command::LoopGain(0, i as f32 / 1000.)]);
    }
    let batch = Batch::new(c.0, c.1, &[Command::LoopGain(0, 0.9)]).unwrap();
    assert_eq!(
        h.host.client.immediate(batch),
        Err(ClientError::Carrier(CarrierError::Full))
    );
    assert_eq!(h.host.client.reliable_in_flight(), 0);
    h.run(16, 16);
    assert_eq!(gain(&mut h), (FIFO_SINGLETONS - 1) as f32 / 1000.);
    h.run(16, 16);
    assert_eq!(gain(&mut h), (admitted - 1) as f32 / 1000.);
    for i in 0..admitted {
        send(&mut h, c, &[Command::LoopGain(0, i as f32 / 1000.)]);
    }
    h.host.disconnect();
    h.pump();
    h.host.connect().unwrap();
    h.pump();
    h.host.client.observe_snapshots(true).unwrap();
    h.host
        .client
        .configure_snapshots(snapshot::SnapshotDelivery::Pull)
        .unwrap();
    let edit = h.send(Request::CreateLoop {
        loop_id: loop_id(1, 1),
        strip: strip(0, 1),
    });
    assert!(!h.results.contains_key(&edit));
    h.run(16, 16);
    h.settle();
    assert_eq!(h.results.get(&edit), Some(&Outcome::Done));
    assert!(
        h.lifecycle
            .iter()
            .all(|message| !matches!(message, EngineMessage::ImmediateResponse(_))),
        "responses from the disconnected command session must not reach its replacement"
    );
    assert_eq!(
        gain(&mut h),
        (FIFO_SINGLETONS - 1) as f32 / 1000.,
        "only RT-published groups survive disconnect"
    );
    let current = configuration(&mut h);
    h.lifecycle.clear();
    send(&mut h, current, &[Command::LoopGain(0, 0.5)]);
    h.run(1, 1);
    let responses: Vec<_> = h
        .lifecycle
        .iter()
        .filter_map(|message| match message {
            EngineMessage::ImmediateResponse(response) => Some(response),
            _ => None,
        })
        .collect();
    assert_eq!(responses.len(), 1);
    assert_eq!(
        responses[0].records()[0].command(),
        Command::LoopGain(0, 0.5)
    );
    assert_eq!(
        h.apply(Request::RemoveLoop {
            loop_id: loop_id(0, 1)
        }),
        Outcome::Done
    );
}

#[shoop_test]
fn encoded_fragmentation_and_paused_receiver_preserve_bounded_command_delivery() {
    use shoop_rt_transport::{
        buffer::Stream,
        carrier::{RecordPort, RecordServer},
        framing::record_len,
    };
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    struct Port {
        input: Rc<RefCell<Stream>>,
        taken: Rc<Cell<usize>>,
        paused: Rc<Cell<bool>>,
        closed: Rc<Cell<bool>>,
        responses: Rc<RefCell<Vec<immediate::Response>>>,
    }
    impl RecordPort for Port {
        fn post(&mut self, bytes: &[u8]) -> Result<(), CarrierError> {
            if self.paused.get() {
                Err(CarrierError::Full)
            } else {
                if let EngineMessage::ImmediateResponse(response) =
                    codec::decode_engine(bytes).unwrap()
                {
                    self.responses.borrow_mut().push(response);
                }
                Ok(())
            }
        }
        fn take(&mut self, out: &mut [u8]) -> Option<usize> {
            let mut stream = self.input.borrow_mut();
            let len = record_len(stream.pending()).unwrap()?;
            out[..len].copy_from_slice(&stream.pending()[..len]);
            stream.consume(len);
            stream.compact();
            self.taken.set(self.taken.get() + 1);
            Some(len)
        }
        fn failure(&mut self) -> Option<CarrierError> {
            self.closed.get().then_some(CarrierError::Disconnected)
        }
    }
    fn append(input: &Rc<RefCell<Stream>>, data: &[u8]) {
        let mut stream = input.borrow_mut();
        stream.free_mut()[..data.len()].copy_from_slice(data);
        assert!(stream.commit(data.len()));
    }
    fn observed(endpoint: &shoop_rt_engine::ControlEndpoint) -> f32 {
        let mut bytes = vec![0; 100_000];
        let (_, len) = endpoint.read_snapshot(&mut bytes).unwrap();
        Records(&bytes[HEADER..len])
            .find_map(|r| match r.unwrap() {
                Record::LoopLevels { gain, .. } => Some(gain),
                _ => None,
            })
            .unwrap()
    }
    let (h, c) = fixture();
    let (mut offline, client) = h.host.into_parts();
    let (endpoint, out) = client.into_carrier().into_parts();
    let input = Rc::new(RefCell::new(Stream::new(16 * MAX_RECORD_BYTES)));
    let taken = Rc::new(Cell::new(0));
    let paused = Rc::new(Cell::new(true));
    let closed = Rc::new(Cell::new(false));
    let responses = Rc::new(RefCell::new(Vec::with_capacity(512)));
    let mut server = RecordServer::new(
        endpoint,
        Port {
            input: input.clone(),
            taken: taken.clone(),
            paused: paused.clone(),
            closed: closed.clone(),
            responses: responses.clone(),
        },
        out,
    );
    let mut bytes = [0; MAX_RECORD_BYTES];
    let batch = Batch::new(c.0, c.1, &[Command::LoopGain(0, 0.25)]).unwrap();
    let len = codec::encode_client(&ClientMessage::Immediate(batch), &mut bytes).unwrap();
    append(&input, &bytes[..len - 1]);
    assert_no_heap(|| server.pump(16));
    assert_no_heap(|| offline.run(16, |_, _| 0.)).unwrap();
    assert_eq!(taken.get(), 0);
    assert_eq!(observed(server.endpoint()), 1.);
    append(&input, &bytes[len - 1..len]);
    assert_no_heap(|| server.pump(16));
    assert_no_heap(|| offline.run(16, |_, _| 0.)).unwrap();
    assert_eq!(observed(server.endpoint()), 0.25);
    let sent = 2 * FIFO_SINGLETONS + STAGED_BATCHES + 2;
    for i in 0..sent {
        let batch = Batch::new(c.0, c.1, &[Command::LoopGain(0, i as f32 / 1000.)]).unwrap();
        let len = codec::encode_client(&ClientMessage::Immediate(batch), &mut bytes).unwrap();
        append(&input, &bytes[..len]);
    }
    assert_no_heap(|| server.pump(256));
    let blocked = taken.get();
    assert_eq!(
        blocked,
        FIFO_SINGLETONS + STAGED_BATCHES + 1,
        "the initial response still reserves space during command intake"
    );
    assert_no_heap(|| server.pump(16));
    assert!(taken.get() <= blocked + 1);
    for _ in 0..5 {
        assert_no_heap(|| offline.run(16, |_, _| 0.)).unwrap();
        assert_no_heap(|| server.pump(16));
    }
    assert!(
        taken.get() < sent + 1,
        "paused result delivery must eventually stop command intake"
    );
    paused.set(false);
    for _ in 0..64 {
        assert_no_heap(|| server.pump(16));
        assert_no_heap(|| offline.run(16, |_, _| 0.)).unwrap();
        assert_no_heap(|| server.pump(16));
        if responses.borrow().len() == sent + 1 {
            break;
        }
    }
    assert_eq!(observed(server.endpoint()), (sent - 1) as f32 / 1000.);
    assert_eq!(taken.get(), sent + 1);
    assert!(input.borrow().pending().is_empty());
    let returned = responses.borrow();
    assert_eq!(returned.len(), sent + 1);
    for (index, response) in returned.iter().enumerate() {
        assert_eq!(response.header.deadline(), None);
        assert_eq!(response.records().len(), 1);
        assert_eq!(response.records()[0].code(), immediate::Code::Applied);
        assert_eq!(
            response.records()[0].command(),
            Command::LoopGain(
                0,
                if index == 0 {
                    0.25
                } else {
                    (index - 1) as f32 / 1000.
                }
            )
        );
    }
    drop(returned);
    closed.set(true);
    assert_no_heap(|| server.pump(16));
    assert_eq!(server.failure(), Some(CarrierError::Disconnected));
}

fn recording_fixture(d: &shoop_rt_engine::EngineDescription) -> (Harness, (u64, u64)) {
    let (mut h, _) = fixture_for(d);
    h.done(Request::CreateLoop {
        loop_id: loop_id(1, 1),
        strip: strip(0, 1),
    });
    for channel in 0..2 {
        h.done(Request::Connect {
            port: port(strip(0, 1), PortKind::AudioIn, channel),
            host: audio_in(u32::from(channel)),
            connected: true,
        });
        h.done(route(strip(0, 1), channel, channel, true));
    }
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::AutomaticMonitoring(true),
    });
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::InputGain(0.5),
    });
    h.input = Box::new(|port, frame| {
        if port == 0 {
            frame as f32 / 1000.
        } else {
            -0.25
        }
    });
    let c = configuration(&mut h);
    (h, c)
}
fn immediate_results(h: &Harness) -> Vec<immediate::Response> {
    h.lifecycle
        .iter()
        .filter_map(|m| match m {
            EngineMessage::ImmediateResponse(r) => Some(*r),
            _ => None,
        })
        .collect()
}
fn observed_loop(h: &mut Harness, id: LoopSlotId) -> Telemetry {
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    Records(&bytes[HEADER..len])
        .find_map(|r| match r.unwrap() {
            Record::Telemetry(t @ Telemetry::Loop { loop_id, .. }) if loop_id == id => Some(t),
            _ => None,
        })
        .unwrap()
}

#[shoop_test]
fn recording_milestone_grouped_capture_finishes_to_play_and_stop_at_exact_frames() {
    use immediate::Code;
    let mut d = description();
    d.ramp_frames = 0;
    let (mut h, c) = recording_fixture(&d);
    h.lifecycle.clear();
    let start = h.frame();
    send(
        &mut h,
        c,
        &[
            Command::Transition(0, TransitionMode::Recording),
            Command::Transition(u32::MAX, TransitionMode::Recording),
            Command::Transition(1, TransitionMode::Recording),
        ],
    );
    h.run(8, 4);
    let r = immediate_results(&h)[0];
    assert_eq!(r.header.frame(), start);
    assert_eq!(r.header.deadline(), None);
    assert_eq!(
        r.records().iter().map(|r| r.code()).collect::<Vec<_>>(),
        [Code::Applied, Code::TargetUnavailable, Code::Applied]
    );
    assert!(matches!(
        observed_loop(&mut h, loop_id(0, 1)),
        Telemetry::Loop {
            mode: LoopMode::Recording,
            ..
        }
    ));
    assert_eq!(h.outputs[0][start as usize], start as f32 / 2000.);
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Playing)],
    );
    h.run(1, 1);
    assert_eq!(
        *h.outputs[0].last().unwrap(),
        start as f32 / 2000. + (start + 8) as f32 / 2000.,
        "other recorder retains automatic monitoring"
    );
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Stopped)],
    );
    h.run(1, 1);
    assert_eq!(
        *h.outputs[0].last().unwrap(),
        (start + 1) as f32 / 2000.,
        "last recorder releases automatic monitoring without muting playback"
    );
    let responses = immediate_results(&h);
    assert_eq!(responses[1].header.frame(), start + 8);
    assert_eq!(responses[2].header.frame(), start + 9);
    assert!(responses[1..]
        .iter()
        .all(|r| r.records()[0].code() == Code::Applied));
    let (frames, audio, _) = h.read(loop_id(0, 1), BulkSelector::AudioFrames);
    assert_eq!(frames, 8);
    let expected: Vec<_> = (start..start + 8)
        .flat_map(|f| [f as f32 / 2000., -0.125])
        .collect();
    assert_eq!(audio, expected);
    let (frames, audio, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    assert_eq!(frames, 9);
    assert_eq!(&audio[..16], &expected);
    assert!(matches!(
        observed_loop(&mut h, loop_id(1, 1)),
        Telemetry::Loop {
            mode: LoopMode::Stopped,
            length: 9,
            ..
        }
    ));
    h.done(Request::StripControl {
        strip: strip(0, 1),
        control: StripControl::Monitoring(true),
    });
    h.run(1, 1);
    let at = h.frame() - 1;
    assert_eq!(
        *h.outputs[0].last().unwrap(),
        (start + (at - start - 8) % 8) as f32 / 2000. + at as f32 / 2000.,
        "manual monitoring remains enabled"
    );
}

#[shoop_test]
fn recording_milestone_buffer_only_callbacks_and_empty_repeated_actions() {
    use immediate::Code;
    let mut d = description();
    d.quantum = 16;
    d.ramp_frames = 0;
    let (mut h, c) = recording_fixture(&d);
    h.run((16 - h.frame() % 16) as usize % 16, 1);
    let frame = h.host.endpoint().command_clock().frame().0;
    h.lifecycle.clear();
    send(
        &mut h,
        c,
        &[
            Command::Transition(0, TransitionMode::Recording),
            Command::Transition(0, TransitionMode::Recording),
            Command::Transition(0, TransitionMode::Playing),
        ],
    );
    h.run(1, 1);
    let r = immediate_results(&h)[0];
    assert_eq!(r.header.frame(), frame);
    assert_eq!(
        r.records().iter().map(|r| r.code()).collect::<Vec<_>>(),
        [Code::Applied, Code::Applied, Code::CannotApply]
    );
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Stopped)],
    );
    h.run(1, 1);
    assert_eq!(immediate_results(&h)[1].records()[0].code(), Code::Applied);
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Recording)],
    );
    h.run(32, 1);
    let end = h.host.endpoint().command_clock().frame().0;
    send(
        &mut h,
        c,
        &[
            Command::Transition(0, TransitionMode::Recording),
            Command::Transition(0, TransitionMode::Playing),
        ],
    );
    h.run(1, 1);
    let responses = immediate_results(&h);
    assert!(responses.iter().all(|r| r.header.deadline().is_none()));
    assert_eq!(responses.last().unwrap().header.frame(), end);
    assert!(responses
        .last()
        .unwrap()
        .records()
        .iter()
        .all(|r| r.code() == Code::Applied));
    h.run(32, 1);
    let (frames, _, _) = h.read(loop_id(0, 1), BulkSelector::AudioFrames);
    assert_eq!(
        frames,
        end - frame,
        "repeat Record preserves the prefix and the original start frame"
    );
}

#[shoop_test]
fn recording_milestone_exhaustion_preserves_prefix_and_finish_reservations() {
    use immediate::Code;
    let mut d = description();
    d.resource_pages = 7;
    d.ramp_frames = 0;
    d.history.seconds = 0;
    let (mut h, c) = recording_fixture(&d);
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Recording)],
    );
    h.run(64, 16);
    assert!(
        matches!(
            observed_loop(&mut h, loop_id(0, 1)),
            Telemetry::Loop {
                mode: LoopMode::Recording,
                ..
            }
        ),
        "{:?}",
        observed_loop(&mut h, loop_id(0, 1))
    );
    send(
        &mut h,
        c,
        &[
            Command::Transition(1, TransitionMode::Recording),
            Command::LoopGain(1, 0.25),
            Command::Transition(0, TransitionMode::Stopped),
        ],
    );
    h.run(1, 1);
    let r = *immediate_results(&h).last().unwrap();
    assert_eq!(
        r.records().iter().map(|r| r.code()).collect::<Vec<_>>(),
        [Code::CaptureUnavailable, Code::Applied, Code::Applied]
    );
    let (frames, original, _) = h.read(loop_id(0, 1), BulkSelector::AudioFrames);
    assert_eq!(
        frames, 64,
        "finish works while there is no room to start another recorder"
    );
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Recording)],
    );
    h.run(1, 1);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::CaptureUnavailable
    );
    let (_, unchanged, _) = h.read(loop_id(0, 1), BulkSelector::AudioFrames);
    assert_eq!(
        original, unchanged,
        "failed acquisition preserves existing media"
    );

    let (mut h, c) = recording_fixture(&d);
    let start = h.frame();
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Recording)],
    );
    h.run(65, 1);
    assert!(matches!(
        observed_loop(&mut h, loop_id(0, 1)),
        Telemetry::Loop {
            mode: LoopMode::Stopped,
            length: 64,
            capture_incomplete: true,
            ..
        }
    ));
    let (frames, audio, _) = h.read(loop_id(0, 1), BulkSelector::AudioFrames);
    assert_eq!(frames, 64);
    assert_eq!(audio[0], start as f32 / 2000.);
    assert_eq!(audio[126], (start + 63) as f32 / 2000.);
    assert_eq!(
        immediate_results(&h).len(),
        1,
        "exhaustion produces no unsolicited response"
    );
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Playing)],
    );
    h.settle();
    assert_eq!(immediate_results(&h).len(), 2);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
    assert!(matches!(
        observed_loop(&mut h, loop_id(0, 1)),
        Telemetry::Loop {
            mode: LoopMode::Playing,
            length: 64,
            capture_incomplete: true,
            ..
        }
    ));
}

#[shoop_test]
fn encoded_record_and_grab_commands_return_results_matching_published_state() {
    use shoop_rt_transport::carrier::{RecordPort, RecordServer};
    use std::{cell::RefCell, rc::Rc};
    struct Port {
        input: Rc<RefCell<Option<Vec<u8>>>>,
        responses: Rc<RefCell<Vec<immediate::Response>>>,
    }
    impl RecordPort for Port {
        fn post(&mut self, bytes: &[u8]) -> Result<(), CarrierError> {
            if let EngineMessage::ImmediateResponse(r) = codec::decode_engine(bytes).unwrap() {
                self.responses.borrow_mut().push(r);
            }
            Ok(())
        }
        fn take(&mut self, out: &mut [u8]) -> Option<usize> {
            let bytes = self.input.borrow_mut().take()?;
            out[..bytes.len()].copy_from_slice(&bytes);
            Some(bytes.len())
        }
    }
    let (h, c) = fixture();
    let (mut offline, client) = h.host.into_parts();
    let (endpoint, out) = client.into_carrier().into_parts();
    let input = Rc::new(RefCell::new(None));
    let responses = Rc::new(RefCell::new(Vec::new()));
    let mut server = RecordServer::new(
        endpoint,
        Port {
            input: input.clone(),
            responses: responses.clone(),
        },
        out,
    );
    let mut bytes = [0; MAX_RECORD_BYTES];
    for finish in [
        Command::Transition(0, TransitionMode::Playing),
        Command::Transition(0, TransitionMode::Stopped),
    ] {
        let start = offline.next_frame();
        for (command, frames) in [
            (Command::Transition(0, TransitionMode::Recording), 8),
            (finish, 1),
        ] {
            let batch = Batch::new(c.0, c.1, &[command]).unwrap();
            let len = codec::encode_client(&ClientMessage::Immediate(batch), &mut bytes).unwrap();
            *input.borrow_mut() = Some(bytes[..len].to_vec());
            server.pump(16);
            assert_no_heap(|| offline.run(frames, |_, _| 0.25)).unwrap();
            server.pump(16);
            assert_eq!(
                responses.borrow().last().unwrap().records()[0].code(),
                immediate::Code::Applied
            );
            assert_eq!(responses.borrow().last().unwrap().header.deadline(), None);
        }
        assert_eq!(responses.borrow().last().unwrap().header.frame(), start + 8);
        let mut payload = vec![0; server.endpoint().snapshot_capacity()];
        let (_, len) = server.endpoint().read_snapshot(&mut payload).unwrap();
        assert!(Records(&payload[HEADER..len]).any(|r| matches!(r,
            Ok(Record::Telemetry(Telemetry::Loop { mode, length: 8, .. }))
                if mode == if matches!(finish, Command::Transition(_, TransitionMode::Playing)) { LoopMode::Playing } else { LoopMode::Stopped }
        )));
    }
    use immediate::Grab;
    for (mode, frames) in [(TransitionMode::Stopped, 1), (TransitionMode::Recording, 4)] {
        let spec = Grab {
            sync: Some(0),
            reverse_start_cycle: Some(1),
            cycles_length: Some(1),
            go_to_cycle: Some(if mode == TransitionMode::Recording {
                1
            } else {
                0
            }),
            mode,
        };
        let command = Command::Grab(0, spec);
        let batch = Batch::new(c.0, c.1, &[command]).unwrap();
        let len = codec::encode_client(&ClientMessage::Immediate(batch), &mut bytes).unwrap();
        *input.borrow_mut() = Some(bytes[..len].to_vec());
        server.pump(16);
        assert_no_heap(|| offline.run(frames, |_, _| 0.25)).unwrap();
        server.pump(16);
        assert_eq!(
            responses.borrow().last().unwrap().records()[0].code(),
            immediate::Code::Applied
        );
    }
    let batch = Batch::new(c.0, c.1, &[Command::Transition(0, TransitionMode::Stopped)]).unwrap();
    let len = codec::encode_client(&ClientMessage::Immediate(batch), &mut bytes).unwrap();
    *input.borrow_mut() = Some(bytes[..len].to_vec());
    server.pump(16);
    assert_no_heap(|| offline.run(1, |_, _| 0.25)).unwrap();
    server.pump(16);
    assert_eq!(
        responses.borrow().last().unwrap().records()[0].code(),
        immediate::Code::Applied
    );
    let mut payload = vec![0; server.endpoint().snapshot_capacity()];
    let (_, len) = server.endpoint().read_snapshot(&mut payload).unwrap();
    assert!(Records(&payload[HEADER..len]).any(|r| matches!(
        r,
        Ok(Record::Telemetry(Telemetry::Loop {
            mode: LoopMode::Stopped,
            length: 12,
            ..
        }))
    )));
    assert_eq!(responses.borrow().len(), 7);
}

#[shoop_test]
fn recording_milestone_disconnect_keeps_active_take_without_replaying_staged_record() {
    let mut d = description();
    d.ramp_frames = 0;
    let (mut h, c) = recording_fixture(&d);
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Recording)],
    );
    h.run(4, 1);
    for _ in 0..FIFO_SINGLETONS {
        send(&mut h, c, &[Command::LoopGain(1, 0.5)]);
    }
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Recording)],
    );
    h.host.disconnect();
    h.pump();
    h.host.connect().unwrap();
    h.pump();
    h.lifecycle.clear();
    h.run(4, 1);
    assert!(matches!(
        observed_loop(&mut h, loop_id(0, 1)),
        Telemetry::Loop {
            mode: LoopMode::Recording,
            ..
        }
    ));
    assert!(matches!(
        observed_loop(&mut h, loop_id(1, 1)),
        Telemetry::Loop {
            mode: LoopMode::Stopped,
            length: 0,
            ..
        }
    ));
    assert!(
        immediate_results(&h).is_empty(),
        "old-session responses are discarded"
    );
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Stopped)],
    );
    h.run(1, 1);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        immediate::Code::Applied
    );
    let (frames, _, _) = h.read(loop_id(0, 1), BulkSelector::AudioFrames);
    assert_eq!(frames, 8);
    let mut stopped = h.host.into_render().into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.media_usage().references, 0);
    assert_eq!(
        stopped.resource_usage().free_pages,
        d.history_capacity().unwrap().total_resource_pages
    );
    assert_eq!(stopped.resource_usage().reserved_pages, 0);
}

fn grab_fixture(q: usize) -> (Harness, (u64, u64)) {
    let mut d = description();
    d.quantum = q;
    d.ramp_frames = 0;
    grab_fixture_for(&d)
}
fn grab_fixture_for(d: &shoop_rt_engine::EngineDescription) -> (Harness, (u64, u64)) {
    let q = d.quantum;
    let (mut h, c) = recording_fixture(d);
    h.run((q as u64 - h.frame() % q as u64) as usize % q, 1);
    send(
        &mut h,
        c,
        &[Command::Transition(0, TransitionMode::Recording)],
    );
    h.run(2 * q.max(8), 1);
    send(
        &mut h,
        c,
        &[
            Command::Transition(0, TransitionMode::Stopped),
            Command::Transition(0, TransitionMode::Playing),
        ],
    );
    h.run(4 * q.max(8), 1);
    let c = configuration(&mut h);
    h.run((q as u64 - h.frame() % q as u64) as usize % q, 1);
    (h, c)
}
fn sync_position(h: &mut Harness) -> (u64, u64) {
    let Telemetry::Loop { length, .. } = observed_loop(h, loop_id(0, 1)) else {
        panic!()
    };
    let start = immediate_results(h)
        .into_iter()
        .find(|r| {
            r.records()
                .iter()
                .any(|v| v.command() == Command::Transition(0, TransitionMode::Playing))
        })
        .unwrap()
        .header
        .frame();
    (
        length,
        (h.host.endpoint().command_clock().frame().0 - start) % length,
    )
}

#[shoop_test]
fn grab_milestone_completed_audio_and_continuation_have_exact_samples_and_cut() {
    use immediate::{Code, Grab};
    let (mut h, c) = grab_fixture(1);
    let cut = h.frame();
    let (length, position) = sync_position(&mut h);
    let spec = Grab {
        sync: Some(0),
        reverse_start_cycle: Some(1),
        cycles_length: Some(1),
        go_to_cycle: Some(0),
        mode: TransitionMode::Stopped,
    };
    send(&mut h, c, &[Command::Grab(1, spec)]);
    h.run(1, 1);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
    let (frames, audio, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    assert_eq!(frames, length);
    let expected: Vec<_> = (cut - position - length..cut - position)
        .flat_map(|f| [f as f32 / 2000., -0.125])
        .collect();
    assert_eq!(audio, expected);
    h.settle();
    let cut = h.frame();
    let (length, position) = sync_position(&mut h);
    let prefix = length + position;
    let spec = Grab {
        reverse_start_cycle: None,
        go_to_cycle: Some(1),
        mode: TransitionMode::Recording,
        ..spec
    };
    send(&mut h, c, &[Command::Grab(1, spec)]);
    h.run(4, 1);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
    assert!(matches!(
        observed_loop(&mut h, loop_id(1, 1)),
        Telemetry::Loop {
            mode: LoopMode::Recording,
            ..
        }
    ));
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Playing)],
    );
    h.run(1, 1);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
    let (frames, audio, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    assert_eq!(frames, prefix + 4);
    let expected: Vec<_> = (cut - prefix..cut + 4)
        .flat_map(|f| [f as f32 / 2000., -0.125])
        .collect();
    assert_eq!(
        audio, expected,
        "history and new input must join without dropped or repeated samples"
    );
}

fn continuing_grab(id: u32) -> Command {
    Command::Grab(
        id,
        immediate::Grab {
            sync: Some(0),
            reverse_start_cycle: None,
            cycles_length: Some(1),
            go_to_cycle: Some(1),
            mode: immediate::TransitionMode::Recording,
        },
    )
}

#[shoop_test]
fn grab_milestone_buffer_only_callbacks_finish_and_preserve_a_repeated_grab_prefix() {
    use immediate::Code;
    let (mut h, c) = grab_fixture(16);
    let cut = h.host.endpoint().command_clock().frame().0;
    let (length, position) = sync_position(&mut h);
    let prefix = length + position;
    let count = immediate_results(&h).len();
    send(
        &mut h,
        c,
        &[
            continuing_grab(1),
            continuing_grab(1),
            Command::LoopGain(1, 0.25),
        ],
    );
    h.run(1, 1);
    let response = *immediate_results(&h).last().unwrap();
    assert_eq!(response.header.frame(), cut);
    assert_eq!(
        response
            .records()
            .iter()
            .map(|r| r.code())
            .collect::<Vec<_>>(),
        [Code::Applied, Code::CannotApply, Code::Applied]
    );
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Stopped)],
    );
    h.run(1, 1);
    assert_eq!(immediate_results(&h).len(), count + 2);
    assert_eq!(immediate_results(&h).last().unwrap().header.frame(), cut);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
    h.run(32, 1);
    let (frames, audio, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    assert_eq!(
        frames, prefix,
        "no new audio was rendered between Grab and Stop"
    );
    let expected: Vec<_> = (cut - prefix..cut)
        .flat_map(|f| [f as f32 / 2000., -0.125])
        .collect();
    assert_eq!(audio, expected);
}

#[shoop_test]
fn grab_milestone_grouped_gestures_share_the_cut_and_fail_independently() {
    use immediate::{Code, GrabGesture};
    let (mut h, _) = grab_fixture(1);
    h.done(Request::CreateLoop {
        loop_id: loop_id(2, 1),
        strip: strip(0, 1),
    });
    let c = configuration(&mut h);
    let (length, position) = sync_position(&mut h);
    let cut = h.frame();
    let spec = GrabGesture {
        sync: 0,
        target: None,
        cycles: 1,
        play_after_record: true,
    };
    send(
        &mut h,
        c,
        &[
            Command::GrabGesture(1, spec),
            Command::GrabGesture(u32::MAX, spec),
            Command::GrabGesture(2, spec),
        ],
    );
    h.run(1, 1);
    let response = *immediate_results(&h).last().unwrap();
    assert_eq!(response.header.frame(), cut);
    assert_eq!(response.header.deadline(), None);
    assert_eq!(
        response
            .records()
            .iter()
            .map(|r| r.code())
            .collect::<Vec<_>>(),
        [Code::Applied, Code::TargetUnavailable, Code::Applied]
    );
    for id in [1, 2] {
        assert!(
            matches!(observed_loop(&mut h, loop_id(id, 1)), Telemetry::Loop { mode: LoopMode::Playing, position: p, length: l, .. } if p == position && l == length)
        );
    }
    let (_, a, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    let (_, b, _) = h.read(loop_id(2, 1), BulkSelector::AudioFrames);
    assert_eq!(a, b);
    assert_eq!(a[0], (cut - position - length) as f32 / 2000.);
}

#[shoop_test]
fn grab_milestone_storage_failures_and_exhaustion_keep_a_usable_prefix() {
    use immediate::Code;
    let mut d = description();
    d.ramp_frames = 0;
    d.history.writers = 1;
    d.history.audio_take_pages = 32;
    let (mut h, _) = grab_fixture_for(&d);
    h.done(Request::CreateLoop {
        loop_id: loop_id(2, 1),
        strip: strip(0, 1),
    });
    let c = configuration(&mut h);
    send(
        &mut h,
        c,
        &[
            continuing_grab(1),
            continuing_grab(2),
            Command::LoopGain(2, 0.25),
        ],
    );
    h.run(1, 1);
    let response = *immediate_results(&h).last().unwrap();
    assert_eq!(
        response
            .records()
            .iter()
            .map(|r| r.code())
            .collect::<Vec<_>>(),
        [Code::Applied, Code::CaptureUnavailable, Code::Applied]
    );
    assert!(matches!(
        observed_loop(&mut h, loop_id(2, 1)),
        Telemetry::Loop {
            mode: LoopMode::Stopped,
            length: 0,
            ..
        }
    ));
    h.run(2048, 16);
    assert!(
        matches!(observed_loop(&mut h, loop_id(1, 1)), Telemetry::Loop { mode: LoopMode::Stopped, length, capture_incomplete: true, .. } if length > 0)
    );
    let responses = immediate_results(&h).len();
    let (frames, audio, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    assert!(frames > 0);
    assert!(audio.chunks_exact(2).all(|s| s[1] == -0.125));
    h.settle();
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Playing)],
    );
    h.settle();
    assert_eq!(
        immediate_results(&h).len(),
        responses + 1,
        "exhaustion generates no unsolicited response"
    );
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
}

#[shoop_test]
fn grab_milestone_missing_history_preserves_existing_audio_and_later_commands() {
    use immediate::{Code, Grab};
    let mut d = description();
    d.history.seconds = 0;
    let (mut h, c) = recording_fixture(&d);
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Recording)],
    );
    h.run(8, 1);
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Stopped)],
    );
    h.run(1, 1);
    let (_, old, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    h.settle();
    send(
        &mut h,
        c,
        &[
            Command::Grab(
                1,
                Grab {
                    sync: None,
                    reverse_start_cycle: None,
                    cycles_length: None,
                    go_to_cycle: None,
                    mode: TransitionMode::Stopped,
                },
            ),
            Command::LoopGain(1, 0.5),
        ],
    );
    h.run(1, 1);
    let response = *immediate_results(&h).last().unwrap();
    assert_eq!(
        response
            .records()
            .iter()
            .map(|r| r.code())
            .collect::<Vec<_>>(),
        [Code::HistoryUnavailable, Code::Applied]
    );
    let (_, current, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    assert_eq!(old, current);
}

#[shoop_test]
fn grab_milestone_disconnect_keeps_capture_and_drops_unsent_finish() {
    let (mut h, c) = grab_fixture(1);
    let (length, position) = sync_position(&mut h);
    let prefix = length + position;
    send(&mut h, c, &[continuing_grab(1)]);
    h.run(4, 1);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        immediate::Code::Applied
    );
    for _ in 0..FIFO_SINGLETONS {
        send(&mut h, c, &[Command::LoopGain(0, 0.5)]);
    }
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Stopped)],
    );
    h.host.disconnect();
    h.pump();
    h.host.connect().unwrap();
    h.pump();
    h.lifecycle.clear();
    h.run(4, 1);
    assert!(matches!(
        observed_loop(&mut h, loop_id(1, 1)),
        Telemetry::Loop {
            mode: LoopMode::Recording,
            ..
        }
    ));
    assert!(immediate_results(&h).is_empty());
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Stopped)],
    );
    h.run(1, 1);
    assert_eq!(immediate_results(&h).len(), 1);
    assert_eq!(
        immediate_results(&h)[0].records()[0].code(),
        immediate::Code::Applied
    );
    let (frames, _, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    assert_eq!(frames, prefix + 8);
    let expected_pages = description()
        .history_capacity()
        .unwrap()
        .total_resource_pages;
    let mut stopped = h.host.into_render().into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.media_usage().references, 0);
    assert_eq!(stopped.resource_usage().free_pages, expected_pages);
    assert_eq!(stopped.resource_usage().reserved_pages, 0);
}

#[shoop_test]
fn grab_milestone_wrap_cut_and_empty_continuation_match_the_current_cycle() {
    use immediate::{Code, Grab};
    let (mut h, c) = grab_fixture(1);
    let (length, position) = sync_position(&mut h);
    h.run((length - position) as usize, 1);
    let cut = h.frame();
    send(
        &mut h,
        c,
        &[Command::Grab(
            1,
            Grab {
                sync: Some(0),
                reverse_start_cycle: Some(1),
                cycles_length: Some(1),
                go_to_cycle: Some(0),
                mode: TransitionMode::Stopped,
            },
        )],
    );
    h.run(1, 1);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
    let (_, audio, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    let expected: Vec<_> = (cut - length..cut)
        .flat_map(|f| [f as f32 / 2000., -0.125])
        .collect();
    assert_eq!(
        audio, expected,
        "a wrap belongs to the new cycle at this cut"
    );

    let (mut h, c) = grab_fixture(16);
    let (length, position) = sync_position(&mut h);
    h.run((length - position) as usize, 1);
    let cut = h.host.endpoint().command_clock().frame().0;
    send(
        &mut h,
        c,
        &[Command::Grab(
            1,
            Grab {
                sync: Some(0),
                reverse_start_cycle: Some(0),
                cycles_length: Some(1),
                go_to_cycle: Some(0),
                mode: TransitionMode::Recording,
            },
        )],
    );
    h.run(1, 1);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
    send(
        &mut h,
        c,
        &[Command::Transition(1, TransitionMode::Stopped)],
    );
    h.run(1, 1);
    assert_eq!(immediate_results(&h).last().unwrap().header.frame(), cut);
    assert_eq!(
        immediate_results(&h).last().unwrap().records()[0].code(),
        Code::Applied
    );
    h.run(32, 1);
    let (frames, audio, _) = h.read(loop_id(1, 1), BulkSelector::AudioFrames);
    assert_eq!(frames, 0);
    assert!(audio.is_empty());
}
