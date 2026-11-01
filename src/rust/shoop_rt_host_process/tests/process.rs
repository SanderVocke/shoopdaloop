#![cfg(not(target_arch = "wasm32"))]
use shoop_backend::Backend;
use shoop_rt_backend::{EngineClock, RtBackend, Sleep};
use shoop_rt_engine::{EngineDescription, Segment};
use shoop_rt_host_native::{DriverConfig, DummyConfig, DummyMode, NativeConfig};
use shoop_rt_host_process::{Pending, Started, Transport};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
use std::{
    process::Command,
    time::{Duration, Instant},
};
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[path = "../../shoop_rt_host_native/tests/support/mod.rs"]
mod support;
fn config() -> NativeConfig {
    NativeConfig {
        driver: DriverConfig::Dummy(DummyConfig {
            sample_rate: 1000,
            buffer_frames: 1,
            ..DummyConfig::default()
        }),
        engine: EngineDescription {
            max_frames: 16,
            segments: vec![Segment {
                strips: 16,
                loops: 64,
            }],
            resource_pages: 2048,
            page_bytes: 256,
            ramp_frames: 4,
            pending_transitions: 64,
            reliable_credits: 32,
            bulk_credits: 2,
            telemetry_interval_frames: 0,
            ..EngineDescription::default()
        },
    }
}
fn command(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "engine_peer", "--nocapture"])
        .env("SHOOP_RT_TEST_DRIVER", mode);
    command
}
fn start(transport: Transport, mode: &str) -> Started {
    Pending::spawn_command(command(mode), transport)
        .unwrap()
        .start(&config())
        .unwrap()
}
fn ramp(port: usize, frame: u64) -> f32 {
    if port == 0 {
        frame as f32 / 1000.0
    } else {
        0.0
    }
}
fn guarded(callback: &mut dyn FnMut()) {
    assert_no_heap(callback);
}
#[shoop_test(no_wasm = "native engine fixture")]
fn engine_peer() {
    if std::env::var_os("SHOOP_RT_PROCESS_ADDRESS").is_none() {
        return;
    }
    let mode = std::env::var("SHOOP_RT_TEST_DRIVER").unwrap_or_default();
    #[cfg(feature = "cpal")]
    if mode == "cpal_mock" || mode == "cpal_fault" {
        use support::cpal_mock::{MockDevice, MockHost};
        let output = MockDevice::output("Mock output", 2)
            .with_buffer_range(1, 1024)
            .threaded(0.0);
        let input = MockDevice::input("Mock input", 2)
            .with_buffer_range(1, 1024)
            .threaded(0.25);
        let mock = MockHost::new(vec![output.clone(), input.clone()]);
        let source = support::TestSource::default();
        let sink = support::TestSink::default();
        shoop_rt_host_process::run_with_start(|config| {
            let DriverConfig::Cpal(cpal) = &config.driver else {
                return Err(shoop_rt_host_native::StartError::InvalidConfig(
                    "expected CPAL",
                ));
            };
            let midi = shoop_rt_host_native::MidiDevices {
                inputs: vec![shoop_rt_host_native::MidiInputDevice {
                    name: "Keys".into(),
                    source: Box::new(source.clone()),
                }],
                outputs: vec![shoop_rt_host_native::MidiOutputDevice {
                    name: "Synth".into(),
                    sink: Box::new(sink.clone()),
                }],
            };
            let started = shoop_rt_host_native::start_cpal(&mock, cpal, &config.engine, midi)?;
            source.push(&[0x90, 60, 100]);
            if mode == "cpal_fault" {
                let stream = output.stream().unwrap();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    stream.fail(cpal::StreamError::DeviceNotAvailable);
                });
            }
            let (endpoint, outbox) = started.client.into_carrier().into_parts();
            Ok(shoop_rt_host_native::StartedEndpoint {
                host: started.host,
                endpoint,
                outbox,
            })
        })
        .unwrap();
        for device in [&output, &input] {
            let stream = device.stream().unwrap();
            assert!(stream.calls() > 0);
            assert_eq!(stream.heap_operations(), 0);
        }
        if mode == "cpal_mock" {
            assert!(sink
                .sent()
                .iter()
                .any(|bytes| bytes.len() == 3 && bytes[1] == 123));
        }
        return;
    }
    shoop_rt_host_process::run_with_start(move |config| {
        let mut config = config.clone();
        if let DriverConfig::Dummy(dummy) = &mut config.driver {
            if mode == "controlled" {
                dummy.mode = DummyMode::Controlled;
            }
            dummy.input = Some(ramp);
            dummy.wrap_cycle = Some(guarded);
        }
        shoop_rt_host_native::start_endpoint(&config)
    })
    .unwrap();
}
struct Clock(shoop_rt_host_process::TestingClock);
impl EngineClock for Clock {
    fn advance(&mut self, elapsed: Duration) {
        self.0.advance(elapsed)
    }
    fn wait(&mut self) -> bool {
        self.0.wait()
    }
}
#[shoop_test(no_wasm = "native two-process backend")]
fn both_process_carriers_run_the_backend_contracts_and_stop_gracefully() {
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let Started {
            client,
            supervisor,
            warnings,
        } = start(transport, "controlled");
        assert!(warnings.is_empty());
        let clock = Clock(supervisor.testing_clock(1000, 1));
        let mut backend = RtBackend::connect(client, clock).unwrap();
        let profile = shoop_backend::contracts::ContractProfile {
            audio_capture_port: "audio_in_1".into(),
            audio_playback_port: "audio_out_1".into(),
            midi_capture_port: Some("midi_in_1".into()),
            midi_playback_port: Some("midi_out_1".into()),
        };
        shoop_backend::contracts::backend_contracts(&mut backend, &profile);
        drop(backend);
        let report = supervisor.stop();
        assert!(
            report.success && report.midi_reset && report.cleaned,
            "{report:?}"
        );
    }
}
#[shoop_test(no_wasm = "native connection lifetime")]
fn data_disconnect_keeps_the_process_running_until_independent_shutdown() {
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let Started {
            client, supervisor, ..
        } = start(transport, "controlled");
        let mut clock = supervisor.testing_clock(1000, 1);
        let backend = RtBackend::connect(client, Clock(supervisor.testing_clock(1000, 1))).unwrap();
        drop(backend);
        assert!(clock.step(1000));
        assert!(clock.frame() >= 1000);
        let report = supervisor.stop();
        assert!(report.success && report.midi_reset && report.cleaned);
    }
}
#[shoop_test(no_wasm = "native process death")]
fn engine_death_reaches_a_polling_client_without_new_mutations() {
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let Started {
            client,
            mut supervisor,
            ..
        } = start(transport, "automatic");
        let mut backend = RtBackend::connect(client, Sleep::default()).unwrap();
        supervisor.kill_for_test().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = backend.poll().unwrap();
            if snapshot.status.driver_state == shoop_backend::BackendDriverState::Failed {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(backend
            .create_direct_track(shoop_backend::DirectTrackRequest {
                port_name_base: "after failure".into(),
                audio_channels: 2,
                midi: false,
                initial_loops: 1
            })
            .is_err());
        drop(backend);
        let _ = supervisor.stop();
    }
}
#[shoop_test(no_wasm = "native startup rollback")]
fn startup_errors_and_missing_companions_fail_without_carrier_fallback() {
    assert!(Pending::spawn(
        std::path::Path::new("/nonexistent/shoop_rt_host_process"),
        Transport::Socket
    )
    .is_err());
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let mut invalid = config();
        invalid.engine.strip_channels = 0;
        assert!(Pending::spawn_command(command("automatic"), transport)
            .unwrap()
            .start(&invalid)
            .is_err());
    }
}

struct Timing {
    backend:
        RtBackend<shoop_rt_transport::records::RecordClient<shoop_rt_host_process::Port>, Clock>,
    _supervisor: shoop_rt_host_process::Supervisor,
    profile: shoop_backend::contracts::ContractProfile,
}
impl Timing {
    fn new(transport: Transport) -> Self {
        let Started {
            client, supervisor, ..
        } = start(transport, "controlled");
        let clock = Clock(supervisor.testing_clock(1000, 1));
        let mut backend = RtBackend::begin(client, clock).unwrap();
        ready(&mut backend);
        Self {
            backend,
            _supervisor: supervisor,
            profile: shoop_backend::contracts::ContractProfile {
                audio_capture_port: "audio_in_1".into(),
                audio_playback_port: "audio_out_1".into(),
                midi_capture_port: Some("midi_in_1".into()),
                midi_playback_port: Some("midi_out_1".into()),
            },
        }
    }
}
impl shoop_backend::contracts::TimingHarness for Timing {
    fn backend(&mut self) -> &mut dyn Backend {
        &mut self.backend
    }
    fn profile(&self) -> &shoop_backend::contracts::ContractProfile {
        &self.profile
    }
    fn frame(&self) -> u64 {
        self.backend.clock().0.frame()
    }
    fn step(&mut self) -> anyhow::Result<()> {
        self.backend.pump();
        anyhow::ensure!(self.backend.clock_mut().0.step(1), "test clock failed");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.backend.clock().0.observations_ready() {
            self.backend.pump();
            anyhow::ensure!(
                Instant::now() < deadline,
                "test observation barrier timed out"
            );
            std::thread::yield_now();
        }
        Ok(())
    }
    fn observation_lag(&self) -> u64 {
        1
    }
}
#[shoop_test(no_wasm = "subprocess timing and media")]
fn both_carriers_preserve_the_shared_transition_signal_and_playback_contracts() {
    let reference = trace(&mut Local::new());
    assert_eq!(reference.audio[0][0].len(), 16);
    assert_eq!(reference.audio[0][0][0], 0.025);
    assert_eq!(reference.audio[1][0].len(), 32);
    assert_eq!(reference.audio[1][0][0], 0.073);
    assert_eq!(
        reference.midi[0].iter().map(|e| e.time).collect::<Vec<_>>(),
        vec![8, 12]
    );
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let mut remote = Timing::new(transport);
        assert_eq!(trace(&mut remote), reference);
        shoop_backend::contracts::signal_chain_contract(&mut Timing::new(transport));
    }
}
#[cfg(feature = "cpal")]
#[shoop_test(no_wasm = "native CPAL and MIDI mocks")]
fn both_carriers_drive_mock_cpal_and_midi_callbacks_without_heap_operations() {
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let mut config = config();
        config.driver = DriverConfig::Cpal(shoop_rt_host_native::CpalConfig {
            buffer_frames: Some(16),
            ..Default::default()
        });
        let Started {
            client,
            supervisor,
            warnings,
        } = Pending::spawn_command(command("cpal_mock"), transport)
            .unwrap()
            .start(&config)
            .unwrap();
        assert!(warnings.is_empty());
        let mut backend = RtBackend::connect(client, Sleep::default()).unwrap();
        let track = backend
            .create_direct_track(shoop_backend::DirectTrackRequest {
                port_name_base: "mock".into(),
                audio_channels: 2,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        backend.wait_idle();
        assert!(backend.poll().unwrap().tracks.contains_key(&track.track_id));
        drop(backend);
        let report = supervisor.stop();
        assert!(
            report.success && report.midi_reset && report.cleaned,
            "{report:?}"
        );
    }
}
#[cfg(feature = "cpal")]
#[shoop_test(no_wasm = "child-side device discovery")]
fn device_discovery_and_rejections_are_served_by_the_child() {
    let mut pending = Pending::spawn_command(command("automatic"), Transport::Socket).unwrap();
    assert!(pending.discover(Some("__missing_host__")).is_err());
    assert!(!pending.discover(None).unwrap().hosts.is_empty());
}

#[shoop_test(no_wasm = "parent crash fixture")]
fn crashing_parent() {
    let Ok(kind) = std::env::var("SHOOP_RT_TEST_CRASH_PARENT") else {
        return;
    };
    let transport = Transport::parse(&kind).unwrap();
    let Started {
        client, supervisor, ..
    } = start(transport, "automatic");
    let mut backend = RtBackend::connect(client, Sleep::default()).unwrap();
    let created = backend
        .create_direct_track(shoop_backend::DirectTrackRequest {
            port_name_base: "survivor".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    backend.wait_idle();
    backend
        .transition_loop(
            created.loops[0],
            shoop_backend::BackendLoopMode::Recording,
            None,
        )
        .unwrap();
    backend.wait_idle();
    assert_eq!(
        backend.poll().unwrap().loops[&created.loops[0]].mode,
        shoop_backend::BackendLoopMode::Recording
    );
    std::fs::write(
        std::env::var_os("SHOOP_RT_TEST_PID_FILE").unwrap(),
        supervisor.id().unwrap().to_string(),
    )
    .unwrap();
    std::process::exit(0);
}
struct Survivor(u32);
impl Drop for Survivor {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = Command::new("kill")
            .args(["-KILL", &self.0.to_string()])
            .status();
        #[cfg(windows)]
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &self.0.to_string()])
            .status();
    }
}
#[shoop_test(no_wasm = "parent crash and surviving real engine")]
fn a_crashed_app_leaves_its_recording_engine_running_without_control() {
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("pid");
        let statusfile = dir.path().join("status");
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crashing_parent", "--nocapture"])
            .env("SHOOP_RT_TEST_CRASH_PARENT", transport.as_str())
            .env("SHOOP_RT_TEST_PID_FILE", &pidfile)
            .env("SHOOP_RT_TEST_STATUS_FILE", &statusfile)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = parent.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        let _survivor = Survivor(std::fs::read_to_string(&pidfile).unwrap().parse().unwrap());
        let mut first = None;
        loop {
            if let Ok(status) = std::fs::read_to_string(&statusfile) {
                let mut fields = status.split_whitespace();
                let phase = fields.next();
                let frame = fields.next().and_then(|s| s.parse::<u64>().ok());
                if let (Some(phase), Some(frame)) = (phase, frame) {
                    assert_eq!(phase, "Running");
                    if let Some(first) = first {
                        if frame > first + 100 {
                            break;
                        }
                    } else {
                        first = Some(frame)
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "surviving engine did not advance"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
#[shoop_test(no_wasm = "terminal protocol fault")]
fn malformed_traffic_retires_control_and_independent_shutdown_still_works() {
    use shoop_rt_transport::carrier::RecordPort;
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let Started {
            mut client,
            supervisor,
            ..
        } = start(transport, "automatic");
        client.connect().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(
            client.poll(),
            Some(shoop_rt_protocol::EngineMessage::Welcome { .. })
        ) {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        client.carrier_mut().port_mut().post(&[3, 0, 255]).unwrap();
        loop {
            if matches!(
                client.poll(),
                Some(shoop_rt_protocol::EngineMessage::Disconnected(_))
            ) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        drop(client);
        let report = supervisor.stop();
        assert!(report.success && report.midi_reset && report.cleaned);
    }
}

fn ready<C: shoop_rt_transport::carrier::Carrier, K: EngineClock>(backend: &mut RtBackend<C, K>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !backend.is_ready() {
        backend.pump();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
trait TraceStack {
    fn backend(&mut self) -> &mut dyn Backend;
    fn step(&mut self);
}
impl TraceStack for Timing {
    fn backend(&mut self) -> &mut dyn Backend {
        &mut self.backend
    }
    fn step(&mut self) {
        shoop_backend::contracts::TimingHarness::step(self).unwrap();
    }
}
struct LocalClock {
    dummy: shoop_rt_host_native::DummyControl,
    frame: u64,
}
impl LocalClock {
    fn render(&mut self, n: u64) {
        self.dummy.request_frames(n);
        assert!(self.dummy.wait_rendered(Duration::from_secs(5)));
        self.frame += n;
    }
}
impl EngineClock for LocalClock {
    fn advance(&mut self, elapsed: Duration) {
        self.render(elapsed.as_millis() as u64)
    }
    fn wait(&mut self) -> bool {
        self.render(1);
        true
    }
}
struct Local {
    backend: RtBackend<
        shoop_rt_transport::in_process::InProcess<shoop_rt_engine::ControlEndpoint>,
        LocalClock,
    >,
    _host: shoop_rt_host_native::NativeHost,
}
impl Local {
    fn new() -> Self {
        let mut config = config();
        if let DriverConfig::Dummy(dummy) = &mut config.driver {
            dummy.mode = DummyMode::Controlled;
            dummy.input = Some(ramp);
            dummy.wrap_cycle = Some(guarded);
        }
        let started = shoop_rt_host_native::start(&config).unwrap();
        let clock = LocalClock {
            dummy: started.host.dummy().unwrap().clone(),
            frame: 0,
        };
        let mut backend = RtBackend::begin(started.client, clock).unwrap();
        ready(&mut backend);
        Self {
            backend,
            _host: started.host,
        }
    }
}
impl TraceStack for Local {
    fn backend(&mut self) -> &mut dyn Backend {
        &mut self.backend
    }
    fn step(&mut self) {
        self.backend.pump();
        self.backend.clock_mut().render(1);
        self.backend.pump();
    }
}
#[derive(Debug, PartialEq)]
struct TraceResult {
    states: Vec<Vec<shoop_backend::BackendLoopState>>,
    audio: Vec<Vec<Vec<f32>>>,
    midi: Vec<Vec<shoop_backend::BackendMidiEvent>>,
}
fn advance(
    stack: &mut impl TraceStack,
    n: usize,
    states: &mut Vec<Vec<shoop_backend::BackendLoopState>>,
) {
    for _ in 0..n {
        stack.step();
        let snapshot = stack.backend().poll().unwrap();
        assert!(
            snapshot.mutation_failures.is_empty(),
            "{:?}",
            snapshot.mutation_failures
        );
        states.push(snapshot.loops.values().cloned().collect());
    }
}
fn trace(stack: &mut impl TraceStack) -> TraceResult {
    use shoop_backend::{
        BackendLoopMode as Mode, BackendMidiEvent, BackendPortRole, DirectTrackRequest,
    };
    let audio = stack
        .backend()
        .create_direct_track(DirectTrackRequest {
            port_name_base: "audio".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 2,
        })
        .unwrap();
    let midi = stack
        .backend()
        .create_direct_track(DirectTrackRequest {
            port_name_base: "midi".into(),
            audio_channels: 0,
            midi: true,
            initial_loops: 1,
        })
        .unwrap();
    let (sync, follower, midi_loop) = (audio.loops[0], audio.loops[1], midi.loops[0]);
    stack
        .backend()
        .set_loop_sync_source(follower, Some(sync))
        .unwrap();
    for port in &audio.ports {
        if port.role == BackendPortRole::AudioInput {
            stack
                .backend()
                .set_port_connected(port.id, "audio_in_1", true)
                .unwrap();
        }
    }
    let mut states = Vec::new();
    advance(stack, 24, &mut states);
    states.clear();
    stack
        .backend()
        .transition_loop(sync, Mode::Recording, None)
        .unwrap();
    stack
        .backend()
        .transition_loop(midi_loop, Mode::Recording, None)
        .unwrap();
    advance(stack, 8, &mut states);
    stack
        .backend()
        .inject_midi_input(
            midi.track_id,
            &[BackendMidiEvent {
                time: 0,
                data: vec![0x90, 60, 100],
            }],
        )
        .unwrap();
    advance(stack, 4, &mut states);
    stack
        .backend()
        .inject_midi_input(
            midi.track_id,
            &[BackendMidiEvent {
                time: 0,
                data: vec![0x80, 60, 0],
            }],
        )
        .unwrap();
    advance(stack, 4, &mut states);
    stack
        .backend()
        .transition_loop(sync, Mode::Playing, None)
        .unwrap();
    stack
        .backend()
        .transition_loop(midi_loop, Mode::Playing, None)
        .unwrap();
    advance(stack, 8, &mut states);
    stack
        .backend()
        .transition_loop(follower, Mode::Recording, Some(1))
        .unwrap();
    advance(stack, 48, &mut states);
    stack
        .backend()
        .transition_loop(follower, Mode::Playing, Some(0))
        .unwrap();
    advance(stack, 32, &mut states);
    for id in [sync, follower, midi_loop] {
        stack
            .backend()
            .transition_loop(id, Mode::Stopped, None)
            .unwrap();
    }
    advance(stack, 8, &mut states);
    let mut audio_data = Vec::new();
    for id in [sync, follower] {
        let mut data = None;
        for _ in 0..1000 {
            if let Some(value) = stack.backend().loop_audio_data(id).unwrap() {
                data = Some(value);
                break;
            }
            stack.step();
        }
        audio_data.push(
            data.expect("audio export")
                .into_iter()
                .map(|channel| channel.to_vec())
                .collect(),
        );
    }
    let mut data = None;
    for _ in 0..1000 {
        if let Some(value) = stack.backend().loop_midi_data(midi_loop).unwrap() {
            data = Some(value);
            break;
        }
        stack.step();
    }
    let midi = data
        .expect("MIDI export")
        .channels
        .into_iter()
        .map(|c| c.events)
        .collect();
    TraceResult {
        states,
        audio: audio_data,
        midi,
    }
}

#[cfg(feature = "cpal")]
#[shoop_test(no_wasm = "native process render failure")]
fn device_fault_remains_terminal_and_uses_the_existing_shutdown_policy() {
    for transport in [Transport::Socket, Transport::SharedMemory] {
        let mut config = config();
        config.driver = DriverConfig::Cpal(shoop_rt_host_native::CpalConfig {
            buffer_frames: Some(16),
            ..Default::default()
        });
        let Started {
            client, supervisor, ..
        } = Pending::spawn_command(command("cpal_fault"), transport)
            .unwrap()
            .start(&config)
            .unwrap();
        let mut backend = RtBackend::connect(client, Sleep::default()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while backend.poll().unwrap().status.driver_state
            != shoop_backend::BackendDriverState::Failed
        {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(backend);
        let report = supervisor.stop();
        assert!(report.success && report.cleaned, "{report:?}");
    }
}
