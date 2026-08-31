fn emit_shoop_marker(marker: &'static str) {
    shoop_tracing::set_tracing_enabled(true);
    let span = shoop_tracing::realtime_span!("shoop.nextest_capture.smoke.zone");
    let tracing_span = tracing::info_span!("shoop.nextest_capture.smoke.tracing_span");
    let _entered = tracing_span.enter();
    tracing::info!(message = "shoop.nextest_capture.smoke.tracing_event");
    log::info!("shoop.nextest_capture.smoke.log_event");
    shoop_tracing::emit_realtime_event(marker);
    shoop_tracing::emit_plot_i64(false, "shoop.test_capture.smoke.plot", 42);
    drop(span);
    shoop_tracing::set_tracing_enabled(false);
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "exercises the native test capture runtime")]
fn passing_attempt_is_discarded() {
    emit_shoop_marker("shoop.nextest_capture.smoke.pass");
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "exercises the native test capture runtime")]
#[ignore = "intentional failure used by the nextest capture canary"]
fn intentional_failure_publishes_trace() {
    emit_shoop_marker("shoop.nextest_capture.smoke.failure");
    panic!("intentional nextest capture smoke failure");
}
