use super::*;
use crate::image::{Description, Identity, Limits, LoopSpec};
use crate::protocol::put;
use shoop_wasm_test_support::shoop_test;

fn image(requests: u32) -> TransitionImage {
    let id = Identity {
        slot: 0,
        generation: 17,
    };
    TransitionImage {
        description: Description {
            quantum: 32,
            loops: vec![LoopSpec {
                id,
                kind: "primitive".into(),
                length: 64,
                sync_source: Some(id),
                repeat_sync: true,
                timeline: None,
            }],
        },
        limits: Limits {
            loop_slots: 2,
            pending_requests: requests,
            events: 2 * 2 + requests,
        },
        dependency_order: vec![id],
    }
}
fn request(sequence: u64) -> [u32; 20] {
    let mut words = [0; 20];
    put(&mut words, 64, 64, 17u64 << 32);
    put(&mut words, 128, 64, sequence);
    words
}
fn ack(transfer: Transfer) -> TpReply {
    let mut words = [0; 26];
    words[0] = transfer.index;
    words[1] = get(&transfer.words, 1, 32) as u32;
    TpReply { error: 0, words }
}
fn ready_install(service: &mut TransitionService, mut transfer: Transfer) -> Transfer {
    while transfer.opcode != 4 {
        let mut words = [0; 26];
        match transfer.opcode {
            18 => put(&mut words, 0, 64, service.stage_counter + 1),
            21 => put(&mut words, 0, 64, get(&transfer.words, 0, 64)),
            19 | 20 => {}
            other => panic!("unexpected setup opcode {other}"),
        }
        transfer = match service.finish(TpReply { error: 0, words }).unwrap() {
            Progress::Transfer(next) => next,
            Progress::Reply(reply) => panic!("setup completed early {reply:?}"),
        };
    }
    transfer
}
fn completed(progress: Progress) -> TpReply {
    match progress {
        Progress::Reply(reply) => reply,
        Progress::Transfer(_) => panic!("unfinished exchange"),
    }
}
fn install(service: &mut TransitionService, sequence: u64) -> Transfer {
    let transfer = service.begin(4, 0, request(sequence)).unwrap();
    let transfer = ready_install(service, transfer);
    assert_eq!(completed(service.finish(ack(transfer)).unwrap()).error, 0);
    transfer
}
fn collect(service: &mut TransitionService, transfer: Transfer) {
    let mut data = [0; 20];
    data[0] = get(&transfer.words, 1, 32) as u32;
    service.begin(6, transfer.index, data).unwrap();
    let mut words = [0; 26];
    let original = service.slots[transfer.index as usize].request.unwrap();
    put(&mut words, 0, 45, get(&original, 0, 45));
    put(&mut words, 0, 1, 1);
    put(&mut words, 1, 32, get(&transfer.words, 1, 32));
    put(&mut words, 37, 3, 1);
    for (source, target, width) in [
        (64, wire::RESULT_TARGET, 32),
        (128, wire::RESULT_SEQUENCE, 64),
        (384, wire::RESULT_POSITION, 64),
    ] {
        put(&mut words, target, width, get(&original, source, width));
    }
    put(&mut words, wire::FRAME, 64, 192);
    service.finish(TpReply { error: 0, words }).unwrap();
}

#[shoop_test]
fn credits_and_sequences_survive_cancellation_and_pending_collection() {
    let mut service = TransitionService::new(&image(2)).unwrap();
    let first = install(&mut service, 9);
    assert_eq!(first.index, 0);
    assert_eq!(service.begin(4, 0, request(9)), Err(4));
    let mut data = [0; 20];
    data[0] = 1;
    service.begin(6, 0, data).unwrap();
    service
        .finish(TpReply {
            error: 0,
            words: [0; 26],
        })
        .unwrap();
    service.begin(5, 0, data).unwrap();
    let mut cancelled = [0; 26];
    cancelled[0] = 1;
    service
        .finish(TpReply {
            error: 0,
            words: cancelled,
        })
        .unwrap();
    assert_eq!(service.begin(4, 0, request(9)), Err(4));
    assert_eq!(install(&mut service, 10).index, 1);
    assert_eq!(service.begin(4, 0, request(11)), Err(2));
    collect(&mut service, first);
    let next = install(&mut service, 9);
    assert_eq!((next.index, get(&next.words, 1, 32)), (0, 2));
    assert_eq!(service.begin(6, 0, data), Err(3));
}

#[shoop_test]
fn validation_precedence_includes_holes_alignment_and_source_relationships() {
    let mut service = TransitionService::new(&image(1)).unwrap();
    install(&mut service, 1);
    let mut words = request(1);
    put(&mut words, 64, 32, 1);
    assert_eq!(service.begin(4, 0, words), Err(3));
    words = request(1);
    put(&mut words, 34, 2, 3);
    put(&mut words, 384, 64, 1);
    assert_eq!(service.begin(4, 0, words), Err(6));
    words = request(1);
    put(&mut words, 33, 1, 1);
    put(&mut words, 256, 64, 1);
    assert_eq!(service.begin(4, 0, words), Err(3));
    words = request(1);
    put(&mut words, 33, 1, 1);
    put(&mut words, 256, 64, 17u64 << 32);
    assert_eq!(service.begin(4, 0, words), Err(4));
    put(&mut words, 128, 64, 2);
    assert_eq!(service.begin(4, 0, words), Err(2));
    let mut topology = image(1);
    topology.description.loops[0].sync_source = None;
    let mut service = TransitionService::new(&topology).unwrap();
    assert_eq!(service.begin(4, 0, words), Err(7));
}

#[shoop_test]
fn refused_install_rolls_back_but_uncertain_or_malformed_replies_poison_ownership() {
    let mut service = TransitionService::new(&image(1)).unwrap();
    let transfer = service.begin(4, 0, request(1)).unwrap();
    assert_eq!(service.begin(4, 0, request(2)), Err(5));
    ready_install(&mut service, transfer);
    service
        .finish(TpReply {
            error: 6,
            words: [0; 26],
        })
        .unwrap();
    let transfer = service.begin(4, 0, request(1)).unwrap();
    let transfer = ready_install(&mut service, transfer);
    let mut wrong = ack(transfer);
    wrong.words[1] += 1;
    assert_eq!(service.finish(wrong), Err(Error::Disconnected));
    assert_eq!(service.begin(4, 0, request(2)), Err(5));
    let mut service = TransitionService::new(&image(1)).unwrap();
    assert_eq!(
        service.exchange(4, 0, request(1), |_| Err(Error::Disconnected)),
        Err(Error::Disconnected)
    );
    assert_eq!(service.begin(4, 0, request(2)), Err(5));
}

#[shoop_test]
fn generation_overflow_retires_a_slot_and_recreation_requires_a_new_device_epoch() {
    let mut service = TransitionService::new(&image(1)).unwrap();
    service.slots[0].generation = u32::MAX;
    let transfer = install(&mut service, 1);
    collect(&mut service, transfer);
    assert_eq!(service.begin(4, 0, request(2)), Err(2));
    assert_eq!(service.begin(5, 0, [0; 20]), Err(3));
    let mut restarted = TransitionService::new(&image(1)).unwrap();
    assert_eq!(get(&install(&mut restarted, 1).words, 1, 32), 1);
}

#[shoop_test]
fn malformed_collection_cannot_release_or_reuse_a_slot() {
    let mut service = TransitionService::new(&image(1)).unwrap();
    let transfer = install(&mut service, 1);
    let mut data = [0; 20];
    data[0] = 1;
    service.begin(6, 0, data).unwrap();
    let mut reply = [0; 26];
    put(&mut reply, 1, 32, get(&transfer.words, 1, 32));
    put(&mut reply, 0, 1, 1);
    put(&mut reply, 37, 3, 1);
    put(&mut reply, wire::RESULT_SEQUENCE, 64, 99);
    assert_eq!(
        service.finish(TpReply {
            error: 0,
            words: reply
        }),
        Err(Error::Disconnected)
    );
    assert_eq!(service.begin(4, 0, request(2)), Err(5));
}

#[shoop_test]
fn static_stream_preparation_reserves_real_credits_without_predicting_outcomes() {
    let mut service = TransitionService::new(&image(2)).unwrap();
    let program = service
        .prepare_static_batch(&[request(10), request(11)])
        .unwrap();
    assert_eq!(
        program.actions.iter().map(|t| t.index).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        (
            program.setup.first().unwrap().opcode,
            program.setup.last().unwrap().opcode
        ),
        (18, 21)
    );
    let mut service = TransitionService::new(&image(2)).unwrap();
    assert!(matches!(
        service.prepare_static_batch(&[request(10), request(10)]),
        Err(4)
    ));
    assert!(matches!(
        service.prepare_static_batch(&[request(10), request(11), request(12)]),
        Err(2)
    ));
    assert_eq!(
        service
            .prepare_static_batch(&[request(12)])
            .unwrap()
            .actions[0]
            .index,
        0
    );
}

#[shoop_test]
fn kernel_codec_supports_positioned_seek_without_expanding_application_whitelist() {
    let command = shoop_loops::Request {
        target: shoop_loops::LoopId::from_parts(0, 17).unwrap(),
        sequence: rt_runtime::Sequence(3),
        at: rt_runtime::Frame(64),
        timing: shoop_loops::Timing::At,
        transition: shoop_loops::Transition::Seek {
            position: 32,
            retrigger: true,
        },
    };
    assert_eq!(
        crate::protocol::transition(command),
        Err(Error::InvalidParameter)
    );
    let words = crate::protocol::kernel_transition(command).unwrap();
    assert_eq!(
        (get(&words, 34, 2), get(&words, 384, 64), get(&words, 36, 1)),
        (3, 32, 1)
    );
}

#[shoop_test]
fn compact_results_reconstruct_originals_and_authoritative_progress() {
    let mut service = TransitionService::new(&image(1)).unwrap();
    let mut original = request(u64::MAX);
    put(&mut original, 33, 1, 1);
    put(&mut original, 256, 64, 17u64 << 32);
    put(&mut original, 192, 64, 65);
    put(&mut original, 320, 64, u64::MAX);
    let transfer = service.begin(4, 0, original).unwrap();
    let transfer = ready_install(&mut service, transfer);
    assert_eq!(get(&transfer.words, wire::AT, 64), 65);
    assert_eq!(get(&transfer.words, wire::SKIP, 64), u64::MAX);
    assert_eq!(get(&transfer.words, wire::SEQUENCE, 64), u64::MAX);
    service.finish(ack(transfer)).unwrap();
    service.begin(13, 0, [0; 20]).unwrap();
    let mut row = [0; 26];
    put(&mut row, 0, 1, 1);
    put(&mut row, 1, 32, 1);
    put(&mut row, 33, 1, 1);
    put(&mut row, wire::RESULT_SEQUENCE, 64, u64::MAX);
    put(&mut row, wire::EFFECTIVE, 64, 96);
    put(&mut row, wire::REMAINING, 64, u64::MAX - 3);
    let observed = service
        .finish(TpReply {
            error: 0,
            words: row,
        })
        .unwrap();
    for (bit, expected) in [
        (128, u64::MAX),
        (192, 65),
        (256, 17u64 << 32),
        (320, u64::MAX),
        (448, 96),
        (512, u64::MAX - 3),
    ] {
        assert_eq!(get(&completed(observed).words, bit, 64), expected);
    }
    let mut identity = [0; 20];
    identity[0] = 1;
    service.begin(6, 0, identity).unwrap();
    put(&mut row, 37, 3, 3);
    put(&mut row, wire::FRAME, 64, 160);
    let collected = service
        .finish(TpReply {
            error: 0,
            words: row,
        })
        .unwrap();
    assert_eq!(get(&completed(collected).words, 576, 64), 160);
    assert_eq!(service.begin(4, 0, request(1)).unwrap().index, 0);
}

#[shoop_test]
fn multitransfer_publication_refusals_and_uncertain_acknowledgements_keep_ownership_safe() {
    for cut in 0..6 {
        let mut service = TransitionService::new(&image(2)).unwrap();
        let mut transfer = service.begin(4, 0, request(1)).unwrap();
        let mut steps = 0;
        while transfer.opcode != 4 && steps < cut {
            let mut words = [0; 26];
            if transfer.opcode == 18 {
                put(&mut words, 0, 64, service.stage_counter + 1);
            }
            if transfer.opcode == 21 {
                put(&mut words, 0, 64, get(&transfer.words, 0, 64));
            }
            transfer = match service.finish(TpReply { error: 0, words }).unwrap() {
                Progress::Transfer(next) => next,
                other => panic!("unexpected completion {other:?}"),
            };
            steps += 1;
        }
        let reply = completed(
            service
                .finish(TpReply {
                    error: 5,
                    words: [0; 26],
                })
                .unwrap(),
        );
        assert_eq!(reply.error, 5);
        assert!(service.slots.iter().all(|s| s.request.is_none()));
        let begin = service.begin(4, 0, request(1)).unwrap();
        assert_eq!(get(&begin.words, 0, 64), service.published);
        let install = ready_install(&mut service, begin);
        assert_eq!(install.index, 0);
        service.finish(ack(install)).unwrap();
        assert_eq!(service.begin(4, 0, request(1)), Err(4));
    }
    for opcode in [18, 19, 20, 21, 4] {
        let mut service = TransitionService::new(&image(2)).unwrap();
        let mut transfer = service.begin(4, 0, request(1)).unwrap();
        while transfer.opcode != opcode {
            let mut words = [0; 26];
            if transfer.opcode == 18 {
                put(&mut words, 0, 64, 1);
            }
            if transfer.opcode == 21 {
                put(&mut words, 0, 64, 1);
            }
            transfer = match service.finish(TpReply { error: 0, words }).unwrap() {
                Progress::Transfer(next) => next,
                other => panic!("unexpected completion {other:?}"),
            };
        }
        let mut wrong = [0; 26];
        wrong[25] = 1;
        assert_eq!(
            service.finish(TpReply {
                error: 0,
                words: wrong
            }),
            Err(Error::Disconnected)
        );
        assert_eq!(service.begin(4, 0, request(2)), Err(5));
    }
}

#[shoop_test]
fn compaction_omits_confirmed_terminals_but_keeps_their_credits_and_sequences() {
    let mut service = TransitionService::new(&image(2)).unwrap();
    install(&mut service, 10);
    install(&mut service, 20);
    service.begin(5, 0, [1; 20]).unwrap();
    service
        .finish(TpReply {
            error: 0,
            words: [0; 26],
        })
        .unwrap();
    let mut operations = vec![];
    let mut nonce = service.stage_counter;
    let result = service
        .exchange(22, 0, [0; 20], |transfer| {
            operations.push(transfer);
            let mut words = [0; 26];
            if transfer.opcode == 18 {
                nonce += 1;
                put(&mut words, 0, 64, nonce);
            }
            if transfer.opcode == 21 {
                put(&mut words, 0, 64, nonce);
            }
            Ok(TpReply { error: 0, words })
        })
        .unwrap();
    assert_eq!(result.words, [0; 26]);
    let nodes: Vec<_> = operations.iter().filter(|t| t.opcode == 19).collect();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].index, 1);
    assert_eq!(service.begin(4, 0, request(10)), Err(4));
    assert_eq!(service.begin(4, 0, request(30)), Err(2));
}
