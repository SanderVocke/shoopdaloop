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
        Operation::RoleSnapshot {
            resource: h.imports[1],
        },
    );
    let b = h.op(
        0,
        Operation::RoleSnapshot {
            resource: h.imports[1],
        },
    );
    h.step(1);
    h.drain();
    let Reply::RoleSource { id: a, .. } = h.reply(a) else {
        panic!("source a")
    };
    let Reply::RoleSource { id: b, .. } = h.reply(b) else {
        panic!("source b")
    };
    let one = h.op(1, Operation::RoleSourceValidate { id: a });
    let other = h.op(2, Operation::RoleSourceValidate { id: b });
    let again = h.op(3, Operation::RoleSourceValidate { id: a });
    h.step(3);
    h.drain();
    for (sequence, id, packets) in [(one, a, 1), (other, b, 0), (again, a, 1)] {
        assert_eq!(
            h.reply(sequence),
            Reply::RoleSourceStatus {
                id,
                status: RoleSourceStatus::Validating { packets }
            }
        );
    }
    let roles = begin(&mut h, 0, 1);
    let resource = h.imports[0];
    let Reply::RoleSource { id: audio, .. } = call(&mut h, Operation::RoleSnapshot { resource })
    else {
        panic!("audio")
    };
    let edit = Operation::RoleAppend {
        id: roles,
        previous: Some(id(1)),
        initial: Some(initial(generation(1, 2), [audio, b], true)),
    };
    assert_eq!(
        call(&mut h, edit),
        Reply::Failed(Failure::Runtime(Error::InvalidState))
    );
    for packets in [Some(2), None] {
        let expected = packets.map_or(RoleSourceStatus::Ready { max_packet: 3 }, |packets| {
            RoleSourceStatus::Validating { packets }
        });
        assert_eq!(
            call(&mut h, Operation::RoleSourceValidate { id: a }),
            Reply::RoleSourceStatus {
                id: a,
                status: expected
            }
        );
    }
    assert_eq!(
        call(&mut h, Operation::RoleStatus { id: roles }),
        Reply::RoleStatus {
            id: roles,
            status: RoleStatus::Receiving {
                received: 0,
                expected: 1
            }
        }
    );
    for changed in [
        RoleInitial {
            gain: f32::NAN,
            ..initial(generation(1, 2), [audio, a], true)
        },
        RoleInitial {
            frames: 3,
            ..initial(generation(1, 2), [audio, a], true)
        },
        RoleInitial {
            midi: Some(audio),
            ..initial(generation(1, 2), [audio, a], true)
        },
    ] {
        assert_eq!(
            call(
                &mut h,
                Operation::RoleAppend {
                    id: roles,
                    previous: Some(id(1)),
                    initial: Some(changed)
                }
            ),
            Reply::Failed(Failure::Runtime(Error::InvalidShape))
        );
    }
    assert_eq!(
        call(&mut h, Operation::RoleSourceStatus { id: audio }),
        Reply::RoleSourceStatus {
            id: audio,
            status: RoleSourceStatus::Ready { max_packet: 0 }
        }
    );
    assert_eq!(
        call(
            &mut h,
            Operation::RoleAppend {
                id: roles,
                previous: Some(id(1)),
                initial: Some(initial(generation(1, 2), [audio, a], true))
            }
        ),
        Reply::RoleStatus {
            id: roles,
            status: RoleStatus::Ready
        }
    );
    for source in [audio, a] {
        assert_eq!(
            call(&mut h, Operation::RoleSourceRelease { id: source }),
            Reply::Failed(Failure::Runtime(Error::InvalidId))
        );
    }
    let mut retained = vec![b];
    for _ in 0..7 {
        let Reply::RoleSource { id, .. } = call(&mut h, Operation::RoleSnapshot { resource })
        else {
            panic!("capacity source")
        };
        retained.push(id);
    }
    assert_eq!(
        call(&mut h, Operation::RoleSnapshot { resource }),
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
        Operation::RoleCancel { id: roles },
        Box::new([]),
    );
    h.step(1);
    h.step(5);
    h.drain();
    let release = h.send(
        Lane::Recovery,
        h.engine.next_frame().0,
        Operation::RoleSourceRelease { id: b },
        Box::new([]),
    );
    h.step(1);
    h.step(10);
    h.drain();
    assert_eq!(
        h.reply(cancel),
        Reply::RoleStatus {
            id: roles,
            status: RoleStatus::Idle
        }
    );
    assert_eq!(h.reply(release), Reply::RoleSourceReleased { id: b });
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
        call(&mut h, Operation::RoleSourceStatus { id: retained[1] }),
        Reply::RoleSourceStatus {
            id: retained[1],
            status: RoleSourceStatus::Ready { max_packet: 0 }
        }
    );
    assert_eq!(
        call(&mut h, Operation::RoleSourceRelease { id: a }),
        Reply::Failed(Failure::Runtime(Error::InvalidId))
    );
    let next = begin(&mut h, 0, 1);
    assert_eq!(next.parts()[2], roles.parts()[2] + 1);
    assert_eq!(
        call(&mut h, Operation::RoleCancel { id: roles }),
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
