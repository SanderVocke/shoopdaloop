//! The dummy driver: real-time and requested rendering, stopping, and a protocol
//! client working against the engine while the driver thread renders.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use shoop_rt_host_native::{
    start, DriverConfig, DummyConfig, DummyMode, NativeConfig, RenderPhase, StartError, Started,
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
    let status = host.status();
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
        input: Some(|_, frame| (frame % 7) as f32 * 0.1),
        wrap_cycle: Some(guarded),
        ..DummyConfig::default()
    }))
    .unwrap();
    let mut received = Received::default();
    connected(&mut client, &mut received);
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
    std::thread::sleep(Duration::from_millis(20));
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
