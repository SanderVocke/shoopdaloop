#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod support;
use shoop_rt_protocol::{
    immediate::{Batch, Command, FIFO_SINGLETONS, STAGED_BATCHES},
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
            Command::Stop(l.0.index),
            Command::Start(l.0.index),
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
        &[Command::Start(l.0.index), Command::LoopGain(l.0.index, 0.9)],
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
    send(&mut h, c, &[Command::Start(l.0.index)]);
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
