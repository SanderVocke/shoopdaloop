#![cfg(not(target_arch = "wasm32"))]

//! Verifies that tracing-enabled realtime cycles keep allocation permission inside the
//! direct Tracy helpers. Ordinary engine work is still enclosed by the global guard.

use assert_no_alloc::assert_no_alloc;
#[cfg(debug_assertions)]
use assert_no_alloc::AllocDisabler;
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::dummy_port::{DummyAudioPort, PortId};
use shoop_engine::port::PortDirection;
use shoop_engine::session::{Port, Session};

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

fn audio_port(id: u64, name: &str, direction: PortDirection) -> Port {
    Port::Dummy(DummyAudioPort::new(PortId(id), name, direction, 64))
}

#[test]
fn coarse_and_detailed_tracy_keep_the_engine_guarded() {
    let mut session = Session::default();
    session.set_buffer_size(64);
    let input = session.add_port(audio_port(1, "input", PortDirection::Input));
    let output = session.add_port(audio_port(2, "output", PortDirection::Output));
    let loop_idx = session.create_loop();
    let channel = session
        .add_audio_channel(loop_idx, 64, ChannelMode::Direct)
        .expect("audio channel");
    session
        .connect_channel_input(channel, input)
        .expect("channel input");
    session
        .connect_channel_output(channel, output)
        .expect("channel output");
    session.apply_graph_changes().expect("graph schedule");

    let (mut engine, mut handle) = shoop_engine::engine::split(session, 8);
    engine.run_cycle(64);

    let _client = tracy_client::Client::start();
    shoop_tracing::set_tracing_enabled(true);
    shoop_tracing::set_tracing_output_enabled(true);

    shoop_tracing::set_engine_detail_enabled(false);
    assert_no_alloc(|| engine.run_cycle(64));

    shoop_tracing::set_engine_detail_enabled(true);
    assert_no_alloc(|| engine.run_cycle(64));

    let (_, report_rx) = handle
        .send_for_result(|session| session.profiling_report())
        .expect("queue profiling report");
    engine.pump();
    let report = shoop_engine::engine::wait_for_result(
        report_rx,
        shoop_engine::engine::DEFAULT_WAIT_TIMEOUT,
    )
    .expect("profiling report");
    assert!(report.items.iter().any(|item| item.n_samples > 0.0));

    shoop_tracing::set_engine_detail_enabled(false);
    shoop_tracing::set_tracing_enabled(false);
}
