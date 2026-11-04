use rt_buffered::{Timing, MAX_BATCH_FRAMES, MAX_LATENCY_FRAMES};
use rt_runtime::{fixed::Quantum, Error};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn buffered_timing_validates_units_limits_and_overflow() {
    for q in [1, 16, 32, 64, 128] {
        let quantum = Quantum::new(q).unwrap();
        for (m, s) in [(1, 0), (2, 1), (3, 0), (3, 7)] {
            let timing = Timing {
                batch_quanta: m,
                slack_quanta: s,
            };
            let prepared = timing.prepare(quantum).unwrap();
            assert_eq!(prepared.quantum(), quantum);
            assert_eq!(prepared.timing(), timing);
            assert_eq!(prepared.batch_frames(), m * q);
            assert_eq!(prepared.latency_frames(), (m + s) * q);
            assert_eq!(prepared.slots(), (m + s).div_ceil(m) + 1);
        }
    }
    let q = Quantum::new(1).unwrap();
    assert_eq!(
        Timing {
            batch_quanta: 0,
            slack_quanta: 0
        }
        .prepare(q),
        Err(Error::InvalidConfig)
    );
    for (m, s) in [
        (usize::MAX, 1),
        (MAX_BATCH_FRAMES + 1, 0),
        (1, MAX_LATENCY_FRAMES),
    ] {
        assert_eq!(
            Timing {
                batch_quanta: m,
                slack_quanta: s
            }
            .prepare(q),
            Err(Error::Capacity)
        );
    }
    assert!(Timing {
        batch_quanta: MAX_BATCH_FRAMES,
        slack_quanta: 0
    }
    .prepare(q)
    .is_ok());
    assert!(Timing {
        batch_quanta: 1,
        slack_quanta: MAX_LATENCY_FRAMES - 1
    }
    .prepare(q)
    .is_ok());
    assert_eq!(
        Timing {
            batch_quanta: usize::MAX,
            slack_quanta: 0
        }
        .prepare(Quantum::new(128).unwrap()),
        Err(Error::Capacity)
    );
}
