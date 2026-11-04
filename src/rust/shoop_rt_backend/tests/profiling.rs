mod support;

use shoop_backend::{Backend, BackendBusRequest, DirectTrackRequest};
use shoop_rt_backend::RtBackend;
use shoop_rt_protocol::profiling::{Clock, HOST_XRUN};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

std::thread_local! {
    static TIME: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
    static COST: core::cell::Cell<u64> = const { core::cell::Cell::new(10_000) };
}
fn now() -> u64 {
    TIME.with(|time| {
        let at = time.get();
        time.set(at + COST.with(core::cell::Cell::get));
        at
    })
}

#[shoop_test]
fn production_protocol_reports_stats_and_resets_every_graph_generation() {
    for quantum in [1, 16, 32, 64, 128] {
        let mut description = description();
        description.quantum = quantum;
        description.max_frames = 2 * quantum;
        let (mut clock, client) = parts(&description);
        clock.offline.render.set_profiling_clock(Clock {
            now,
            resolution_ns: 10_000,
        });
        let mut backend = RtBackend::connect(client, clock).unwrap();
        backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "Timing track".into(),
                audio_channels: 2,
                midi: true,
                initial_loops: 2,
            })
            .unwrap();
        backend.wait_idle();
        let before = TIME.with(core::cell::Cell::get);
        backend.clock_mut().render(2 * quantum);
        assert_eq!(TIME.with(core::cell::Cell::get), before);
        backend.set_profiling(true).unwrap();
        backend.pump();
        backend.clock_mut().render(2 * quantum);
        let first = backend.poll().unwrap().profiling;
        assert!(first.active && first.available);
        assert!(!backend.mutations_pending_through(backend.mutation_cursor()));
        assert!(first.live.rows.len() >= 8, "{first:?}");
        assert!(first
            .live
            .rows
            .iter()
            .any(|row| row.name.contains("playback")));
        for row in &first.live.rows {
            assert_eq!(row.avg_ns, 10_000.);
            assert_eq!(row.worst_ns, 10_000);
            assert_eq!(row.stddev_ns, 0.);
        }
        COST.with(|cost| cost.set(30_000));
        backend.clock_mut().render(2 * quantum);
        backend.pump();
        backend.set_profiling(false).unwrap();
        backend.pump();
        let before = TIME.with(core::cell::Cell::get);
        backend.clock_mut().render(2 * quantum);
        assert_eq!(TIME.with(core::cell::Cell::get), before);
        let final_view = backend.poll().unwrap().profiling;
        assert!(!final_view.active);
        for row in &final_view.live.rows {
            assert_eq!(row.avg_ns, 20_000.);
            assert_eq!(row.worst_ns, 30_000);
            assert_eq!(row.stddev_ns, 10_000.);
        }
        backend.set_profiling(true).unwrap();
        backend.pump();
        backend.clock_mut().render(2 * quantum);
        backend.pump();
        backend.clock_mut().offline.host.add_xruns(1);
        for _ in 0..5 {
            backend.clock_mut().render(2 * quantum);
            backend.pump();
        }
        let captured = backend.poll().unwrap().profiling;
        let capture = captured.capture.as_ref().expect("automatic xrun capture");
        assert!(capture.complete);
        assert_ne!(capture.trigger & HOST_XRUN, 0);
        assert!(!capture.rows.is_empty());
        backend
            .create_bus(BackendBusRequest {
                name: "New topology".into(),
                channel_count: 2,
                fx: None,
            })
            .unwrap();
        backend.wait_idle();
        backend.clock_mut().render(2 * quantum);
        let rebuilt = backend.poll().unwrap().profiling;
        assert!(rebuilt.generation > captured.generation);
        assert!(rebuilt.capture.is_none());
        assert!(rebuilt.live.rows.iter().all(|row| row.avg_ns == 30_000.));
        COST.with(|cost| cost.set(10_000));
    }
}
