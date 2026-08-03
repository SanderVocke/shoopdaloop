use assert_no_alloc::assert_no_alloc;
#[cfg(debug_assertions)]
use assert_no_alloc::AllocDisabler;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn disabled_helper_does_not_allocate() {
    shoop_tracing::set_tracing_enabled(false);
    shoop_tracing::set_tracing_output_enabled(true);
    assert!(!shoop_tracing::is_tracing_requested());
    assert_no_alloc(|| {
        let span = shoop_tracing::realtime_span!("engine.rt.no_alloc_disabled");
        let detail = shoop_tracing::realtime_span_detail!("engine.rt.no_alloc_detail_disabled");
        shoop_tracing::realtime_frame_mark!("engine.no_alloc_disabled");
        assert!(!span.entered_tracy());
        assert!(!detail.entered_tracy());
        drop(detail);
        drop(span);
    });
}

#[test]
fn enabled_helper_scopes_its_allocation_exception() {
    let _client = tracy_client::Client::start();
    shoop_tracing::set_tracing_enabled(true);
    shoop_tracing::set_tracing_output_enabled(true);
    assert!(shoop_tracing::is_tracing_requested());
    assert_no_alloc(|| {
        let span = shoop_tracing::realtime_span!("engine.rt.allocation_exception_test", value = 64);
        assert!(span.entered_tracy());
        drop(span);
    });
    shoop_tracing::set_tracing_enabled(false);
}
