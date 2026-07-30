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
    AudioDriver, AudioDriverSettings, AudioPort, BackendSession, JackAudioDriverSettings,
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
    channel.connect_input(&input);
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
    channel.connect_output(&output);

    let n = buffer_size * 8;
    channel.load_data(&vec![0.5; n]);
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
    channel.connect_output(&output);
    let n = buffer_size * 8;
    channel.load_data(&vec![0.5; n]);
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
    channel2.connect_output(&output);

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
