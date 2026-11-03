#![cfg(all(not(target_arch = "wasm32"), feature = "jack"))]

use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_api::*;
use shoop_app_client::{Client, State};
use shoop_app_protocol::{self as protocol, persistence as p, *};
use shoop_host::{
    config::HostConfig,
    connection::{self, Connection},
    remote::Target as HostEntry,
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
fn standalone_cli_starts_direct_jack_with_custom_ports_and_reports_configuration() {
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
                "--audio-driver",
                "jack",
                "--jack-client-name",
                "CLI-Studio",
                "--jack-audio-inputs",
                "capture_l,capture_r",
                "--jack-audio-outputs",
                "main_l,main_r",
                "--jack-midi-inputs=",
                "--jack-midi-outputs",
                "events",
                "--export-profile",
            ])
            .arg(&profile)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
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
    connection::shutdown_ownerless(&target, Duration::from_secs(5)).unwrap();
    assert_eq!(report.effective.audio_driver, AudioDriverState::Running);
    let AudioDriverConfig::Jack(jack) = &report.requested.unwrap().driver else {
        panic!("CLI did not start direct JACK");
    };
    assert_eq!(jack.client_name, "CLI-Studio");
    assert_eq!(jack.audio_inputs, ["capture_l", "capture_r"]);
    assert_eq!(jack.audio_outputs, ["main_l", "main_r"]);
    assert!(jack.midi_inputs.is_empty());
    assert_eq!(jack.midi_outputs, ["events"]);
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
        entry: HostEntry,
        _owner: ChildOwner,
        _root: tempfile::TempDir,
    },
}
struct Fixture {
    client: Client,
    delivery: Delivery,
}
impl Fixture {
    fn new(driver: AudioDriverConfig, independent: bool) -> Self {
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
        let delivery = if independent {
            let root = tempfile::tempdir().unwrap();
            let (child, entry) = shoop_host::launch::spawn(
                &HostConfig {
                    settings,
                    ..Default::default()
                },
                shoop_host::serve::StartOptions::new().unwrap(),
                std::path::Path::new(env!("CARGO_BIN_EXE_shoop_host")),
            )
            .unwrap();
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
    fn until(&mut self, mut done: impl FnMut(&mut Client) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.tick();
            if done(&mut self.client) {
                return;
            }
            assert!(Instant::now() < deadline, "JACK service workflow timed out");
        }
    }
    fn attach(&mut self) {
        let message = self.client.attach().unwrap();
        self.send(message);
        self.until(|client| client.state() == State::Ready);
    }
    fn command(&mut self, command: protocol::Command) -> Outcome {
        let message = self.client.command(command, None).unwrap();
        let ClientMessage::Submit { operation, .. } = &message else {
            unreachable!()
        };
        let operation = *operation;
        self.send(message);
        self.until(|client| client.results().contains_key(&operation));
        let outcome = self.client.results()[&operation].clone();
        assert!(!matches!(outcome, Outcome::Refused { .. }), "{outcome:?}");
        outcome
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
            connections: vec![
                (format!("{}:source", peer.name()), "audio_in_1".into()),
                ("audio_out_1".into(), format!("{}:sink", peer.name())),
                (format!("{}:keys", peer.name()), "midi_in_1".into()),
                ("midi_out_1".into(), format!("{}:synth", peer.name())),
            ],
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
            (PortRole::AudioInput, "jack:audio_in_1"),
            (PortRole::MidiInput, "jack:midi_in_1"),
            (PortRole::MidiOutput, "jack:midi_out_1"),
        ] {
            let port = ports.iter().find(|port| port.role == role).unwrap();
            f.command(protocol::Command::SetPortConnected {
                port: port.id,
                host_port: HostPortId::new(name),
                connected: true,
            });
        }
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
        f.until(|client| client.snapshot().unwrap().tracks[1].loops[0].mode == LoopMode::Playing);
        f.until(|_| audio.load(Ordering::Relaxed) > 3 && midi.load(Ordering::Relaxed) > 6);
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
        assert!(f
            .client
            .snapshot()
            .unwrap()
            .connections
            .confirmed_links
            .iter()
            .any(|link| link.host_port_id.as_str() == "jack:midi_out_1"));
        assert_eq!(f.capture(3).payloads, image.payloads);
        drop(f);
        drop(peer);
    }
}
