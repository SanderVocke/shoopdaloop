use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[shoop_test]
fn program_probe_stops_before_old_child_actions_and_never_publishes_shadow_work() {
    let id = |slot| LoopId::from_parts(slot, 1).unwrap();
    let description = TopologyDescription {
        loops: (0..4)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: if slot == 1 {
                    LoopKind::Composite
                } else {
                    LoopKind::Primitive
                },
                length: if slot == 0 { 1 } else { 8 },
                sync_source: (slot == 1).then(|| id(0)),
            })
            .collect(),
        programs: vec![ProgramDescription {
            source: id(1),
            nominal_unit_frames: 1,
            sections: vec![Section {
                entries: vec![
                    Entry {
                        target: id(2),
                        delay: 0,
                        duration: Some(1),
                        repetitions: 1,
                        mode: Some(Mode::Recording),
                    },
                    Entry {
                        target: id(3),
                        delay: 1,
                        duration: Some(1),
                        repetitions: 1,
                        mode: Some(Mode::Playing),
                    },
                ],
            }],
        }],
        triggers: vec![Trigger {
            source: id(1),
            target: id(3),
            mode: Mode::Playing,
        }],
    };
    let mut resolver =
        Resolver::prepare(compile(Limits::default(), &description).unwrap()).unwrap();
    for slot in 0..2 {
        assert_no_heap(|| {
            resolver.submit(Request {
                target: id(slot),
                sequence: Sequence(slot as u64 + 1),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            })
        })
        .unwrap();
    }
    for _ in 0..8 {
        let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
        assert!(probe.enters_zero());
        assert_eq!(probe.state.mode, Mode::Playing);
        assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Stopped);
        assert_eq!(resolver.scratch.states[2].unwrap().mode, Mode::Stopped);
        assert!(resolver.scratch.intents[2].is_none());
        assert!(resolver.scratch.intents[3].is_none());
        assert!(resolver
            .scratch
            .records
            .iter()
            .flatten()
            .all(|r| r.terminal.is_some()));
        assert!(resolver
            .records
            .iter()
            .flatten()
            .all(|r| r.terminal.is_none()));
        assert!(resolver.events().is_empty());
        assert_eq!(
            assert_no_heap(|| resolver.commit()),
            Err(Error::InvalidState)
        );
    }
    let interval = assert_no_heap(|| resolver.preview(Frame(1), &[])).unwrap();
    assert_no_heap(|| resolver.commit()).unwrap();
    assert_eq!(resolver.state(id(2)).unwrap().mode, Mode::Recording);
    assert_no_heap(|| resolver.finish(interval.end)).unwrap();
    let before = resolver.state(id(2)).unwrap();
    let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
    assert_eq!(probe.state.composite.unwrap().iteration, 1);
    assert!(!probe.enters_zero());
    assert_eq!(resolver.scratch.states[2], Some(before));
    assert!(resolver.scratch.intents[2].is_none());
    assert!(resolver.scratch.intents[3].is_none());
    let interval = assert_no_heap(|| resolver.preview(Frame(2), &[])).unwrap();
    assert_no_heap(|| resolver.commit()).unwrap();
    assert_eq!(resolver.state(id(2)).unwrap().mode, Mode::Stopped);
    assert_eq!(resolver.state(id(3)).unwrap().mode, Mode::Playing);
    assert_no_heap(|| resolver.finish(interval.end)).unwrap();
    let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
    assert_eq!(probe.state.mode, Mode::Stopped);
    assert_eq!(probe.state.composite.unwrap().passes, 1);
    assert_eq!(resolver.scratch.states[3].unwrap().mode, Mode::Playing);
    assert!(resolver.scratch.intents[3].is_none());
    assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Playing);
}
