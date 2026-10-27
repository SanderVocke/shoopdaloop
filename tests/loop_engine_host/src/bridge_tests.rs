use crate::{
    wire::{self, Error, Phase, Request, Response, Status},
    Bridge, Client,
};
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::Frame;
use shoop_loop_engine::{LoopId, Transition};

fn process(bridge: &mut Bridge, frames: usize) -> (Vec<[f32; 2]>, Vec<(u64, Vec<u8>)>) {
    let start = bridge.next_frame();
    let mut left = [0.; 128];
    let mut right = [0.; 128];
    let mut input_left = [0.; 128];
    let mut input_right = [0.; 128];
    let mut input = [MidiBuffer::new(2, 6).unwrap()];
    let mut output = [MidiBuffer::new(256, 1024).unwrap()];
    for offset in 0..frames {
        if start + (offset as u64) < 4 {
            input_left[offset] = 2. * (start + offset as u64 + 1) as f32;
            input_right[offset] = input_left[offset] * 10.;
        }
    }
    for (at, packet) in [(0, [0x90, 60, 100]), (2, [0x80, 60, 45])] {
        if at >= start && at < start + frames as u64 {
            input[0].push((at - start) as u32, frames, &packet).unwrap();
        }
    }
    crate::guarded(|| {
        bridge.process(
            AudioBlock {
                frame: Frame(start),
                frames,
                inputs: &[&input_left[..frames], &input_right[..frames]],
                outputs: &mut [&mut left[..frames], &mut right[..frames]],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
    })
    .unwrap();
    let audio = (0..frames)
        .map(|offset| [left[offset], right[offset]])
        .collect();
    let midi = (0..output[0].len())
        .map(|index| {
            let (offset, packet) = output[0].get(index).unwrap();
            (start + u64::from(offset), packet.to_vec())
        })
        .collect();
    (audio, midi)
}
fn drain(bridge: &mut Bridge, client: &mut Client) -> Vec<Response> {
    let mut responses = Vec::new();
    while let Some(packet) = bridge.next_response().unwrap() {
        responses.push(crate::guarded(|| client.receive(&packet)).unwrap());
        assert!(responses.len() <= 2 * wire::SLOTS);
    }
    responses
}
fn finish(bridge: Bridge) {
    let mut stopped = bridge.into_stopped();
    stopped.cleanup().unwrap();
    stopped.cleanup().unwrap();
    for _ in 0..512 {
        stopped.resources().begin_callback();
        stopped.resources().reclaim();
    }
    let usage = stopped.resource_usage();
    assert_eq!(
        (usage.free_pages, usage.free_versions, usage.free_snapshots),
        (256, 16, 16)
    );
    assert_eq!(stopped.media_usage().references, 0);
}
fn send(
    client: &mut Client,
    bridge: &mut Bridge,
    at: u64,
    transition: Transition,
    recovery: bool,
    cancelled: bool,
) -> Request {
    let packet =
        crate::guarded(|| client.prepare(at, bridge.target(), transition, recovery, cancelled))
            .unwrap();
    bridge.ingest(&packet).unwrap();
    Request::decode(&packet).unwrap()
}

#[test]
fn wire_path_matches_complete_native_media_across_partitions() {
    let expected = crate::run(&[13]);
    for partitions in [
        vec![13],
        vec![1; 13],
        vec![3, 5, 2, 3],
        vec![0, 4, 0, 8, 0, 1, 0],
    ] {
        let session = (1u64 << 60) + 97;
        let mut bridge = Bridge::new(session).unwrap();
        let mut client = Client::new(session).unwrap();
        send(
            &mut client,
            &mut bridge,
            0,
            Transition::Record {
                monitor_previous: false,
            },
            false,
            false,
        );
        send(
            &mut client,
            &mut bridge,
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
            false,
            false,
        );
        send(&mut client, &mut bridge, 12, Transition::Stop, false, false);
        let mut samples = Vec::new();
        let mut midi = Vec::new();
        let mut terminals = Vec::new();
        let mut admissions = 0;
        for frames in partitions {
            let (audio, packets) = process(&mut bridge, frames);
            samples.extend(audio);
            midi.extend(packets);
            for response in drain(&mut bridge, &mut client) {
                assert_eq!(response.error, 0);
                match response.phase {
                    Phase::Admission => {
                        assert_eq!(response.status, Status::Admitted);
                        assert_eq!(response.observed, 0);
                        assert_eq!(response.effective, response.requested);
                        admissions += 1;
                    }
                    Phase::Execution => {
                        assert_eq!(response.status, Status::Applied);
                        terminals.push((
                            response.requested,
                            response.delivered.unwrap(),
                            response.observed,
                        ));
                    }
                }
            }
        }
        assert_eq!(samples, expected.samples);
        assert_eq!(midi, expected.midi);
        assert_eq!(terminals, expected.transitions);
        assert_eq!(admissions, 3);
        assert_eq!((client.outstanding(), bridge.outstanding()), (0, 0));
        finish(bridge);
    }
}

#[test]
fn stalled_normal_returns_preserve_music_and_independent_recovery() {
    let mut bridge = Bridge::new(9).unwrap();
    let mut client = Client::new(9).unwrap();
    for (at, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (
            8,
            Transition::Seek {
                position: 0,
                retrigger: false,
            },
        ),
        (
            12,
            Transition::Seek {
                position: 0,
                retrigger: false,
            },
        ),
    ] {
        send(&mut client, &mut bridge, at, transition, false, false);
    }
    let (samples, _) = process(&mut bridge, 13);
    for (at, sample) in samples.iter().enumerate() {
        let expected = if at < 4 {
            (at + 1) as f32
        } else {
            (at % 4 + 1) as f32 * 0.5
        };
        assert_eq!(*sample, [expected, expected * 10.]);
    }
    assert_eq!((client.outstanding(), bridge.outstanding()), (4, 4));
    assert_eq!(
        client.prepare(13, bridge.target(), Transition::Stop, false, false),
        Err(Error::Busy)
    );
    // A rejected use of an occupied wire slot cannot advance the sequence watermark.
    let rejected = Request {
        session: 9,
        sequence: 5,
        slot: 0,
        at: 13,
        target: bridge.target(),
        transition: Transition::Stop,
        recovery: false,
        cancelled: false,
    };
    assert_eq!(bridge.ingest(&rejected.encode().unwrap()), Err(Error::Busy));
    let recovery = send(&mut client, &mut bridge, 13, Transition::Stop, true, false);
    assert_eq!(recovery.sequence, 5);
    let (audio, midi) = process(&mut bridge, 1);
    assert_eq!(audio, [[0., 0.]]);
    assert_eq!(midi, [(13, vec![0x80, 60, 0])]);
    assert_eq!(
        client.prepare(14, bridge.target(), Transition::Stop, true, false),
        Err(Error::Busy)
    );
    assert_eq!(drain(&mut bridge, &mut client).len(), 10);
    assert_eq!((client.outstanding(), bridge.outstanding()), (0, 0));
    finish(bridge);
}

#[test]
fn transport_cancellation_stale_targets_domain_refusal_and_lateness_are_distinct() {
    let mut bridge = Bridge::new(12).unwrap();
    let mut client = Client::new(12).unwrap();
    let cancelled = send(&mut client, &mut bridge, 8, Transition::Stop, false, false);
    assert_eq!(
        bridge.cancel(cancelled.slot, cancelled.sequence + 1),
        Err(Error::Sequence)
    );
    bridge.cancel(cancelled.slot, cancelled.sequence).unwrap();
    let stale = client
        .prepare(
            0,
            LoopId::from_parts(0, 2).unwrap(),
            Transition::Stop,
            false,
            false,
        )
        .unwrap();
    bridge.ingest(&stale).unwrap();
    send(
        &mut client,
        &mut bridge,
        0,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
        false,
        false,
    );
    process(&mut bridge, 1);
    let responses = drain(&mut bridge, &mut client);
    assert_eq!(responses.len(), 5);
    assert!(responses.iter().any(|r| r.sequence == cancelled.sequence
        && r.phase == Phase::Admission
        && r.status == Status::Cancelled));
    assert!(responses
        .iter()
        .any(|r| r.phase == Phase::Execution && r.status == Status::Failed && r.error == 259));
    assert!(responses
        .iter()
        .any(|r| r.phase == Phase::Execution && r.status == Status::Failed && r.error == 268));
    send(&mut client, &mut bridge, 0, Transition::Stop, false, false);
    process(&mut bridge, 1);
    let responses = drain(&mut bridge, &mut client);
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0].effective, 1);
    assert_eq!(responses[1].requested, 0);
    assert_eq!(responses[1].delivered, Some(1));
    assert_eq!(responses[1].observed, 1);
    send(&mut client, &mut bridge, 100, Transition::Stop, true, false);
    process(&mut bridge, 1);
    let responses = drain(&mut bridge, &mut client);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].phase, Phase::Admission);
    assert_eq!(responses[0].status, Status::Rejected);
    assert_eq!(responses[0].error, 7);
    assert_eq!(client.outstanding(), 0);
    finish(bridge);
}

#[test]
fn malformed_foreign_duplicate_requests_and_disconnect_keep_owners_bounded() {
    let mut bridge = Bridge::new(17).unwrap();
    let mut client = Client::new(17).unwrap();
    let packet = client
        .prepare(100, bridge.target(), Transition::Stop, false, false)
        .unwrap();
    for end in 0..wire::REQUEST_BYTES {
        assert_eq!(bridge.ingest(&packet[..end]), Err(Error::Size));
    }
    let mut foreign = Request::decode(&packet).unwrap();
    foreign.session += 1;
    assert_eq!(
        bridge.ingest(&foreign.encode().unwrap()),
        Err(Error::Session)
    );
    assert_eq!(bridge.outstanding(), 0);
    bridge.ingest(&packet).unwrap();
    assert_eq!(bridge.ingest(&packet), Err(Error::Sequence));
    process(&mut bridge, 1);
    let responses = drain(&mut bridge, &mut client);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].status, Status::Admitted);
    assert_eq!((client.outstanding(), bridge.outstanding()), (1, 1));
    drop(client);
    finish(bridge);
}

#[test]
fn wire_layout_high_bits_and_canonical_rejections_are_heap_free() {
    let request = Request {
        session: 0x0102030405060708,
        sequence: 0x1112131415161718,
        at: 0x2122232425262728,
        slot: 3,
        target: LoopId::from_parts(0x31323334, 0x41424344).unwrap(),
        transition: Transition::Play {
            position: 0x5152535455565758,
            retrigger: true,
        },
        recovery: false,
        cancelled: false,
    };
    let expected = [
        76, 76, 67, 49, 1, 0, 2, 1, 8, 7, 6, 5, 4, 3, 2, 1, 24, 23, 22, 21, 20, 19, 18, 17, 40, 39,
        38, 37, 36, 35, 34, 33, 3, 0, 0, 0, 52, 51, 50, 49, 68, 67, 66, 65, 0, 0, 0, 0, 88, 87, 86,
        85, 84, 83, 82, 81, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    assert_eq!(crate::guarded(|| request.encode()).unwrap(), expected);
    assert_eq!(
        crate::guarded(|| Request::decode(&expected)).unwrap(),
        request
    );
    for at in [44, 45, 46, 47, 56, 57, 58, 59, 60, 61, 62, 63] {
        let mut invalid = expected;
        invalid[at] = 1;
        assert_eq!(
            crate::guarded(|| Request::decode(&invalid)),
            Err(Error::Reserved)
        );
    }
    let mut seed = 0x73ce_98b4_a013_65d2u64;
    for index in 0..1024 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut next = request;
        next.at = seed;
        next.sequence = u64::MAX - index;
        next.transition = Transition::Seek {
            position: seed.rotate_left(17),
            retrigger: seed & 1 != 0,
        };
        crate::guarded(|| assert_eq!(Request::decode(&next.encode().unwrap()).unwrap(), next));
    }
}

#[test]
fn client_rejects_duplicate_foreign_and_reordered_receipts_without_releasing_credit() {
    let mut client = Client::new(23).unwrap();
    let packet = client
        .prepare(
            u64::MAX - 1,
            LoopId::from_parts(0, 1).unwrap(),
            Transition::Stop,
            false,
            false,
        )
        .unwrap();
    let request = Request::decode(&packet).unwrap();
    let admission = Response {
        session: 23,
        sequence: request.sequence,
        slot: request.slot,
        phase: Phase::Admission,
        status: Status::Admitted,
        runtime_sequence: 77,
        requested: request.at,
        effective: request.at,
        delivered: None,
        observed: 0,
        error: 0,
    };
    let terminal = Response {
        phase: Phase::Execution,
        status: Status::Applied,
        delivered: Some(request.at),
        observed: request.at,
        ..admission
    };
    assert_eq!(
        crate::guarded(|| client.receive(&terminal.encode().unwrap())),
        Err(Error::State)
    );
    let foreign = Response {
        session: 24,
        ..admission
    };
    assert_eq!(
        client.receive(&foreign.encode().unwrap()),
        Err(Error::Session)
    );
    assert_eq!(client.outstanding(), 1);
    crate::guarded(|| client.receive(&admission.encode().unwrap())).unwrap();
    assert_eq!(
        client.receive(&admission.encode().unwrap()),
        Err(Error::State)
    );
    let wrong_sequence = Response {
        runtime_sequence: 78,
        ..terminal
    };
    assert_eq!(
        client.receive(&wrong_sequence.encode().unwrap()),
        Err(Error::Sequence)
    );
    assert_eq!(client.outstanding(), 1);
    crate::guarded(|| client.receive(&terminal.encode().unwrap())).unwrap();
    assert_eq!(client.outstanding(), 0);
    client
        .prepare(0, request.target, Transition::Stop, false, false)
        .unwrap();
    assert_eq!(
        client.receive(&terminal.encode().unwrap()),
        Err(Error::Sequence)
    );
    assert_eq!(client.outstanding(), 1);
}

#[test]
fn repeated_slot_reuse_rejects_old_cancellation_and_receipts() {
    let mut bridge = Bridge::new(31).unwrap();
    let mut client = Client::new(31).unwrap();
    let mut previous: Option<Response> = None;
    for frame in 0..128 {
        let cancelled = frame % 2 != 0;
        for index in 0..wire::NORMAL_SLOTS {
            send(
                &mut client,
                &mut bridge,
                frame,
                Transition::Stop,
                false,
                cancelled && index == 0,
            );
        }
        if let Some(old) = previous {
            assert_eq!(bridge.cancel(old.slot, old.sequence), Err(Error::Sequence));
            assert_eq!(
                crate::guarded(|| client.receive(&old.encode().unwrap())),
                Err(Error::Sequence)
            );
        }
        process(&mut bridge, 1);
        let responses = drain(&mut bridge, &mut client);
        assert_eq!(responses.len(), if cancelled { 7 } else { 8 });
        assert_eq!(
            responses
                .iter()
                .filter(|r| r.status == Status::Applied)
                .count(),
            1
        );
        assert_eq!(
            responses
                .iter()
                .filter(|r| r.status == Status::Superseded)
                .count(),
            if cancelled { 2 } else { 3 }
        );
        assert_eq!(
            responses
                .iter()
                .filter(|r| r.status == Status::Cancelled)
                .count(),
            usize::from(cancelled)
        );
        assert!(responses.iter().all(|r| r.error == 0));
        previous = Some(responses[0]);
        assert_eq!((client.outstanding(), bridge.outstanding()), (0, 0));
    }
    assert_eq!(bridge.next_frame(), 128);
    finish(bridge);
}

#[test]
fn callback_fault_is_permanent_and_pending_wire_ownership_is_reclaimed_off_render() {
    let mut bridge = Bridge::new(37).unwrap();
    let mut client = Client::new(37).unwrap();
    send(
        &mut client,
        &mut bridge,
        0,
        Transition::Record {
            monitor_previous: false,
        },
        false,
        false,
    );
    let input = [MidiBuffer::new(2, 6).unwrap()];
    let mut output = [MidiBuffer::new(256, 1024).unwrap()];
    let mut left = [0.; 1];
    let mut right = [0.; 1];
    for (frame, expected) in [
        (1, rt_runtime::Error::InvalidFrame),
        (0, rt_runtime::Error::InvalidState),
    ] {
        let result = bridge.process(
            AudioBlock {
                frame: Frame(frame),
                frames: 1,
                inputs: &[&[0.], &[0.]],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        );
        assert_eq!(result, Err(expected));
        assert_eq!(bridge.next_frame(), 0);
    }
    let another = client
        .prepare(0, bridge.target(), Transition::Stop, true, false)
        .unwrap();
    assert_eq!(bridge.ingest(&another), Err(Error::State));
    assert_eq!(bridge.next_response().unwrap(), None);
    assert_eq!(bridge.outstanding(), 1);
    drop(client);
    finish(bridge);
}

#[test]
fn response_codec_preserves_error_origin_and_rejects_noncanonical_records() {
    let base = Response {
        session: u64::MAX,
        sequence: u64::MAX,
        slot: 4,
        phase: Phase::Execution,
        status: Status::Rejected,
        runtime_sequence: u64::MAX,
        requested: u64::MAX - 2,
        effective: u64::MAX - 1,
        delivered: Some(u64::MAX - 1),
        observed: u64::MAX,
        error: 1,
    };
    for code in (1..=20).chain(257..=268).chain(513..=532) {
        let response = Response {
            status: if code < 256 {
                Status::Rejected
            } else {
                Status::Failed
            },
            error: code,
            ..base
        };
        crate::guarded(|| {
            assert_eq!(
                Response::decode(&response.encode().unwrap()).unwrap(),
                response
            )
        });
    }
    let packet = base.encode().unwrap();
    for end in 0..wire::RESPONSE_BYTES {
        assert_eq!(
            crate::guarded(|| Response::decode(&packet[..end])),
            Err(Error::Size)
        );
    }
    let mut oversized = [0; wire::RESPONSE_BYTES + 1];
    oversized[..wire::RESPONSE_BYTES].copy_from_slice(&packet);
    assert_eq!(Response::decode(&oversized), Err(Error::Size));
    for offset in 76..wire::RESPONSE_BYTES {
        let mut invalid = packet;
        invalid[offset] = 1;
        assert_eq!(
            crate::guarded(|| Response::decode(&invalid)),
            Err(Error::Reserved)
        );
    }
    for (offset, value, expected) in [
        (0, 0, Error::Format),
        (4, 2, Error::Version),
        (6, 0, Error::Operation),
        (7, 2, Error::Reserved),
        (24, 5, Error::Slot),
        (28, 0, Error::Operation),
        (72, 0, Error::State),
    ] {
        let mut invalid = packet;
        invalid[offset] = value;
        assert_eq!(crate::guarded(|| Response::decode(&invalid)), Err(expected));
    }
}
