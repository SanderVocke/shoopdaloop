//! Runs the cpal driver against a real audio device.
//!
//! Only test in this suite that touches hardware, so it skips rather than fails when
//! there is no output device -- headless CI has none, and a skipped test there is more
//! useful than a red one.
//!
//! It plays silence on purpose: what is being checked is that the device's callback
//! drives the engine, not what comes out of the speakers.

#![cfg(feature = "cpal")]

use shoop_engine::cpal_driver::{start_output, CpalError};
use shoop_engine::session::Session;
use std::sync::atomic::Ordering;

#[test]
fn the_device_callback_drives_the_engine() {
    let mut s = Session::default();
    s.apply_graph_changes().expect("schedule");

    let driver = match start_output(s, 64, |_s, ports| {
        assert!(!ports.is_empty(), "a port per device channel");
        Ok(())
    }) {
        Ok(d) => d,
        Err(CpalError::NoOutputDevice) => {
            eprintln!("no output device; skipping");
            return;
        }
        Err(e) => panic!("could not start the cpal driver: {e}"),
    };

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

    // Long enough for a device at any sane buffer size to have called back.
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

    let mut driver = match start_output(s, 64, |s, ports| {
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
    }) {
        Ok(d) => d,
        Err(CpalError::NoOutputDevice) => {
            eprintln!("no output device; skipping");
            return;
        }
        Err(e) => panic!("could not start the cpal driver: {e}"),
    };

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

/// Duplex against real devices: the input stream feeds a ring, the output stream drains
/// it and drives the engine.
///
/// Skips when either device is missing, and tolerates the input stream being refused --
/// on macOS, microphone access is a permission the process may not have, and that is
/// not a fault in this code.
#[test]
fn duplex_bridges_the_two_streams() {
    use shoop_engine::cpal_driver::start_duplex;

    let mut s = Session::default();
    s.apply_graph_changes().expect("schedule");

    let mut driver = match start_duplex(s, 64, 4096, |_s, out_ports, in_ports| {
        assert!(!out_ports.is_empty());
        assert!(!in_ports.is_empty());
        Ok(())
    }) {
        Ok(d) => d,
        Err(CpalError::NoOutputDevice) | Err(CpalError::NoInputDevice) => {
            eprintln!("no duplex pair available; skipping");
            return;
        }
        Err(CpalError::Build(e)) => {
            eprintln!("input stream refused ({e}); skipping");
            return;
        }
        Err(e) => panic!("could not start duplex: {e}"),
    };

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
