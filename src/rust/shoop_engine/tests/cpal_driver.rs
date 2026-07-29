//! Runs the cpal driver against a software host that fires its callbacks from a regular
//! thread, so the audio path is exercised end-to-end without an ALSA / CoreAudio /
//! WASAPI device. The mock lives in `tests/mock_host`; without it these tests would
//! skip (or fail, when ALSA exposes a stub default that cannot be configured) on the
//! fresh headless image CI uses.
//!
//! What is being checked is that the device's callback drives the engine, not what
//! comes out of the speakers, so the data the mock hands the callback is silence.

#![cfg(feature = "cpal")]

mod mock_host;

use mock_host::MockHost;
use shoop_engine::cpal_driver::{
    start_duplex_on_host, start_output_on_host, CpalError,
};
use shoop_engine::session::Session;
use std::sync::atomic::Ordering;

#[test]
fn the_device_callback_drives_the_engine() {
    let mut s = Session::default();
    s.apply_graph_changes().expect("schedule");

    let driver = start_output_on_host(MockHost::new(), s, 64, |_s, ports| {
        assert!(!ports.is_empty(), "a port per device channel");
        Ok(())
    })
    .expect("mock host should always start");

    assert!(driver.n_channels() > 0);
    // Through the trait, so the trait is exercised by a real driver rather than only by
    // the dummy one.
    {
        use shoop_engine::driver::Driver;
        let d: &dyn Driver = &driver;
        assert!(d.sample_rate() > 0);
        assert!(!d.client_name().is_empty());
        eprintln!("driver: {} at {} Hz", d.client_name(), d.sample_rate());
    }

    // Long enough for a device at any sane buffer size to have called back. The mock
    // runs at the configured sample rate, so this is more than enough.
    let stats = {
        let mut d = driver;
        let stats = d.handle().stats().clone();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let cycles = stats.cycles.load(Ordering::Relaxed);
        let frames = stats.frames.load(Ordering::Relaxed);
        eprintln!(
            "cpal: {} Hz, {} channels, {cycles} cycles, {frames} frames",
            d.sample_rate(),
            d.n_channels()
        );
        (cycles, frames)
    };

    assert!(stats.0 > 0, "the device never called back");
    assert!(stats.1 > 0, "no frames were processed");
}

/// A loop playing through the driver, to show the engine is really wired to the device
/// rather than merely being ticked by it.
#[test]
fn a_playing_loop_reaches_the_device_ports() {
    use shoop_engine::channel_mode::ChannelMode;
    use shoop_engine::loop_mode::LoopMode;

    let mut s = Session::default();
    s.apply_graph_changes().expect("schedule");

    let mut driver = start_output_on_host(MockHost::new(), s, 64, |s, ports| {
        let l = s.create_loop();
        let c = s
            .add_audio_channel(l, 1024, ChannelMode::Direct)
            .expect("channel");
        s.connect_channel_output(c, ports[0]).expect("connect");
        // Silence, so nothing is audible; the peak still shows it was played.
        s.loop_mut(l)
            .expect("loop")
            .audio_channel_mut(0)
            .expect("channel")
            .load_data(&vec![0.0f32; 1024]);
        s.loop_mut(l).expect("loop").set_length(1024);
        s.set_loop_mode(l, LoopMode::Playing).expect("mode");
        Ok(())
    })
    .expect("mock host should always start");

    std::thread::sleep(std::time::Duration::from_millis(300));

    let snap = driver.handle().poll().cloned();
    let snap = snap.expect("the engine published state");
    assert_eq!(snap.loops.len(), 1);
    assert_eq!(snap.loops[0].mode, LoopMode::Playing);
    // A playing loop advances, which only happens if the device is driving cycles.
    assert!(
        snap.loops[0].position > 0,
        "the loop never advanced: {:?}",
        snap.loops[0]
    );
}

/// Duplex against the mock: the input stream feeds a ring, the output stream drains
/// it and drives the engine.
///
/// The production code's match arms (`CpalError::NoOutputDevice`,
/// `CpalError::NoInputDevice`, `CpalError::Build`) were written for real hardware,
/// where a missing input is a permission or a missing cable. The mock always has
/// both, so this test exercises the happy path -- which is the path CI never
/// reached before the mock existed.
#[test]
fn duplex_bridges_the_two_streams() {
    let mut s = Session::default();
    s.apply_graph_changes().expect("schedule");

    let mut driver = start_duplex_on_host(MockHost::new(), s, 64, 4096, |_s, out_ports, in_ports| {
        assert!(!out_ports.is_empty());
        assert!(!in_ports.is_empty());
        Ok(())
    })
    .expect("mock host should always start");

    assert!(driver.n_capture_channels() > 0);

    let stats = driver.handle().stats().clone();
    std::thread::sleep(std::time::Duration::from_millis(500));

    let cycles = stats.cycles.load(Ordering::Relaxed);
    let underruns = stats.capture_underruns.load(Ordering::Relaxed);
    let overruns = stats.capture_overruns.load(Ordering::Relaxed);
    eprintln!(
        "duplex: {} in / {} out channels, {cycles} cycles, {underruns} underruns, {overruns} overruns",
        driver.n_capture_channels(),
        driver.n_channels()
    );

    assert!(cycles > 0, "the output device never called back");
    // The first cycles legitimately underrun while the input stream spins up; what would
    // be wrong is every cycle underrunning, which would mean the ring never fills.
    assert!(
        underruns < cycles,
        "every cycle underran: the capture ring is never filling"
    );
}

/// Bonus test that was previously impossible: the production code's
/// `CpalError::NoOutputDevice` path. Driving a host that has no default device through
/// the same code path that previously made CI panic is the whole reason the mock
/// interface exists.
#[test]
fn a_host_with_no_output_device_returns_no_output_device() {
    let host = mock_host::MockHostNoOutput::with_default_input();
    let mut s = Session::default();
    s.apply_graph_changes().expect("schedule");

    let result = start_output_on_host(host, s, 64, |_s, _ports| Ok(()));
    match result {
        Err(CpalError::NoOutputDevice) => {}
        Err(other) => panic!("expected NoOutputDevice, got: {other}"),
        Ok(_) => panic!("expected NoOutputDevice, got Ok"),
    }
}