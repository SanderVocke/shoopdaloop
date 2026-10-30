//! The CPAL driver through a software CPAL host: device ports, the render callback,
//! the duplex and MIDI bridges, xruns, buffer refusal and stopping. Every data
//! callback runs inside a heap measurement.
#![cfg(feature = "cpal")]

mod support;

use std::sync::Arc;

use cpal::StreamError;
use shoop_rt_engine::EngineDescription;
use shoop_rt_host_native::{
    start_cpal, CpalConfig, DeviceSelection, MidiDevices, MidiInputDevice, MidiOutputDevice,
    NativeClient, NativeHost, RenderPhase, StartError, Started, StreamFailure, BRIDGE_MESSAGES,
};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::cpal_mock::{MockDevice, MockHost, MockStreamHandle};
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

const MIC: u32 = 0;
const SPEAKERS_LEFT: u32 = 1;
const KEYS: u32 = 3;
const SYNTH: u32 = 4;

struct Rig {
    host: Option<NativeHost>,
    client: Option<NativeClient>,
    output: Arc<MockStreamHandle>,
    input: Option<Arc<MockStreamHandle>>,
    received: Received,
    source: TestSource,
    sink: TestSink,
    /// The sample every captured frame carries.
    level: f32,
    /// Interleaved output of every callback so far.
    played: Vec<f32>,
}

fn speakers() -> MockDevice {
    MockDevice::output("Speakers", 2)
}
fn mic() -> MockDevice {
    MockDevice::input("Mic", 1)
}

fn open(
    config: &CpalConfig,
    engine: &EngineDescription,
    output: MockDevice,
    input: Option<MockDevice>,
) -> Result<Rig, StartError> {
    let mock = MockHost::new(
        [Some(output.clone()), input.clone()]
            .into_iter()
            .flatten()
            .collect(),
    );
    let source = TestSource::default();
    let sink = TestSink::default();
    let midi = MidiDevices {
        inputs: vec![MidiInputDevice {
            name: "Keys".into(),
            source: Box::new(source.clone()),
        }],
        outputs: vec![MidiOutputDevice {
            name: "Synth".into(),
            sink: Box::new(sink.clone()),
        }],
    };
    let Started { host, client } = start_cpal(&mock, config, engine, midi)?;
    Ok(Rig {
        host: Some(host),
        client: Some(client),
        output: output.stream().expect("an output stream"),
        input: input.and_then(|device| device.stream()),
        received: Received::default(),
        source,
        sink,
        level: 0.,
        played: Vec::new(),
    })
}

fn rig() -> Rig {
    open(
        &CpalConfig::default(),
        &description(),
        speakers(),
        Some(mic()),
    )
    .unwrap()
}

impl Rig {
    fn host(&self) -> &NativeHost {
        self.host.as_ref().unwrap()
    }
    fn client(&mut self) -> &mut NativeClient {
        self.client.as_mut().unwrap()
    }
    fn capture(&self, frames: usize) {
        let input = self.input.as_ref().unwrap();
        input.capture(&vec![self.level; frames * input.channels]);
    }
    fn render(&mut self, frames: usize) {
        let output = self.output.render(frames);
        self.played.extend(output);
        if let Some(client) = self.client.as_mut() {
            self.received.take(client);
        }
    }
    /// One duplex period: the input callback, then the output callback.
    fn tick(&mut self, frames: usize) {
        if self.input.is_some() {
            self.capture(frames);
        }
        self.render(frames);
    }
    fn connect(&mut self) {
        self.client().connect().unwrap();
        self.tick(16);
        assert!(matches!(
            self.received.lifecycle.first(),
            Some(EngineMessage::Welcome { .. })
        ));
    }
    fn done(&mut self, request: Request) {
        let correlation = self.client().request(request).unwrap();
        for _ in 0..100 {
            self.tick(16);
            if let Some(outcome) = self.received.results.get(&correlation) {
                assert_eq!(*outcome, Outcome::Done, "{request:?}");
                return;
            }
        }
        panic!("no answer to {request:?}");
    }
    /// Channel `channel` of every frame played so far.
    fn played(&self, channel: usize) -> Vec<f32> {
        self.played
            .iter()
            .skip(channel)
            .step_by(2)
            .copied()
            .collect()
    }
    fn assert_no_heap_in_callbacks(&self) {
        assert!(self.output.calls() > 0);
        assert_eq!(self.output.heap_operations(), 0);
        if let Some(input) = &self.input {
            assert_eq!(input.heap_operations(), 0);
        }
    }
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn heap_guards_are_live() {
    assert_heap_guard_is_live();
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn device_channels_and_midi_ports_become_host_ports_with_the_default_outputs_preferred() {
    let mut rig = rig();
    assert!(rig.host().warnings().is_empty());
    let layout = rig.host().layout().clone();
    assert_eq!(layout.master_connections, [(0, 0), (1, 1)]);
    rig.connect();
    rig.client().resync().unwrap();
    rig.tick(16);
    let ports: Vec<_> = rig
        .received
        .state
        .iter()
        .filter_map(|record| match record {
            StateRecord::HostPort(port) => Some((
                port.id.0,
                port.kind,
                port.direction,
                port.name.as_str().to_owned(),
                port.preferred,
            )),
            _ => None,
        })
        .collect();
    use Direction::*;
    use HostPortKind::*;
    assert_eq!(
        ports,
        [
            (MIC, Audio, Input, "Mic:capture_1".into(), false),
            (
                SPEAKERS_LEFT,
                Audio,
                Output,
                "Speakers:playback_1".into(),
                true
            ),
            (2, Audio, Output, "Speakers:playback_2".into(), true),
            (KEYS, Midi, Input, "Keys".into(), false),
            (SYNTH, Midi, Output, "Synth".into(), false),
        ]
    );
    let status = rig.host().status();
    assert_eq!(status.driver.kind, DriverKind::Cpal);
    assert_eq!(status.driver.state, DriverRunState::Running);
    assert_eq!(status.driver.sample_rate, 48_000);
    assert_eq!(status.driver.buffer_frames, 16);
    let (_, _, driver) = rig.received.last_engine().unwrap();
    assert_eq!(driver, status.driver);
    rig.assert_no_heap_in_callbacks();
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn captured_audio_reaches_the_output_one_prefill_later_at_absolute_frames() {
    let mut rig = rig();
    rig.connect();
    let track = strip(0);
    rig.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, false),
    });
    rig.done(connect(track, PortKind::AudioIn, MIC));
    rig.done(connect(track, PortKind::AudioOut, SPEAKERS_LEFT));
    rig.done(monitoring(track));
    for _ in 0..8 {
        rig.tick(16);
    }
    let captured_from = rig.played.len() / 2;
    rig.level = 0.25;
    for _ in 0..12 {
        rig.tick(16);
    }
    let left = rig.played(0);
    // Without a fixed buffer the bridge starts with one largest callback of silence.
    let delay = description().max_frames;
    assert!(left[..captured_from + delay].iter().all(|s| *s == 0.));
    assert!(left[captured_from + delay..].iter().all(|s| *s == 0.25));
    assert!(rig.played(1).iter().all(|s| *s == 0.));
    let status = rig.host().status();
    assert_eq!(status.next_frame, left.len() as u64);
    assert_eq!(status.stats.xruns(), 0);
    rig.assert_no_heap_in_callbacks();
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn callbacks_above_the_prepared_maximum_are_rendered_in_parts() {
    let mut rig = rig();
    rig.level = 0.5;
    rig.connect();
    let track = strip(0);
    rig.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, false),
    });
    rig.done(connect(track, PortKind::AudioIn, MIC));
    rig.done(connect(track, PortKind::AudioOut, SPEAKERS_LEFT));
    rig.done(monitoring(track));
    for _ in 0..4 {
        rig.tick(16);
    }
    let before = rig.host().status();
    let from = rig.played.len() / 2;
    rig.tick(100);
    rig.tick(100);
    let status = rig.host().status();
    assert_eq!(status.stats.split_callbacks, 2);
    assert_eq!(status.next_frame, before.next_frame + 200);
    assert_eq!(status.driver.buffer_frames, 100);
    assert!(rig.played(0)[from..].iter().all(|s| *s == 0.5));
    assert!(rig.played.iter().all(|s| s.is_finite()));
    assert_eq!(status.stats.xruns(), 0);
    rig.assert_no_heap_in_callbacks();
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn bridge_underruns_and_overruns_and_backend_underruns_are_counted_as_xruns() {
    let mut rig = rig();
    rig.connect();
    // The bridge holds one largest callback; four empty it.
    for _ in 0..5 {
        rig.render(16);
    }
    assert_eq!(rig.host().status().stats.input_underruns, 1);
    // It holds four largest callbacks of captured audio.
    for _ in 0..17 {
        rig.capture(16);
    }
    assert_eq!(rig.host().status().stats.input_overruns, 1);
    for _ in 0..8 {
        rig.render(16);
    }
    rig.output.fail(StreamError::BufferUnderrun);
    rig.tick(16);
    rig.tick(16);
    let status = rig.host().status();
    assert_eq!(status.stats.stream_underruns, 1);
    assert_eq!(status.stats.xruns(), 3);
    assert_eq!(status.driver.state, DriverRunState::Running);
    assert_eq!(rig.received.last_engine().unwrap().1, 3);
    rig.assert_no_heap_in_callbacks();
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn a_stream_error_marks_the_driver_failed() {
    let mut rig = rig();
    rig.connect();
    rig.input
        .as_ref()
        .unwrap()
        .fail(StreamError::DeviceNotAvailable);
    let status = rig.host().status();
    assert_eq!(status.driver.state, DriverRunState::Failed);
    assert_eq!(status.failure, Some(StreamFailure::DeviceNotAvailable));
    rig.tick(16);
    let (_, _, driver) = rig.received.last_engine().unwrap();
    assert_eq!(driver.state, DriverRunState::Failed);
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn buffers_the_prepared_engine_cannot_render_are_refused_at_open() {
    let engine = description();
    let fixed = |frames| CpalConfig {
        buffer_frames: Some(frames),
        ..CpalConfig::default()
    };
    assert_eq!(
        open(&fixed(128), &engine, speakers(), None).err(),
        Some(StartError::BufferTooLarge {
            requested: 128,
            max: 64
        })
    );
    assert_eq!(
        open(
            &CpalConfig::default(),
            &engine,
            speakers().with_buffer_range(128, 512),
            None
        )
        .err(),
        Some(StartError::BufferTooLarge {
            requested: 128,
            max: 64
        })
    );
    assert_eq!(
        open(
            &fixed(64),
            &engine,
            speakers().with_buffer_range(16, 48),
            None
        )
        .err(),
        Some(StartError::BufferUnsupported {
            requested: 64,
            min: 16,
            max: 48
        })
    );
    let rig = open(&fixed(32), &engine, speakers(), None).unwrap();
    assert_eq!(rig.host().status().driver.buffer_frames, 32);
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn devices_are_selected_by_name_and_missing_ones_are_refused() {
    let engine = description();
    let named = |output: &str, input: DeviceSelection| CpalConfig {
        output_device: DeviceSelection::Named(output.into()),
        input_device: input,
        ..CpalConfig::default()
    };
    assert_eq!(
        open(
            &named("Headphones", DeviceSelection::None),
            &engine,
            speakers(),
            None
        )
        .err(),
        Some(StartError::NoDevice {
            direction: "output",
            name: Some("Headphones".into())
        })
    );
    let rig = open(
        &named("Speakers", DeviceSelection::None),
        &engine,
        speakers(),
        Some(mic()),
    )
    .unwrap();
    assert!(rig.host().layout().ports.audio_inputs.is_empty());
    let rig = open(&CpalConfig::default(), &engine, speakers(), None).unwrap();
    assert_eq!(rig.host().warnings().len(), 1);
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn midi_passes_through_the_bridges_and_full_bridges_drop_and_count() {
    let mut rig = rig();
    rig.connect();
    let track = strip(0);
    rig.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, true),
    });
    rig.done(connect(track, PortKind::MidiIn, KEYS));
    rig.done(connect(track, PortKind::MidiOut, SYNTH));
    rig.done(monitoring(track));
    assert!(rig.source.push(&[0x90, 60, 100]));
    assert!(rig.source.push(&[0x80, 60, 0]));
    rig.tick(16);
    assert_eq!(
        rig.sink.wait_for(2),
        [vec![0x90, 60, 100], vec![0x80, 60, 0]]
    );
    assert!(!rig.source.push(&[]));
    assert!(!rig.source.push(&[0xf0; 65]));
    assert!(rig.source.push(&[0x90]));
    for note in 0..BRIDGE_MESSAGES + 3 {
        rig.source.push(&[0xb0, 1, (note % 128) as u8]);
    }
    let stats = rig.host().status().stats;
    assert_eq!(stats.midi_in_dropped, 4);
    for _ in 0..40 {
        rig.tick(16);
    }
    let stats = rig.host().status().stats;
    assert_eq!(stats.midi_in_refused, 3);
    assert_eq!(rig.host().status().phase, RenderPhase::Running);
    assert_eq!(
        rig.sink.wait_for(2 + BRIDGE_MESSAGES - 1).len(),
        2 + BRIDGE_MESSAGES - 1
    );
    assert_eq!(stats.midi_out_dropped, 0);
    rig.assert_no_heap_in_callbacks();
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn stopping_after_the_client_sends_a_midi_reset_and_cleans_up_off_the_audio_thread() {
    let mut rig = rig();
    rig.connect();
    let track = strip(0);
    let looped = loop_id(1);
    rig.done(Request::CreateStrip {
        strip: track,
        shape: shape(1, true),
    });
    rig.done(connect(track, PortKind::AudioIn, MIC));
    rig.done(Request::CreateLoop {
        loop_id: looped,
        strip: track,
    });
    rig.level = 0.1;
    rig.done(Request::LoopTransition {
        loop_id: looped,
        mode: LoopMode::Recording,
        delay: None,
    });
    for _ in 0..10 {
        rig.tick(16);
    }
    rig.client.take();
    rig.tick(16);
    let report = rig.host.take().unwrap().stop();
    assert!(report.midi_reset);
    assert_eq!(report.phase, RenderPhase::Disconnected);
    let resets: Vec<Vec<u8>> = (0..16u8)
        .flat_map(|channel| [vec![0xb0 | channel, 120, 0], vec![0xb0 | channel, 123, 0]])
        .collect();
    assert_eq!(rig.sink.sent(), resets);
    assert!(rig.played[rig.played.len() - 32..].iter().all(|s| *s == 0.));
    assert!(!rig.output.is_live());
    assert!(!rig.input.as_ref().unwrap().is_live());
    let mut stopped = report.stopped.unwrap();
    assert_eq!(stopped.media_usage().references, 0);
    assert_eq!(
        stopped.resource_usage().free_pages,
        description().resource_pages
    );
    assert!(stopped.cleanup().is_ok());
    assert_eq!(report.stats.midi_out_failed, 0);
    rig.assert_no_heap_in_callbacks();
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn stopping_with_a_live_client_still_cleans_up() {
    let mut rig = rig();
    rig.connect();
    let report = rig.host.take().unwrap().stop();
    assert!(!report.midi_reset);
    assert!(report.stopped.is_ok());
    assert!(rig.sink.sent().is_empty());
}

#[shoop_test(no_wasm = "the native host does not build for wasm")]
fn threaded_streams_render_while_a_client_works_without_heap_operations() {
    let output = speakers().threaded(0.);
    let input = mic().threaded(0.5);
    let config = CpalConfig {
        buffer_frames: Some(32),
        ..CpalConfig::default()
    };
    let mut rig = open(&config, &description(), output, Some(input)).unwrap();
    let mut client = rig.client.take().unwrap();
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
        connect(track, PortKind::AudioIn, MIC),
    );
    request_done(&mut client, &mut received, monitoring(track));
    until(&mut client, &mut received, |r| {
        r.strip_peaks(track).is_some_and(|(input, output)| {
            input.as_slice()[0] == 0.5 && output.as_slice()[0] == 0.5
        })
    });
    drop(client);
    let report = rig.host.take().unwrap().stop();
    assert!(report.midi_reset);
    assert!(report.stopped.is_ok());
    assert!(rig.input.as_ref().unwrap().calls() > 0);
    rig.assert_no_heap_in_callbacks();
}
