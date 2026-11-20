#![cfg(all(not(target_arch = "wasm32"), feature = "jack"))]

use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_api::*;
use shoop_app_client::{Client, State};
use shoop_app_protocol::{self as protocol, persistence as p, *};
use shoop_host::{
    config::{AppTransport, HostConfig},
    connection::{self, Connection},
    remote::{RemoteProfile, Target},
};
use shoop_rt_backend::{RtBackend, Sleep};
use shoop_rt_host_native::NativeHost;
use shoop_wasm_test_support::shoop_test;
use std::{
    process::Child,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Peer {
    source: jack::Port<jack::AudioOut>,
    sink: jack::Port<jack::AudioIn>,
    keys: jack::Port<jack::MidiOut>,
    synth: jack::Port<jack::MidiIn>,
    audio: Arc<AtomicUsize>,
    midi: Arc<AtomicUsize>,
}
impl jack::ProcessHandler for Peer {
    fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
        self.source.as_mut_slice(scope).fill(0.25);
        let mut writer = self.keys.writer(scope);
        for offset in [0, scope.n_frames() - 1] {
            let _ = writer.write(&jack::RawMidi {
                time: offset,
                bytes: &[0xb0, 1, 77],
            });
        }
        if self
            .sink
            .as_slice(scope)
            .iter()
            .all(|sample| *sample == 0.25)
        {
            self.audio.fetch_add(1, Ordering::Relaxed);
        }
        for event in self.synth.iter(scope) {
            if event.bytes == [0xb0, 1, 77] {
                self.midi.fetch_add(1, Ordering::Relaxed);
            }
        }
        jack::Control::Continue
    }
}
struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[shoop_test(no_wasm = "native host executable and JACK server")]
fn standalone_cli_publishes_session_ports_without_a_fixed_bank() {
    let _peer = match jack::Client::new("jack-cli-probe", jack::ClientOptions::NO_START_SERVER) {
        Ok((client, _)) => client,
        Err(_) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => return,
        Err(error) => panic!("required JACK server unavailable: {error}"),
    };
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("explicit-export.json");
    let mut child = ChildOwner(
        std::process::Command::new(env!("CARGO_BIN_EXE_shoop_host"))
            .args([
                "--driver",
                "jack",
                "--jack-client-name",
                "CLI-Studio",
                "--export-profile",
            ])
            .arg(&profile)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + shoop_host::launch::NATIVE_READINESS_TIMEOUT;
    let target = loop {
        if let Ok(profile) = shoop_host::remote::RemoteProfile::read(&profile) {
            if let Ok(target) = connection::resolve_target(&profile, Duration::from_secs(1)) {
                break target;
            }
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "CLI host exited before readiness"
        );
        assert!(Instant::now() < deadline, "CLI host did not become ready");
        thread::sleep(Duration::from_millis(20));
    };
    let report = connection::host_report(&target, Duration::from_secs(2)).unwrap();
    assert_eq!(report.effective.audio_driver, AudioDriverState::Running);
    let AudioDriverConfig::Jack(jack) = &report.requested.unwrap().driver else {
        panic!("CLI did not start direct JACK");
    };
    assert_eq!(jack.client_name, "CLI-Studio");
    let public = _peer.ports(Some("^CLI-Studio:"), None, jack::PortFlags::empty());
    assert_eq!(
        public.len(),
        4,
        "only sync and Master session ports should be registered"
    );
    assert!(public.iter().all(|name| !name.contains("audio_in_")
        && !name.contains("audio_out_")
        && !name.contains("midi_in_")
        && !name.contains("midi_out_")));
    connection::shutdown_ownerless(&target, Duration::from_secs(5)).unwrap();
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
enum Delivery {
    Local {
        service: Box<Service>,
        _host: NativeHost,
        id: ConnectionId,
    },
    Remote {
        connection: Option<Connection>,
        entry: Target,
        _owner: ChildOwner,
        _root: tempfile::TempDir,
    },
}
#[derive(Clone, Copy, Debug)]
enum DeliveryMode {
    Local,
    Socket,
    SharedMemory,
    Tcp,
}

struct Fixture {
    client: Client,
    delivery: Delivery,
}
impl Fixture {
    fn new(driver: AudioDriverConfig, independent: bool) -> Self {
        Self::new_on(
            driver,
            if independent {
                DeliveryMode::Socket
            } else {
                DeliveryMode::Local
            },
        )
    }
    fn new_on(driver: AudioDriverConfig, mode: DeliveryMode) -> Self {
        let settings = HostSettings {
            driver,
            pools: NewEnginePools {
                tracks: 4,
                loops: 32,
                loop_memory_mb: 16,
                ..Default::default()
            },
            ..Default::default()
        };
        Self::with_settings(settings, mode)
    }

    fn with_settings(settings: HostSettings, mode: DeliveryMode) -> Self {
        let delivery = if !matches!(mode, DeliveryMode::Local) {
            let root = tempfile::tempdir().unwrap();
            let profile = root.path().join("profile.json");
            let mut options = shoop_host::serve::StartOptions::new().unwrap();
            options.driver_preferences = Some(root.path().join("audio-driver.json"));
            options.use_configured_driver = true;
            if matches!(mode, DeliveryMode::Tcp) {
                options.lan = true;
                options.export_profile = Some(profile.clone());
            }
            let (child, entry) = shoop_host::launch::spawn(
                &HostConfig {
                    settings,
                    transport: if matches!(mode, DeliveryMode::SharedMemory) {
                        AppTransport::SharedMemory
                    } else {
                        AppTransport::Socket
                    },
                    ..Default::default()
                },
                options,
                std::path::Path::new(env!("CARGO_BIN_EXE_shoop_host")),
            )
            .unwrap();

            let entry = if matches!(mode, DeliveryMode::Tcp) {
                RemoteProfile::read(&profile).unwrap().into()
            } else {
                entry
            };
            let connection = Some(connection::connect(&entry, Duration::from_secs(2)).unwrap());
            Delivery::Remote {
                connection,
                entry,
                _owner: ChildOwner(child),
                _root: root,
            }
        } else {
            let (native, warnings) = shoop_host::engine::native_config(
                &settings.driver,
                &settings.pools,
                &shoop_host::engine::inventory(&settings.driver),
            );
            assert!(warnings.is_empty());
            let started = shoop_rt_host_native::start(&native).unwrap();
            let status = started.host.status().driver;
            let mut backend = RtBackend::connect(started.client, Sleep::default()).unwrap();
            backend.set_driver_profile(ResolvedAudioDriverConfig {
                configured: settings.driver.clone(),
                sample_rate: status.sample_rate,
                buffer_size: status.buffer_frames,
                instance_name: started.host.instance_name().into(),
            });
            let mut service = Service::new(
                Box::new(backend),
                ServiceOptions {
                    master_auto_connect: true,
                    loop_smoothing_ms: 0,
                },
            )
            .unwrap();
            if let Some(controller) = shoop_host::host_graph::controller(&started.host) {
                service.set_host_graph_controller(controller).unwrap();
            }
            service.set_host_settings(settings, Vec::new()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !service.is_ready() {
                service.tick(Duration::from_millis(2));
                assert!(
                    Instant::now() < deadline,
                    "local service initialization timed out"
                );
                thread::sleep(Duration::from_millis(2));
            }
            Delivery::Local {
                service: Box::new(service),
                _host: started.host,
                id: ConnectionId(1),
            }
        };
        let mut result = Self {
            client: Client::new(),
            delivery,
        };
        result.attach();
        result
    }
    fn send(&mut self, message: ClientMessage) {
        match &mut self.delivery {
            Delivery::Local { service, id, .. } => service.handle(*id, message).unwrap(),
            Delivery::Remote { connection, .. } => {
                connection.as_mut().unwrap().send(message).unwrap()
            }
        }
    }
    fn tick(&mut self) {
        match &mut self.delivery {
            Delivery::Local { service, id, .. } => {
                service.tick(Duration::from_millis(2));
                while let Some(message) = service.take_update(*id) {
                    self.client.receive(message).unwrap();
                }
            }
            Delivery::Remote { connection, .. } => {
                connection.as_mut().unwrap().poll(&mut self.client).unwrap();
            }
        }
        if let Some(message) = self.client.ready_message() {
            self.send(message);
            self.client.ready_sent().unwrap();
        }
        thread::sleep(Duration::from_millis(2));
    }
    #[track_caller]
    fn until(&mut self, done: impl FnMut(&mut Client) -> bool) {
        self.until_with_timeout(Duration::from_secs(10), done);
    }
    #[track_caller]
    fn until_with_timeout(&mut self, timeout: Duration, mut done: impl FnMut(&mut Client) -> bool) {
        let deadline = Instant::now() + timeout;
        loop {
            self.tick();
            if done(&mut self.client) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "JACK service workflow timed out: {:?}",
                self.client.snapshot()
            );
        }
    }
    fn attach(&mut self) {
        let message = self.client.attach().unwrap();
        self.send(message);
        self.until(|client| client.state() == State::Ready);
    }
    #[track_caller]
    fn command(&mut self, command: protocol::Command) -> Outcome {
        let outcome = self.outcome(command);
        assert!(!matches!(outcome, Outcome::Refused { .. }), "{outcome:?}");
        let revision = match outcome {
            Outcome::Confirmed { revision }
            | Outcome::TrackCreated { revision, .. }
            | Outcome::LoopCreated { revision, .. } => Some(revision),
            _ => None,
        };
        if let Some(revision) = revision {
            self.until(|client| {
                client
                    .snapshot()
                    .is_some_and(|state| state.revision >= revision)
            });
        }
        outcome
    }
    fn outcome(&mut self, command: protocol::Command) -> Outcome {
        let timeout = if matches!(command, protocol::Command::AudioDriver(_)) {
            shoop_host::launch::NATIVE_READINESS_TIMEOUT
        } else {
            Duration::from_secs(10)
        };
        let message = self.client.command(command, None).unwrap();
        let ClientMessage::Submit { operation, .. } = &message else {
            unreachable!()
        };
        let operation = *operation;
        self.send(message);
        self.until_with_timeout(timeout, |client| client.results().contains_key(&operation));
        self.client.results()[&operation].clone()
    }
    fn reconnect(&mut self) {
        match &mut self.delivery {
            Delivery::Local { service, id, .. } => {
                service.detach(*id);
                id.0 += 1;
            }
            Delivery::Remote { connection, .. } => {
                connection.take();
            }
        }
        thread::sleep(Duration::from_millis(50));
        if let Delivery::Remote {
            connection, entry, ..
        } = &mut self.delivery
        {
            *connection = Some(connection::connect(entry, Duration::from_secs(2)).unwrap());
        }
        self.client = Client::new();
        self.attach();
    }
    fn reply(&mut self, request: p::Request) -> p::Reply {
        let message = self.client.persistence(request).unwrap();
        self.send(message);
        let mut result = None;
        self.until(|client| {
            result = client.take_persistence_reply();
            result.is_some()
        });
        let result = result.unwrap();
        assert!(!matches!(result, p::Reply::Refused { .. }), "{result:?}");
        result
    }
    fn capture(&mut self, transfer: u64) -> Arc<persistence_image::Image> {
        let state = self.client.snapshot().unwrap();
        let key = p::Key {
            transfer: TransferId(transfer),
            epoch: state.persistence_epoch,
        };
        let target = p::Target::Session {
            revision: state.revision,
        };
        let p::Reply::Captured(description) = self.reply(p::Request::Capture { key, target })
        else {
            panic!("capture refused")
        };
        let mut objects = Vec::new();
        for object in 0..description.objects {
            let p::Reply::Object {
                object: descriptor, ..
            } = self.reply(p::Request::Describe { key, object })
            else {
                panic!("missing object")
            };
            let mut bytes = Vec::new();
            while (bytes.len() as u64) < descriptor.bytes {
                let p::Reply::Chunk { bytes: chunk, .. } = self.reply(p::Request::Read {
                    key,
                    object,
                    offset: bytes.len() as u64,
                }) else {
                    panic!("missing media")
                };
                bytes.extend_from_slice(&chunk);
            }
            objects.push(p::Object {
                description: descriptor,
                bytes,
            });
        }
        assert!(matches!(
            self.reply(p::Request::Cancel { key }),
            p::Reply::Closed { .. }
        ));
        persistence_transfer::assemble(objects).unwrap()
    }
    fn load(&mut self, image: Arc<persistence_image::Image>) {
        let state = self.client.snapshot().unwrap();
        let key = p::Key {
            transfer: TransferId(2),
            epoch: state.persistence_epoch,
        };
        let target = p::Target::Session {
            revision: state.revision,
        };
        let mut stream = persistence_transfer::UploadStream::new(image, key, target).unwrap();
        let mut ready = false;
        while let Some(request) = stream.next_request().unwrap() {
            if matches!(self.reply(request), p::Reply::Ready { .. }) {
                ready = true;
            }
        }
        if !ready {
            self.until(|client| {
                while let Some(reply) = client.take_persistence_reply() {
                    if matches!(reply, p::Reply::Ready { .. }) {
                        return true;
                    }
                    assert!(!matches!(reply, p::Reply::Refused { .. }), "{reply:?}");
                }
                false
            });
        }
        self.command(protocol::Command::CommitPersistence(key));
        self.until(|client| client.snapshot().unwrap().persistence_epoch != key.epoch);
    }
}

#[cfg(feature = "carla")]
#[shoop_test(no_wasm = "native Carla session companion and driver replacement")]
fn native_carla_session_archive_survives_driver_rate_and_period_change() {
    use persistence_image as image;
    use protocol::audio_driver::Command as Driver;
    use shoop_carla::state::{encode_state, CarlaChainType};
    use shoop_persistence_codec::codec::{Value, Work};

    if std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_none() {
        return;
    }
    for (hosting, delivery) in [
        (CarlaHosting::InProcess, DeliveryMode::Socket),
        (CarlaHosting::Subprocess, DeliveryMode::SharedMemory),
    ] {
        let mut processors = ProcessorSettings {
            builtin_fx: ProcessorPolicy::Off,
            oxisynth: ProcessorPolicy::Off,
            ..Default::default()
        };
        processors.carla.hosting = hosting;
        processors.carla.patchbay16x_instances = 2;
        let mut fixture = Fixture::with_settings(
            HostSettings {
                processors,
                driver: AudioDriverConfig::Dummy(DummyAudioDriverConfig::default()),
                pools: NewEnginePools {
                    tracks: 2,
                    loops: 16,
                    channels: 16,
                    buses: 2,
                    bus_channels: 16,
                    loop_memory_mb: 16,
                    ..Default::default()
                },
                ..Default::default()
            },
            delivery,
        );
        let patchbay = TrackProcessorTypeId::new(TrackProcessorTypeId::CARLA_PATCHBAY_16X);
        let Outcome::TrackCreated { track, .. } = fixture.command(Command::AddTrack(TrackSpec {
            name: "Archived Wide Track".into(),
            topology: TrackSpecTopology::DryWet {
                dry_audio_channels: 16,
                wet_audio_channels: 16,
                dry_midi: true,
                processor_type: patchbay.clone(),
                default_playback_mode: Default::default(),
            },
            latency: Default::default(),
            initial_output_bus_names: vec![],
            creation_request_id: None,
        })) else {
            panic!("Carla track creation")
        };
        fixture.command(Command::Track {
            track,
            action: TrackAction::InputMonitoringChanged {
                enabled: true,
                respect_auto_mute: false,
            },
        });
        fixture.command(Command::AddBus(BusSpec {
            name: "Archived Patchbay".into(),
            channel_count: 16,
            fx: Some(BusFxSpec {
                processor_type: patchbay,
            }),
            creation_request_id: None,
        }));
        fixture.until(|client| {
            let state = client.snapshot().unwrap();
            state.tracks.iter().any(|entry| {
                entry.name == "Archived Wide Track" && entry.fx.as_ref().is_some_and(|fx| fx.active)
            }) && state.buses.iter().any(|entry| {
                entry.name == "Archived Patchbay" && entry.fx.as_ref().is_some_and(|fx| fx.active)
            })
        });

        let mut captured = (*fixture.capture(1)).clone();
        let image::Manifest::Session(session) = &mut captured.manifest else {
            panic!("session capture")
        };
        session.global.sync = false;
        let track = session
            .tracks
            .iter_mut()
            .find(|entry| entry.name == "Archived Wide Track")
            .unwrap();
        let bus = session
            .buses
            .iter_mut()
            .find(|entry| entry.name == "Archived Patchbay")
            .unwrap();
        let recorded =
            include_str!("../../shoop_engine/test_data/carla_legacy_patchbay16_loaded_state.json");
        let project = include_str!("../../shoop_engine/test_data/carla_legacy_patchbay16_loaded_project.xml")
            .replace("Audio Input:Capture 2<", "Audio Input:Capture 16<")
            .replace("Audio Output:Playback 2<", "Audio Output:Playback 16<")
            .replacen("<Value>1</Value>", "<Value>0.5</Value>", 1)
            .replace(" </Patchbay>", concat!(
                "  <Connection><Source>Audio Input:Capture 8</Source><Target>Audio Output:Playback 8</Target></Connection>\n",
                "  <Connection><Source>Audio Input:Capture 9</Source><Target>Audio Output:Playback 9</Target></Connection>\n",
                " </Patchbay>"));
        let checkpoint = Arc::new(
            encode_state(CarlaChainType::Patchbay16x, project.as_bytes())
                .unwrap()
                .into_bytes(),
        );
        captured.payloads[track.processor_checkpoint.unwrap() as usize - 1] =
            image::Payload::ProcessorCheckpoint(Arc::clone(&checkpoint));
        captured.payloads[bus.fx.as_ref().unwrap().processor_checkpoint.unwrap() as usize - 1] =
            image::Payload::ProcessorCheckpoint(checkpoint);
        let take = &mut track.loops[0];
        take.frames = 1024;
        captured
            .payloads
            .push(image::Payload::ProcessorCheckpoint(Arc::new(
                recorded.as_bytes().to_vec(),
            )));
        take.recorded_processor_checkpoint = Some(captured.payloads.len() as u32);
        let lanes = [
            (0, 0.25_f32, 0.0625_f32),
            (7, 0.375, 0.375),
            (8, 0.5, 0.5),
            (15, 0.625, 0.15625),
        ];
        let mut dry = 0;
        for channel in &mut take.audio {
            let mut amplitude = 0.;
            if channel.mode == image::ChannelMode::Dry {
                amplitude = lanes
                    .iter()
                    .find(|&&(lane, _, _)| lane == dry)
                    .map_or(0., |&(_, sample, _)| sample);
                dry += 1;
            }
            captured
                .payloads
                .push(image::Payload::Audio(Arc::new(vec![amplitude; 1024])));
            channel.object = Some(captured.payloads.len() as u32);
            channel.frames = 1024;
        }
        captured.payloads.push(image::Payload::Midi(Arc::new(vec![
            MidiEventState {
                frame: 59,
                data: vec![0x90, 61, 100].into(),
            },
            MidiEventState {
                frame: 244,
                data: vec![0x80, 61, 0].into(),
            },
        ])));
        take.midi[0].channel.object = Some(captured.payloads.len() as u32);
        take.midi[0].channel.frames = 1024;
        session.routes.extend(
            track
                .ports
                .iter()
                .filter(|port| port.state.role == PortRole::AudioOutput)
                .zip(&bus.channels)
                .map(|(port, channel)| MixerRouteState {
                    source_port_id: port.state.id,
                    destination_channel_id: channel.id,
                }),
        );
        let archive = |image| match (Work::Encode {
            image,
            kind: IoTaskKind::SaveSession,
            wav: false,
            standard: false,
            channels: vec![],
        })
        .run()
        .unwrap()
        {
            Value::File { bytes, .. } => bytes,
            _ => panic!("session archive"),
        };
        let load = |bytes, rate| {
            let Value::Decoded(decoded) = (Work::Decode {
                kind: IoTaskKind::LoadSession,
                bytes,
                rate,
            })
            .run()
            .unwrap() else {
                panic!("decoded session")
            };
            let Value::Image(image) = (Work::Prepare {
                decoded,
                mapping: vec![],
                update_length: true,
            })
            .run()
            .unwrap() else {
                panic!("prepared session")
            };
            image
        };
        fixture.load(load(archive(Arc::new(captured)), 48_000));
        let track = fixture
            .client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|entry| entry.name == "Archived Wide Track")
            .unwrap();
        let (track, take) = (track.id, track.loops[0].id);
        let expected_audio = |client: &mut Client| {
            let state = client.snapshot().unwrap();
            let bus = state
                .buses
                .iter()
                .find(|entry| entry.name == "Archived Patchbay")
                .unwrap();
            lanes.iter().all(|&(lane, _, expected)| {
                (bus.output_peaks_db[lane] - 20. * expected.log10()).abs() < 0.02
            })
        };
        let play = |fixture: &mut Fixture| {
            fixture.command(Command::Loop {
                track,
                loop_id: take,
                action: LoopAction::PlayDryClicked,
            });
            fixture.until(expected_audio);
        };
        play(&mut fixture);

        let request_driver = |fixture: &mut Fixture, buffer_size| {
            fixture.command(Command::AudioDriver(Driver::Request(
                AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate: 96_000,
                    buffer_size,
                    ..Default::default()
                }),
            )));
            fixture.until(|client| {
                client.snapshot().unwrap().audio_drivers.switch.status
                    == AudioDriverSwitchStatus::AwaitingConfirmation
            });
            let request_id = fixture
                .client
                .snapshot()
                .unwrap()
                .audio_drivers
                .switch
                .request_id;
            Command::AudioDriver(Driver::Confirm {
                request_id,
                accept: true,
            })
        };
        let switch = request_driver(&mut fixture, 512);
        fixture.command(switch);
        fixture.until(|client| {
            let switch = &client.snapshot().unwrap().audio_drivers.switch;
            assert!(
                !matches!(
                    switch.status,
                    AudioDriverSwitchStatus::Fatal | AudioDriverSwitchStatus::Failed
                ),
                "{switch:?}"
            );
            switch.status == AudioDriverSwitchStatus::Completed
        });
        let active = fixture
            .client
            .snapshot()
            .unwrap()
            .audio_drivers
            .active
            .as_ref()
            .unwrap();
        assert_eq!((active.sample_rate, active.buffer_size), (96_000, 512));
        play(&mut fixture);
        let saved = load(archive(fixture.capture(3)), 96_000);
        let image::Manifest::Session(session) = &saved.manifest else {
            panic!("session manifest")
        };
        let saved_track = session
            .tracks
            .iter()
            .find(|entry| entry.name == "Archived Wide Track")
            .unwrap();
        let saved_take = &saved_track.loops[0];
        assert_eq!(saved_take.frames, 2048);
        assert_eq!(saved_take.audio.len(), 32);
        let image::Payload::Midi(events) =
            &saved.payloads[saved_take.midi[0].channel.object.unwrap() as usize - 1]
        else {
            panic!("retimed MIDI")
        };
        assert_eq!(events[0].frame, 118);
        assert!(saved_take.recorded_processor_checkpoint.is_some());
        assert!(saved_track.processor_checkpoint.is_some());
        assert!(session
            .buses
            .iter()
            .find(|bus| bus.name == "Archived Patchbay")
            .unwrap()
            .fx
            .as_ref()
            .unwrap()
            .processor_checkpoint
            .is_some());

        #[cfg(target_os = "linux")]
        if hosting == CarlaHosting::Subprocess
            && std::env::var_os("SHOOP_REQUIRE_CARLA_UI_TESTS").is_some()
        {
            let Delivery::Remote { _owner, .. } = &fixture.delivery else {
                unreachable!("isolated Carla has a standalone host")
            };
            let parent = _owner.0.id();
            let state = fixture.client.snapshot().unwrap();
            let bus = state
                .buses
                .iter()
                .find(|entry| entry.name == "Archived Patchbay")
                .unwrap()
                .id;
            let fx_status = |client: &Client| {
                let state = client.snapshot().unwrap();
                let track = state.tracks.iter().find(|t| t.id == track).unwrap();
                let bus = state.buses.iter().find(|b| b.id == bus).unwrap();
                assert_eq!(track.loops[0].mode, LoopMode::PlayingDryThroughWet);
                [track.fx.as_ref().unwrap(), bus.fx.as_ref().unwrap()]
                    .map(|fx| (fx.lifecycle, fx.generation, fx.active))
            };
            // The new driver's bank has two live claims and two spares.
            for chain in [b"1", b"2"] {
                let mut before = fx_status(&fixture.client);
                carla_worker::kill(parent, chain);
                let mut crashed = None;
                fixture.until(|client| {
                    let status = fx_status(client);
                    crashed = status.iter().position(|&(lifecycle, _, active)| {
                        lifecycle == FxLifecycle::Crashed && !active
                    });
                    crashed.is_some() && !expected_audio(client)
                });
                let failed = crashed.unwrap();
                assert_eq!(fx_status(&fixture.client)[1 - failed], before[1 - failed]);
                fixture.command(if failed == 0 {
                    Command::Track {
                        track,
                        action: TrackAction::FxToggleOrRecover,
                    }
                } else {
                    Command::Bus {
                        bus,
                        action: BusAction::FxToggleOrRecover,
                    }
                });
                before[failed].1 += 1;
                fixture.until(|client| fx_status(client) == before && expected_audio(client));
            }
        }

        // This period exceeds Carla Native's maximum block. The target engine
        // cannot prepare a Carla bank, so the prior format and projects must return.
        let switch = request_driver(&mut fixture, 16_384);
        let outcome = fixture.outcome(switch);
        assert!(matches!(outcome, Outcome::Refused { .. }), "{outcome:?}");
        fixture.until_with_timeout(shoop_host::launch::NATIVE_READINESS_TIMEOUT, |client| {
            let switch = &client.snapshot().unwrap().audio_drivers.switch;
            assert_ne!(switch.status, AudioDriverSwitchStatus::Fatal, "{switch:?}");
            switch.status == AudioDriverSwitchStatus::Failed
        });
        let snapshot = fixture.client.snapshot().unwrap();
        assert_eq!(
            snapshot.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Failed
        );
        assert!(
            snapshot
                .audio_drivers
                .switch
                .message
                .contains("prior driver and session were restored"),
            "{:?}",
            snapshot.audio_drivers.switch
        );
        let restored = snapshot.audio_drivers.active.as_ref().unwrap();
        assert_eq!((restored.sample_rate, restored.buffer_size), (96_000, 512));
        play(&mut fixture);
    }
}

#[cfg(all(feature = "carla", target_os = "linux"))]
#[path = "support/carla_worker.rs"]
mod carla_worker;

#[shoop_test(no_wasm = "native JACK application service and independent host")]
fn native_jack_service_records_replays_reconnects_and_restores_audio_midi_and_routing() {
    for independent in [false, true] {
        let name = format!("jack-service-peer-{}-{independent}", std::process::id());
        let peer = match jack::Client::new(&name, jack::ClientOptions::NO_START_SERVER) {
            Ok((client, _)) => client,
            Err(error) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => {
                eprintln!("JACK unavailable: {error}");
                return;
            }
            Err(error) => panic!("required JACK server unavailable: {error}"),
        };
        let audio = Arc::new(AtomicUsize::new(0));
        let midi = Arc::new(AtomicUsize::new(0));
        let source = peer
            .register_port("source", jack::AudioOut::default())
            .unwrap();
        let sink = peer
            .register_port("sink", jack::AudioIn::default())
            .unwrap();
        let keys = peer
            .register_port("keys", jack::MidiOut::default())
            .unwrap();
        let synth = peer
            .register_port("synth", jack::MidiIn::default())
            .unwrap();
        let driver = JackAudioDriverConfig {
            client_name: format!("jack-service-{}-{independent}", std::process::id()),
            ..Default::default()
        };
        let peer = peer
            .activate_async(
                (),
                Peer {
                    source,
                    sink,
                    keys,
                    synth,
                    audio: audio.clone(),
                    midi: midi.clone(),
                },
            )
            .unwrap();
        let mut f = Fixture::new(AudioDriverConfig::Jack(driver), independent);
        let active = f
            .client
            .snapshot()
            .unwrap()
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .clone();
        assert_eq!(active.configured.kind(), AudioDriverKind::Jack);
        assert!(active.instance_name.starts_with("jack-service-"));
        f.command(protocol::Command::Global(GlobalControlAction::SetSync(
            false,
        )));
        let Outcome::TrackCreated { track, loops, .. } =
            f.command(protocol::Command::AddTrack(TrackSpec {
                name: "JACK take".into(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 1,
                    midi: true,
                },
                latency: Default::default(),
                initial_output_bus_names: vec!["Master".into()],
                creation_request_id: None,
            }))
        else {
            panic!("track missing")
        };
        let ports: Vec<_> = f.client.snapshot().unwrap().connections.application_ports.iter().filter(|port| matches!(port.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track)).cloned().collect();
        for (role, name) in [
            (PortRole::AudioInput, "source"),
            (PortRole::MidiInput, "keys"),
            (PortRole::MidiOutput, "synth"),
        ] {
            let port = ports.iter().find(|port| port.role == role).unwrap();
            f.command(protocol::Command::SetPortConnected {
                port: port.id,
                host_port: HostPortId::new(format!("{}:{name}", peer.as_client().name())),
                connected: true,
            });
        }
        let master = f
            .client
            .snapshot()
            .unwrap()
            .buses
            .iter()
            .find(|bus| bus.name == "Master")
            .unwrap()
            .channels[0]
            .output_port_id;
        f.command(protocol::Command::SetPortConnected {
            port: master,
            host_port: HostPortId::new(format!("{}:sink", peer.as_client().name())),
            connected: true,
        });
        f.command(protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        });
        f.until(|client| client.snapshot().unwrap().tracks[1].loops[0].mode == LoopMode::Recording);
        thread::sleep(Duration::from_millis(80));
        f.command(protocol::Command::SetLoopTimeline {
            loop_id: loops[0],
            start_offset: Some(1),
            preplay_samples: Some(17),
            loop_length: None,
        });
        f.command(protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::PlayClicked,
        });
        f.until(|client| client.snapshot().unwrap().tracks[1].loops[0].mode == LoopMode::Playing);
        f.until(|_| audio.load(Ordering::Relaxed) > 3 && midi.load(Ordering::Relaxed) > 6);
        let raw_length = f.client.snapshot().unwrap().tracks[1].loops[0].length_frames;
        f.command(protocol::Command::SetLoopTimeline {
            loop_id: loops[0],
            start_offset: None,
            preplay_samples: None,
            loop_length: Some(raw_length / 2),
        });
        assert!(f.client.snapshot().unwrap().tracks[1].loops[0]
            .channel_timings
            .iter()
            .all(|channel| channel.start_offset == 1 && channel.preplay_samples == 17));
        let before = (audio.load(Ordering::Relaxed), midi.load(Ordering::Relaxed));
        f.reconnect();
        assert_eq!(f.client.snapshot().unwrap().tracks[1].loops[0].id, loops[0]);
        assert_eq!(
            f.client.snapshot().unwrap().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
        assert_eq!(
            f.client
                .snapshot()
                .unwrap()
                .audio_drivers
                .active
                .as_ref()
                .unwrap()
                .instance_name,
            active.instance_name
        );
        f.until(|_| {
            audio.load(Ordering::Relaxed) > before.0 && midi.load(Ordering::Relaxed) > before.1
        });
        let read = f.client.read_media(loops[0]).unwrap();
        f.send(read);
        f.until(|client| client.media().is_some());
        let media = f.client.media().unwrap().as_ref().unwrap();
        assert!(media.audio[0].samples.len() as u64 >= raw_length);
        assert!(f.client.snapshot().unwrap().tracks[1].loops[0].length_frames < raw_length);
        assert!(media.audio[0].samples.iter().any(|sample| *sample == 0.25));
        assert!(media.midi[0]
            .events
            .iter()
            .any(|event| event.data.as_ref() == [0xb0, 1, 77]));
        let image = f.capture(1);
        use shoop_persistence_codec::codec::{Value, Work};
        let Value::File { bytes, .. } = (Work::Encode {
            image: image.clone(),
            kind: IoTaskKind::SaveSession,
            wav: false,
            standard: false,
            channels: Vec::new(),
        })
        .run()
        .unwrap() else {
            panic!("session encode failed")
        };
        let Value::Decoded(decoded) = (Work::Decode {
            kind: IoTaskKind::LoadSession,
            bytes,
            rate: active.sample_rate,
        })
        .run()
        .unwrap() else {
            panic!("session decode failed")
        };
        let Value::Image(restored) = (Work::Prepare {
            decoded,
            mapping: Vec::new(),
            update_length: true,
        })
        .run()
        .unwrap() else {
            panic!("session prepare failed")
        };
        assert_eq!(restored.payloads, image.payloads);
        let image = restored;
        f.load(image.clone());
        assert_ne!(f.client.snapshot().unwrap().tracks[1].loops[0].id, loops[0]);
        assert_eq!(
            f.client.snapshot().unwrap().tracks[1].loops[0].mode,
            LoopMode::Stopped
        );
        assert!(
            f.client
                .snapshot()
                .unwrap()
                .connections
                .confirmed_links
                .iter()
                .any(|link| link.host_port_id.as_str()
                    == format!("{}:synth", peer.as_client().name()))
        );
        let state = &f.client.snapshot().unwrap().tracks[1].loops[0];
        assert_eq!(state.length_frames, raw_length / 2);
        assert!(state
            .channel_timings
            .iter()
            .all(|channel| channel.start_offset == 1 && channel.preplay_samples == 17));
        assert_eq!(f.capture(3).payloads, image.payloads);
        drop(f);
        drop(peer);
    }
}

fn peer_links(graph: &HostGraphState, peer: &str) -> usize {
    graph
        .links
        .iter()
        .filter(|link| {
            [link.source, link.destination].iter().any(|id| {
                graph
                    .port(*id)
                    .is_some_and(|port| port.name.starts_with(&format!("{peer}:")))
            })
        })
        .count()
}

fn graph_request(f: &Fixture, local: &str, peer: &str, connected: bool) -> HostGraphRequest {
    let state = &f.client.snapshot().unwrap().host_graph;
    HostGraphRequest {
        epoch: state.epoch,
        local: state
            .ports
            .iter()
            .find(|port| port.name == local)
            .unwrap()
            .id,
        peer: state
            .ports
            .iter()
            .find(|port| port.name == peer)
            .unwrap()
            .id,
        connected,
    }
}
fn graph_link(f: &mut Fixture, local: &str, peer: &str, connected: bool) {
    let request = graph_request(f, local, peer, connected);
    f.command(protocol::Command::SetHostGraphConnected(request));
    f.until(|client| {
        let state = &client.snapshot().unwrap().host_graph;
        let local = state
            .ports
            .iter()
            .find(|port| port.name == local)
            .unwrap()
            .id;
        let peer = state
            .ports
            .iter()
            .find(|port| port.name == peer)
            .unwrap()
            .id;
        state.connected(local, peer) == connected
    });
}

#[shoop_test(no_wasm = "real JACK graph over native session carriers")]
fn native_jack_graph_hotplug_control_observation_and_detached_playback() {
    for mode in [
        DeliveryMode::Local,
        DeliveryMode::Socket,
        DeliveryMode::SharedMemory,
        DeliveryMode::Tcp,
    ] {
        eprintln!("JACK graph delivery: {mode:?}");
        let probe = match jack::Client::new("graph-probe", jack::ClientOptions::NO_START_SERVER) {
            Ok((client, _)) => client,
            Err(_) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => return,
            Err(error) => panic!("required JACK server unavailable: {error}"),
        };
        drop(probe);
        let driver = JackAudioDriverConfig {
            client_name: format!("graph-host-{}-{mode:?}", std::process::id()),
            ..Default::default()
        };
        let mut f = Fixture::new_on(AudioDriverConfig::Jack(driver), mode);
        f.until(|client| client.snapshot().unwrap().host_graph.status == HostGraphStatus::Ready);
        let initial = f.client.snapshot().unwrap().host_graph.clone();
        assert!(initial.valid());
        assert_eq!(
            initial
                .ports
                .iter()
                .filter(|port| port.local.is_some())
                .count(),
            4
        );
        let owner = f
            .client
            .snapshot()
            .unwrap()
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .instance_name
            .clone();
        let local = |short| format!("{owner}:{short}");
        let peer_name = format!("graph-peer-{}-{mode:?}", std::process::id());
        let (peer, _) =
            jack::Client::new(&peer_name, jack::ClientOptions::NO_START_SERVER).unwrap();
        let name = |short| format!("{}:{short}", peer.name());
        let source_name = name("source");
        let sink_name = name("sink");
        let keys_name = name("keys");
        let synth_name = name("synth");
        let source = peer
            .register_port("source", jack::AudioOut::default())
            .unwrap();
        let sink = peer
            .register_port("sink", jack::AudioIn::default())
            .unwrap();
        let keys = peer
            .register_port("keys", jack::MidiOut::default())
            .unwrap();
        let synth = peer
            .register_port("synth", jack::MidiIn::default())
            .unwrap();
        let audio = Arc::new(AtomicUsize::new(0));
        let midi = Arc::new(AtomicUsize::new(0));
        let peer = peer
            .activate_async(
                (),
                Peer {
                    source,
                    sink,
                    keys,
                    synth,
                    audio: audio.clone(),
                    midi: midi.clone(),
                },
            )
            .unwrap();
        f.until(|client| {
            let graph = &client.snapshot().unwrap().host_graph;
            graph
                .ports
                .iter()
                .filter(|port| port.name.starts_with(&format!("{peer_name}:")))
                .count()
                == 4
        });
        f.command(protocol::Command::Global(GlobalControlAction::SetSync(
            false,
        )));
        let Outcome::TrackCreated { track, loops, .. } =
            f.command(protocol::Command::AddTrack(TrackSpec {
                name: "Graph take".into(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 1,
                    midi: true,
                },
                latency: Default::default(),
                initial_output_bus_names: vec!["Master".into()],
                creation_request_id: None,
            }))
        else {
            panic!("track missing")
        };
        let mut wrong_epoch = graph_request(&f, &local("graph_take_direct_in"), &source_name, true);
        wrong_epoch.epoch = wrong_epoch.epoch.wrapping_add(1).max(1);
        assert!(matches!(
            f.outcome(protocol::Command::SetHostGraphConnected(wrong_epoch)),
            Outcome::Refused {
                reason: Refusal::RevisionConflict,
                ..
            }
        ));
        for (a, b) in [
            (local("graph_take_direct_in"), keys_name.clone()),
            (local("master_out_1"), source_name.clone()),
            (source_name.clone(), sink_name.clone()),
        ] {
            let request = graph_request(&f, &a, &b, true);
            assert!(matches!(
                f.outcome(protocol::Command::SetHostGraphConnected(request)),
                Outcome::Refused {
                    reason: Refusal::Invalid,
                    ..
                }
            ));
        }
        for (bank, other) in [
            ("graph_take_direct_in", &source_name),
            ("master_out_1", &sink_name),
            ("graph_take_direct_midi_in", &keys_name),
            ("graph_take_direct_midi_out", &synth_name),
        ] {
            graph_link(&mut f, &local(bank), other, true);
            graph_link(&mut f, &local(bank), other, true);
        }
        assert_eq!(
            peer_links(&f.client.snapshot().unwrap().host_graph, &peer_name),
            4
        );
        f.command(protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        });
        f.until(|client| client.snapshot().unwrap().tracks[1].loops[0].mode == LoopMode::Recording);
        thread::sleep(Duration::from_millis(80));
        f.command(protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::PlayClicked,
        });
        f.until(|_| audio.load(Ordering::Relaxed) > 3 && midi.load(Ordering::Relaxed) > 6);
        let before = (audio.load(Ordering::Relaxed), midi.load(Ordering::Relaxed));
        if !matches!(mode, DeliveryMode::SharedMemory) {
            f.reconnect();
        }
        f.until(|_| {
            audio.load(Ordering::Relaxed) > before.0 + 3
                && midi.load(Ordering::Relaxed) > before.1 + 6
        });
        assert_eq!(f.client.snapshot().unwrap().host_graph.epoch, initial.epoch);
        assert_eq!(
            peer_links(&f.client.snapshot().unwrap().host_graph, &peer_name),
            4
        );
        graph_link(&mut f, &local("master_out_1"), &sink_name, false);
        graph_link(
            &mut f,
            &local("graph_take_direct_midi_out"),
            &synth_name,
            false,
        );
        thread::sleep(Duration::from_millis(30));
        let disconnected = (audio.load(Ordering::Relaxed), midi.load(Ordering::Relaxed));
        thread::sleep(Duration::from_millis(30));
        assert_eq!(
            disconnected,
            (audio.load(Ordering::Relaxed), midi.load(Ordering::Relaxed))
        );
        peer.as_client()
            .connect_ports_by_name(&local("master_out_1"), &sink_name)
            .unwrap();
        peer.as_client()
            .connect_ports_by_name(&local("graph_take_direct_midi_out"), &synth_name)
            .unwrap();
        f.until(|client| peer_links(&client.snapshot().unwrap().host_graph, &peer_name) == 4);
        f.until(|_| {
            audio.load(Ordering::Relaxed) > disconnected.0 + 3
                && midi.load(Ordering::Relaxed) > disconnected.1 + 6
        });
        let before_rename = graph_request(&f, &local("graph_take_direct_in"), &source_name, true);
        let mut renamed = peer.as_client().port_by_name(&source_name).unwrap();
        renamed.set_name("source_renamed").unwrap();
        let renamed_name = format!("{peer_name}:source_renamed");
        f.until(|client| {
            let graph = &client.snapshot().unwrap().host_graph;
            graph.ports.iter().any(|port| port.name == renamed_name)
                && !graph.ports.iter().any(|port| port.name == source_name)
                && graph
                    .links
                    .iter()
                    .any(|link| link.source == before_rename.peer)
        });
        assert!(f
            .client
            .snapshot()
            .unwrap()
            .host_graph
            .ports
            .iter()
            .any(|port| port.id == before_rename.peer && port.name == renamed_name));
        f.command(protocol::Command::SetHostGraphConnected(before_rename));
        renamed.set_name("source").unwrap();
        f.until(|client| {
            client
                .snapshot()
                .unwrap()
                .host_graph
                .ports
                .iter()
                .any(|port| port.name == source_name)
        });
        drop(renamed);
        let old = graph_request(&f, &local("graph_take_direct_in"), &source_name, true);
        drop(peer);
        f.until(|client| {
            let graph = &client.snapshot().unwrap().host_graph;
            peer_links(graph, &peer_name) == 0
                && !graph
                    .ports
                    .iter()
                    .any(|port| port.name.starts_with(&format!("{peer_name}:")))
        });
        let (replacement, _) =
            jack::Client::new(&peer_name, jack::ClientOptions::NO_START_SERVER).unwrap();
        let _source = replacement
            .register_port("source", jack::AudioOut::default())
            .unwrap();
        f.until(|client| {
            client
                .snapshot()
                .unwrap()
                .host_graph
                .ports
                .iter()
                .any(|port| port.name == source_name)
        });
        assert!(matches!(
            f.outcome(protocol::Command::SetHostGraphConnected(old)),
            Outcome::Refused {
                reason: Refusal::RevisionConflict,
                ..
            }
        ));
    }
}

#[shoop_test(no_wasm = "dynamic JACK publications over every native session carrier")]
fn native_jack_dynamic_tracks_buses_connections_and_session_restore() {
    for mode in [
        DeliveryMode::Local,
        DeliveryMode::Socket,
        DeliveryMode::SharedMemory,
        DeliveryMode::Tcp,
    ] {
        eprintln!("dynamic JACK delivery: {mode:?}");
        let (peer, _) = match jack::Client::new(
            &format!("full-jack-peer-{}-{mode:?}", std::process::id()),
            jack::ClientOptions::NO_START_SERVER,
        ) {
            Ok(peer) => peer,
            Err(_) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => return,
            Err(error) => panic!("required JACK server unavailable: {error}"),
        };
        let peer_name = peer.name().to_owned();
        let source = peer
            .register_port("source", jack::AudioOut::default())
            .unwrap();
        let sink = peer
            .register_port("sink", jack::AudioIn::default())
            .unwrap();
        let keys = peer
            .register_port("keys", jack::MidiOut::default())
            .unwrap();
        let synth = peer
            .register_port("synth", jack::MidiIn::default())
            .unwrap();
        let audio = Arc::new(AtomicUsize::new(0));
        let midi = Arc::new(AtomicUsize::new(0));
        let peer = peer
            .activate_async(
                (),
                Peer {
                    source,
                    sink,
                    keys,
                    synth,
                    audio: audio.clone(),
                    midi: midi.clone(),
                },
            )
            .unwrap();
        let mut f = Fixture::new_on(
            AudioDriverConfig::Jack(JackAudioDriverConfig {
                client_name: format!("full-jack-{}-{mode:?}", std::process::id()),
            }),
            mode,
        );
        f.command(protocol::Command::Global(GlobalControlAction::SetSync(
            false,
        )));
        f.command(protocol::Command::AddBus(BusSpec {
            name: "Live".into(),
            channel_count: 1,
            fx: None,
            creation_request_id: None,
        }));
        let bus = f
            .client
            .snapshot()
            .unwrap()
            .buses
            .iter()
            .find(|bus| bus.name == "Live")
            .unwrap()
            .clone();
        let Outcome::TrackCreated { track, loops, .. } =
            f.command(protocol::Command::AddTrack(TrackSpec {
                name: "Full JACK".into(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 1,
                    midi: true,
                },
                latency: Default::default(),
                initial_output_bus_names: vec!["Live".into()],
                creation_request_id: None,
            }))
        else {
            panic!("track missing")
        };
        let ports: Vec<_> = f.client.snapshot().unwrap().connections.application_ports.iter().filter(|port| matches!(port.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track)).cloned().collect();
        assert_eq!(ports.len(), 4);
        for port in &ports {
            let graph = &f.client.snapshot().unwrap().host_graph;
            let publication = graph.publication(port.publication.unwrap()).unwrap();
            assert!(matches!(
                publication.state,
                shoop_port_graph::PublicationState::Published(_)
            ));
            assert_eq!(publication.spec.name, port.name);
            assert!(port.name.starts_with("full_jack_direct_"));
        }
        for (role, target) in [
            (PortRole::AudioInput, "source"),
            (PortRole::MidiInput, "keys"),
            (PortRole::MidiOutput, "synth"),
        ] {
            let port = ports.iter().find(|port| port.role == role).unwrap();
            f.command(protocol::Command::SetPortConnected {
                port: port.id,
                host_port: HostPortId::new(format!("{peer_name}:{target}")),
                connected: true,
            });
        }
        f.command(protocol::Command::SetPortConnected {
            port: bus.channels[0].output_port_id,
            host_port: HostPortId::new(format!("{peer_name}:sink")),
            connected: true,
        });
        f.command(protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        });
        f.until(|client| client.snapshot().unwrap().tracks[1].loops[0].mode == LoopMode::Recording);
        thread::sleep(Duration::from_millis(60));
        f.command(protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::PlayClicked,
        });
        f.until(|_| audio.load(Ordering::Relaxed) > 3 && midi.load(Ordering::Relaxed) > 6);
        // External patchbay changes must appear in the ordinary connection view.
        let bus_port = f
            .client
            .snapshot()
            .unwrap()
            .connections
            .application_ports
            .iter()
            .find(|port| port.id == bus.channels[0].output_port_id)
            .unwrap()
            .clone();
        let owner = f
            .client
            .snapshot()
            .unwrap()
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .instance_name
            .clone();
        let local_bus = format!("{owner}:{}", bus_port.name);
        peer.as_client()
            .disconnect_ports_by_name(&local_bus, &format!("{peer_name}:sink"))
            .unwrap();
        f.until(|client| {
            !client
                .snapshot()
                .unwrap()
                .connections
                .confirmed_links
                .iter()
                .any(|link| link.application_port_id == bus_port.id)
        });
        peer.as_client()
            .connect_ports_by_name(&local_bus, &format!("{peer_name}:sink"))
            .unwrap();
        f.until(|client| {
            client
                .snapshot()
                .unwrap()
                .connections
                .confirmed_links
                .iter()
                .any(|link| link.application_port_id == bus_port.id)
        });
        let image = f.capture(1);
        if let persistence_image::Manifest::Session(session) = &image.manifest {
            assert!(session
                .tracks
                .iter()
                .flat_map(|track| &track.ports)
                .all(|port| port.state.publication.is_none()));
            assert!(session
                .tracks
                .iter()
                .flat_map(|track| &track.ports)
                .any(|port| port
                    .external_connections
                    .iter()
                    .any(|name| name == &format!("{peer_name}:synth"))));
        } else {
            panic!("session capture missing")
        }
        if !matches!(mode, DeliveryMode::SharedMemory) {
            f.reconnect();
        }
        let before = (audio.load(Ordering::Relaxed), midi.load(Ordering::Relaxed));
        f.until(|_| {
            audio.load(Ordering::Relaxed) > before.0 + 3
                && midi.load(Ordering::Relaxed) > before.1 + 6
        });
        drop(peer);
        f.until(|client| {
            !client
                .snapshot()
                .unwrap()
                .host_graph
                .ports
                .iter()
                .any(|port| port.name.starts_with(&format!("{peer_name}:")))
        });
        f.load(image);
        assert!(f
            .client
            .snapshot()
            .unwrap()
            .connections
            .pending_links
            .iter()
            .any(|link| link.host_port_id.as_str() == format!("{peer_name}:synth")));
        let (replacement, _) =
            jack::Client::new(&peer_name, jack::ClientOptions::NO_START_SERVER).unwrap();
        let source = replacement
            .register_port("source", jack::AudioOut::default())
            .unwrap();
        let sink = replacement
            .register_port("sink", jack::AudioIn::default())
            .unwrap();
        let keys = replacement
            .register_port("keys", jack::MidiOut::default())
            .unwrap();
        let synth = replacement
            .register_port("synth", jack::MidiIn::default())
            .unwrap();
        let peer = replacement
            .activate_async(
                (),
                Peer {
                    source,
                    sink,
                    keys,
                    synth,
                    audio: audio.clone(),
                    midi: midi.clone(),
                },
            )
            .unwrap();
        let restored = f
            .client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|track| track.name == "Full JACK")
            .unwrap()
            .clone();
        assert_ne!(restored.id, track);
        f.until(|client| {
            client
                .snapshot()
                .unwrap()
                .connections
                .confirmed_links
                .iter()
                .any(|link| link.host_port_id.as_str() == format!("{peer_name}:synth"))
        });
        f.command(protocol::Command::Loop {
            track: restored.id,
            loop_id: restored.loops[0].id,
            action: LoopAction::PlayClicked,
        });
        let before = (audio.load(Ordering::Relaxed), midi.load(Ordering::Relaxed));
        f.until(|_| {
            audio.load(Ordering::Relaxed) > before.0 + 3
                && midi.load(Ordering::Relaxed) > before.1 + 6
        });
        let published: Vec<_> = f.client.snapshot().unwrap().connections.application_ports.iter().filter(|port| matches!(port.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == restored.id)).map(|port| port.publication.unwrap()).collect();
        f.command(protocol::Command::Track {
            track: restored.id,
            action: TrackAction::Remove,
        });
        assert!(published.iter().all(|key| f
            .client
            .snapshot()
            .unwrap()
            .host_graph
            .publication(*key)
            .is_none()));
        let bus = f
            .client
            .snapshot()
            .unwrap()
            .buses
            .iter()
            .find(|bus| bus.name == "Live")
            .unwrap()
            .id;
        f.command(protocol::Command::Bus {
            bus,
            action: BusAction::Remove,
        });
        assert!(peer.as_client().port_by_name(&local_bus).is_none());
    }
}

#[shoop_test(no_wasm = "real JACK dynamic Master auto-connect and session restoration")]
fn native_jack_startup_playback_has_one_master_signal_path() {
    for mode in [
        DeliveryMode::Local,
        DeliveryMode::Socket,
        DeliveryMode::SharedMemory,
        DeliveryMode::Tcp,
    ] {
        let (probe, _) =
            match jack::Client::new("startup-route-probe", jack::ClientOptions::NO_START_SERVER) {
                Ok(probe) => probe,
                Err(_) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => return,
                Err(error) => panic!("required JACK server unavailable: {error}"),
            };
        let sinks = probe.ports(
            None,
            Some("32 bit float mono audio"),
            jack::PortFlags::IS_PHYSICAL | jack::PortFlags::IS_INPUT,
        );
        assert!(sinks.len() >= 2);
        let mut f = Fixture::new_on(
            AudioDriverConfig::Jack(JackAudioDriverConfig {
                client_name: format!("startup-master-route-{mode:?}"),
                ..Default::default()
            }),
            mode,
        );
        for (channel, target) in f
            .client
            .snapshot()
            .unwrap()
            .buses
            .iter()
            .find(|b| b.name == "Master")
            .unwrap()
            .channels
            .clone()
            .iter()
            .zip([&sinks[1], &sinks[0]])
        {
            let links: Vec<_> = f
                .client
                .snapshot()
                .unwrap()
                .connections
                .confirmed_links
                .iter()
                .filter(|link| link.application_port_id == channel.output_port_id)
                .cloned()
                .collect();
            assert_eq!(links.len(), 1);
            for link in links {
                f.command(protocol::Command::SetPortConnected {
                    port: channel.output_port_id,
                    host_port: link.host_port_id,
                    connected: false,
                });
            }
            f.command(protocol::Command::SetPortConnected {
                port: channel.output_port_id,
                host_port: HostPortId::new(target),
                connected: true,
            });
        }
        for reload in [false, true] {
            if reload {
                let image = f.capture(1);
                f.load(image);
            }
            f.until(|client| {
                client
                    .snapshot()
                    .unwrap()
                    .connections
                    .pending_links
                    .is_empty()
            });
            let state = f.client.snapshot().unwrap();
            let master = state.buses.iter().find(|bus| bus.name == "Master").unwrap();
            for (channel, sink) in master.channels.iter().zip([&sinks[1], &sinks[0]]) {
                let links: Vec<_> = state
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| link.application_port_id == channel.output_port_id)
                    .collect();
                assert_eq!(
                    links.len(),
                    1,
                    "duplicate Master playback bindings ({mode:?}, reload={reload}): {links:?}"
                );
                assert_eq!(links[0].host_port_id.as_str(), sink);
            }
        }
    }
}

struct ProcessorPeer {
    source: jack::Port<jack::AudioOut>,
    output: jack::Port<jack::AudioIn>,
    send: jack::Port<jack::AudioIn>,
    keys: jack::Port<jack::MidiOut>,
    midi_send: jack::Port<jack::MidiIn>,
    audio: Arc<AtomicUsize>,
    sent: Arc<AtomicUsize>,
    midi: Arc<AtomicUsize>,
}
impl jack::ProcessHandler for ProcessorPeer {
    fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
        self.source.as_mut_slice(scope).fill(0.25);
        let mut keys = self.keys.writer(scope);
        for (time, bytes) in [(0, [0x90, 60, 100]), (scope.n_frames() - 1, [0x80, 60, 0])] {
            let _ = keys.write(&jack::RawMidi {
                time,
                bytes: &bytes,
            });
        }
        if self
            .output
            .as_slice(scope)
            .iter()
            .any(|sample| sample.abs() > 1e-4)
        {
            self.audio.fetch_add(1, Ordering::Relaxed);
        }
        if self
            .send
            .as_slice(scope)
            .iter()
            .all(|sample| *sample == 0.5)
        {
            self.sent.fetch_add(1, Ordering::Relaxed);
        }
        for event in self.midi_send.iter(scope) {
            if event.bytes == [0x90, 60, 100] {
                self.midi.fetch_add(1, Ordering::Relaxed);
            }
        }
        jack::Control::Continue
    }
}

#[shoop_test(no_wasm = "actual native JACK processed ports and companion callbacks")]
fn native_jack_processed_tracks_publish_real_roles_and_record_host_dsp() {
    let (peer, _) = match jack::Client::new(
        &format!("processor-peer-{}", std::process::id()),
        jack::ClientOptions::NO_START_SERVER,
    ) {
        Ok(client) => client,
        Err(_) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => return,
        Err(error) => panic!("required JACK server unavailable: {error}"),
    };
    let peer_name = peer.name().to_owned();
    let audio = Arc::new(AtomicUsize::new(0));
    let sent = Arc::new(AtomicUsize::new(0));
    let midi = Arc::new(AtomicUsize::new(0));
    let handler = ProcessorPeer {
        source: peer
            .register_port("source", jack::AudioOut::default())
            .unwrap(),
        output: peer
            .register_port("output", jack::AudioIn::default())
            .unwrap(),
        send: peer
            .register_port("send", jack::AudioIn::default())
            .unwrap(),
        keys: peer
            .register_port("keys", jack::MidiOut::default())
            .unwrap(),
        midi_send: peer
            .register_port("midi-send", jack::MidiIn::default())
            .unwrap(),
        audio: audio.clone(),
        sent: sent.clone(),
        midi: midi.clone(),
    };
    let _peer = peer.activate_async((), handler).unwrap();
    let mut f = Fixture::new_on(
        AudioDriverConfig::Jack(JackAudioDriverConfig {
            client_name: format!("processed-host-{}", std::process::id()),
        }),
        DeliveryMode::Socket,
    );
    f.command(protocol::Command::Global(GlobalControlAction::SetSync(
        false,
    )));
    let mut tested = 0;
    for kind in [
        TrackProcessorTypeId::EXTERNAL,
        TrackProcessorTypeId::BUILTIN_FX,
        TrackProcessorTypeId::OXISYNTH,
    ] {
        let available = f
            .client
            .snapshot()
            .unwrap()
            .track_processors
            .iter()
            .any(|p| p.id.as_str() == kind && p.available);
        if !available {
            continue;
        }
        let wet = if kind == TrackProcessorTypeId::EXTERNAL {
            1
        } else {
            2
        };
        let Outcome::TrackCreated { track, loops, .. } =
            f.command(protocol::Command::AddTrack(TrackSpec {
                name: format!("JACK {kind}"),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 2,
                    wet_audio_channels: wet,
                    dry_midi: true,
                    processor_type: TrackProcessorTypeId::new(kind),
                    default_playback_mode: DefaultPlaybackMode::DryThroughWet,
                },
                latency: Default::default(),
                initial_output_bus_names: vec![],
                creation_request_id: None,
            }))
        else {
            panic!("processed JACK admission failed: {kind}")
        };
        let ports: Vec<_> = f.client.snapshot().unwrap().connections.application_ports.iter().filter(|port| matches!(port.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track)).cloned().collect();
        if kind != TrackProcessorTypeId::EXTERNAL {
            assert!(!ports.iter().any(|port| matches!(
                port.role,
                PortRole::AudioSend | PortRole::AudioReturn | PortRole::MidiSend
            )));
        }
        for port in &ports {
            let target = match port.role {
                PortRole::AudioInput | PortRole::AudioReturn => "source",
                PortRole::AudioOutput => "output",
                PortRole::AudioSend => "send",
                PortRole::MidiInput => "keys",
                PortRole::MidiSend => "midi-send",
                _ => panic!("unexpected processed port: {:?}", port.role),
            };
            let graph = &f.client.snapshot().unwrap().host_graph;
            let publication = graph.publication(port.publication.unwrap()).unwrap();
            assert!(matches!(
                publication.state,
                shoop_port_graph::PublicationState::Published(_)
            ));
            f.command(protocol::Command::SetPortConnected {
                port: port.id,
                host_port: HostPortId::new(format!("{peer_name}:{target}")),
                connected: true,
            });
        }
        audio.store(0, Ordering::Relaxed);
        sent.store(0, Ordering::Relaxed);
        midi.store(0, Ordering::Relaxed);
        f.command(protocol::Command::Track {
            track,
            action: TrackAction::InputMonitoringChanged {
                enabled: true,
                respect_auto_mute: false,
            },
        });
        f.until(|_| {
            audio.load(Ordering::Relaxed) > 3
                && (kind != TrackProcessorTypeId::EXTERNAL
                    || (sent.load(Ordering::Relaxed) > 3 && midi.load(Ordering::Relaxed) > 3))
        });
        f.command(protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        });
        thread::sleep(Duration::from_millis(60));
        f.command(protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::StopClicked,
        });
        let message = f.client.read_media(loops[0]).unwrap();
        f.send(message);
        f.until(|client| client.media().is_some());
        let media = f.client.media().unwrap().as_ref().unwrap();
        assert_eq!(media.audio.len(), 2 + wet as usize);
        assert_eq!(media.midi.len(), 1);
        assert!(
            media.audio[..2]
                .iter()
                .all(|channel| !channel.samples.is_empty()
                    && channel.samples.iter().all(|sample| *sample == 0.25)),
            "dry capture: {kind}"
        );
        assert!(
            media.audio[2..]
                .iter()
                .all(|channel| channel.samples.iter().any(|sample| sample.abs() > 1e-4)),
            "wet capture: {kind}"
        );
        assert!(media.midi[0]
            .events
            .iter()
            .any(|event| event.data.as_ref() == [0x90, 60, 100]));
        let previous = audio.load(Ordering::Relaxed);
        f.reconnect();
        f.until(|_| audio.load(Ordering::Relaxed) > previous + 3);
        assert!(f
            .client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .any(|t| t.id == track));
        f.command(protocol::Command::Track {
            track,
            action: TrackAction::Remove,
        });
        f.until(|client| {
            client
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .all(|t| t.id != track)
        });
        tested += 1;
    }
    assert_eq!(
        tested,
        1 + usize::from(cfg!(feature = "builtin-fx")) + usize::from(cfg!(feature = "oxisynth"))
    );
}
