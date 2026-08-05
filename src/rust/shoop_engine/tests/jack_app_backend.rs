//! Real-JACK integration tests against the driver the application actually runs.
//!
//! These began life in `jack_driver.rs`, a second JACK implementation that nothing outside
//! its own tests ever used. They tested a callback the app never called, so a JACK output
//! path that produced pure silence sat green in CI. They are here now, driving
//! `app_backend`'s `JackProcess` through the same `AudioDriver` / `BackendSession` /
//! `AudioPort` handles the frontend uses.
//!
//! They need a running JACK server. Absent one they fail, rather than returning early and
//! reporting a pass -- set `SHOOP_ALLOW_MISSING_BACKENDS=1` to downgrade that to a skip on
//! a machine without JACK.

#![cfg(all(feature = "jack", feature = "app_backend"))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shoop_engine::app_backend::{
    AudioDriver, AudioDriverSettings, AudioPort, BackendSession, JackAudioDriverSettings, MidiPort,
};
use shoop_engine::{AudioDriverType, ChannelMode, LoopMode, PortDirection};

mod backend_availability;
use backend_availability::require_backend;

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_millis(2000);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    done()
}

/// A raw JACK client used as the other end of the connection under test.
fn peer_client(name: &str) -> Option<jack::Client> {
    match jack::Client::new(name, jack::ClientOptions::NO_START_SERVER) {
        Ok((client, _status)) => Some(client),
        Err(e) => {
            require_backend("JACK", &format!("could not open JACK client: {e}"));
            None
        }
    }
}

/// Starts the application's JACK driver and an attached session.
fn app_jack(client_name: &str) -> Option<(AudioDriver, BackendSession)> {
    let driver = AudioDriver::new(AudioDriverType::Jack, None).expect("create driver");
    let settings = AudioDriverSettings::Jack(JackAudioDriverSettings {
        client_name_hint: client_name.to_string(),
        maybe_server_name: None,
    });
    if let Err(e) = driver.start(&settings) {
        require_backend("JACK", &format!("could not start JACK driver: {e}"));
        return None;
    }
    let session = BackendSession::new().expect("session");
    session.set_audio_driver(&driver).expect("attach driver");
    Some((driver, session))
}

struct AudioProducer {
    port: jack::Port<jack::AudioOut>,
    frames: Arc<AtomicUsize>,
}
impl jack::ProcessHandler for AudioProducer {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let start = self
            .frames
            .fetch_add(ps.n_frames() as usize, Ordering::Relaxed);
        for (idx, sample) in self.port.as_mut_slice(ps).iter_mut().enumerate() {
            *sample = (start + idx + 1) as f32;
        }
        jack::Control::Continue
    }
}

struct AudioConsumer {
    port: jack::Port<jack::AudioIn>,
    captured: Arc<Mutex<Vec<f32>>>,
}
impl jack::ProcessHandler for AudioConsumer {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        self.captured
            .lock()
            .unwrap()
            .extend_from_slice(self.port.as_slice(ps));
        jack::Control::Continue
    }
}

struct ExternalAudioProcessor {
    source: jack::Port<jack::AudioOut>,
    dry_send: jack::Port<jack::AudioIn>,
    wet_return: jack::Port<jack::AudioOut>,
    wet_output: jack::Port<jack::AudioIn>,
    captured: Arc<Mutex<Vec<f32>>>,
}
impl jack::ProcessHandler for ExternalAudioProcessor {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        self.source.as_mut_slice(ps).fill(1.0);
        let processed = self
            .dry_send
            .as_slice(ps)
            .iter()
            .map(|sample| sample * 2.0)
            .collect::<Vec<_>>();
        self.wet_return.as_mut_slice(ps).copy_from_slice(&processed);
        self.captured
            .lock()
            .unwrap()
            .extend_from_slice(self.wet_output.as_slice(ps));
        jack::Control::Continue
    }
}

struct MidiFanoutPeer {
    source: jack::Port<jack::MidiOut>,
    monitored: jack::Port<jack::MidiIn>,
    muted: jack::Port<jack::MidiIn>,
    monitored_events: Arc<Mutex<Vec<Vec<u8>>>>,
    muted_events: Arc<Mutex<Vec<Vec<u8>>>>,
}
impl jack::ProcessHandler for MidiFanoutPeer {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let mut writer = self.source.writer(ps);
        let _ = writer.write(&jack::RawMidi {
            time: 0,
            bytes: &[0x90, 72, 100],
        });
        let _ = writer.write(&jack::RawMidi {
            time: 1.min(ps.n_frames().saturating_sub(1)),
            bytes: &[0x80, 72, 0],
        });
        self.monitored_events
            .lock()
            .unwrap()
            .extend(self.monitored.iter(ps).map(|event| event.bytes.to_vec()));
        self.muted_events
            .lock()
            .unwrap()
            .extend(self.muted.iter(ps).map(|event| event.bytes.to_vec()));
        jack::Control::Continue
    }
}

#[test]
fn registered_ports_are_visible_to_jack_with_direction_flags() {
    let suffix = std::process::id();
    let name = format!("shoop-app-flags-{suffix}");
    let Some((driver, session)) = app_jack(&name) else {
        return;
    };
    let actual = driver.get_state().maybe_instance_name;

    let _ain = AudioPort::new_driver_port(&session, &driver, "audio_in", &PortDirection::Input, 0)
        .unwrap();
    let _aout =
        AudioPort::new_driver_port(&session, &driver, "audio_out", &PortDirection::Output, 0)
            .unwrap();

    let Some(peer) = peer_client(&format!("shoop-app-flags-peer-{suffix}")) else {
        return;
    };
    assert_eq!(
        peer.ports(
            Some(&format!("{actual}:audio_in")),
            None,
            jack::PortFlags::IS_INPUT
        ),
        vec![format!("{actual}:audio_in")]
    );
    assert_eq!(
        peer.ports(
            Some(&format!("{actual}:audio_out")),
            None,
            jack::PortFlags::IS_OUTPUT
        ),
        vec![format!("{actual}:audio_out")]
    );
}

#[test]
fn jack_audio_input_reaches_a_recording_channel() {
    let suffix = std::process::id();
    let Some(producer_client) = peer_client(&format!("shoop-app-producer-{suffix}")) else {
        return;
    };
    let producer_port = producer_client
        .register_port("out", jack::AudioOut::default())
        .expect("producer output");
    let producer_name = producer_port.name().expect("producer port name");
    let produced_frames = Arc::new(AtomicUsize::new(0));
    let producer = producer_client
        .activate_async(
            (),
            AudioProducer {
                port: producer_port,
                frames: produced_frames.clone(),
            },
        )
        .expect("activate producer");

    let Some((driver, session)) = app_jack(&format!("shoop-app-reader-{suffix}")) else {
        return;
    };
    let buffer_size = driver.get_state().buffer_size;
    let input = AudioPort::new_driver_port(
        &session,
        &driver,
        "in",
        &PortDirection::Input,
        buffer_size.max(1),
    )
    .expect("input port");
    let loop_ = session.create_loop().expect("loop");
    let channel = loop_
        .add_audio_channel(ChannelMode::Direct)
        .expect("channel");
    channel
        .connect_input(&input)
        .expect("queue input connection");
    loop_
        .transition(LoopMode::Recording, -1, -1)
        .expect("recording");
    driver.wait_process();

    input.connect_external_port(&producer_name);

    assert!(wait_until(|| produced_frames.load(Ordering::Relaxed) > 0));
    assert!(
        wait_until(|| channel.get_data().iter().any(|s| *s > 0.0)),
        "JACK input should have reached the recording channel"
    );
    let _ = producer.deactivate();
}

/// The one that matters: this is the path that was silently producing nothing.
#[test]
fn session_output_reaches_a_jack_consumer() {
    let suffix = std::process::id();
    let Some(consumer_client) = peer_client(&format!("shoop-app-consumer-{suffix}")) else {
        return;
    };
    let consumer_port = consumer_client
        .register_port("in", jack::AudioIn::default())
        .expect("consumer input");
    let consumer_name = consumer_port.name().expect("consumer port name");
    let captured = Arc::new(Mutex::new(Vec::new()));
    let consumer = consumer_client
        .activate_async(
            (),
            AudioConsumer {
                port: consumer_port,
                captured: captured.clone(),
            },
        )
        .expect("activate consumer");

    let Some((driver, session)) = app_jack(&format!("shoop-app-writer-{suffix}")) else {
        return;
    };
    let buffer_size = driver.get_state().buffer_size.max(64) as usize;
    let output = AudioPort::new_driver_port(&session, &driver, "out", &PortDirection::Output, 0)
        .expect("output port");
    let loop_ = session.create_loop().expect("loop");
    let channel = loop_
        .add_audio_channel(ChannelMode::Direct)
        .expect("channel");
    channel
        .connect_output(&output)
        .expect("queue output connection");

    let n = buffer_size * 8;
    channel.load_data(&vec![0.5; n]).expect("queue data");
    loop_.set_length(n as u32).expect("length");
    loop_
        .transition(LoopMode::Playing, -1, -1)
        .expect("playing");
    driver.wait_process();

    output.connect_external_port(&consumer_name);

    assert!(
        wait_until(|| captured.lock().unwrap().iter().any(|s| *s == 0.5)),
        "JACK output should carry samples produced by the session"
    );
    let _ = consumer.deactivate();
}

/// A topology change mid-stream must not silence the session.
///
/// Before the graph work this was the actual failure: connecting a channel to a port left
/// the schedule stale, `Session::process` refused every subsequent cycle, and the session
/// went permanently quiet with nothing logged. Audio must survive a live rewire.
#[test]
fn audio_keeps_flowing_across_a_mid_stream_topology_change() {
    let suffix = std::process::id();
    let Some(consumer_client) = peer_client(&format!("shoop-app-rewire-peer-{suffix}")) else {
        return;
    };
    let consumer_port = consumer_client
        .register_port("in", jack::AudioIn::default())
        .expect("consumer input");
    let consumer_name = consumer_port.name().expect("consumer port name");
    let captured = Arc::new(Mutex::new(Vec::new()));
    let consumer = consumer_client
        .activate_async(
            (),
            AudioConsumer {
                port: consumer_port,
                captured: captured.clone(),
            },
        )
        .expect("activate consumer");

    let Some((driver, session)) = app_jack(&format!("shoop-app-rewire-{suffix}")) else {
        return;
    };
    let buffer_size = driver.get_state().buffer_size.max(64) as usize;
    let output = AudioPort::new_driver_port(&session, &driver, "out", &PortDirection::Output, 0)
        .expect("output port");
    let loop_ = session.create_loop().expect("loop");
    let channel = loop_
        .add_audio_channel(ChannelMode::Direct)
        .expect("channel");
    channel
        .connect_output(&output)
        .expect("queue output connection");
    let n = buffer_size * 8;
    channel.load_data(&vec![0.5; n]).expect("queue data");
    loop_.set_length(n as u32).expect("length");
    loop_
        .transition(LoopMode::Playing, -1, -1)
        .expect("playing");
    driver.wait_process();
    output.connect_external_port(&consumer_name);

    assert!(
        wait_until(|| captured.lock().unwrap().iter().any(|s| *s == 0.5)),
        "audio should be flowing before the rewire"
    );

    // Add a second loop and wire it up while the first one is playing. Every one of these
    // marks the graph stale.
    let loop2 = session.create_loop().expect("second loop");
    let channel2 = loop2
        .add_audio_channel(ChannelMode::Direct)
        .expect("second channel");
    channel2
        .connect_output(&output)
        .expect("queue second output connection");

    captured.lock().unwrap().clear();
    assert!(
        wait_until(|| captured.lock().unwrap().iter().any(|s| *s != 0.0)),
        "audio must keep flowing while the new schedule is built"
    );

    // And the rebuild must actually land, rather than leaving the session stale forever.
    driver.wait_process();
    assert_eq!(
        driver.get_state().stale_graph_cycles,
        {
            std::thread::sleep(Duration::from_millis(100));
            driver.get_state().stale_graph_cycles
        },
        "stale-cycle count must stop climbing once the graph has been applied"
    );

    let _ = consumer.deactivate();
}

// Purpose: Exercise the complete JACK dry-send to processor to wet-return audio graph.
// Use case: A user connects an external effects client and hears transformed input at wet out.
// Failure: Expected transformed sample 2.0; observed max 0 across 96,256 captured samples.
// The real JACK port-to-port passthrough graph may not propagate internal connections.
#[test]
fn external_dry_wet_audio_round_trip_reaches_jack_output() {
    let suffix = std::process::id();
    let Some(peer) = peer_client(&format!("shoop-dry-wet-peer-{suffix}")) else {
        return;
    };
    let source = peer
        .register_port("source", jack::AudioOut::default())
        .expect("source port");
    let dry_send = peer
        .register_port("dry_send", jack::AudioIn::default())
        .expect("dry send port");
    let wet_return = peer
        .register_port("wet_return", jack::AudioOut::default())
        .expect("wet return port");
    let wet_output = peer
        .register_port("wet_output", jack::AudioIn::default())
        .expect("wet output port");
    let source_name = source.name().expect("source name");
    let dry_send_name = dry_send.name().expect("dry send name");
    let wet_return_name = wet_return.name().expect("wet return name");
    let wet_output_name = wet_output.name().expect("wet output name");
    let captured = Arc::new(Mutex::new(Vec::new()));

    let Some((driver, session)) = app_jack(&format!("shoop-dry-wet-{suffix}")) else {
        return;
    };
    let ring = driver.get_state().buffer_size.max(1);
    let app_dry_input = AudioPort::new_driver_port(
        &session,
        &driver,
        "audio_dry_in",
        &PortDirection::Input,
        ring,
    )
    .expect("app dry input");
    let app_dry_send = AudioPort::new_driver_port(
        &session,
        &driver,
        "audio_dry_send",
        &PortDirection::Output,
        0,
    )
    .expect("app dry send");
    let app_wet_return = AudioPort::new_driver_port(
        &session,
        &driver,
        "audio_wet_return",
        &PortDirection::Input,
        ring,
    )
    .expect("app wet return");
    let app_wet_output = AudioPort::new_driver_port(
        &session,
        &driver,
        "audio_wet_out",
        &PortDirection::Output,
        0,
    )
    .expect("app wet output");
    app_dry_input
        .connect_internal(&app_dry_send)
        .expect("dry internal route");
    app_wet_return
        .connect_internal(&app_wet_output)
        .expect("wet internal route");
    driver.wait_process();
    std::thread::sleep(Duration::from_millis(100));
    driver.wait_process();

    let active = peer
        .activate_async(
            (),
            ExternalAudioProcessor {
                source,
                dry_send,
                wet_return,
                wet_output,
                captured: captured.clone(),
            },
        )
        .expect("activate external processor");

    app_dry_input.connect_external_port(&source_name);
    app_dry_send.connect_external_port(&dry_send_name);
    app_wet_return.connect_external_port(&wet_return_name);
    app_wet_output.connect_external_port(&wet_output_name);
    driver.wait_process();

    let received_processed =
        wait_until(|| captured.lock().unwrap().iter().any(|sample| *sample == 2.0));
    let observed = captured.lock().unwrap();
    let observed_max = observed.iter().copied().fold(0.0_f32, f32::max);
    let observed_len = observed.len();
    drop(observed);
    let _ = active.deactivate();
    assert!(
        received_processed,
        "expected transformed sample 2.0, observed max {observed_max} across {observed_len} samples"
    );
}

// Purpose: Verify one JACK MIDI source reaches only the external track with passthrough enabled.
// Use case: A shared controller feeds several external synth tracks but only the monitored one sounds.
// Failure: Expected monitored [[0x90,72,100],[0x80,72,0]] and muted []; observed both [].
// The real JACK MIDI input-to-send internal connection may not enter the session propagation graph.
#[test]
fn external_midi_fanout_respects_each_tracks_passthrough_mute() {
    let suffix = std::process::id();
    let Some(peer) = peer_client(&format!("shoop-midi-fanout-peer-{suffix}")) else {
        return;
    };
    let source = peer
        .register_port("source", jack::MidiOut::default())
        .expect("MIDI source port");
    let monitored = peer
        .register_port("monitored", jack::MidiIn::default())
        .expect("monitored sink port");
    let muted = peer
        .register_port("muted", jack::MidiIn::default())
        .expect("muted sink port");
    let source_name = source.name().expect("source name");
    let monitored_name = monitored.name().expect("monitored name");
    let muted_name = muted.name().expect("muted name");
    let monitored_events = Arc::new(Mutex::new(Vec::new()));
    let muted_events = Arc::new(Mutex::new(Vec::new()));

    let Some((driver, session)) = app_jack(&format!("shoop-midi-fanout-{suffix}")) else {
        return;
    };
    let ring = driver.get_state().buffer_size.max(1);
    let monitored_input = MidiPort::new_driver_port(
        &session,
        &driver,
        "monitored_dry_midi_in",
        &PortDirection::Input,
        ring,
    )
    .expect("monitored input");
    let monitored_send = MidiPort::new_driver_port(
        &session,
        &driver,
        "monitored_dry_midi_send",
        &PortDirection::Output,
        0,
    )
    .expect("monitored send");
    let muted_input = MidiPort::new_driver_port(
        &session,
        &driver,
        "muted_dry_midi_in",
        &PortDirection::Input,
        ring,
    )
    .expect("muted input");
    let muted_send = MidiPort::new_driver_port(
        &session,
        &driver,
        "muted_dry_midi_send",
        &PortDirection::Output,
        0,
    )
    .expect("muted send");
    monitored_input
        .connect_internal(&monitored_send)
        .expect("monitored internal route");
    muted_input
        .connect_internal(&muted_send)
        .expect("muted internal route");
    muted_input
        .set_passthrough_muted(true)
        .expect("mute second track passthrough");
    driver.wait_process();
    std::thread::sleep(Duration::from_millis(100));
    driver.wait_process();

    let active = peer
        .activate_async(
            (),
            MidiFanoutPeer {
                source,
                monitored,
                muted,
                monitored_events: monitored_events.clone(),
                muted_events: muted_events.clone(),
            },
        )
        .expect("activate MIDI fanout peer");

    monitored_input.connect_external_port(&source_name);
    muted_input.connect_external_port(&source_name);
    monitored_send.connect_external_port(&monitored_name);
    muted_send.connect_external_port(&muted_name);
    driver.wait_process();

    let received_monitored = wait_until(|| monitored_events.lock().unwrap().len() >= 2);
    std::thread::sleep(Duration::from_millis(100));
    let monitored = monitored_events.lock().unwrap().clone();
    let muted = muted_events.lock().unwrap().clone();
    let _ = active.deactivate();
    assert!(
        received_monitored,
        "expected monitored note-on/note-off, observed {monitored:?}"
    );
    assert!(
        muted.is_empty(),
        "passthrough-muted JACK track must emit no MIDI events; observed {muted:?}"
    );
    assert!(monitored.iter().any(|event| event == &[0x90, 72, 100]));
    assert!(monitored.iter().any(|event| event == &[0x80, 72, 0]));
}
