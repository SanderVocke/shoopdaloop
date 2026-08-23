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

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use shoop_engine::app_backend::{
    AudioDriver, AudioDriverSettings, AudioPort, BackendSession, JackAudioDriverSettings, MidiPort,
};
use shoop_engine::{AudioDriverType, ChannelMode, LoopMode, PortDirection};

mod backend_availability;
use backend_availability::require_backend;

fn jack_test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

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

struct ConstantAudioSource {
    port: jack::Port<jack::AudioOut>,
}
impl jack::ProcessHandler for ConstantAudioSource {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        self.port.as_mut_slice(ps).fill(1.0);
        jack::Control::Continue
    }
}

struct DryProcessorInput {
    port: jack::Port<jack::AudioIn>,
    sample_bits: Arc<AtomicU32>,
    seen: Arc<AtomicBool>,
}
impl jack::ProcessHandler for DryProcessorInput {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        if let Some(sample) = self
            .port
            .as_slice(ps)
            .iter()
            .copied()
            .find(|sample| *sample != 0.0)
        {
            self.sample_bits.store(sample.to_bits(), Ordering::Release);
            self.seen.store(true, Ordering::Release);
        }
        jack::Control::Continue
    }
}

struct WetProcessorOutput {
    port: jack::Port<jack::AudioOut>,
    sample_bits: Arc<AtomicU32>,
    dry_seen: Arc<AtomicBool>,
    produced: Arc<AtomicBool>,
}
impl jack::ProcessHandler for WetProcessorOutput {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let sample = if self.dry_seen.load(Ordering::Acquire) {
            f32::from_bits(self.sample_bits.load(Ordering::Acquire)) * 2.0
        } else {
            0.0
        };
        self.port.as_mut_slice(ps).fill(sample);
        if sample != 0.0 {
            self.produced.store(true, Ordering::Release);
        }
        jack::Control::Continue
    }
}

struct AtomicAudioConsumer {
    port: jack::Port<jack::AudioIn>,
    max_bits: Arc<AtomicU32>,
}
impl jack::ProcessHandler for AtomicAudioConsumer {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        for sample in self.port.as_slice(ps).iter().copied() {
            self.max_bits
                .fetch_max(sample.max(0.0).to_bits(), Ordering::Relaxed);
        }
        jack::Control::Continue
    }
}

const TIMING_PULSE: f32 = 0.812_345;
const UNSEEN_FRAME: u32 = u32::MAX;

struct TimedPulseSource {
    port: jack::Port<jack::AudioOut>,
    armed: Arc<AtomicBool>,
    sent_frame: Arc<AtomicU32>,
}
impl jack::ProcessHandler for TimedPulseSource {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let output = self.port.as_mut_slice(ps);
        output.fill(0.0);
        if self.armed.swap(false, Ordering::AcqRel) {
            let offset = 3.min(output.len().saturating_sub(1));
            output[offset] = TIMING_PULSE;
            self.sent_frame.store(
                ps.last_frame_time().wrapping_add(offset as u32),
                Ordering::Release,
            );
        }
        jack::Control::Continue
    }
}

struct CycleCopyProcessor {
    input: jack::Port<jack::AudioIn>,
    output: jack::Port<jack::AudioOut>,
}
impl jack::ProcessHandler for CycleCopyProcessor {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        self.output
            .as_mut_slice(ps)
            .copy_from_slice(self.input.as_slice(ps));
        jack::Control::Continue
    }
}

struct TimedPulseSink {
    port: jack::Port<jack::AudioIn>,
    received_frame: Arc<AtomicU32>,
}
impl jack::ProcessHandler for TimedPulseSink {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        if self.received_frame.load(Ordering::Acquire) != UNSEEN_FRAME {
            return jack::Control::Continue;
        }
        if let Some(offset) = self
            .port
            .as_slice(ps)
            .iter()
            .position(|sample| *sample == TIMING_PULSE)
        {
            let _ = self.received_frame.compare_exchange(
                UNSEEN_FRAME,
                ps.last_frame_time().wrapping_add(offset as u32),
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        jack::Control::Continue
    }
}

struct JackBufferSizeRestore<'a> {
    client: &'a jack::Client,
    buffer_size: u32,
}

impl Drop for JackBufferSizeRestore<'_> {
    fn drop(&mut self) {
        let _ = self.client.set_buffer_size(self.buffer_size);
    }
}

struct MidiSource {
    port: jack::Port<jack::MidiOut>,
}
impl jack::ProcessHandler for MidiSource {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let mut writer = self.port.writer(ps);
        let _ = writer.write(&jack::RawMidi {
            time: 0,
            bytes: &[0x90, 72, 100],
        });
        let _ = writer.write(&jack::RawMidi {
            time: 1.min(ps.n_frames().saturating_sub(1)),
            bytes: &[0x80, 72, 0],
        });
        jack::Control::Continue
    }
}

struct MidiSinks {
    monitored: jack::Port<jack::MidiIn>,
    muted: jack::Port<jack::MidiIn>,
    monitored_note_on: Arc<AtomicBool>,
    monitored_note_off: Arc<AtomicBool>,
    muted_events: Arc<AtomicUsize>,
}
impl jack::ProcessHandler for MidiSinks {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        for event in self.monitored.iter(ps) {
            if event.bytes == [0x90, 72, 100] {
                self.monitored_note_on.store(true, Ordering::Release);
            }
            if event.bytes == [0x80, 72, 0] {
                self.monitored_note_off.store(true, Ordering::Release);
            }
        }
        self.muted_events
            .fetch_add(self.muted.iter(ps).count(), Ordering::Relaxed);
        jack::Control::Continue
    }
}

struct DeclaredPeerLatency {
    source: *mut jack_sys::jack_port_t,
    sink: *mut jack_sys::jack_port_t,
}

struct DeclaredPeerLatencyProcess {
    source: jack::Port<jack::AudioOut>,
    sink: jack::Port<jack::AudioIn>,
}

impl jack::ProcessHandler for DeclaredPeerLatencyProcess {
    fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
        let _ = self.sink.as_slice(scope);
        self.source.as_mut_slice(scope).fill(0.0);
        jack::Control::Continue
    }
}

unsafe extern "C" fn declared_peer_latency_callback(
    mode: jack_sys::jack_latency_callback_mode_t,
    arg: *mut std::ffi::c_void,
) {
    let ports = &*(arg as *const DeclaredPeerLatency);
    let (port, minimum, maximum) = if mode == jack_sys::JackCaptureLatency {
        (ports.source, 11, 13)
    } else {
        (ports.sink, 17, 19)
    };
    let mut range = jack_sys::jack_latency_range_t {
        min: minimum,
        max: maximum,
    };
    jack_sys::jack_port_set_latency_range(port, mode, &mut range);
}

fn connect_checked(client: &jack::Client, source: &str, destination: &str) {
    client
        .connect_ports_by_name(source, destination)
        .unwrap_or_else(|error| panic!("connect {source} -> {destination}: {error}"));
    let source_port = client
        .port_by_name(source)
        .unwrap_or_else(|| panic!("connected source port disappeared: {source}"));
    assert!(
        source_port
            .get_connections()
            .iter()
            .any(|connection| connection == destination),
        "JACK did not retain connection {source} -> {destination}"
    );
}

#[shoop_wasm_test_support::shoop_test]
fn registered_ports_are_visible_to_jack_with_direction_flags() {
    let _exclusive = jack_test_lock();
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

#[shoop_wasm_test_support::shoop_test]
fn jack_latency_callback_publishes_connected_port_ranges() {
    let _exclusive = jack_test_lock();
    let suffix = std::process::id();
    let Some((driver, session)) = app_jack(&format!("shoop-app-latency-{suffix}")) else {
        return;
    };
    let input =
        AudioPort::new_driver_port(&session, &driver, "in", &PortDirection::Input, 64).unwrap();
    let output =
        AudioPort::new_driver_port(&session, &driver, "out", &PortDirection::Output, 64).unwrap();
    let Some(peer) = peer_client(&format!("shoop-app-latency-peer-{suffix}")) else {
        return;
    };
    let source = peer
        .register_port("source", jack::AudioOut::default())
        .unwrap();
    let sink = peer
        .register_port("sink", jack::AudioIn::default())
        .unwrap();
    let declared = Box::new(DeclaredPeerLatency {
        source: source.raw(),
        sink: sink.raw(),
    });
    assert_eq!(
        unsafe {
            jack_sys::jack_set_latency_callback(
                peer.raw(),
                Some(declared_peer_latency_callback),
                (&*declared as *const DeclaredPeerLatency).cast_mut().cast(),
            )
        },
        0
    );
    let source_name = source.name().unwrap();
    let sink_name = sink.name().unwrap();
    let active_peer = peer
        .activate_async((), DeclaredPeerLatencyProcess { source, sink })
        .unwrap();
    connect_checked(
        active_peer.as_client(),
        &source_name,
        &format!("{}:in", driver.get_state().maybe_instance_name),
    );
    connect_checked(
        active_peer.as_client(),
        &format!("{}:out", driver.get_state().maybe_instance_name),
        &sink_name,
    );
    unsafe {
        jack_sys::jack_recompute_total_latencies(active_peer.as_client().raw());
    }
    assert!(wait_until(|| {
        input.get_state().is_ok_and(|state| {
            state
                .capture_latency
                .range
                .is_some_and(|range| (range.min(), range.max()) == (11, 13))
        }) && output.get_state().is_ok_and(|state| {
            state
                .playback_latency
                .range
                .is_some_and(|range| (range.min(), range.max()) == (17, 19))
        })
    }));
    let capture = input.get_state().unwrap().capture_latency;
    let playback = output.get_state().unwrap().playback_latency;
    assert_eq!(
        capture.range.map(|range| (range.min(), range.max())),
        Some((11, 13))
    );
    assert_eq!(
        playback.range.map(|range| (range.min(), range.max())),
        Some((17, 19))
    );
    assert!(driver.get_state().capture_latency.range.is_some());
    assert!(driver.get_state().playback_latency.range.is_some());
    let (peer, _, _) = active_peer.deactivate().unwrap();
    unsafe {
        jack_sys::jack_set_latency_callback(peer.raw(), None, std::ptr::null_mut());
    }
    drop(declared);
}

#[shoop_wasm_test_support::shoop_test]
fn jack_latency_port_add_remove_stress_retires_callback_handles() {
    let _exclusive = jack_test_lock();
    let suffix = std::process::id();
    let Some((driver, session)) = app_jack(&format!("shoop-app-latency-churn-{suffix}")) else {
        return;
    };
    for index in 0..128 {
        let port = AudioPort::new_driver_port(
            &session,
            &driver,
            &format!("churn-{index}"),
            if index % 2 == 0 {
                &PortDirection::Input
            } else {
                &PortDirection::Output
            },
            64,
        )
        .unwrap();
        if index % 2 == 0 {
            driver.unregister_audio_port(&port).unwrap();
        }
    }
    assert!(driver.get_state().capture_latency.range.is_some());
}

#[shoop_wasm_test_support::shoop_test]
fn jack_audio_input_reaches_a_recording_channel() {
    let _exclusive = jack_test_lock();
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
#[shoop_wasm_test_support::shoop_test]
fn session_output_reaches_a_jack_consumer() {
    let _exclusive = jack_test_lock();
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
#[shoop_wasm_test_support::shoop_test]
fn audio_keeps_flowing_across_a_mid_stream_topology_change() {
    let _exclusive = jack_test_lock();
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
#[shoop_wasm_test_support::shoop_test]
fn external_dry_wet_audio_round_trip_reaches_jack_output() {
    let _exclusive = jack_test_lock();
    let suffix = std::process::id();
    let Some(source_client) = peer_client(&format!("shoop-dry-source-{suffix}")) else {
        return;
    };
    let source = source_client
        .register_port("source", jack::AudioOut::default())
        .expect("source port");
    let source_name = source.name().expect("source name");
    let Some(dry_client) = peer_client(&format!("shoop-dry-processor-{suffix}")) else {
        return;
    };
    let dry_send = dry_client
        .register_port("dry_send", jack::AudioIn::default())
        .expect("dry send port");
    let dry_send_name = dry_send.name().expect("dry send name");
    let Some(wet_client) = peer_client(&format!("shoop-wet-processor-{suffix}")) else {
        return;
    };
    let wet_return = wet_client
        .register_port("wet_return", jack::AudioOut::default())
        .expect("wet return port");
    let wet_return_name = wet_return.name().expect("wet return name");
    let Some(consumer_client) = peer_client(&format!("shoop-wet-consumer-{suffix}")) else {
        return;
    };
    let wet_output = consumer_client
        .register_port("wet_output", jack::AudioIn::default())
        .expect("wet output port");
    let wet_output_name = wet_output.name().expect("wet output name");
    let dry_sample_bits = Arc::new(AtomicU32::new(0));
    let dry_seen = Arc::new(AtomicBool::new(false));
    let wet_produced = Arc::new(AtomicBool::new(false));
    let output_max_bits = Arc::new(AtomicU32::new(0));

    let Some((driver, session)) = app_jack(&format!("shoop-dry-wet-{suffix}")) else {
        return;
    };
    let app_name = driver.get_state().maybe_instance_name;
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

    let source_active = source_client
        .activate_async((), ConstantAudioSource { port: source })
        .expect("activate source");
    let dry_active = dry_client
        .activate_async(
            (),
            DryProcessorInput {
                port: dry_send,
                sample_bits: dry_sample_bits.clone(),
                seen: dry_seen.clone(),
            },
        )
        .expect("activate dry processor input");
    let wet_active = wet_client
        .activate_async(
            (),
            WetProcessorOutput {
                port: wet_return,
                sample_bits: dry_sample_bits,
                dry_seen: dry_seen.clone(),
                produced: wet_produced.clone(),
            },
        )
        .expect("activate wet processor output");
    let consumer_active = consumer_client
        .activate_async(
            (),
            AtomicAudioConsumer {
                port: wet_output,
                max_bits: output_max_bits.clone(),
            },
        )
        .expect("activate wet consumer");

    let connector = source_active.as_client();
    connect_checked(connector, &source_name, &format!("{app_name}:audio_dry_in"));
    connect_checked(
        connector,
        &format!("{app_name}:audio_dry_send"),
        &dry_send_name,
    );
    connect_checked(
        connector,
        &wet_return_name,
        &format!("{app_name}:audio_wet_return"),
    );
    connect_checked(
        connector,
        &format!("{app_name}:audio_wet_out"),
        &wet_output_name,
    );
    driver.wait_process();

    let received_processed =
        wait_until(|| f32::from_bits(output_max_bits.load(Ordering::Acquire)) >= 2.0);
    let observed_max = f32::from_bits(output_max_bits.load(Ordering::Acquire));
    assert!(
        dry_seen.load(Ordering::Acquire),
        "dry send never reached processor input"
    );
    assert!(
        wet_produced.load(Ordering::Acquire),
        "processor never produced a wet return after observing dry input"
    );
    let _ = consumer_active.deactivate();
    let _ = wet_active.deactivate();
    let _ = dry_active.deactivate();
    let _ = source_active.deactivate();
    assert!(
        received_processed,
        "expected transformed sample 2.0 at wet output, observed max {observed_max}"
    );
}

#[shoop_wasm_test_support::shoop_test]
fn external_send_return_adds_one_callback_period_at_two_buffer_sizes() {
    let _exclusive = jack_test_lock();
    let suffix = std::process::id();
    let Some(source_client) = peer_client(&format!("shoop-latency-source-{suffix}")) else {
        return;
    };
    let source = source_client
        .register_port("source", jack::AudioOut::default())
        .expect("source port");
    let source_name = source.name().expect("source name");
    let Some(processor_client) = peer_client(&format!("shoop-latency-processor-{suffix}")) else {
        return;
    };
    let processor_input = processor_client
        .register_port("input", jack::AudioIn::default())
        .expect("processor input");
    let processor_input_name = processor_input.name().expect("processor input name");
    let processor_output = processor_client
        .register_port("output", jack::AudioOut::default())
        .expect("processor output");
    let processor_output_name = processor_output.name().expect("processor output name");
    let Some(sink_client) = peer_client(&format!("shoop-latency-sink-{suffix}")) else {
        return;
    };
    let sink = sink_client
        .register_port("sink", jack::AudioIn::default())
        .expect("sink port");
    let sink_name = sink.name().expect("sink name");

    let Some((driver, session)) = app_jack(&format!("shoop-latency-app-{suffix}")) else {
        return;
    };
    let app_name = driver.get_state().maybe_instance_name;
    let initial_buffer_size = source_client.buffer_size();
    let ring = initial_buffer_size.max(1);
    let app_input =
        AudioPort::new_driver_port(&session, &driver, "send_input", &PortDirection::Input, ring)
            .expect("application send input");
    let app_send =
        AudioPort::new_driver_port(&session, &driver, "send_output", &PortDirection::Output, 0)
            .expect("application send output");
    let app_return = AudioPort::new_driver_port(
        &session,
        &driver,
        "return_input",
        &PortDirection::Input,
        ring,
    )
    .expect("application return input");
    let app_output = AudioPort::new_driver_port(
        &session,
        &driver,
        "return_output",
        &PortDirection::Output,
        0,
    )
    .expect("application return output");
    app_input
        .connect_internal(&app_send)
        .expect("application send route");
    app_return
        .connect_internal(&app_output)
        .expect("application return route");
    driver.wait_process();

    let armed = Arc::new(AtomicBool::new(false));
    let sent_frame = Arc::new(AtomicU32::new(UNSEEN_FRAME));
    let received_frame = Arc::new(AtomicU32::new(UNSEEN_FRAME));
    let source_active = source_client
        .activate_async(
            (),
            TimedPulseSource {
                port: source,
                armed: armed.clone(),
                sent_frame: sent_frame.clone(),
            },
        )
        .expect("activate source");
    let processor_active = processor_client
        .activate_async(
            (),
            CycleCopyProcessor {
                input: processor_input,
                output: processor_output,
            },
        )
        .expect("activate processor");
    let sink_active = sink_client
        .activate_async(
            (),
            TimedPulseSink {
                port: sink,
                received_frame: received_frame.clone(),
            },
        )
        .expect("activate sink");

    let connector = source_active.as_client();
    let restore_buffer_size = JackBufferSizeRestore {
        client: connector,
        buffer_size: initial_buffer_size,
    };
    connect_checked(connector, &source_name, &format!("{app_name}:send_input"));
    connect_checked(
        connector,
        &format!("{app_name}:send_output"),
        &processor_input_name,
    );
    connect_checked(
        connector,
        &processor_output_name,
        &format!("{app_name}:return_input"),
    );
    connect_checked(connector, &format!("{app_name}:return_output"), &sink_name);
    driver.wait_process();

    for buffer_size in [64, 128] {
        connector
            .set_buffer_size(buffer_size)
            .unwrap_or_else(|error| panic!("set JACK buffer size to {buffer_size}: {error}"));
        if !wait_until(|| {
            connector.buffer_size() == buffer_size
                && driver.get_state().last_processed == buffer_size
        }) {
            require_backend(
                "JACK variable buffer size",
                &format!(
                    "requested {buffer_size}, server remained {}, app processed {} (initial={initial_buffer_size})",
                    connector.buffer_size(),
                    driver.get_state().last_processed,
                ),
            );
            return;
        }
        driver.wait_process();
        sent_frame.store(UNSEEN_FRAME, Ordering::Release);
        received_frame.store(UNSEEN_FRAME, Ordering::Release);
        armed.store(true, Ordering::Release);
        assert!(
            wait_until(|| received_frame.load(Ordering::Acquire) != UNSEEN_FRAME),
            "timing pulse did not complete the external route at {buffer_size} frames"
        );
        let sent = sent_frame.load(Ordering::Acquire);
        let received = received_frame.load(Ordering::Acquire);
        assert_ne!(sent, UNSEEN_FRAME);
        assert_eq!(
            received.wrapping_sub(sent),
            buffer_size,
            "external send/return callback delay at buffer size {buffer_size}"
        );
    }

    drop(restore_buffer_size);
    let _ = sink_active.deactivate();
    let _ = processor_active.deactivate();
    let _ = source_active.deactivate();
}

// Purpose: Verify one JACK MIDI source reaches only the external track with passthrough enabled.
// Use case: A shared controller feeds several external synth tracks but only the monitored one sounds.
#[shoop_wasm_test_support::shoop_test]
fn external_midi_fanout_respects_each_tracks_passthrough_mute() {
    let _exclusive = jack_test_lock();
    let suffix = std::process::id();
    let Some(source_client) = peer_client(&format!("shoop-midi-source-{suffix}")) else {
        return;
    };
    let source = source_client
        .register_port("source", jack::MidiOut::default())
        .expect("MIDI source port");
    let source_name = source.name().expect("source name");
    let Some(sink_client) = peer_client(&format!("shoop-midi-sinks-{suffix}")) else {
        return;
    };
    let monitored = sink_client
        .register_port("monitored", jack::MidiIn::default())
        .expect("monitored sink port");
    let muted = sink_client
        .register_port("muted", jack::MidiIn::default())
        .expect("muted sink port");
    let monitored_name = monitored.name().expect("monitored name");
    let muted_name = muted.name().expect("muted name");
    let monitored_note_on = Arc::new(AtomicBool::new(false));
    let monitored_note_off = Arc::new(AtomicBool::new(false));
    let muted_events = Arc::new(AtomicUsize::new(0));

    let Some((driver, session)) = app_jack(&format!("shoop-midi-fanout-{suffix}")) else {
        return;
    };
    let app_name = driver.get_state().maybe_instance_name;
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

    let source_active = source_client
        .activate_async((), MidiSource { port: source })
        .expect("activate MIDI source");
    let sink_active = sink_client
        .activate_async(
            (),
            MidiSinks {
                monitored,
                muted,
                monitored_note_on: monitored_note_on.clone(),
                monitored_note_off: monitored_note_off.clone(),
                muted_events: muted_events.clone(),
            },
        )
        .expect("activate MIDI sinks");

    let connector = source_active.as_client();
    connect_checked(
        connector,
        &source_name,
        &format!("{app_name}:monitored_dry_midi_in"),
    );
    connect_checked(
        connector,
        &source_name,
        &format!("{app_name}:muted_dry_midi_in"),
    );
    connect_checked(
        connector,
        &format!("{app_name}:monitored_dry_midi_send"),
        &monitored_name,
    );
    connect_checked(
        connector,
        &format!("{app_name}:muted_dry_midi_send"),
        &muted_name,
    );
    driver.wait_process();

    let received_monitored = wait_until(|| {
        monitored_note_on.load(Ordering::Acquire) && monitored_note_off.load(Ordering::Acquire)
    });
    std::thread::sleep(Duration::from_millis(100));
    let muted_count = muted_events.load(Ordering::Acquire);
    let _ = sink_active.deactivate();
    let _ = source_active.deactivate();
    assert!(
        received_monitored,
        "monitored JACK sink did not receive both note-on and note-off"
    );
    assert_eq!(
        muted_count, 0,
        "passthrough-muted JACK track emitted {muted_count} MIDI events"
    );
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
