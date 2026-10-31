use super::*;
use crate::{
    bridge_tests::{drain, finish, process, send},
    graph_transfer::{Payload, Record},
    guarded, Bridge,
};
use rt_runtime::graph::PreparedPlan;
use shoop_loops::Transition;
use std::sync::{atomic::AtomicU32, Arc};

fn reference(bypass: bool) -> PreparedPlan {
    let (control, engine, _) = crate::scenario::prepare_with_fault(0, Arc::new(AtomicU32::new(0)));
    let plan = control
        .compile_graph(&crate::scenario::routes(bypass))
        .unwrap();
    drop(control);
    let mut stopped = engine.into_stopped();
    stopped.cleanup().unwrap();
    for _ in 0..512 {
        stopped.resources().begin_callback();
        stopped.resources().reclaim();
    }
    assert_eq!(stopped.resource_usage().free_pages, 256);
    assert_eq!(stopped.media_usage().references, 0);
    plan
}
fn load(bridge: &mut Bridge, sequence: &mut u64, id: Id, epoch: Epoch, plan: &PreparedPlan) {
    let mut record = |payload| {
        let bytes = Record {
            session: 7,
            sequence: *sequence + 1,
            id,
            payload,
        }
        .encode()
        .unwrap();
        bridge.graph_payload(&bytes).unwrap();
        *sequence += 1;
    };
    record(Payload::Begin {
        epoch,
        counts: Counts {
            steps: plan.descriptor().steps.len() as u32,
            edges: plan.descriptor().connections.len() as u32,
            bindings: plan.planning().bindings.len() as u32,
        },
        planned: plan.planning().step.is_some(),
    });
    for (offset, &step) in plan.descriptor().steps.iter().enumerate() {
        record(Payload::Step {
            offset: offset as u32,
            step,
        });
    }
    for (offset, &edge) in plan.descriptor().connections.iter().enumerate() {
        record(Payload::Edge {
            offset: offset as u32,
            edge,
        });
    }
    if let Some(step) = plan.planning().step {
        record(Payload::Planning(step));
    }
    for (offset, &binding) in plan.planning().bindings.iter().enumerate() {
        record(Payload::Binding {
            offset: offset as u32,
            binding,
        });
    }
    record(Payload::Seal);
}
fn gsend(
    client: &mut Client,
    bridge: &mut Bridge,
    operation: Operation,
    cancelled: bool,
) -> Request {
    let packet = guarded(|| client.prepare(operation, cancelled)).unwrap();
    let request = Request::decode(&packet).unwrap();
    bridge.graph_ingest(&packet).unwrap();
    request
}
fn gdrain(client: &mut Client, bridge: &mut Bridge) -> Vec<Response> {
    let mut replies = Vec::new();
    while let Some(packet) = bridge.next_graph_response().unwrap() {
        replies.push(guarded(|| client.receive(&packet)).unwrap());
        assert!(replies.len() <= 2 * CREDITS);
    }
    replies
}
fn stage_response(request: Request) -> Response {
    let Operation::Stage { bank, epoch, slot } = request.operation else {
        panic!("stage");
    };
    Response {
        session: request.session,
        sequence: request.sequence,
        slot: request.slot,
        kind: 1,
        phase: Phase::Admission,
        status: Status::Staged,
        runtime_sequence: u64::MAX,
        requested: 0,
        effective: 0,
        delivered: None,
        observed: u64::MAX,
        error: 0,
        graph: Some(StageId::from_parts(u32::MAX, slot, u32::MAX).unwrap()),
        graph_epoch: epoch,
        topology_epoch: Epoch(0),
        bank: Some(Bank {
            id: bank,
            capacity: Counts {
                steps: u32::MAX,
                edges: u32::MAX,
                bindings: u32::MAX,
            },
        }),
    }
}

#[test]
fn graph_command_codec_preserves_full_width_fields_and_canonical_padding() {
    let id = StageId::from_parts(u32::MAX, u32::MAX, u32::MAX).unwrap();
    let ops = [
        Operation::Stage {
            bank: Id {
                slot: u32::MAX,
                generation: u32::MAX,
            },
            slot: u32::MAX,
            epoch: Epoch(u64::MAX),
        },
        Operation::Activate {
            graph: id,
            at: u64::MAX,
            graph_epoch: Epoch(u64::MAX),
            topology_epoch: Epoch(u64::MAX),
        },
        Operation::Release { graph: id },
    ];
    for (i, operation) in ops.into_iter().enumerate() {
        let request = Request {
            session: u64::MAX,
            sequence: u64::MAX,
            slot: 1,
            cancelled: true,
            operation,
        };
        let bytes = guarded(|| request.encode()).unwrap();
        let mut expected = [0; REQUEST_BYTES];
        expected[..8].copy_from_slice(&[76, 76, 71, 49, 1, 0, i as u8 + 1, 1]);
        expected[8..24].fill(255);
        expected[24] = 1;
        match i {
            0 => expected[28..48].fill(255),
            1 => expected[40..76].fill(255),
            _ => expected[64..76].fill(255),
        }
        assert_eq!(bytes, expected);
        assert_eq!(guarded(|| Request::decode(&bytes)), Ok(request));
        for len in 0..REQUEST_BYTES {
            assert_eq!(Request::decode(&bytes[..len]), Err(Error::Size));
        }
        let used: &[std::ops::Range<usize>] = match i {
            0 => &[0..48],
            1 => &[0..28, 40..76],
            _ => &[0..28, 64..76],
        };
        for at in 0..REQUEST_BYTES {
            if !used.iter().any(|r| r.contains(&at)) {
                let mut bad = bytes;
                bad[at] = 1;
                assert!(Request::decode(&bad).is_err(), "{i}:{at}");
            }
        }
        let response = if i == 0 {
            stage_response(request)
        } else {
            Response {
                session: u64::MAX,
                sequence: u64::MAX,
                slot: 1,
                kind: 2,
                phase: Phase::Execution,
                status: Status::Activated,
                runtime_sequence: u64::MAX,
                requested: u64::MAX,
                effective: u64::MAX,
                delivered: Some(u64::MAX),
                observed: u64::MAX,
                error: 0,
                graph: Some(id),
                graph_epoch: Epoch(u64::MAX),
                topology_epoch: Epoch(u64::MAX),
                bank: None,
            }
        };
        let bytes = guarded(|| response.encode()).unwrap();
        let mut expected = [0; RESPONSE_BYTES];
        expected[..6].copy_from_slice(b"LLS1\x01\x00");
        expected[8..24].fill(255);
        expected[24] = 1;
        if i == 0 {
            expected[6] = 1;
            expected[7] = 6;
            expected[28] = 1;
            expected[124] = 1;
            for range in [32..40, 64..72, 76..96, 104..124] {
                expected[range].fill(255);
            }
        } else {
            expected[6] = 2;
            expected[7] = 3;
            expected[28] = 4;
            expected[124] = 2;
            expected[32..72].fill(255);
            expected[76..104].fill(255);
        }
        assert_eq!(bytes, expected);
        assert_eq!(guarded(|| Response::decode(&bytes)), Ok(response));
        for len in 0..RESPONSE_BYTES {
            assert_eq!(Response::decode(&bytes[..len]), Err(Error::Size));
        }
        for at in 125..128 {
            let mut bad = bytes;
            bad[at] = 1;
            assert_eq!(Response::decode(&bad), Err(Error::Reserved));
        }
        let mut absent = response;
        absent.bank = None;
        if i == 0 {
            assert_eq!(absent.encode(), Err(Error::State));
        }
        let mut contradictory = response;
        contradictory.phase = Phase::Admission;
        contradictory.delivered = Some(0);
        assert_eq!(contradictory.encode(), Err(Error::State));
    }
}

#[test]
fn graph_client_credits_validate_identity_phase_and_last_sequence_without_wrapping() {
    let mut client = Client::new(u64::MAX).unwrap();
    let operation = Operation::Stage {
        bank: Id {
            slot: 0,
            generation: u32::MAX,
        },
        slot: 1,
        epoch: Epoch(u64::MAX),
    };
    let one = Request::decode(&guarded(|| client.prepare(operation, false)).unwrap()).unwrap();
    guarded(|| client.prepare(operation, true)).unwrap();
    assert_eq!(
        guarded(|| client.prepare(operation, false)),
        Err(Error::Busy)
    );
    let response = stage_response(one);
    let bytes = response.encode().unwrap();
    let mut invalid = response;
    invalid.session ^= 1;
    assert_eq!(
        guarded(|| client.receive(&invalid.encode().unwrap())),
        Err(Error::Session)
    );
    invalid = response;
    invalid.bank.as_mut().unwrap().id.generation -= 1;
    assert_eq!(
        guarded(|| client.receive(&invalid.encode().unwrap())),
        Err(Error::Identity)
    );
    assert_eq!(client.outstanding(), 2);
    assert_eq!(guarded(|| client.receive(&bytes)), Ok(response));
    assert_eq!(guarded(|| client.receive(&bytes)), Err(Error::State));
    assert_eq!(client.outstanding(), 1);
    assert_eq!(client.retained_graphs(), 1);
    let release = Request::decode(
        &client
            .prepare(
                Operation::Release {
                    graph: response.graph.unwrap(),
                },
                false,
            )
            .unwrap(),
    )
    .unwrap();
    let released = Response {
        session: release.session,
        sequence: release.sequence,
        slot: release.slot,
        kind: 3,
        phase: Phase::Admission,
        status: Status::Released,
        runtime_sequence: 10,
        requested: 0,
        effective: 0,
        delivered: None,
        observed: 10,
        error: 0,
        graph: response.graph,
        graph_epoch: Epoch(0),
        topology_epoch: Epoch(0),
        bank: Some(Bank {
            id: Id {
                slot: 0,
                generation: 0,
            },
            capacity: Counts {
                steps: 0,
                edges: 0,
                bindings: 0,
            },
        }),
    };
    let mut wrong_bank = released;
    wrong_bank.bank.as_mut().unwrap().id.slot = 1;
    assert_eq!(
        guarded(|| client.receive(&wrong_bank.encode().unwrap())),
        Err(Error::Identity)
    );
    wrong_bank = released;
    wrong_bank.bank.as_mut().unwrap().id.generation = 1;
    assert_eq!(
        guarded(|| client.receive(&wrong_bank.encode().unwrap())),
        Err(Error::Identity)
    );
    assert_eq!(client.outstanding(), 2);
    assert_eq!(client.retained_graphs(), 1);
    guarded(|| client.receive(&released.encode().unwrap())).unwrap();
    assert_eq!(client.outstanding(), 1);
    assert_eq!(client.retained_graphs(), 0);
    client.next_sequence = Some(u64::MAX);
    let final_packet = guarded(|| {
        client.prepare(
            Operation::Release {
                graph: response.graph.unwrap(),
            },
            false,
        )
    })
    .unwrap();
    assert_eq!(Request::decode(&final_packet).unwrap().sequence, u64::MAX);
    let request = Request::decode(&final_packet).unwrap();
    let rejected = Response {
        session: request.session,
        sequence: request.sequence,
        slot: request.slot,
        kind: 3,
        phase: Phase::Admission,
        status: Status::Rejected,
        runtime_sequence: 0,
        requested: 0,
        effective: 0,
        delivered: None,
        observed: 0,
        error: 8,
        graph: None,
        graph_epoch: Epoch(0),
        topology_epoch: Epoch(0),
        bank: None,
    };
    guarded(|| client.receive(&rejected.encode().unwrap())).unwrap();
    assert_eq!(
        guarded(|| client.prepare(operation, false)),
        Err(Error::Exhausted)
    );

    let mut client = Client::new(7).unwrap();
    let id = StageId::from_parts(8, 0, 1).unwrap();
    let request = Request::decode(
        &client
            .prepare(
                Operation::Activate {
                    graph: id,
                    at: 4,
                    graph_epoch: Epoch(1),
                    topology_epoch: Epoch(0),
                },
                false,
            )
            .unwrap(),
    )
    .unwrap();
    let terminal = Response {
        session: 7,
        sequence: request.sequence,
        slot: request.slot,
        kind: 2,
        phase: Phase::Execution,
        status: Status::Activated,
        runtime_sequence: 9,
        requested: 4,
        effective: 5,
        delivered: Some(5),
        observed: 5,
        error: 0,
        graph: Some(id),
        graph_epoch: Epoch(2),
        topology_epoch: Epoch(0),
        bank: None,
    };
    assert_eq!(
        client.receive(&terminal.encode().unwrap()),
        Err(Error::State)
    );
    let admitted = Response {
        phase: Phase::Admission,
        status: Status::Admitted,
        delivered: None,
        graph_epoch: Epoch(0),
        ..terminal
    };
    client.receive(&admitted.encode().unwrap()).unwrap();
    assert_eq!(
        client.receive(&admitted.encode().unwrap()),
        Err(Error::State)
    );
    let mut wrong = terminal;
    wrong.runtime_sequence += 1;
    assert_eq!(
        client.receive(&wrong.encode().unwrap()),
        Err(Error::Sequence)
    );
    wrong = terminal;
    wrong.graph = Some(StageId::from_parts(9, 0, 1).unwrap());
    assert_eq!(
        client.receive(&wrong.encode().unwrap()),
        Err(Error::Identity)
    );
    wrong = terminal;
    wrong.graph_epoch = Epoch(3);
    assert_eq!(
        client.receive(&wrong.encode().unwrap()),
        Err(Error::Identity)
    );
    wrong = terminal;
    wrong.topology_epoch = Epoch(1);
    assert_eq!(
        client.receive(&wrong.encode().unwrap()),
        Err(Error::Identity)
    );
    assert_eq!(client.outstanding(), 1);
    client.receive(&terminal.encode().unwrap()).unwrap();
    assert_eq!(client.outstanding(), 0);
}

#[test]
fn actual_bridge_graph_staging_activation_and_retirement_match_all_media_and_receipts() {
    let plan = reference(true);
    for parts in [vec![3, 8, 1], vec![1; 12], vec![0, 3, 0, 5, 3, 1, 0]] {
        let mut bridge = Bridge::with_graphs(7).unwrap();
        let mut client = Client::new(7).unwrap();
        let mut primitive = crate::Client::new(7).unwrap();
        let mut sequence = 0;
        let bank = Id {
            slot: 0,
            generation: 1,
        };
        load(&mut bridge, &mut sequence, bank, Epoch(1), &plan);
        gsend(
            &mut client,
            &mut bridge,
            Operation::Stage {
                bank,
                slot: 0,
                epoch: Epoch(1),
            },
            false,
        );
        for (at, t) in [
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
            (12, Transition::Stop),
        ] {
            send(&mut primitive, &mut bridge, at, t, false, false);
        }
        let (mut samples, mut midi) = process(&mut bridge, 1);
        let ready = gdrain(&mut client, &mut bridge);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].status, Status::Staged);
        let graph = ready[0].graph.unwrap();
        assert_eq!(ready[0].bank.unwrap().id, bank);
        assert_eq!(bridge.retained_graphs(), 1);
        gsend(
            &mut client,
            &mut bridge,
            Operation::Activate {
                graph,
                at: 4,
                graph_epoch: Epoch(1),
                topology_epoch: Epoch(0),
            },
            false,
        );
        for frames in parts {
            let (a, m) = process(&mut bridge, frames);
            samples.extend(a);
            midi.extend(m);
        }
        let expected: Vec<_> = [1., 2., 3., 4., 1., 2., 3., 4., 1., 2., 3., 4., 0.]
            .into_iter()
            .map(|n| [n, n * 10.])
            .collect();
        assert_eq!(samples, expected);
        assert_eq!(midi, crate::run(&[13]).midi);
        let replies = gdrain(&mut client, &mut bridge);
        assert_eq!(replies.len(), 2);
        assert_eq!(
            (replies[0].status, replies[1].status),
            (Status::Admitted, Status::Activated)
        );
        assert_eq!(
            (
                replies[1].requested,
                replies[1].effective,
                replies[1].delivered,
                replies[1].observed
            ),
            (4, 4, Some(4), 4)
        );
        assert_eq!(replies[1].graph_epoch, Epoch(2));
        let replies = drain(&mut bridge, &mut primitive);
        assert_eq!(replies.len(), 6);
        assert_eq!(
            replies
                .iter()
                .filter(|r| r.phase == Phase::Execution)
                .map(|r| (r.requested, r.delivered.unwrap(), r.observed))
                .collect::<Vec<_>>(),
            [(0, 0, 0), (4, 4, 4), (12, 12, 12)]
        );
        assert_eq!(bridge.outstanding(), 0);
        gsend(
            &mut client,
            &mut bridge,
            Operation::Release { graph },
            false,
        );
        process(&mut bridge, 1);
        let returned = gdrain(&mut client, &mut bridge);
        assert_eq!(returned[0].status, Status::Released);
        let next = returned[0].bank.unwrap();
        assert_eq!(
            next.id,
            Id {
                slot: 0,
                generation: 2
            }
        );
        assert_eq!(
            next.capacity,
            Counts {
                steps: 3,
                edges: 12,
                bindings: 2
            }
        );
        assert_eq!(bridge.retained_graphs(), 0);
        assert_eq!(client.outstanding(), 0);
        load(&mut bridge, &mut sequence, next.id, Epoch(2), &plan);
        gsend(
            &mut client,
            &mut bridge,
            Operation::Stage {
                bank: next.id,
                slot: 0,
                epoch: Epoch(2),
            },
            false,
        );
        process(&mut bridge, 1);
        let graph2 = gdrain(&mut client, &mut bridge)[0].graph.unwrap();
        assert_eq!(graph2.parts()[2], graph.parts()[2] + 1);
        gsend(
            &mut client,
            &mut bridge,
            Operation::Release { graph: graph2 },
            false,
        );
        process(&mut bridge, 1);
        assert_eq!(
            gdrain(&mut client, &mut bridge)[0]
                .bank
                .unwrap()
                .id
                .generation,
            3
        );
        assert_eq!(bridge.outstanding(), 0);
        assert_eq!(bridge.retained_graphs(), 0);
        let mut stopped = bridge.into_stopped();
        assert_eq!(
            stopped
                .observer()
                .as_any()
                .downcast_ref::<crate::journal::Journal>()
                .unwrap()
                .bytes(),
            crate::journal::expected_primitive()
        );
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
}

#[test]
fn stalled_graph_credits_preserve_music_and_independent_primitive_recovery() {
    let plan = reference(false);
    let mut bridge = Bridge::with_graphs(7).unwrap();
    let mut client = Client::new(7).unwrap();
    let mut primitive = crate::Client::new(7).unwrap();
    let mut sequence = 0;
    for slot in 0..2 {
        let bank = Id {
            slot,
            generation: 1,
        };
        load(&mut bridge, &mut sequence, bank, Epoch(1), &plan);
        gsend(
            &mut client,
            &mut bridge,
            Operation::Stage {
                bank,
                slot,
                epoch: Epoch(1),
            },
            false,
        );
    }
    for (at, t) in [
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
        (12, Transition::Stop),
    ] {
        send(&mut primitive, &mut bridge, at, t, false, false);
    }
    send(
        &mut primitive,
        &mut bridge,
        12,
        Transition::Stop,
        false,
        false,
    );
    let (samples, midi) = process(&mut bridge, 13);
    let expected = crate::run(&[13]);
    assert_eq!(samples, expected.samples);
    assert_eq!(midi, expected.midi);
    assert_eq!(client.outstanding(), 2);
    assert_eq!(primitive.outstanding(), 4);
    assert_eq!(bridge.retained_graphs(), 2);
    assert_eq!(
        client.prepare(
            Operation::Release {
                graph: StageId::from_parts(1, 0, 1).unwrap()
            },
            false
        ),
        Err(Error::Busy)
    );
    send(
        &mut primitive,
        &mut bridge,
        13,
        Transition::Stop,
        true,
        false,
    );
    process(&mut bridge, 1);
    let primitive_replies = drain(&mut bridge, &mut primitive);
    assert_eq!(primitive_replies.len(), 10);
    let winner = primitive_replies
        .iter()
        .find(|r| r.sequence == 4 && r.phase == Phase::Execution)
        .unwrap();
    assert_eq!(
        (
            winner.status,
            winner.requested,
            winner.delivered,
            winner.observed
        ),
        (crate::wire::Status::Applied, 12, Some(12), 12)
    );
    assert_eq!(primitive.outstanding(), 0);
    let staged = gdrain(&mut client, &mut bridge);
    assert_eq!(staged.len(), 2);
    for response in staged {
        gsend(
            &mut client,
            &mut bridge,
            Operation::Release {
                graph: response.graph.unwrap(),
            },
            false,
        );
    }
    process(&mut bridge, 1);
    let returned = gdrain(&mut client, &mut bridge);
    assert_eq!(returned.len(), 2);
    assert!(returned
        .iter()
        .all(|r| r.status == Status::Released && r.bank.unwrap().id.generation == 2));
    assert_eq!(bridge.outstanding(), 0);
    assert_eq!(bridge.retained_graphs(), 0);
    finish(bridge);
}

#[test]
fn graph_candidate_returns_and_domain_failure_origins_survive_128_wire_cycles() {
    let plan = reference(false);
    let mut invalid = PreparedPlan::with_capacity(plan.config()).unwrap();
    for &step in &plan.descriptor().steps {
        invalid.push_step(step).unwrap();
    }
    for &edge in &plan.descriptor().connections {
        invalid.push_connection(edge).unwrap();
    }
    let mut bridge = Bridge::with_graphs(7).unwrap();
    let mut client = Client::new(7).unwrap();
    let mut sequence = 0;
    let mut bank = Id {
        slot: 0,
        generation: 1,
    };
    let mut old_graph = None;
    for round in 0..128 {
        let epoch = Epoch(if round % 4 == 1 { 0 } else { 1 });
        load(
            &mut bridge,
            &mut sequence,
            bank,
            epoch,
            if round % 4 == 2 { &invalid } else { &plan },
        );
        let request = gsend(
            &mut client,
            &mut bridge,
            Operation::Stage {
                bank,
                slot: 0,
                epoch,
            },
            round % 4 == 0,
        );
        assert_eq!(
            bridge.graph_cancel(request.slot, request.sequence + 1),
            Err(Error::Sequence)
        );
        process(&mut bridge, 1);
        let replies = gdrain(&mut client, &mut bridge);
        assert_eq!(replies.len(), 1);
        let response = replies[0];
        match round % 4 {
            0 => {
                assert_eq!(response.status, Status::Cancelled);
                assert_eq!(response.error, 0);
            }
            1 => {
                assert_eq!(response.status, Status::Rejected);
                assert_eq!(response.error, 14);
            }
            2 => {
                assert_eq!(response.status, Status::Rejected);
                assert_eq!(response.error, 11);
            }
            _ => {
                assert_eq!(response.status, Status::Staged);
                let graph = response.graph.unwrap();
                old_graph = Some(graph);
                gsend(
                    &mut client,
                    &mut bridge,
                    Operation::Release { graph },
                    false,
                );
                process(&mut bridge, 1);
                let reply = gdrain(&mut client, &mut bridge)[0];
                assert_eq!(reply.status, Status::Released);
                bank = reply.bank.unwrap().id;
            }
        }
        if round % 4 != 3 {
            bank = response.bank.unwrap().id;
        }
        assert_eq!(bank.generation, round + 2);
        assert_eq!(client.outstanding(), 0);
        assert_eq!(bridge.retained_graphs(), 0);
    }
    let graph = old_graph.unwrap();
    let at = bridge.next_frame();
    gsend(
        &mut client,
        &mut bridge,
        Operation::Activate {
            graph,
            at,
            graph_epoch: Epoch(1),
            topology_epoch: Epoch(0),
        },
        false,
    );
    process(&mut bridge, 1);
    let replies = gdrain(&mut client, &mut bridge);
    assert_eq!(
        (replies[0].status, replies[1].status),
        (Status::Admitted, Status::Failed)
    );
    assert_eq!(replies[1].error, 518);
    assert_eq!(replies[1].delivered, Some(at));
    gsend(
        &mut client,
        &mut bridge,
        Operation::Release { graph },
        false,
    );
    process(&mut bridge, 1);
    let reply = gdrain(&mut client, &mut bridge)[0];
    assert_eq!(reply.status, Status::Rejected);
    assert_eq!(reply.error, 6);
    assert_eq!(bridge.outstanding(), 0);
    finish(bridge);
}

#[test]
fn malformed_graph_requests_preserve_receiver_state_and_queued_ready_owners_drop_off_render() {
    let plan = reference(false);
    for render in [false, true] {
        let mut bridge = Bridge::with_graphs(7).unwrap();
        let mut client = Client::new(7).unwrap();
        let mut sequence = 0;
        let bank = Id {
            slot: 0,
            generation: 1,
        };
        load(&mut bridge, &mut sequence, bank, Epoch(1), &plan);
        let request = Request {
            session: 7,
            sequence: 1,
            slot: 0,
            cancelled: false,
            operation: Operation::Stage {
                bank,
                slot: 0,
                epoch: Epoch(1),
            },
        };
        let mut bad = request;
        bad.session = 8;
        assert_eq!(
            bridge.graph_ingest(&bad.encode().unwrap()),
            Err(Error::Session)
        );
        bad = request;
        bad.operation = Operation::Stage {
            bank,
            slot: 0,
            epoch: Epoch(2),
        };
        assert_eq!(
            bridge.graph_ingest(&bad.encode().unwrap()),
            Err(Error::State)
        );
        let packet = client.prepare(request.operation, false).unwrap();
        bridge.graph_ingest(&packet).unwrap();
        assert_eq!(bridge.graph_ingest(&packet), Err(Error::Sequence));
        let abort = Record {
            session: 7,
            sequence: sequence + 1,
            id: bank,
            payload: Payload::Abort,
        }
        .encode()
        .unwrap();
        assert_eq!(bridge.graph_payload(&abort), Err(rt_runtime::Error::Busy));
        if render {
            process(&mut bridge, 1);
            assert_eq!(bridge.retained_graphs(), 1);
        } else {
            assert_eq!(bridge.retained_graphs(), 0);
            process(&mut bridge, 0);
            assert!(bridge.next_graph_response().unwrap().is_none());
        }
        assert_eq!(client.outstanding(), 1);
        assert_eq!(bridge.outstanding(), 1);
        finish(bridge);
    }
    let mut disabled = Bridge::new(7).unwrap();
    assert!(disabled.next_graph_response().is_err());
    assert_eq!(
        disabled.graph_payload(&[0; 96]),
        Err(rt_runtime::Error::InvalidState)
    );
    finish(disabled);
}
