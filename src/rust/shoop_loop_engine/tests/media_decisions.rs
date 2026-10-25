#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use shoop_loop_engine::*;
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn primitive(slot: u32, length: u64, sync: Option<u32>) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind: LoopKind::Primitive,
        length,
        sync_source: sync.map(id),
    }
}
fn prepare(
    loops: Vec<LoopSpec>,
    journal_events: usize,
    programs: Vec<ProgramDescription>,
) -> Resolver {
    Resolver::prepare(
        compile(
            Limits {
                journal_events,
                ..Limits::default()
            },
            &TopologyDescription {
                loops,
                triggers: vec![],
                programs,
            },
        )
        .unwrap(),
    )
    .unwrap()
}
fn request(target: u32, sequence: u64, at: u64, transition: Transition) -> Request {
    Request {
        target: id(target),
        sequence: Sequence(sequence),
        at: Frame(at),
        timing: Timing::At,
        transition,
    }
}
fn play(position: u64, retrigger: bool) -> Transition {
    Transition::Play {
        position,
        retrigger,
    }
}
fn decision(resolver: &Resolver, target: u32) -> Decision {
    *resolver
        .decisions()
        .unwrap()
        .iter()
        .find(|d| d.state.id == id(target))
        .unwrap()
}
fn step(resolver: &mut Resolver, end: u64) {
    resolver.preview(Frame(end), &[]).unwrap();
    resolver.commit().unwrap();
    resolver.finish(Frame(end)).unwrap();
}
#[shoop_test]
fn primitive_media_flags_cover_noops_monitoring_failures_and_same_state_seeks() {
    let script = [
        (0, play(0, false)),
        (1, play(3, false)),
        (
            2,
            Transition::Seek {
                position: 2,
                retrigger: false,
            },
        ),
        (
            3,
            Transition::Replace {
                length: 4,
                revision: Revision(0),
                preserve_position: true,
            },
        ),
        (5, play(1, true)),
        (
            6,
            Transition::Record {
                monitor_previous: true,
            },
        ),
        (
            7,
            Transition::Record {
                monitor_previous: true,
            },
        ),
        (10, Transition::Stop),
        (
            11,
            Transition::Record {
                monitor_previous: true,
            },
        ),
        (
            12,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            13,
            Transition::Record {
                monitor_previous: true,
            },
        ),
        (14, Transition::Stop),
        (15, play(0, false)),
        (16, play(0, false)),
        (
            17,
            Transition::Seek {
                position: 0,
                retrigger: false,
            },
        ),
        (
            18,
            Transition::Replace {
                length: 0,
                revision: Revision(9),
                preserve_position: false,
            },
        ),
        (19, play(0, false)),
    ];
    let expected = [
        true, false, true, true, true, true, false, false, true, false, false, true, false, true,
        false, true, true, true, false, false,
    ];
    for journal in [0, 1, 1024] {
        let mut resolver = prepare(vec![primitive(0, 4, None)], journal, vec![]);
        assert_no_heap(|| {
            for (sequence, (frame, transition)) in script.into_iter().enumerate() {
                resolver
                    .submit(request(0, sequence as u64 + 1, frame, transition))
                    .unwrap();
            }
            for (frame, expected) in expected.into_iter().enumerate() {
                let before = resolver.state(id(0)).unwrap();
                let observations = [Observation::CaptureExhausted { target: id(0) }];
                let observations = if frame == 9 { &observations[..] } else { &[] };
                resolver
                    .preview(Frame(frame as u64 + 1), observations)
                    .unwrap();
                let first = decision(&resolver, 0);
                assert_eq!(
                    first.playback_discontinuity, expected,
                    "frame {frame}, journal {journal}"
                );
                if frame == 2 || frame == 3 {
                    assert_eq!(first.state, before);
                }
                resolver
                    .preview(Frame(frame as u64 + 1), observations)
                    .unwrap();
                assert_eq!(decision(&resolver, 0), first);
                resolver.commit().unwrap();
                assert_eq!(decision(&resolver, 0), first);
                if frame == 2 || frame == 3 {
                    assert!(resolver.events().is_empty());
                }
                resolver.finish(Frame(frame as u64 + 1)).unwrap();
            }
        });
    }
}
#[shoop_test]
fn cancelled_losing_and_refused_previews_do_not_leak_discontinuities() {
    let mut resolver = prepare(vec![primitive(0, 4, None)], 0, vec![]);
    assert_no_heap(|| {
        resolver.submit(request(0, 1, 0, play(0, false))).unwrap();
        step(&mut resolver, 1);
        let loser = resolver
            .submit(request(
                0,
                2,
                1,
                Transition::Seek {
                    position: 1,
                    retrigger: false,
                },
            ))
            .unwrap();
        let winner = resolver.submit(request(0, 3, 1, play(0, false))).unwrap();
        resolver.preview(Frame(2), &[]).unwrap();
        assert!(!decision(&resolver, 0).playback_discontinuity);
        resolver.commit().unwrap();
        resolver.finish(Frame(2)).unwrap();
        assert_eq!(
            resolver.completion(loser).unwrap().unwrap().outcome,
            Outcome::Superseded { frame: Frame(1) }
        );
        assert_eq!(
            resolver.completion(winner).unwrap().unwrap().outcome,
            Outcome::Applied { frame: Frame(1) }
        );
        let cancelled = resolver
            .submit(request(
                0,
                4,
                2,
                Transition::Seek {
                    position: 2,
                    retrigger: false,
                },
            ))
            .unwrap();
        assert_eq!(resolver.cancel(cancelled), Ok(CancelResult::Cancelled));
        resolver.preview(Frame(3), &[]).unwrap();
        assert!(!decision(&resolver, 0).playback_discontinuity);
        resolver.commit().unwrap();
        resolver.finish(Frame(3)).unwrap();
        let refused = resolver
            .submit(request(
                0,
                5,
                3,
                Transition::Seek {
                    position: 0,
                    retrigger: false,
                },
            ))
            .unwrap();
        resolver.preview(Frame(4), &[]).unwrap();
        assert!(decision(&resolver, 0).playback_discontinuity);
        assert_eq!(resolver.state(id(0)).unwrap().position, 3);
        resolver
            .preview(
                Frame(4),
                &[Observation::Refused {
                    request: refused,
                    reason: Error::Capacity,
                }],
            )
            .unwrap();
        assert!(!decision(&resolver, 0).playback_discontinuity);
        resolver.commit().unwrap();
        resolver.finish(Frame(4)).unwrap();
        assert_eq!(
            resolver.completion(refused).unwrap().unwrap().outcome,
            Outcome::Failed {
                frame: Frame(3),
                reason: Error::Capacity
            }
        );
        resolver.preview(Frame(5), &[]).unwrap();
        assert!(decision(&resolver, 0).playback_discontinuity);
        assert_eq!(decision(&resolver, 0).state.position, 0);
    });
}
#[shoop_test]
fn reactive_final_modes_mask_or_restore_the_boundary_flag() {
    for reactive_stop in [true, false] {
        let mut resolver = prepare(
            vec![primitive(0, 2, None), primitive(1, 3, Some(0))],
            0,
            vec![],
        );
        assert_no_heap(|| {
            resolver.submit(request(0, 1, 0, play(0, false))).unwrap();
            resolver.submit(request(1, 2, 0, play(0, false))).unwrap();
            if reactive_stop {
                let mut stop = request(1, 3, 0, Transition::Stop);
                stop.timing = Timing::AfterTicks {
                    source: id(0),
                    skip: 0,
                };
                resolver.submit(stop).unwrap();
                resolver.preview(Frame(1), &[]).unwrap();
                assert!(decision(&resolver, 0).playback_discontinuity);
                assert!(!decision(&resolver, 1).playback_discontinuity);
                assert_eq!(decision(&resolver, 1).state.mode, Mode::Stopped);
            } else {
                resolver.submit(request(1, 3, 2, Transition::Stop)).unwrap();
                let mut restart = request(1, 4, 1, play(1, true));
                restart.timing = Timing::AfterTicks {
                    source: id(0),
                    skip: 0,
                };
                resolver.submit(restart).unwrap();
                step(&mut resolver, 1);
                step(&mut resolver, 2);
                resolver.preview(Frame(3), &[]).unwrap();
                assert!(decision(&resolver, 1).playback_discontinuity);
                assert_eq!(decision(&resolver, 1).state.mode, Mode::Playing);
                assert_eq!(decision(&resolver, 1).state.position, 1);
            }
        });
    }
}
#[shoop_test]
fn nested_program_snapshots_emit_primitive_flags_without_a_journal() {
    let programs = (1..=2)
        .map(|parent| ProgramDescription {
            source: id(parent),
            nominal_unit_frames: 1,
            sections: vec![Section {
                entries: vec![Entry {
                    target: id(parent - 1),
                    delay: 0,
                    duration: Some(1),
                    repetitions: 1,
                    mode: None,
                }],
            }],
        })
        .collect();
    let mut resolver = prepare(
        vec![
            primitive(0, 1, None),
            LoopSpec {
                kind: LoopKind::Composite,
                ..primitive(1, 0, None)
            },
            LoopSpec {
                kind: LoopKind::Composite,
                ..primitive(2, 0, None)
            },
        ],
        0,
        programs,
    );
    assert_no_heap(|| {
        resolver
            .submit(request(
                2,
                1,
                0,
                Transition::PlayIteration {
                    iteration: 0,
                    retrigger: false,
                },
            ))
            .unwrap();
        resolver
            .submit(request(
                2,
                2,
                1,
                Transition::SeekIteration {
                    iteration: 0,
                    retrigger: false,
                },
            ))
            .unwrap();
        for frame in 0..2 {
            resolver.preview(Frame(frame + 1), &[]).unwrap();
            assert!(decision(&resolver, 0).playback_discontinuity);
            assert_eq!(decision(&resolver, 0).state.position, 0);
            assert!(!decision(&resolver, 1).playback_discontinuity);
            assert!(!decision(&resolver, 2).playback_discontinuity);
            resolver.commit().unwrap();
            assert!(resolver.events().is_empty());
            resolver.finish(Frame(frame + 1)).unwrap();
        }
    });
}
