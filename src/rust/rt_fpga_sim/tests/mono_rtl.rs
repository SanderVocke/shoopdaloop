#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_adapter::looping::{LoopAdapter, Request};
use rt_fpga_sim::{
    Simulator,
    mono::{self, Native},
    runtime,
};
use rt_runtime::{
    Epoch, Frame,
    graph::{self, Catalog},
    transport::Lane,
};
use shoop_loops::{
    Timing, Transition,
    runtime::{Cut, Operation, Reply, Selection},
};
use shoop_wasm_test_support::shoop_test;
fn cut(a: Cut<rt_runtime::Q824>, b: Cut<rt_runtime::Q824>) {
    assert_eq!(a.state, b.state);
    assert_eq!(
        (a.frame, a.graph_epoch, a.topology_epoch),
        (b.frame, b.graph_epoch, b.topology_epoch)
    );
    assert_eq!(
        (
            a.raw_audio_frames,
            a.raw_midi_frames,
            a.windows,
            a.timing_revision
        ),
        (
            b.raw_audio_frames,
            b.raw_midi_frames,
            b.windows,
            b.timing_revision
        )
    );
    assert_eq!(
        (
            a.playback.is_some(),
            a.latest.is_some(),
            a.gain,
            a.balance,
            a.smoothing,
            a.capture_failure,
            a.midi_overflows
        ),
        (
            b.playback.is_some(),
            b.latest.is_some(),
            b.gain,
            b.balance,
            b.smoothing,
            b.capture_failure,
            b.midi_overflows
        )
    );
}
#[shoop_test]
fn queued_mono_loop_matches_native_at_settled_queries_and_after_each_quantum() {
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    let rt = runtime(&cache).unwrap();
    let sim = Simulator::new(&rt, None, true).unwrap();
    let p = mono::prepare().unwrap();
    let plan = graph::compile(
        &Catalog::new(
            mono::config(),
            &p.nodes.iter().map(|n| n.spec()).collect::<Vec<_>>(),
        )
        .unwrap(),
        &mono::graph(),
    )
    .unwrap();
    let (mut control, mut hardware) =
        LoopAdapter::prepare(mono::config(), p.startup, p.nodes, sim).unwrap();
    control
        .submit(Lane::Normal, Request::commit(Epoch(0), plan))
        .map_err(|e| e.error)
        .unwrap();
    let mut native = Native::new().unwrap();
    for q in 0..12u64 {
        let transition = match q {
            0 | 5 => Some(Transition::Record {
                monitor_previous: false,
            }),
            2 | 8 => Some(Transition::Stop),
            3 | 9 => Some(Transition::Play {
                position: 0,
                retrigger: false,
            }),
            _ => None,
        };
        let mut operations = vec![];
        if let Some(transition) = transition {
            operations.push(Operation::Transition {
                target: mono::loop_id(),
                timing: Timing::At,
                transition,
            });
        }
        operations.push(Operation::State {
            target: mono::loop_id(),
        });
        if q == 2 || q == 8 {
            operations.push(Operation::Export {
                target: mono::loop_id(),
                selection: Selection::Latest,
            });
        }
        for op in operations {
            native.send(op, q * 32).unwrap();
            control
                .submit(Lane::Normal, native.client.command(Frame(q * 32), op))
                .map_err(|e| e.error)
                .unwrap();
        }
        let input = (0..32)
            .map(|i| {
                if i % 2 == 0 {
                    1_000_003 + q as i32 * 139 + i * 17
                } else {
                    -1_000_003 - q as i32 * 139 - i * 17
                }
            })
            .collect::<Vec<_>>();
        let expected = native.step(&input).unwrap();
        let actual = hardware
            .process(&[input])
            .unwrap_or_else(|e| panic!("q{q} {e}: {:?}", hardware.device().last_error));
        assert_eq!(actual[0], expected, "audio q{q}");
        let report = hardware.memory_report().unwrap();
        assert_eq!(report.frame, (q + 1) * 32);
        assert!(report.valid());
        assert_eq!(report.accounted_bytes, None);
        let sample = report.resources[0].unwrap();
        assert_eq!(sample.capacity, Some(128 * 32 * 4));
        if q <= 2 {
            assert_eq!(sample.used, (q.min(1) + 1) * 32 * 4);
        }
        assert_eq!(hardware.memory_report().unwrap(), report);
        assert_eq!(hardware.device().frame(), report.frame);
        assert_eq!(
            hardware.state(),
            native
                .runtime
                .planner()
                .resolver()
                .state(mono::loop_id())
                .unwrap(),
            "state q{q}"
        );
        assert_eq!(
            hardware.events(),
            native.runtime.planner().resolver().events(),
            "events q{q}"
        );
        let ca = std::iter::from_fn(|| native.control.try_completion()).collect::<Vec<_>>();
        let cb = std::iter::from_fn(|| control.try_completion()).collect::<Vec<_>>();
        assert_eq!(ca.len(), cb.len());
        for (a, b) in ca.iter().zip(&cb) {
            assert_eq!(
                (a.sequence, a.outcome, a.frame, a.epoch),
                (b.sequence, b.outcome, b.frame, b.epoch),
                "completion q{q}"
            );
        }
        let a = std::iter::from_fn(|| native.control.try_execution()).collect::<Vec<_>>();
        let b = std::iter::from_fn(|| control.try_execution()).collect::<Vec<_>>();
        assert_eq!(a.len(), b.len());
        for (a, b) in a.into_iter().zip(b) {
            assert_eq!(
                (
                    a.sequence,
                    a.requested_frame,
                    a.delivered_frame,
                    a.actual_frame,
                    a.result,
                    a.processor_result
                ),
                (
                    b.sequence,
                    b.requested_frame,
                    b.delivered_frame,
                    b.actual_frame,
                    b.result,
                    b.processor_result
                ),
                "receipt q{q}"
            );
            match (a.operation_result.unwrap(), b.operation_result.unwrap()) {
                (Reply::Transition(a), Reply::Transition(b)) => assert_eq!(a, b),
                (Reply::State(a), Reply::State(b)) => cut(a, b),
                (
                    Reply::Export {
                        cut: a, media: ma, ..
                    },
                    Reply::Export {
                        cut: b, media: mb, ..
                    },
                ) => {
                    cut(a, b);
                    assert_eq!(ma.view.description, mb.view.description);
                    let sa = ma.view.audio.unwrap().snapshot;
                    let sb = mb.view.audio.unwrap().snapshot;
                    assert_eq!(
                        (sa.revision, sa.description, sa.cut_frame, sa.capture),
                        (sb.revision, sb.description, sb.cut_frame, sb.capture)
                    );
                    assert_eq!(ma.view.windows, mb.view.windows);
                }
                replies => panic!("reply q{q} {replies:?}"),
            }
        }
    }
    assert_eq!(hardware.metrics.compilations, 1);
}

#[shoop_test]
fn actual_rtl_fault_and_arena_exhaustion_surface_without_losing_future_receipts() {
    for exhaust in [false, true] {
        let cache =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
        let rt = runtime(&cache).unwrap();
        let sim = Simulator::new(&rt, None, true).unwrap();
        let mut p = mono::prepare().unwrap();
        if exhaust {
            p.startup.max_chunks = 1;
        }
        let plan = graph::compile(
            &Catalog::new(
                mono::config(),
                &p.nodes.iter().map(|n| n.spec()).collect::<Vec<_>>(),
            )
            .unwrap(),
            &mono::graph(),
        )
        .unwrap();
        let (mut c, mut a) = LoopAdapter::prepare(mono::config(), p.startup, p.nodes, sim).unwrap();
        c.submit(Lane::Normal, Request::commit(Epoch(0), plan))
            .map_err(|e| e.error)
            .unwrap();
        a.service().unwrap();
        assert_eq!(
            c.try_completion().unwrap().outcome,
            rt_runtime::transport::Outcome::Applied
        );
        let planner = rt_runtime::PlannerId::from_parts(0, 1).unwrap();
        if exhaust {
            c.submit(
                Lane::Normal,
                Request::Schedule {
                    at: Frame(0),
                    action: rt_runtime::schedule::Action::Planner {
                        planner,
                        operation: Operation::Transition {
                            target: mono::loop_id(),
                            timing: Timing::At,
                            transition: Transition::Record {
                                monitor_previous: false,
                            },
                        },
                    },
                    payload: Box::new([]),
                },
            )
            .map_err(|e| e.error)
            .unwrap();
            a.process(&[vec![12345; 32]]).unwrap();
            while c.try_execution().is_some() {}
            while c.try_completion().is_some() {}
        } else {
            a.device_mut()
                .state(0, Some([255, 0, 0, 0, 0]), true)
                .unwrap();
        }
        c.submit(
            Lane::Normal,
            Request::Schedule {
                at: Frame(96),
                action: rt_runtime::schedule::Action::Planner {
                    planner,
                    operation: Operation::State {
                        target: mono::loop_id(),
                    },
                },
                payload: Box::new([]),
            },
        )
        .map_err(|e| e.error)
        .unwrap();
        let error = if exhaust {
            rt_runtime::Error::Capacity
        } else {
            rt_runtime::Error::InvalidState
        };
        assert_eq!(a.process(&[vec![12345; 32]]), Err(error));
        assert_eq!(a.fault(), Some(error));
        assert_eq!(a.memory_report(), Err(error));
        assert!(
            matches!(c.try_execution().unwrap().operation_result,Some(Reply::Failed(failure)) if failure==shoop_loops::runtime::Failure::Runtime(error))
        );
        assert!(
            a.device()
                .last_error
                .as_ref()
                .unwrap()
                .contains("pipeline fault stage")
        );
    }
}
