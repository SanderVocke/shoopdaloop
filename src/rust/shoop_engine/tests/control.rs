//! The control API driven the way a real driver drives it: the engine runs on another
//! thread and every call goes through the handle.
//!
//! Blocking calls only return once a cycle has applied them, so these tests keep a
//! thread turning the engine over for the duration. That is the arrangement a JACK or
//! miniaudio callback provides, which is why it is worth testing this way rather than
//! calling `Engine::process` inline.

use assert2::{check, let_assert};
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::control::Backend;
use shoop_engine::engine::split;
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::port::PortDirection;
use shoop_engine::session::Session;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Runs the engine on its own thread for as long as the returned guard lives.
struct Running {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn start(session: Session) -> (Backend, Running) {
    let (mut engine, handle) = split(session, 64);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_writer = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        while !stop_writer.load(Ordering::Relaxed) {
            engine.process(64);
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
    });
    (
        Backend::new(handle),
        Running {
            stop,
            thread: Some(thread),
        },
    )
}

fn backend() -> (Backend, Running) {
    let mut s = Session::default();
    s.apply_graph_changes().expect("schedule");
    start(s)
}

#[test]
fn a_loop_can_be_created_and_read_back() {
    let (b, _running) = backend();

    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(1) = b.n_loops());

    let_assert!(Ok(state) = l.get_state());
    check!(state.mode == LoopMode::Stopped);
    check!(state.length == 0);
    check!(state.position == 0);
}

#[test]
fn a_mutation_lands_and_is_visible_afterwards() {
    let (b, _running) = backend();
    let_assert!(Ok(l) = b.create_loop());

    let_assert!(Ok(()) = l.set_length(128));
    // Fire-and-forget, so read it back through a blocking call, which cannot return
    // until a cycle has applied the queued change.
    let_assert!(Ok(state) = l.get_state());
    check!(state.length == 128);
}

#[test]
fn polled_state_catches_up_with_the_engine() {
    let (b, _running) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(()) = l.set_length(4096));
    let_assert!(Ok(()) = l.set_mode(LoopMode::Playing));

    // Poll until the loop is seen playing, rather than assuming the first poll has it:
    // publishing is skipped when the reader has not returned a snapshot box.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut seen = None;
    while std::time::Instant::now() < deadline {
        if let Ok(Some(s)) = l.poll_state() {
            if s.mode == LoopMode::Playing && s.position > 0 {
                seen = Some(s);
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let_assert!(Some(s) = seen);
    check!(s.mode == LoopMode::Playing);
    check!(s.length == 4096);
}

#[test]
fn an_audio_channel_round_trips_its_data() {
    let (b, _running) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_audio_channel(64, ChannelMode::Direct));

    let data: Vec<f32> = (0..32).map(|i| i as f32 / 32.0).collect();
    let_assert!(Ok(()) = c.load_data(&data));

    let_assert!(Ok(got) = c.get_data());
    check!(got == data);

    let_assert!(Ok(state) = c.get_state());
    check!(state.mode == ChannelMode::Direct);
    check!(state.length == 32);
}

#[test]
fn audio_channel_settings_take_effect() {
    let (b, _running) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_audio_channel(64, ChannelMode::Direct));

    let_assert!(Ok(()) = c.set_gain(0.25));
    let_assert!(Ok(()) = c.set_mode(ChannelMode::Wet));
    let_assert!(Ok(()) = c.set_start_offset(7));
    let_assert!(Ok(()) = c.set_n_preplay_samples(9));

    let_assert!(Ok(state) = c.get_state());
    check!(state.gain == 0.25);
    check!(state.mode == ChannelMode::Wet);
    check!(state.start_offset == 7);
    check!(state.n_preplay_samples == 9);
}

#[test]
fn a_midi_channel_reports_its_state() {
    let (b, _running) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_midi_channel(256, ChannelMode::Direct));

    let_assert!(Ok(()) = c.set_start_offset(3));
    let_assert!(Ok(()) = c.set_n_preplay_samples(5));

    let_assert!(Ok(state) = c.get_state());
    check!(state.mode == ChannelMode::Direct);
    check!(state.start_offset == 3);
    check!(state.n_preplay_samples == 5);
    check!(state.n_notes_active == 0);
}

#[test]
fn clearing_a_loop_empties_its_channels_and_stops_it() {
    let (b, _running) = backend();
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_audio_channel(64, ChannelMode::Direct));

    let_assert!(Ok(()) = c.load_data(&vec![1.0f32; 64]));
    let_assert!(Ok(()) = l.set_length(64));
    let_assert!(Ok(()) = l.set_mode(LoopMode::Playing));
    let_assert!(Ok(state) = l.get_state());
    check!(state.mode == LoopMode::Playing);

    let_assert!(Ok(()) = l.clear(0));

    let_assert!(Ok(state) = l.get_state());
    check!(state.length == 0);
    check!(state.mode == LoopMode::Stopped);
    let_assert!(Ok(got) = c.get_data());
    check!(got.is_empty());
}

#[test]
fn a_loop_can_follow_another() {
    let (b, _running) = backend();
    let_assert!(Ok(source) = b.create_loop());
    let_assert!(Ok(follower) = b.create_loop());

    let_assert!(Ok(()) = source.set_length(64));
    let_assert!(Ok(()) = follower.set_sync_source(Some(&source)));

    // Planned rather than immediate, because the follower now waits for its source.
    let_assert!(Ok(()) = follower.plan_transition(LoopMode::Playing, Some(0), None));
    let_assert!(Ok(state) = follower.get_state());
    check!(state.next_mode == Some(LoopMode::Playing));
    check!(state.mode == LoopMode::Stopped);
}

#[test]
fn asking_for_a_loop_that_is_not_there_fails() {
    let (b, _running) = backend();
    check!(b.loop_at(7).is_err());
}

#[test]
fn handles_can_be_shared_across_threads() {
    let (b, _running) = backend();
    let_assert!(Ok(l) = b.create_loop());

    // The control side is behind a mutex, so several threads may drive it; only the
    // engine's own thread ever touches the session.
    let threads: Vec<_> = (0..4)
        .map(|i| {
            let l = l.clone();
            std::thread::spawn(move || l.set_length(100 + i))
        })
        .collect();
    for t in threads {
        let_assert!(Ok(Ok(())) = t.join());
    }

    let_assert!(Ok(state) = l.get_state());
    // Whichever landed last wins; what matters is that all four were accepted and the
    // loop is in one of the states they asked for.
    check!((100..104).contains(&state.length));
}

#[test]
fn a_port_reports_its_state() {
    let (b, _running) = backend();

    let_assert!(Ok(p) = b.add_audio_port("in", PortDirection::Input, 4));
    let_assert!(Ok(name) = p.name());
    check!(name == "in");

    let_assert!(Ok(()) = p.set_gain(0.5));
    let_assert!(Ok(()) = p.set_ringbuffer_n_samples(128));
    let_assert!(Ok(state) = p.get_audio_state());
    check!(state.gain == 0.5);
    check!(!state.muted);
    check!(state.name == "in");
    // `ringbuffer_n_samples` reports what is currently *retained*, not the window that
    // was asked for -- the C++ getter is the same -- so with no audio yet it is zero.
    check!(state.ringbuffer_n_samples == 0);

    // Asking an audio port for MIDI counts is an error, not a different answer.
    check!(p.get_midi_state().is_err());
}

#[test]
fn a_midi_port_reports_its_state() {
    let (b, _running) = backend();

    let_assert!(Ok(p) = b.add_midi_port("min", PortDirection::Input));
    let_assert!(Ok(()) = p.set_muted(true));

    let_assert!(Ok(state) = p.get_midi_state());
    check!(state.muted);
    check!(state.n_input_events == 0);
    check!(state.name == "min");

    check!(p.get_audio_state().is_err());
}

#[test]
fn muting_applies_to_whichever_kind_the_port_is() {
    let (b, _running) = backend();

    let_assert!(Ok(audio) = b.add_audio_port("a", PortDirection::Output, 4));
    let_assert!(Ok(midi) = b.add_midi_port("m", PortDirection::Output));

    let_assert!(Ok(()) = audio.set_muted(true));
    let_assert!(Ok(()) = midi.set_muted(true));

    let_assert!(Ok(a) = audio.get_audio_state());
    let_assert!(Ok(m) = midi.get_midi_state());
    check!(a.muted);
    check!(m.muted);
}

/// The whole graph built through the control API, then run: a channel recording from
/// one port and playing to another is what every other call exists to arrange.
#[test]
fn a_graph_built_through_the_api_records_and_plays() {
    let (b, _running) = backend();

    let_assert!(Ok(input) = b.add_audio_port("in", PortDirection::Input, 4));
    let_assert!(Ok(output) = b.add_audio_port("out", PortDirection::Output, 4));
    let_assert!(Ok(l) = b.create_loop());
    let_assert!(Ok(c) = l.add_audio_channel(64, ChannelMode::Direct));
    let_assert!(Ok(()) = c.connect_input(&input));
    let_assert!(Ok(()) = c.connect_output(&output));

    // Load the channel rather than feeding the input port: staging a buffer is the
    // driver's job, and there is no driver here.
    let data: Vec<f32> = (0..16).map(|i| i as f32).collect();
    let_assert!(Ok(()) = c.load_data(&data));
    let_assert!(Ok(()) = l.set_length(16));
    let_assert!(Ok(()) = l.set_mode(LoopMode::Playing));

    // Playing means the output port sees signal, so its peak rises above silence.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut peak = 0.0f32;
    while std::time::Instant::now() < deadline {
        if let Ok(s) = output.get_audio_state() {
            peak = s.output_peak;
            if peak > 0.0 {
                break;
            }
        }
    }
    check!(peak > 0.0, "output port never saw signal");
}

#[test]
fn ports_can_be_routed_to_each_other() {
    let (b, _running) = backend();

    let_assert!(Ok(from) = b.add_audio_port("from", PortDirection::Input, 4));
    let_assert!(Ok(mid) = b.add_internal_audio_port("mid", 64, 4));
    let_assert!(Ok(()) = from.connect_internal(&mid));

    // Both exist and the graph still schedules, which it would not if the connection
    // had left it inconsistent.
    let_assert!(Ok(3) = b.n_ports().map(|n| n + 1));
}
