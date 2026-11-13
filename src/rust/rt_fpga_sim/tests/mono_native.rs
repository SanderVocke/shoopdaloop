#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_sim::mono::{Native, loop_id};
use shoop_loops::{
    Timing, Transition,
    runtime::{Operation, Reply, Selection},
};
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn native_queries_settle_before_capture_and_exports_keep_the_original_capture_cut() {
    let mut native = Native::new().unwrap();
    let mut exported = None;
    for q in 0..8u64 {
        let transition = match q {
            0 | 5 => Some(Transition::Record {
                monitor_previous: false,
            }),
            2 | 7 => Some(Transition::Stop),
            3 => Some(Transition::Play {
                position: 0,
                retrigger: false,
            }),
            _ => None,
        };
        if let Some(transition) = transition {
            native
                .send(
                    Operation::Transition {
                        target: loop_id(),
                        timing: Timing::At,
                        transition,
                    },
                    q * 32,
                )
                .unwrap();
        }
        native
            .send(Operation::State { target: loop_id() }, q * 32)
            .unwrap();
        if q == 2 {
            native
                .send(
                    Operation::Export {
                        target: loop_id(),
                        selection: Selection::Latest,
                    },
                    q * 32,
                )
                .unwrap();
        }
        let input = vec![1_000_003 + q as i32; 32];
        native.step(&input).unwrap();
        while let Some(c) = native.control.try_completion() {
            assert!(!matches!(
                c.outcome,
                rt_runtime::transport::Outcome::Rejected(_)
            ));
        }
        while let Some(e) = native.control.try_execution() {
            println!(
                "q{q} frame {:?} delivered {:?} reply {:?}",
                e.actual_frame, e.delivered_frame, e.operation_result
            );
            match e.operation_result.unwrap() {
                Reply::State(cut) => {
                    assert_eq!(cut.frame.0, q * 32);
                    if q == 0 {
                        assert_eq!(cut.state.capture_frames, 0);
                    }
                    if q == 1 {
                        assert_eq!(cut.state.capture_frames, 32);
                    }
                }
                Reply::Export { id, media, .. } => {
                    assert_eq!(media.view.description.frames, 64);
                    exported = Some(id);
                }
                Reply::Transition(_) => {}
                reply => panic!("unexpected {reply:?}"),
            }
        }
    }
    native
        .send(
            Operation::Cursor {
                export: exported.unwrap(),
                midi: false,
            },
            256,
        )
        .unwrap();
    native.step(&vec![0; 32]).unwrap();
    let cursor = loop {
        let e = native.control.try_execution().unwrap();
        if let Some(Reply::Cursor(c)) = e.operation_result {
            break c;
        }
    };
    native
        .control
        .submit(
            rt_runtime::transport::Lane::Normal,
            rt_fpga_adapter::looping::Request::resource(rt_runtime::resource::Request::Read {
                cursor,
                bytes: vec![0; 128].into_boxed_slice(),
            }),
        )
        .map_err(|e| e.error)
        .unwrap();
    native.step(&vec![0; 32]).unwrap();
    while let Some(c) = native.control.try_completion() {
        if let rt_fpga_adapter::looping::Request::Resource(r) = c.request {
            println!("read result {:?}", r.reply());
            let rt_runtime::resource::Request::Read { bytes, .. } = r.into_request() else {
                panic!()
            };
            assert_eq!(
                i32::from_le_bytes(bytes[..4].try_into().unwrap()),
                1_000_003
            );
        }
    }
}

#[shoop_test]
fn native_empty_play_and_future_domain_cancellation_distinguish_delivery_from_completion() {
    let mut n = Native::new().unwrap();
    n.send(
        Operation::Transition {
            target: loop_id(),
            timing: Timing::At,
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        },
        0,
    )
    .unwrap();
    n.step(&vec![0; 32]).unwrap();
    let reply = n.control.try_execution().unwrap().operation_result.unwrap();
    assert!(matches!(
        reply,
        Reply::Transition(shoop_loops::Completion {
            outcome: shoop_loops::Outcome::Failed {
                reason: shoop_loops::Error::EmptyContent,
                ..
            },
            ..
        })
    ));
    let future = n
        .send(
            Operation::Transition {
                target: loop_id(),
                timing: Timing::At,
                transition: Transition::Record {
                    monitor_previous: false,
                },
            },
            96,
        )
        .unwrap();
    n.send(Operation::Cancel { sequence: future }, 32).unwrap();
    n.step(&vec![0; 32]).unwrap();
    let receipts = std::iter::from_fn(|| n.control.try_execution()).collect::<Vec<_>>();
    let cancelled = receipts.iter().find(|r| r.sequence == future).unwrap();
    assert_eq!(cancelled.delivered_frame, None);
    assert_eq!(cancelled.actual_frame.0, 32);
    assert!(matches!(
        cancelled.operation_result,
        Some(Reply::Cancelled {
            delivered: false,
            ..
        })
    ));
    assert_eq!(
        n.runtime
            .planner()
            .resolver()
            .state(loop_id())
            .unwrap()
            .mode,
        shoop_loops::Mode::Stopped
    );
}
