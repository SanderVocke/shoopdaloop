mod support;

use shoop_engine::basic_loop::BasicLoop;
use shoop_engine::loop_mode::LoopMode;
use std::time::Duration;

fn exercise_engine() {
    let mut engine_loop = BasicLoop::default();
    engine_loop.set_mode(LoopMode::Recording);
    engine_loop.update_poi();
    for _ in 0..8 {
        let _span = shoop_tracing::realtime_span!("collector.fixture.engine_cycle");
        engine_loop.process(64);
    }
    assert_eq!(engine_loop.length(), 512);
}

#[test]
fn traced_passes() {
    let _trace = support::startup();
    exercise_engine();
}

#[test]
fn traced_failure() {
    let trace = support::startup();
    exercise_engine();
    if trace.as_ref().is_some_and(support::TraceAttempt::active) {
        panic!("controlled traced failure");
    }
}

#[test]
fn traced_abort() {
    let trace = support::startup();
    exercise_engine();
    if trace.as_ref().is_some_and(support::TraceAttempt::active) {
        std::process::abort();
    }
}

#[test]
fn traced_timeout() {
    let trace = support::startup();
    exercise_engine();
    if trace.as_ref().is_some_and(support::TraceAttempt::active) {
        std::thread::sleep(Duration::from_secs(30));
    }
}
