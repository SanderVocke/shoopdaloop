//! The control API driven the way the application drives it: a real driver cycling the
//! engine on its own thread, and every call going through the command queue.
//!
//! These began life against `control.rs`, a second handle API that nothing outside these
//! tests ever used. They now exercise `app_backend`'s handles -- the ones the frontend holds
//! -- through a dummy `AudioDriver`, so what they assert is the path the application takes.
//! That is the whole point: the previous arrangement is how a JACK output path that produced
//! pure silence sat green in CI.
//!
//! The driver runs in automatic mode, so cycles arrive continuously without anything here
//! asking for them. That is what makes the blocking calls return and the published snapshots
//! advance, exactly as a JACK or CPAL callback would.

#![cfg(feature = "app_backend")]

use assert2::{check, let_assert};
use shoop_engine::app_backend::{
    AudioDriver, AudioDriverSettings, AudioPort, BackendSession, DummyAudioDriverSettings, MidiPort,
};
use shoop_engine::{AudioDriverType, ChannelMode, LoopMode, MidiEvent, PortDirection};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

/// Unique per session, so concurrently running tests cannot collide on a client name.
static NEXT: AtomicU32 = AtomicU32::new(0);

// Application-backend driver services are process-global. Serialize these integration tests while
// the rest of the workspace remains free to run in parallel.
static DRIVER_TEST_LOCK: Mutex<()> = Mutex::new(());

/// A running dummy driver with a session attached.
///
/// The driver is returned and must be kept alive: dropping it stops the thread that cycles
/// the engine, after which nothing would answer a blocking call.
fn backend() -> (MutexGuard<'static, ()>, AudioDriver, BackendSession) {
    let exclusive = DRIVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("create driver");
    driver
        .start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
            client_name: format!("control-test-{n}"),
            sample_rate: 48_000,
            buffer_size: 64,
        }))
        .expect("start driver");
    let session = BackendSession::new().expect("session");
    session.set_audio_driver(&driver).expect("attach driver");
    (exclusive, driver, session)
}

/// Polls until `f` yields a value or the deadline passes.
///
/// Published state is a cycle or two behind by design, so a test reading it has to allow for
/// that rather than assume the first look succeeds.
fn eventually<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Some(v) = f() {
            return Some(v);
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    f()
}

#[shoop_wasm_test_support::shoop_test]
fn a_loop_can_be_created_and_read_back() {
    let (_exclusive, _driver, b) = backend();

    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Some(state) = eventually(|| l.get_state().ok()));
    check!(state.mode == LoopMode::Stopped);
    check!(state.length == 0);
    check!(state.position == 0);
}

/// Exact read-after-write is available through an explicit command fence.
#[shoop_wasm_test_support::shoop_test]
fn an_explicit_fence_makes_a_mutation_visible() {
    let (_exclusive, _driver, b) = backend();
    let_assert!(Ok(l) = b.create_loop());

    let_assert!(Ok(sequence) = l.set_length(128));
    b.wait_for_command(sequence, std::time::Duration::from_secs(1))
        .expect("length command");
    let_assert!(Ok(state) = l.get_state());
    check!(state.length == 128);
}

/// The frame-rate path: state the audio thread published, read without blocking.
#[shoop_wasm_test_support::shoop_test]
fn polled_state_catches_up_with_the_engine() {
    let (_exclusive, _driver, b) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(_) = l.set_length(4096));
    let_assert!(Ok(_) = l.transition(LoopMode::Playing, -1, -1));

    let_assert!(
        Some(s) = eventually(|| {
            l.poll_state()
                .filter(|s| s.mode == LoopMode::Playing && s.position > 0)
        })
    );
    check!(s.mode == LoopMode::Playing);
    check!(s.length == 4096);
}

#[shoop_wasm_test_support::shoop_test]
fn an_audio_channel_round_trips_its_data() {
    let (_exclusive, _driver, b) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_audio_channel(ChannelMode::Direct));

    let data: Vec<f32> = (0..32).map(|i| i as f32 / 32.0).collect();
    let sequence = c.load_data(&data).expect("queue data");
    b.wait_for_command(sequence, std::time::Duration::from_secs(1))
        .expect("data command");

    check!(c.get_data() == data);

    let_assert!(Ok(state) = c.get_state());
    check!(state.mode == ChannelMode::Direct);
    check!(state.length == 32);
}

#[shoop_wasm_test_support::shoop_test]
fn audio_channel_settings_take_effect() {
    let (_exclusive, _driver, b) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_audio_channel(ChannelMode::Direct));

    c.set_gain(0.25).expect("queue gain");
    c.set_mode(ChannelMode::Wet).expect("queue mode");
    c.set_start_offset(7).expect("queue offset");
    let sequence = c.set_n_preplay_samples(9).expect("queue preplay");
    b.wait_for_command(sequence, std::time::Duration::from_secs(1))
        .expect("settings commands");

    let_assert!(Ok(state) = c.get_state());
    check!(state.gain == 0.25);
    check!(state.mode == ChannelMode::Wet);
    check!(state.start_offset == 7);
    check!(state.n_preplay_samples == 9);
}

#[shoop_wasm_test_support::shoop_test]
fn a_midi_channel_reports_its_state() {
    let (_exclusive, _driver, b) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_midi_channel(ChannelMode::Direct));

    c.set_start_offset(3).expect("queue offset");
    let sequence = c.set_n_preplay_samples(5).expect("queue preplay");
    b.wait_for_command(sequence, std::time::Duration::from_secs(1))
        .expect("settings commands");

    let_assert!(Ok(state) = c.get_state());
    check!(state.mode == ChannelMode::Direct);
    check!(state.start_offset == 3);
    check!(state.n_preplay_samples == 5);
    check!(state.n_notes_active == 0);
}

#[shoop_wasm_test_support::shoop_test]
fn clearing_a_loop_empties_its_channels_and_stops_it() {
    let (_exclusive, _driver, b) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_audio_channel(ChannelMode::Direct));

    c.load_data(&vec![1.0f32; 64]).expect("queue data");
    let_assert!(Ok(_) = l.set_length(64));
    let_assert!(Ok(sequence) = l.transition(LoopMode::Playing, -1, -1));
    b.wait_for_command(sequence, std::time::Duration::from_secs(1))
        .expect("playing command");
    let_assert!(Ok(state) = l.get_state());
    check!(state.mode == LoopMode::Playing);

    let_assert!(Ok(sequence) = l.clear(0));
    b.wait_for_command(sequence, std::time::Duration::from_secs(1))
        .expect("clear command");

    let_assert!(Ok(state) = l.get_state());
    check!(state.length == 0);
    check!(state.mode == LoopMode::Stopped);
    check!(c.get_data().is_empty());
}

#[shoop_wasm_test_support::shoop_test]
fn a_loop_can_follow_another() {
    let (_exclusive, _driver, b) = backend();
    let_assert!(Ok(source) = b.create_loop());
    let_assert!(Ok(follower) = b.create_loop());

    let_assert!(Ok(_) = source.set_length(64));
    let_assert!(Ok(_) = follower.set_sync_source(Some(&source)));

    // Planned rather than immediate, because the follower now waits for its source.
    let_assert!(Ok(sequence) = follower.transition(LoopMode::Playing, 0, -1));
    b.wait_for_command(sequence, std::time::Duration::from_secs(1))
        .expect("transition command");
    let_assert!(Ok(state) = follower.get_state());
    check!(state.maybe_next_mode == Some(LoopMode::Playing));
    check!(state.mode == LoopMode::Stopped);
}

/// Several threads may drive the control side at once.
///
/// The handle is behind a mutex that no audio thread ever waits on, and the session itself is
/// only ever touched by the engine's owner, so this is safe for a reason it was not before:
/// the threads are contending for the queue, not for the session.
#[shoop_wasm_test_support::shoop_test]
fn handles_can_be_shared_across_threads() {
    let (_exclusive, _driver, b) = backend();
    let_assert!(Ok(l) = b.create_loop());

    let threads: Vec<_> = (0..4)
        .map(|i| {
            let l = l.clone();
            std::thread::spawn(move || l.set_length(100 + i))
        })
        .collect();
    let mut sequences = Vec::new();
    for t in threads {
        let_assert!(Ok(Ok(sequence)) = t.join());
        sequences.push(sequence);
    }
    let sequence = sequences.into_iter().max().expect("commands");
    b.wait_for_command(sequence, std::time::Duration::from_secs(1))
        .expect("all length commands");

    let_assert!(Ok(state) = l.get_state());
    // Whichever landed last wins; what matters is that all four were accepted and the loop
    // is in one of the states they asked for.
    check!((100..104).contains(&state.length));
}

#[shoop_wasm_test_support::shoop_test]
fn a_port_reports_its_state() {
    let (_exclusive, driver, b) = backend();

    let_assert!(Ok(p) = AudioPort::new_driver_port(&b, &driver, "in", &PortDirection::Input, 4));

    p.set_gain(0.5).expect("queue gain");
    p.set_ringbuffer_n_samples(128).expect("queue ring size");
    let_assert!(Ok(state) = p.get_state());
    check!(state.gain == 0.5);
    check!(!state.muted);
    // The name comes from this side, not from the audio thread, which cannot publish a
    // `String` -- so it is worth asserting it survives the round trip.
    check!(state.name == "in");
    // Accepted scalar intent is visible immediately, without waiting for the audio thread.
    check!(state.ringbuffer_n_samples == 128);
}

#[shoop_wasm_test_support::shoop_test]
fn a_midi_port_reports_its_state() {
    let (_exclusive, driver, b) = backend();

    let_assert!(Ok(p) = MidiPort::new_driver_port(&b, &driver, "min", &PortDirection::Input, 0));
    p.set_muted(true).expect("queue mute");

    let_assert!(Ok(state) = p.get_state());
    check!(state.muted);
    check!(state.n_input_events == 0);
    check!(state.name == "min");

    p.dummy_queue_msgs(vec![
        MidiEvent::new(0, vec![0xc1, 7]),
        MidiEvent::new(1, vec![0xb3, 19, 88]),
    ])
    .expect("queue MIDI input");
    let_assert!(
        Some(message) = eventually(|| {
            p.poll_state()
                .and_then(|state| state.latest_input_message)
                .filter(|message| message.data() == [0xb3, 19, 88])
        })
    );
    check!(message.data() == [0xb3, 19, 88]);
    check!(p.get_state().unwrap().latest_input_message.unwrap().data() == [0xb3, 19, 88]);
}

#[shoop_wasm_test_support::shoop_test]
fn muting_applies_to_whichever_kind_the_port_is() {
    let (_exclusive, driver, b) = backend();

    let_assert!(
        Ok(audio) = AudioPort::new_driver_port(&b, &driver, "a", &PortDirection::Output, 4)
    );
    let_assert!(Ok(midi) = MidiPort::new_driver_port(&b, &driver, "m", &PortDirection::Output, 0));

    audio.set_muted(true).expect("queue audio mute");
    midi.set_muted(true).expect("queue MIDI mute");

    let_assert!(Ok(a) = audio.get_state());
    let_assert!(Ok(m) = midi.get_state());
    check!(a.muted);
    check!(m.muted);
}

/// The whole graph built through the API, then run: a channel playing to a port is what every
/// other call exists to arrange.
#[shoop_wasm_test_support::shoop_test]
fn a_graph_built_through_the_api_records_and_plays() {
    let (_exclusive, driver, b) = backend();

    let_assert!(
        Ok(input) = AudioPort::new_driver_port(&b, &driver, "in", &PortDirection::Input, 4)
    );
    let_assert!(
        Ok(output) = AudioPort::new_driver_port(&b, &driver, "out", &PortDirection::Output, 4)
    );
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_audio_channel(ChannelMode::Direct));
    c.connect_input(&input).expect("queue input connection");
    c.connect_output(&output).expect("queue output connection");

    // Load the channel rather than feeding the input port: staging a buffer is the driver's
    // job, and the dummy driver stages nothing.
    let data: Vec<f32> = (0..16).map(|i| i as f32).collect();
    c.load_data(&data).expect("queue data");
    let_assert!(Ok(_) = l.set_length(16));
    let_assert!(Ok(_) = l.transition(LoopMode::Playing, -1, -1));

    // Playing means the output port sees signal, so its peak rises above silence. This is the
    // assertion that fails if the graph is left stale and never rebuilt.
    let_assert!(
        Some(peak) = eventually(|| {
            output
                .get_state()
                .ok()
                .map(|s| s.output_peak)
                .filter(|p| *p > 0.0)
        })
    );
    check!(peak > 0.0);
}

#[shoop_wasm_test_support::shoop_test]
fn ports_can_be_routed_to_each_other() {
    let (_exclusive, driver, b) = backend();

    let_assert!(
        Ok(from) = AudioPort::new_driver_port(&b, &driver, "from", &PortDirection::Input, 4)
    );
    let_assert!(Ok(to) = AudioPort::new_driver_port(&b, &driver, "to", &PortDirection::Output, 4));
    from.connect_internal(&to)
        .expect("queue internal connection");

    // Both still report, which they would not if the connection had left the graph in a state
    // that refused to schedule.
    driver.wait_process();
    let_assert!(Ok(_) = from.get_state());
    let_assert!(Ok(_) = to.get_state());
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
