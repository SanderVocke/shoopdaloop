use tracy_nextest_capture::tracy_capture_test;

fn emit_shoop_marker(marker: &str) {
    let Some(client) = tracy_client::Client::running() else {
        return;
    };
    shoop_tracing::set_tracing_enabled(true);
    let span = shoop_tracing::realtime_span!("shoop.nextest_capture.smoke.zone");
    assert!(span.entered_tracy());
    let tracing_span = tracing::info_span!("shoop.nextest_capture.smoke.tracing_span");
    let _entered = tracing_span.enter();
    tracing::info!(message = "shoop.nextest_capture.smoke.tracing_event");
    log::info!("shoop.nextest_capture.smoke.log_event");
    client.message(marker, 0);
    drop(span);
    shoop_tracing::set_tracing_enabled(false);
}

#[tracy_capture_test]
fn passing_attempt_is_discarded() {
    emit_shoop_marker("shoop.nextest_capture.smoke.pass");
}

#[tracy_capture_test]
#[ignore = "intentional failure used by the Tracy nextest capture canary"]
fn intentional_failure_publishes_trace() {
    emit_shoop_marker("shoop.nextest_capture.smoke.failure");
    panic!("intentional Tracy nextest capture smoke failure");
}
