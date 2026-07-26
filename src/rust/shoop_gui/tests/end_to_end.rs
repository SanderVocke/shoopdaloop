//! The whole application without the window: instrument, engine and device together.
//!
//! What the GUI does when a key is played and a loop is recorded, driven from a test, so
//! the wiring between the three is checked rather than only the parts. Needs a real
//! output device, so it skips where there is none.

use shoop_engine::cpal_driver::{start_output_with_hook, CpalError, CycleHook};
use shoop_engine::driver::Driver;
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi;
use shoop_engine::session::Session;
use shoop_gui::{instrument, session as layout};

use std::sync::atomic::Ordering;

#[test]
fn a_played_note_reaches_the_device_and_can_be_recorded() {
    // The same wiring `App::start` does, minus the window.
    let started = start_output_with_hook(Session::default(), 256, |session, outs| {
        let l = layout::configure(session, outs);
        let (keys, mut voice, settings, n_voices) =
            instrument::split(l.instrument_port, session.sample_rate());
        voice.set_sample_rate(session.sample_rate());
        let hook: CycleHook = Box::new(move |s: &mut Session, n: usize| voice.render_into(s, n));
        Ok(((keys, settings, n_voices, l), hook))
    });

    let (mut driver, (mut keys, _settings, n_voices, l)) = match started {
        Ok(v) => v,
        Err(CpalError::NoOutputDevice) => {
            eprintln!("no output device; skipping");
            return;
        }
        Err(e) => panic!("could not start: {e}"),
    };

    // Let the device settle, so cycles are running before anything is asked of it.
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!(
        driver.stats().cycles.load(Ordering::Relaxed) > 0,
        "the device never called back"
    );

    let first = l.loops[0].loop_idx;
    driver
        .handle()
        .send(Box::new(move |s: &mut Session| {
            // Immediate rather than planned: a planned one waits for the sync loop's
            // boundary, which is two seconds away.
            let _ = s.set_loop_mode(first, LoopMode::Recording);
        }))
        .expect("queue has room");

    keys.send(&midi::note_on(0, 69, 110));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        n_voices.load(Ordering::Relaxed) > 0,
        "the instrument never sounded"
    );
    keys.send(&midi::note_off(0, 69, 64));

    driver
        .handle()
        .send(Box::new(move |s: &mut Session| {
            let _ = s.set_loop_mode(first, LoopMode::Stopped);
        }))
        .expect("queue has room");
    std::thread::sleep(std::time::Duration::from_millis(100));

    let (length, peak) = driver
        .handle()
        .send_and_wait(
            move |s: &mut Session| {
                s.loop_(first)
                    .and_then(|lp| lp.audio_channel(0))
                    .map(|c| {
                        let data = c.data();
                        (c.length(), data.iter().fold(0.0f32, |a, b| a.max(b.abs())))
                    })
                    .unwrap_or((0, 0.0))
            },
            std::time::Duration::from_secs(2),
        )
        .expect("the engine answered");

    eprintln!("recorded {length} frames, peak {peak}");
    assert!(length > 0, "nothing was recorded");
    assert!(peak > 0.0, "what was recorded is silent");
}

#[test]
fn the_configured_session_is_playable_without_a_device() {
    // The part that needs no hardware, so it runs everywhere.
    let mut s = Session::default();
    s.set_sample_rate(48000);
    let out = s.add_port(shoop_engine::session::Port::External(
        shoop_engine::external_audio_port::ExternalAudioPort::new(
            "out",
            shoop_engine::port::PortDirection::Output,
            0,
        ),
    ));
    let l = layout::configure(&mut s, &[out]);

    let (mut keys, mut voice, _settings, n_voices) = instrument::split(l.instrument_port, 48000);
    keys.send(&midi::note_on(0, 60, 100));

    // Record into the first loop for a few cycles.
    let first = l.loops[0].loop_idx;
    s.set_loop_mode(first, LoopMode::Recording).expect("mode");
    for _ in 0..8 {
        voice.render_into(&mut s, 256);
        s.process(256).expect("cycle");
    }

    assert!(n_voices.load(Ordering::Relaxed) > 0);
    let ch = s
        .loop_(first)
        .and_then(|lp| lp.audio_channel(0))
        .expect("channel");
    assert!(ch.length() > 0, "nothing was recorded");
    let peak = ch.data().iter().fold(0.0f32, |a, b| a.max(b.abs()));
    assert!(peak > 0.0, "what was recorded is silent");
}

/// A track's gain reaches its port, and muting silences it.
///
/// No device needed: the wiring being checked is loop to track port, which is the same
/// with or without one.
#[test]
fn track_gain_and_muting_apply_to_the_track_port() {
    use shoop_engine::external_audio_port::ExternalAudioPort;
    use shoop_engine::port::PortDirection;
    use shoop_engine::session::Port;

    let mut s = Session::default();
    s.set_sample_rate(48000);
    let out = s.add_port(Port::External(ExternalAudioPort::new(
        "out",
        PortDirection::Output,
        0,
    )));
    let l = layout::configure(&mut s, &[out]);

    // A loud loop playing on track 0.
    let first = l.loops[0].loop_idx;
    if let Some(c) = s.loop_mut(first).and_then(|lp| lp.audio_channel_mut(0)) {
        c.load_data(&vec![1.0f32; 1024]);
    }
    s.loop_mut(first).expect("loop").set_length(1024);
    s.set_loop_mode(first, LoopMode::Playing).expect("mode");

    let track = l.track_ports[0];

    // At half gain the track's level is halved.
    if let Some(a) = s.port_mut(track).and_then(|p| p.audio_mut()) {
        a.set_gain(0.5);
        a.reset_output_peak();
    }
    s.process(256).expect("cycle");
    let half = s
        .port(track)
        .and_then(|p| p.audio())
        .map(|a| a.output_peak())
        .expect("track port");
    assert!(
        (half - 0.5).abs() < 0.01,
        "expected about half level, got {half}"
    );

    // Muted, it is silent regardless of what is playing.
    if let Some(a) = s.port_mut(track).and_then(|p| p.audio_mut()) {
        a.set_muted(true);
        a.reset_output_peak();
    }
    s.process(256).expect("cycle");
    let muted = s
        .port(track)
        .and_then(|p| p.audio())
        .map(|a| a.output_peak())
        .expect("track port");
    assert_eq!(muted, 0.0, "a muted track still had level");
}

/// One track's level does not leak into another's.
#[test]
fn tracks_are_metered_independently() {
    use shoop_engine::external_audio_port::ExternalAudioPort;
    use shoop_engine::port::PortDirection;
    use shoop_engine::session::Port;

    let mut s = Session::default();
    s.set_sample_rate(48000);
    let out = s.add_port(Port::External(ExternalAudioPort::new(
        "out",
        PortDirection::Output,
        0,
    )));
    let l = layout::configure(&mut s, &[out]);

    // Only track 0 plays.
    let first = l.loops[0].loop_idx;
    if let Some(c) = s.loop_mut(first).and_then(|lp| lp.audio_channel_mut(0)) {
        c.load_data(&vec![0.75f32; 1024]);
    }
    s.loop_mut(first).expect("loop").set_length(1024);
    s.set_loop_mode(first, LoopMode::Playing).expect("mode");
    s.process(256).expect("cycle");

    let peak_of = |s: &Session, t: usize| {
        s.port(l.track_ports[t])
            .and_then(|p| p.audio())
            .map(|a| a.output_peak())
            .unwrap_or(0.0)
    };
    assert!(peak_of(&s, 0) > 0.0, "the playing track had no level");
    for t in 1..layout::N_TRACKS {
        assert_eq!(peak_of(&s, t), 0.0, "track {t} picked up another's signal");
    }
}

/// A session's audio survives a save and load, landing in the same grid cells.
///
/// Exercised against a plain `Session` rather than through the window, because what is
/// being checked is that the captured shape maps back onto the grid.
#[test]
fn a_saved_session_restores_its_loops() {
    use shoop_engine::external_audio_port::ExternalAudioPort;
    use shoop_engine::port::PortDirection;
    use shoop_engine::session::Port;
    use shoop_gui::persist::{
        SavedInstrument, SavedLoop, SavedSession, SavedTrack, FORMAT_VERSION,
    };

    let mut s = Session::default();
    s.set_sample_rate(48000);
    let out = s.add_port(Port::External(ExternalAudioPort::new(
        "out",
        PortDirection::Output,
        0,
    )));
    let l = layout::configure(&mut s, &[out]);

    // Something recognisable in one cell, so a mix-up is visible rather than plausible.
    let material: Vec<f32> = (0..64).map(|i| i as f32 / 64.0).collect();
    let saved = SavedSession {
        version: FORMAT_VERSION,
        sample_rate: 48000,
        sync_length: 48000,
        tracks: vec![
            SavedTrack {
                gain: 0.25,
                muted: true,
            };
            layout::N_TRACKS
        ],
        loops: vec![SavedLoop {
            track: 2,
            row: 1,
            length: material.len() as u32,
            samples: material.clone(),
        }],
        instrument: SavedInstrument {
            waveform: "square".to_string(),
            gain: 0.4,
        },
    };

    // Apply it the way `App::restore` does.
    for sl in &saved.loops {
        let target = l.loop_at(sl.track, sl.row).expect("cell").loop_idx;
        if let Some(c) = s.loop_mut(target).and_then(|lp| lp.audio_channel_mut(0)) {
            c.load_data(&sl.samples);
        }
        s.loop_mut(target).expect("loop").set_length(sl.length);
    }

    // It landed in the cell it named, and nowhere else.
    let target = l.loop_at(2, 1).expect("cell").loop_idx;
    let ch = s
        .loop_(target)
        .and_then(|lp| lp.audio_channel(0))
        .expect("channel");
    assert_eq!(ch.length(), material.len());
    assert_eq!(&ch.data()[..material.len()], &material[..]);

    for cell in &l.loops {
        if cell.loop_idx == target {
            continue;
        }
        let other = s
            .loop_(cell.loop_idx)
            .and_then(|lp| lp.audio_channel(0))
            .expect("channel");
        // Every loop is one bar long in this model, so an untouched one is silent rather than
        // zero-length.
        let peak = other.data().iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert_eq!(
            peak, 0.0,
            "loop at {},{} was written to as well",
            cell.track, cell.row
        );
    }

    // And the file itself round-trips, so what was applied is what would be written.
    let text = saved.to_json().expect("serialise");
    assert_eq!(SavedSession::from_json(&text).expect("parse"), saved);
}

/// A real controller message drives the grid: sent over a virtual MIDI port, captured, and
/// resolved to an action.
///
/// Virtual ports are Unix-only in `midir`, so this skips elsewhere. It checks the whole
/// control path rather than only the mapping, which is already unit-tested.
#[cfg(unix)]
#[test]
fn a_controller_message_resolves_to_an_action() {
    use shoop_engine::external_midi_port::ExternalMidiPort;
    use shoop_engine::midir_driver::{create_virtual_input, open_output};
    use shoop_engine::port::PortDirection;
    use shoop_gui::midi_control::{ControlAction, Mapping};
    use shoop_gui::selection::Cell;

    let name = "shoop-test-control";
    let Ok((mut capture, _conn)) = create_virtual_input("shoop-ctl-in", name) else {
        eprintln!("no virtual MIDI port available; skipping");
        return;
    };
    let Ok(mut out) = open_output("shoop-ctl-out", "to-virtual", name) else {
        eprintln!("virtual port not visible to a sender; skipping");
        return;
    };

    // Note 36 is the first pad in the default mapping.
    let mut sender = ExternalMidiPort::new("sender", PortDirection::Output);
    sender.prepare(64);
    sender.write_event(
        shoop_engine::midi_storage::MidiStorageElem::new(0, &midi::note_on(0, 36, 100))
            .expect("valid"),
    );
    sender.process(64);
    assert_eq!(out.send_from(&sender), 1);

    // Wait for it to arrive, then resolve it the way the app does.
    let mapping = Mapping::default_for_grid(4, 4);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut fired = Vec::new();
    while std::time::Instant::now() < deadline && fired.is_empty() {
        let mut scratch = ExternalMidiPort::new("scratch", PortDirection::Input);
        capture.drain_into(&mut scratch);
        scratch.prepare(0);
        for e in scratch.visible_events() {
            fired.extend(mapping.resolve(e.data()));
        }
        if fired.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    assert!(!fired.is_empty(), "the controller message never arrived");
    assert_eq!(
        fired[0].action,
        ControlAction::Play(Cell { track: 0, row: 0 })
    );
}

/// Device enumeration works, and an unknown name falls back rather than failing.
///
/// The fallback is the part worth testing: a session that remembers a device which has since
/// been unplugged must still start.
#[test]
fn an_unknown_device_falls_back_to_the_default() {
    use shoop_engine::cpal_driver::{
        default_output_device_name, output_device_names, start_output_on_device,
    };

    let names = output_device_names();
    if names.is_empty() {
        eprintln!("no output devices; skipping");
        return;
    }
    // Enumeration and the default agree with each other.
    let default = default_output_device_name().expect("a default device");
    assert!(
        names.contains(&default),
        "the default device {default:?} is not among {names:?}"
    );

    let started = start_output_on_device(
        Session::default(),
        16,
        Some("a device that does not exist".to_string()),
        |session, outs| {
            let l = layout::configure(session, outs);
            Ok((l, Box::new(|_: &mut Session, _: usize| {}) as CycleHook))
        },
    );

    match started {
        Ok((driver, _)) => {
            // Fell back rather than refusing.
            assert!(!driver.client_name().is_empty());
        }
        Err(CpalError::NoOutputDevice) => eprintln!("no output device; skipping"),
        Err(e) => panic!("should have fallen back, got {e}"),
    }
}

/// A loop must be audible at the *device*, not merely at its track port.
///
/// The distinction matters: an earlier test checked the track port's level and passed while nothing
/// reached the device at all, because internal port connections ordered the graph without copying
/// any audio.
#[test]
fn a_playing_loop_reaches_the_device_through_its_track() {
    use shoop_engine::external_audio_port::ExternalAudioPort;
    use shoop_engine::port::PortDirection;
    use shoop_engine::session::Port;

    let mut s = Session::default();
    s.set_sample_rate(48000);
    let out = s.add_port(Port::External(ExternalAudioPort::new(
        "out",
        PortDirection::Output,
        0,
    )));
    let l = layout::configure(&mut s, &[out]);

    let first = l.loops[0].loop_idx;
    if let Some(c) = s.loop_mut(first).and_then(|lp| lp.audio_channel_mut(0)) {
        c.load_data(&vec![0.5f32; 512]);
    }
    s.loop_mut(first).expect("loop").set_length(512);
    s.set_loop_mode(first, LoopMode::Playing).expect("mode");
    s.process(256).expect("cycle");

    let device_peak = s
        .port(out)
        .and_then(|p| p.audio())
        .map(|a| a.output_peak())
        .expect("output port");
    assert!(
        device_peak > 0.0,
        "the loop never reached the device: the track's routing carries no audio"
    );
}

/// Playing the instrument is audible at the device without recording first.
#[test]
fn the_instrument_is_audible_at_the_device() {
    use shoop_engine::external_audio_port::ExternalAudioPort;
    use shoop_engine::port::PortDirection;
    use shoop_engine::session::Port;

    let mut s = Session::default();
    s.set_sample_rate(48000);
    let out = s.add_port(Port::External(ExternalAudioPort::new(
        "out",
        PortDirection::Output,
        0,
    )));
    let l = layout::configure(&mut s, &[out]);

    let (mut keys, mut voice, _settings, _n) = instrument::split(l.instrument_port, 48000);
    keys.send(&midi::note_on(0, 69, 120));

    // Several cycles, so the envelope has risen past its ramp.
    for _ in 0..8 {
        voice.render_into(&mut s, 256);
        s.process(256).expect("cycle");
    }

    let device_peak = s
        .port(out)
        .and_then(|p| p.audio())
        .map(|a| a.output_peak())
        .expect("output port");
    assert!(
        device_peak > 0.0,
        "pressing a key produced nothing at the device"
    );
}

// --- the fixed-bar model ---
//
// A loop is always exactly one bar long, every loop shares the bar's position, and playing into a
// loop replaces those frames rather than starting a take of its own. These tests pin each of those
// three claims, because the whole workflow -- lay down a bar, then overlay another in time with it
// -- rests on them and none of them held under the previous variable-length model.

mod fixed_bar {
    use super::*;
    use shoop_engine::external_audio_port::ExternalAudioPort;
    use shoop_engine::port::PortDirection;
    use shoop_engine::session::Port;

    /// A session with a device output, one bar of `bar` frames, and the instrument port exposed.
    fn rig(bar: u32) -> (Session, layout::Layout, usize) {
        let mut s = Session::default();
        s.set_sample_rate(48000);
        let out = s.add_port(Port::External(ExternalAudioPort::new(
            "out",
            PortDirection::Output,
            0,
        )));
        // A short bar, so a test can drive whole cycles by hand. Asked for up front rather than
        // resized afterwards, so these tests exercise the layout's own sizing.
        let l = layout::configure_with_bar(&mut s, &[out], bar);
        (s, l, out)
    }

    /// Feeds `level` into the instrument port and runs one cycle of `n` frames.
    fn cycle(s: &mut Session, instrument: usize, level: f32, n: usize) {
        if let Some(p) = s.port_mut(instrument).and_then(Port::as_external_mut) {
            p.stage_input(&vec![level; n]);
        }
        s.process(n).expect("cycle");
    }

    fn contents(s: &Session, loop_idx: usize) -> Vec<f32> {
        s.loop_(loop_idx)
            .and_then(|lp| lp.audio_channel(0))
            .map(|c| c.data()[..c.length()].to_vec())
            .expect("channel")
    }

    #[test]
    fn every_loop_is_one_bar_long_before_anything_is_recorded() {
        let (s, l, _out) = rig(64);
        for cell in &l.loops {
            let lp = s.loop_(cell.loop_idx).expect("loop");
            assert_eq!(
                lp.length(),
                64,
                "loop at {},{} is not a bar long",
                cell.track,
                cell.row
            );
        }
    }

    #[test]
    fn a_fresh_loop_holds_a_bar_of_silence() {
        let (s, l, _out) = rig(64);
        let c = contents(&s, l.loops[0].loop_idx);
        assert_eq!(c.len(), 64);
        assert_eq!(c.iter().fold(0.0f32, |a, b| a.max(b.abs())), 0.0);
    }

    #[test]
    fn playing_into_a_loop_writes_the_bar_without_changing_its_length() {
        let (mut s, l, _out) = rig(64);
        let target = l.loops[0].loop_idx;

        s.set_loop_mode(target, LoopMode::Replacing).expect("mode");
        cycle(&mut s, l.instrument_port, 0.5, 64);

        let lp = s.loop_(target).expect("loop");
        assert_eq!(lp.length(), 64, "replacing changed the loop's length");
        let c = contents(&s, target);
        assert!(
            c.iter().all(|v| (*v - 0.5).abs() < 1e-6),
            "the bar was not written: {:?}",
            &c[..8]
        );
    }

    /// The property that makes a second pass an erase rather than a layer.
    #[test]
    fn silence_overwrites_what_was_recorded() {
        let (mut s, l, _out) = rig(64);
        let target = l.loops[0].loop_idx;

        s.set_loop_mode(target, LoopMode::Replacing).expect("mode");
        cycle(&mut s, l.instrument_port, 0.5, 64);
        assert!(contents(&s, target).iter().any(|v| *v != 0.0));

        // Second pass over the same bar, playing nothing.
        s.loop_mut(target).expect("loop").set_position(0);
        cycle(&mut s, l.instrument_port, 0.0, 64);

        let c = contents(&s, target);
        assert_eq!(
            c.iter().fold(0.0f32, |a, b| a.max(b.abs())),
            0.0,
            "silence did not erase the bar"
        );
    }

    /// Nothing can come out longer than a bar, however long the player holds a key.
    #[test]
    fn a_take_cannot_outgrow_the_bar() {
        let (mut s, l, _out) = rig(64);
        let target = l.loops[0].loop_idx;

        s.set_loop_mode(target, LoopMode::Replacing).expect("mode");
        // Four bars' worth of continuous input.
        for _ in 0..4 {
            cycle(&mut s, l.instrument_port, 0.5, 64);
        }

        assert_eq!(s.loop_(target).expect("loop").length(), 64);
        assert_eq!(contents(&s, target).len(), 64);
    }

    /// Clearing a loop that has something in it leaves silence, not the old take.
    ///
    /// The trap this guards: the engine's `clear` is a faithful port of the C++ `PROC_clear`, which
    /// only makes samples addressable and never zeroes them. That is harmless when clearing to
    /// length zero, since the old audio becomes unreachable -- but a fixed-bar loop is cleared to a
    /// bar, which leaves every one of those samples both audible and drawn in the waveform.
    #[test]
    fn clearing_a_recorded_loop_leaves_silence() {
        let (mut s, l, _out) = rig(64);
        let target = l.loops[0].loop_idx;

        s.set_loop_mode(target, LoopMode::Replacing).expect("mode");
        cycle(&mut s, l.instrument_port, 0.5, 64);
        assert!(
            contents(&s, target).iter().any(|v| *v != 0.0),
            "nothing was recorded, so the clear proves nothing"
        );

        s.resize_loop(target, 64).expect("clear");

        let c = contents(&s, target);
        assert_eq!(c.len(), 64, "clearing changed the loop's length");
        assert_eq!(
            c.iter().fold(0.0f32, |a, b| a.max(b.abs())),
            0.0,
            "the previous take survived the clear"
        );
    }

    /// Stopping takes effect at once, and nothing queued behind it restarts the loop.
    ///
    /// Deliberately not synced: a loop is a bar long by construction, so waiting for the boundary
    /// cannot improve the alignment and only makes the button look broken.
    #[test]
    fn stopping_takes_effect_immediately() {
        let (mut s, l, _out) = rig(64);
        let target = l.loops[0].loop_idx;

        s.set_loop_mode(target, LoopMode::Replacing).expect("mode");
        // Part of a bar, so the stop lands mid-cycle.
        cycle(&mut s, l.instrument_port, 0.5, 16);

        // What the UI's `play after record` queues behind a take.
        s.loop_mut(target)
            .expect("loop")
            .plan_transition(LoopMode::Playing, Some(1), None);

        // The stop, as the button sends it.
        s.loop_mut(target)
            .expect("loop")
            .clear_planned_transitions();
        s.set_loop_mode(target, LoopMode::Stopped).expect("mode");

        assert_eq!(s.loop_(target).expect("loop").mode(), LoopMode::Stopped);

        // Still stopped several bars later: the queued play was dropped rather than deferred.
        for _ in 0..4 {
            cycle(&mut s, l.instrument_port, 0.0, 64);
        }
        assert_eq!(
            s.loop_(target).expect("loop").mode(),
            LoopMode::Stopped,
            "something queued behind the stop started the loop again"
        );
        // And the take it did capture is still a bar long.
        assert_eq!(s.loop_(target).expect("loop").length(), 64);
    }

    /// Recording starts at once, mid-bar, and what was played is kept.
    ///
    /// The bug this pins: with the take planned for the next bar boundary, everything played before
    /// that boundary went nowhere -- while still being audible through the monitoring path, so it
    /// read as the recording silently failing. There is nothing to wait for in a fixed-bar model.
    #[test]
    fn recording_starts_at_once_and_keeps_what_was_played() {
        let (mut s, l, _out) = rig(64);
        let target = l.loops[0].loop_idx;

        // Part-way into the bar, as a key press always is.
        cycle(&mut s, l.instrument_port, 0.0, 24);
        let started_at = s.loop_(l.sync_loop).expect("sync").position();
        assert!(
            started_at > 0,
            "the bar had not moved, so nothing is proven"
        );

        // Record, exactly as the button does.
        s.loop_mut(target)
            .expect("loop")
            .plan_transition(LoopMode::Replacing, None, Some(0));
        assert_eq!(
            s.loop_(target).expect("loop").mode(),
            LoopMode::Replacing,
            "recording did not start until some later boundary"
        );

        cycle(&mut s, l.instrument_port, 0.5, 16);

        let c = contents(&s, target);
        assert!(
            c.iter().any(|v| *v != 0.0),
            "nothing was captured by a take started mid-bar"
        );

        // And it landed where the bar was, not at the start of the loop -- which is what keeps it in
        // time with everything else.
        let at_start = c[..started_at as usize]
            .iter()
            .fold(0.0f32, |a, b| a.max(b.abs()));
        assert_eq!(
            at_start, 0.0,
            "the take was written from frame 0 instead of the bar's position"
        );
    }

    /// Stopping a take keeps it, and plays it when asked to.
    #[test]
    fn stopping_a_take_keeps_it() {
        let (mut s, l, _out) = rig(64);
        let target = l.loops[0].loop_idx;

        s.loop_mut(target)
            .expect("loop")
            .plan_transition(LoopMode::Replacing, None, Some(0));
        cycle(&mut s, l.instrument_port, 0.5, 20);

        // What the stop button sends with `play after record` on.
        s.loop_mut(target)
            .expect("loop")
            .clear_planned_transitions();
        s.set_loop_mode(target, LoopMode::Playing).expect("mode");

        assert!(
            contents(&s, target).iter().any(|v| *v != 0.0),
            "stopping threw the take away"
        );
        assert_eq!(s.loop_(target).expect("loop").mode(), LoopMode::Playing);
    }

    /// The metronome is heard but never recorded.
    ///
    /// It used to share the instrument's port, which is the port loops record from, so every take
    /// captured the click. Two assertions, because either one alone would pass a broken wiring: it
    /// has to reach the device *and* stay out of the loops.
    #[test]
    fn the_metronome_reaches_the_device_but_not_a_loop() {
        let (mut s, l, out) = rig(64);
        let target = l.loops[0].loop_idx;

        assert_ne!(
            l.click_port, l.instrument_port,
            "the click shares the instrument's port, so takes will capture it"
        );

        // A loop recording, and a click sounding, in the same cycle.
        s.loop_mut(target)
            .expect("loop")
            .plan_transition(LoopMode::Replacing, None, Some(0));
        if let Some(p) = s.port_mut(l.click_port).and_then(Port::as_external_mut) {
            p.stage_input(&vec![0.5f32; 64]);
        }
        s.process(64).expect("cycle");

        let device_peak = s
            .port(out)
            .and_then(|p| p.audio())
            .map(|a| a.output_peak())
            .expect("output port");
        assert!(device_peak > 0.0, "the click never reached the device");

        assert_eq!(
            contents(&s, target)
                .iter()
                .fold(0.0f32, |a, b| a.max(b.abs())),
            0.0,
            "the take captured the metronome"
        );
    }

    /// Two loops written in different passes line up, which is the point of the whole model.
    #[test]
    fn two_loops_written_a_bar_apart_share_the_bar_position() {
        let (mut s, l, _out) = rig(64);
        let (a, b) = (l.loops[0].loop_idx, l.loops[1].loop_idx);

        // First bar into A.
        s.set_loop_mode(a, LoopMode::Replacing).expect("mode");
        cycle(&mut s, l.instrument_port, 0.5, 64);
        s.set_loop_mode(a, LoopMode::Playing).expect("mode");

        // Second bar into B, while A plays back.
        s.set_loop_mode(b, LoopMode::Replacing).expect("mode");
        cycle(&mut s, l.instrument_port, 0.25, 64);
        s.set_loop_mode(b, LoopMode::Playing).expect("mode");

        // Same length and, once both are playing, the same position every cycle.
        for _ in 0..3 {
            cycle(&mut s, l.instrument_port, 0.0, 64);
            let pa = s.loop_(a).expect("loop").position();
            let pb = s.loop_(b).expect("loop").position();
            assert_eq!(pa, pb, "the two loops drifted apart");
        }
    }
}
