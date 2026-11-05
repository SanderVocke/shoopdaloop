//! The dummy driver: real-time and requested rendering, stopping, and a protocol
//! client working against the engine while the driver thread renders.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use shoop_rt_host_native::{
    start, DriverConfig, DummyConfig, DummyMode, NativeClient, NativeConfig, RenderPhase,
    StartError, Started,
};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn config(dummy: DummyConfig) -> NativeConfig {
    NativeConfig {
        driver: DriverConfig::Dummy(dummy),
        engine: description(),
    }
}

fn controlled() -> DummyConfig {
    DummyConfig {
        buffer_frames: 32,
        mode: DummyMode::Controlled,
        ..DummyConfig::default()
    }
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn automatic_mode_renders_buffers_in_real_time_until_stopped() {
    let Started { host, client } = start(&config(DummyConfig {
        sample_rate: 48_000,
        buffer_frames: 48,
        ..DummyConfig::default()
    }))
    .unwrap();
    let begun = Instant::now();
    while host.status().next_frame < 2_400 {
        assert!(
            begun.elapsed() < Duration::from_secs(5),
            "the driver does not render"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let status = loop {
        let status = host.status();
        if status.stats.callbacks == status.next_frame / 48 {
            break status;
        }
        assert!(
            begun.elapsed() < Duration::from_secs(5),
            "a completed callback was not observed"
        );
        std::thread::yield_now();
    };
    let elapsed = begun.elapsed().as_secs_f64();
    assert_eq!(status.driver.kind, DriverKind::Dummy);
    assert_eq!(status.driver.state, DriverRunState::Running);
    assert_eq!(status.driver.buffer_frames, 48);
    assert_eq!(status.next_frame % 48, 0);
    // Paced by the buffer period: never ahead of real time by more than its start.
    assert!(status.next_frame as f64 <= 48_000. * elapsed + 2. * 48.);
    assert_eq!(status.stats.callbacks, status.next_frame / 48);
    drop(client);
    let report = host.stop();
    assert!(report.midi_reset);
    assert_eq!(report.phase, RenderPhase::Disconnected);
    let stopped = report.stopped.unwrap();
    assert_eq!(
        stopped.resource_usage().free_pages,
        description().resource_pages
    );
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn heap_guards_are_live() {
    assert_heap_guard_is_live();
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn controlled_mode_renders_exactly_the_requested_frames() {
    let Started { host, client } = start(&config(controlled())).unwrap();
    let dummy = host.dummy().unwrap().clone();
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(host.status().next_frame, 0);
    dummy.request_frames(100);
    assert!(dummy.wait_rendered(Duration::from_secs(5)));
    assert_eq!(host.status().next_frame, 100);
    assert_eq!(host.status().stats.callbacks, 4);
    // 10.51 ms at 48 kHz is 504.48 frames; the fraction carries over.
    dummy.advance(Duration::from_micros(10_510));
    dummy.advance(Duration::from_micros(11));
    assert!(dummy.wait_rendered(Duration::from_secs(5)));
    assert_eq!(host.status().next_frame, 100 + 505);
    drop(client);
    let report = host.stop();
    assert!(report.midi_reset);
    assert!(report.stopped.is_ok());
}

#[shoop_test(no_wasm = "native driver and threads")]
fn startup_and_cleanup_fit_the_native_one_megabyte_stack() {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            let Started { host, client } = start(&config(controlled())).unwrap();
            let dummy = host.dummy().unwrap();
            dummy.request_frames(32);
            assert!(dummy.wait_rendered(Duration::from_secs(5)));
            assert_eq!(host.status().phase, RenderPhase::Running);
            drop(client);
            assert!(host.stop().stopped.is_ok());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[shoop_test(no_wasm = "native driver and threads")]
fn quantum_native_startup_handles_partial_callbacks_and_returns_all_owners() {
    for q in [16, 32, 64, 128] {
        let mut settings = config(DummyConfig {
            buffer_frames: 7,
            mode: DummyMode::Controlled,
            wrap_cycle: Some(guarded),
            ..DummyConfig::default()
        });
        settings.engine.quantum = q;
        let Started { host, mut client } = start(&settings).unwrap();
        let dummy = host.dummy().unwrap().clone();
        let mut received = Received::default();
        connected(&mut client, &mut received);
        assert!(received.lifecycle.iter().any(|m| matches!(m,
            EngineMessage::Welcome { limits, .. } if limits.quantum == q as u32 && limits.processing_latency_frames == q as u32)));
        let strip = strip(0);
        for request in [
            Request::CreateStrip {
                strip,
                shape: shape(1, false),
            },
            Request::CreateLoop {
                loop_id: loop_id(0),
                strip,
            },
            Request::LoopTransition {
                loop_id: loop_id(0),
                mode: LoopMode::Recording,
                delay: None,
            },
        ] {
            let correlation = client.request(request).unwrap();
            until(&mut client, &mut received, |r| {
                if r.results.contains_key(&correlation) {
                    return true;
                }
                dummy.request_frames(q as u64);
                assert!(dummy.wait_rendered(Duration::from_secs(5)));
                false
            });
            assert_eq!(received.results[&correlation], Outcome::Done);
        }
        let before = host.status().next_frame;
        dummy.request_frames((2 * q + 3) as u64);
        assert!(dummy.wait_rendered(Duration::from_secs(5)));
        assert_eq!(host.status().next_frame, before + (2 * q + 3) as u64);
        assert_eq!(host.status().phase, RenderPhase::Running);
        drop(client);
        let stopped = host.stop().stopped.unwrap();
        assert_eq!(
            stopped.resource_usage().free_pages,
            settings.engine.resource_pages
        );
        assert_eq!(HEAP_OPERATIONS.load(Ordering::Relaxed), 0);
    }
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn buffers_above_the_prepared_maximum_are_refused() {
    let refused = start(&config(DummyConfig {
        buffer_frames: 65,
        ..DummyConfig::default()
    }));
    assert_eq!(
        refused.err(),
        Some(StartError::BufferTooLarge {
            requested: 65,
            max: 64
        })
    );
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn a_client_creates_a_strip_and_sees_its_telemetry_while_the_driver_renders() {
    let Started { host, mut client } = start(&config(DummyConfig {
        buffer_frames: 64,
        input: Some(|port, _| if port == 0 { 0.5 } else { 0. }),
        ..DummyConfig::default()
    }))
    .unwrap();
    let mut received = Received::default();
    connected(&mut client, &mut received);
    let track = strip(0);
    request_done(
        &mut client,
        &mut received,
        Request::CreateStrip {
            strip: track,
            shape: shape(1, false),
        },
    );
    request_done(
        &mut client,
        &mut received,
        connect(track, PortKind::AudioIn, 0),
    );
    request_done(&mut client, &mut received, monitoring(track));
    until(&mut client, &mut received, |r| {
        r.strip_peaks(track).is_some_and(|(input, output)| {
            input.as_slice()[0] == 0.5 && output.as_slice()[0] == 0.5
        })
    });
    let (frame, _, driver) = received.last_engine().unwrap();
    assert!(frame > 0);
    assert_eq!(driver.kind, DriverKind::Dummy);
    assert_eq!(driver.state, DriverRunState::Running);
    assert_eq!(driver.sample_rate, 48_000);
    drop(client);
    assert!(host.stop().midi_reset);
}

static HEAP_OPERATIONS: AtomicUsize = AtomicUsize::new(0);
static GUARDED_CYCLES: AtomicUsize = AtomicUsize::new(0);

fn guarded(cycle: &mut dyn FnMut()) {
    let ((), counts) = measure(cycle);
    HEAP_OPERATIONS.fetch_add(
        counts.allocations + counts.reallocations + counts.deallocations,
        Ordering::Relaxed,
    );
    GUARDED_CYCLES.fetch_add(1, Ordering::Relaxed);
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn the_driver_loop_does_not_touch_the_heap_through_structural_edits() {
    let Started { host, mut client } = start(&config(DummyConfig {
        buffer_frames: 32,
        mode: DummyMode::Controlled,
        input: Some(|_, frame| (frame % 7) as f32 * 0.1),
        wrap_cycle: Some(guarded),
        ..DummyConfig::default()
    }))
    .unwrap();
    let dummy = host.dummy().unwrap().clone();
    let mut received = Received::default();
    connected(&mut client, &mut received);
    let request_done = |client: &mut NativeClient, received: &mut Received, request| {
        let correlation = client.request(request).unwrap();
        until(client, received, |r| {
            if r.results.contains_key(&correlation) {
                return true;
            }
            dummy.request_frames(32);
            assert!(dummy.wait_rendered(Duration::from_secs(5)));
            false
        });
        assert_eq!(received.results[&correlation], Outcome::Done, "{request:?}");
    };
    let track = strip(1);
    let looped = loop_id(2);
    for edit in [
        Request::CreateStrip {
            strip: track,
            shape: shape(2, true),
        },
        connect(track, PortKind::AudioIn, 0),
        connect(track, PortKind::AudioOut, 2),
        monitoring(track),
        Request::CreateLoop {
            loop_id: looped,
            strip: track,
        },
        Request::LoopTransition {
            loop_id: looped,
            mode: LoopMode::Recording,
            delay: None,
        },
    ] {
        request_done(&mut client, &mut received, edit);
    }
    let recording_frame = host.status().next_frame;
    dummy.advance(Duration::from_millis(20));
    assert!(dummy.wait_rendered(Duration::from_secs(5)));
    assert_eq!(host.status().next_frame, recording_frame + 960);
    for edit in [
        Request::LoopTransition {
            loop_id: looped,
            mode: LoopMode::Playing,
            delay: None,
        },
        Request::RemoveLoop { loop_id: looped },
        Request::RemoveStrip { strip: track },
    ] {
        request_done(&mut client, &mut received, edit);
    }
    drop(client);
    let report = host.stop();
    assert!(report.midi_reset);
    assert!(report.stopped.is_ok());
    assert!(GUARDED_CYCLES.load(Ordering::Relaxed) > 10);
    assert_eq!(HEAP_OPERATIONS.load(Ordering::Relaxed), 0);
}

#[shoop_test(no_wasm = "native driver and threads")]
fn retiring_a_connection_preserves_the_endpoint_and_running_host() {
    use shoop_rt_transport::carrier::Endpoint;
    let shoop_rt_host_native::StartedEndpoint {
        host,
        mut endpoint,
        mut outbox,
    } = shoop_rt_host_native::start_endpoint(&config(controlled())).unwrap();
    endpoint.deliver(
        ClientMessage::Hello {
            version: PROTOCOL_VERSION,
        },
        &mut outbox,
    );
    assert_no_heap(|| endpoint.disconnect());
    let dummy = host.dummy().unwrap().clone();
    dummy.request_frames(640);
    assert!(dummy.wait_rendered(Duration::from_secs(5)));
    assert_eq!(host.status().next_frame, 640);
    assert_eq!(host.status().phase, RenderPhase::Running);
    assert_no_heap(|| endpoint.poll(&mut outbox));
    drop(endpoint);
    let report = host.stop();
    assert!(report.midi_reset);
    assert!(report.stopped.is_ok());
}
