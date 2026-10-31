use super::*;

fn call(h: &mut Harness, operation: Operation) -> Reply {
    let sequence = h.op(h.engine.next_frame().0, operation);
    h.step(1);
    h.drain();
    h.reply(sequence)
}
#[shoop_test]
fn source_validation_shares_callback_work_and_cancel_recovers_owned_inputs_under_saturation() {
    let mut h = prepared(true, 0, false, 1, false);
    h.op(0, play(0));
    let a = h.op(
        0,
        Operation::NodePairSnapshot {
            resource: h.imports[1],
        },
    );
    let b = h.op(
        0,
        Operation::NodePairSnapshot {
            resource: h.imports[1],
        },
    );
    h.step(1);
    h.drain();
    let Reply::NodePairSource { id: a, .. } = h.reply(a) else {
        panic!("source a")
    };
    let Reply::NodePairSource { id: b, .. } = h.reply(b) else {
        panic!("source b")
    };
    let one = h.op(1, Operation::NodePairSourceValidate { id: a });
    let other = h.op(2, Operation::NodePairSourceValidate { id: b });
    let again = h.op(3, Operation::NodePairSourceValidate { id: a });
    h.step(3);
    h.drain();
    for (sequence, id, packets) in [(one, a, 1), (other, b, 0), (again, a, 1)] {
        assert_eq!(
            h.reply(sequence),
            Reply::NodePairSourceStatus {
                id,
                status: NodePairSourceStatus::Validating { packets }
            }
        );
    }
    let node_pairs = begin(&mut h, 0, 1);
    let resource = h.imports[0];
    let Reply::NodePairSource { id: audio, .. } =
        call(&mut h, Operation::NodePairSnapshot { resource })
    else {
        panic!("audio")
    };
    let edit = Operation::NodePairAppend {
        id: node_pairs,
        previous: Some(id(1)),
        initial: Some(initial(generation(1, 2), [audio, b], true)),
    };
    assert_eq!(
        call(&mut h, edit),
        Reply::Failed(Failure::Runtime(Error::InvalidState))
    );
    for packets in [Some(2), None] {
        let expected = packets.map_or(NodePairSourceStatus::Ready { max_packet: 3 }, |packets| {
            NodePairSourceStatus::Validating { packets }
        });
        assert_eq!(
            call(&mut h, Operation::NodePairSourceValidate { id: a }),
            Reply::NodePairSourceStatus {
                id: a,
                status: expected
            }
        );
    }
    assert_eq!(
        call(&mut h, Operation::NodePairStatus { id: node_pairs }),
        Reply::NodePairStatus {
            id: node_pairs,
            status: NodePairStatus::Receiving {
                received: 0,
                expected: 1
            }
        }
    );
    for changed in [
        NodePairInitial {
            gain: f32::NAN,
            ..initial(generation(1, 2), [audio, a], true)
        },
        NodePairInitial {
            frames: 3,
            ..initial(generation(1, 2), [audio, a], true)
        },
        NodePairInitial {
            midi: Some(audio),
            ..initial(generation(1, 2), [audio, a], true)
        },
    ] {
        assert_eq!(
            call(
                &mut h,
                Operation::NodePairAppend {
                    id: node_pairs,
                    previous: Some(id(1)),
                    initial: Some(changed)
                }
            ),
            Reply::Failed(Failure::Runtime(Error::InvalidShape))
        );
    }
    assert_eq!(
        call(&mut h, Operation::NodePairSourceStatus { id: audio }),
        Reply::NodePairSourceStatus {
            id: audio,
            status: NodePairSourceStatus::Ready { max_packet: 0 }
        }
    );
    assert_eq!(
        call(
            &mut h,
            Operation::NodePairAppend {
                id: node_pairs,
                previous: Some(id(1)),
                initial: Some(initial(generation(1, 2), [audio, a], true))
            }
        ),
        Reply::NodePairStatus {
            id: node_pairs,
            status: NodePairStatus::Ready
        }
    );
    for source in [audio, a] {
        assert_eq!(
            call(&mut h, Operation::NodePairSourceRelease { id: source }),
            Reply::Failed(Failure::Runtime(Error::InvalidId))
        );
    }
    let mut retained = vec![b];
    for _ in 0..7 {
        let Reply::NodePairSource { id, .. } =
            call(&mut h, Operation::NodePairSnapshot { resource })
        else {
            panic!("capacity source")
        };
        retained.push(id);
    }
    assert_eq!(
        call(&mut h, Operation::NodePairSnapshot { resource }),
        Reply::Failed(Failure::Runtime(Error::Capacity))
    );
    assert_eq!(retained[1].parts()[1], a.parts()[1]);
    assert_eq!(retained[1].parts()[2], a.parts()[2] + 1);
    let at = h.engine.next_frame().0;
    let mut pending = Vec::new();
    for _ in 0..16 {
        pending.push(h.op(
            at,
            Operation::Transition {
                target: id(1),
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 1000,
                },
                transition: Transition::Stop,
            },
        ));
    }
    h.step(1);
    let cancel = h.send(
        Lane::Recovery,
        h.engine.next_frame().0,
        Operation::NodePairCancel { id: node_pairs },
        Box::new([]),
    );
    h.step(1);
    h.step(5);
    h.drain();
    let release = h.send(
        Lane::Recovery,
        h.engine.next_frame().0,
        Operation::NodePairSourceRelease { id: b },
        Box::new([]),
    );
    h.step(1);
    h.step(10);
    h.drain();
    assert_eq!(
        h.reply(cancel),
        Reply::NodePairStatus {
            id: node_pairs,
            status: NodePairStatus::Idle
        }
    );
    assert_eq!(h.reply(release), Reply::NodePairSourceReleased { id: b });
    h.send(
        Lane::Recovery,
        h.engine.next_frame().0,
        Operation::Cancel {
            sequence: pending[0],
        },
        Box::new([]),
    );
    h.step(1);
    h.drain();
    assert_eq!(
        call(&mut h, Operation::NodePairSourceStatus { id: retained[1] }),
        Reply::NodePairSourceStatus {
            id: retained[1],
            status: NodePairSourceStatus::Ready { max_packet: 0 }
        }
    );
    assert_eq!(
        call(&mut h, Operation::NodePairSourceRelease { id: a }),
        Reply::Failed(Failure::Runtime(Error::InvalidId))
    );
    let next = begin(&mut h, 0, 1);
    assert_eq!(next.parts()[2], node_pairs.parts()[2] + 1);
    assert_eq!(
        call(&mut h, Operation::NodePairCancel { id: node_pairs }),
        Reply::Failed(Failure::Runtime(Error::InvalidId))
    );
    let end = h.engine.next_frame().0;
    assert!(end < 128);
    let log = h.finish().unwrap();
    for frame in 0..end as usize {
        let clock = log.states[frame][0].unwrap();
        assert_eq!(
            (clock.mode, clock.position, clock.cycles, clock.last_pulse),
            (Mode::Playing, 0, frame as u64, Some(Frame(frame as u64)))
        );
    }
}
