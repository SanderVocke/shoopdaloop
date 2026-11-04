#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use shoop_loops::*;
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn description(q: u64) -> TopologyDescription {
    TopologyDescription {
        loops: (0..4)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: if slot == 3 {
                    LoopKind::Composite
                } else {
                    LoopKind::Primitive
                },
                length: [2 * q, 3 * q, 5 * q, 0][slot as usize],
                sync_source: (slot == 3).then(|| id(0)),
            })
            .collect(),
        programs: vec![ProgramDescription {
            source: id(3),
            nominal_unit_frames: 2 * q,
            sections: vec![Section {
                entries: vec![
                    Entry {
                        target: id(1),
                        delay: 0,
                        duration: Some(1),
                        repetitions: 1,
                        mode: Some(Mode::Playing),
                    },
                    Entry {
                        target: id(2),
                        delay: 1,
                        duration: Some(2),
                        repetitions: 1,
                        mode: Some(Mode::Playing),
                    },
                ],
            }],
        }],
        triggers: vec![],
    }
}
fn submit(r: &mut Resolver, slot: u32, seq: u64, at: u64, transition: Transition) -> RequestId {
    assert_no_heap(|| {
        r.submit(Request {
            target: id(slot),
            sequence: Sequence(seq),
            at: Frame(at),
            timing: Timing::At,
            transition,
        })
    })
    .unwrap()
}
fn prepare(q: usize) -> Resolver {
    let r = Resolver::prepare(
        compile(
            Limits {
                loop_slots: 4,
                program_entries: 8,
                program_segments: 16,
                pending_requests: 8,
                journal_events: 256,
                relations: 8,
            },
            &description(q as u64),
        )
        .unwrap(),
        rt_runtime::Quantum::new(q).unwrap(),
    )
    .unwrap();

    r
}

#[shoop_test]
fn fixed_sync_and_explicit_programs_follow_the_shared_tick_clock() {
    for q in [16, 32, 64, 128] {
        let mut r = prepare(q);
        let requests = [0, 3].map(|slot| {
            submit(
                &mut r,
                slot,
                slot as u64 + 1,
                0,
                Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            )
        });
        for tick in 0..10 {
            let interval = assert_no_heap(|| r.preview_tick(&[])).unwrap();
            assert_eq!(
                (interval.start, interval.end),
                (Frame(tick * q as u64), Frame((tick + 1) * q as u64))
            );
            assert_no_heap(|| r.commit()).unwrap();
            for (slot, active) in [
                (0, true),
                (1, tick < 2),
                (2, (2..6).contains(&tick)),
                (3, tick < 6),
            ] {
                assert_eq!(
                    r.state(id(slot)).unwrap().mode,
                    if active { Mode::Playing } else { Mode::Stopped },
                    "Q={q}, tick={tick}, slot={slot}"
                );
            }
            assert!(r.events().iter().all(|event| event.frame.0 % q as u64 == 0));
            assert_no_heap(|| r.finish()).unwrap();
        }
        for request in requests {
            assert_eq!(
                r.completion(request).unwrap().unwrap().outcome,
                Outcome::Applied { frame: Frame(0) }
            );
        }
    }
}

#[shoop_test]
fn fixed_requests_round_up_preserve_requested_times_and_refuse_unaligned_media() {
    for q in [16, 32, 64, 128] {
        let mut r = prepare(q);
        let request = submit(
            &mut r,
            0,
            1,
            q as u64 + 1,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        assert_eq!(
            r.submit(Request {
                target: id(0),
                sequence: Sequence(2),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Seek {
                    position: 1,
                    retrigger: false
                }
            }),
            Err(Error::InvalidFrame)
        );
        for tick in 0..3 {
            assert_no_heap(|| r.preview_tick(&[])).unwrap();
            assert_no_heap(|| r.commit()).unwrap();
            assert_eq!(
                r.state(id(0)).unwrap().mode,
                if tick < 2 {
                    Mode::Stopped
                } else {
                    Mode::Playing
                }
            );
            assert_no_heap(|| r.finish()).unwrap();
        }
        let completion = r.completion(request).unwrap().unwrap();
        assert_eq!(completion.requested, Frame(q as u64 + 1));
        assert_eq!(completion.effective, Frame(2 * q as u64));
        assert_eq!(
            completion.outcome,
            Outcome::Applied {
                frame: Frame(2 * q as u64)
            }
        );
        let mut invalid = description(q as u64);
        invalid.loops[1].length += 1;
        let candidate = compile(
            Limits {
                loop_slots: 4,
                program_entries: 8,
                program_segments: 16,
                pending_requests: 8,
                journal_events: 256,
                relations: 8,
            },
            &invalid,
        )
        .unwrap();
        let state = r.state(id(0)).unwrap();
        assert_eq!(
            r.replace_topology(r.epoch(), candidate).err().unwrap().0,
            Error::InvalidFrame
        );
        assert_eq!(r.state(id(0)).unwrap(), state);
    }
}
