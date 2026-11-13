#![cfg(all(feature = "fpga-sim", target_os = "linux"))]
use shoop_app_api::*;
use shoop_app_client::{Client, State};
use shoop_app_protocol::{ClientMessage, Command, Outcome};
use shoop_host::{
    config::{AppTransport, HostConfig},
    connection::{self, Connection},
    remote::Target,
};
use shoop_wasm_test_support::shoop_test;
use std::{
    process::{Child, Command as Process, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Host(Child);
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn poll_until(client: &mut Client, connection: &mut Connection, done: impl Fn(&Client) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        connection.poll(client).unwrap();
        if let Some(message) = client.ready_message() {
            connection.send(message).unwrap();
            client.ready_sent().unwrap();
        }
        if done(client) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "client timed out: {:?}",
            client.snapshot()
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn command(client: &mut Client, connection: &mut Connection, command: Command) -> Outcome {
    let message = client.command(command, None).unwrap();
    let ClientMessage::Submit { operation, .. } = message else {
        unreachable!()
    };
    connection.send(message).unwrap();
    poll_until(client, connection, |c| c.results().contains_key(&operation));
    client.results()[&operation].clone()
}
fn confirmed(client: &mut Client, connection: &mut Connection, value: Command) {
    let result = command(client, connection, value);
    assert!(matches!(result, Outcome::Confirmed { .. }), "{result:?}");
}

#[shoop_test]
fn standalone_rtl_host_runs_the_sync_loop_over_regular_carriers_and_refuses_extra_tracks() {
    for transport in [AppTransport::Socket, AppTransport::SharedMemory] {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.json");
        let settings = HostConfig {
            settings: HostSettings {
                backend: AudioBackendKind::FpgaRtlSim,
                driver: AudioDriverConfig::Simulated(Default::default()),
                ..Default::default()
            },
            transport,
            ..Default::default()
        };
        std::fs::write(&config, serde_json::to_vec(&settings).unwrap()).unwrap();
        let profile = root.path().join("profile.json");
        let log = root.path().join("host.log");
        let mut host = Host(
            Process::new(env!("CARGO_BIN_EXE_shoop_host"))
                .args([
                    "--audio-backend",
                    "fpga_rtl_sim",
                    "--simulated-input",
                    "synthetic",
                    "--config",
                ])
                .arg(&config)
                .arg("--export-profile")
                .arg(&profile)
                .arg("--driver-preferences")
                .arg(root.path().join("preferences.json"))
                .stdout(Stdio::null())
                .stderr(Stdio::from(std::fs::File::create(&log).unwrap()))
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(180);
        while !profile.exists() {
            assert!(
                host.0.try_wait().unwrap().is_none(),
                "{}",
                std::fs::read_to_string(&log).unwrap()
            );
            assert!(
                Instant::now() < deadline,
                "{}",
                std::fs::read_to_string(&log).unwrap()
            );
            thread::sleep(Duration::from_millis(10));
        }
        let target = Target::read(&profile).unwrap();
        let mut connection = connection::connect(&target, Duration::from_secs(3)).unwrap();
        let mut client = Client::new();
        connection.send(client.attach().unwrap()).unwrap();
        poll_until(&mut client, &mut connection, |c| c.state() == State::Ready);
        let snapshot = client.snapshot().unwrap();
        assert_eq!(snapshot.tracks.len(), 1);
        assert_eq!(snapshot.tracks[0].name, "Sync");
        let track = snapshot.tracks[0].id;
        let loop_id = snapshot.tracks[0].loops[0].id;
        let input = snapshot.connections.application_ports.iter().find(|p| p.data_type == PortDataType::Audio && p.direction == PortDirection::Input && matches!(p.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track)).unwrap().id;
        let host_input = snapshot
            .connections
            .host_ports
            .iter()
            .find(|p| {
                p.available
                    && p.data_type == PortDataType::Audio
                    && p.direction == PortDirection::Output
            })
            .unwrap()
            .id
            .clone();
        let output = snapshot.connections.application_ports.iter().find(|p| p.data_type == PortDataType::Audio && p.direction == PortDirection::Output && matches!(p.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track)).unwrap().id;
        let destination = snapshot.buses[0].channels[0].id;
        confirmed(
            &mut client,
            &mut connection,
            Command::SetPortConnected {
                port: input,
                host_port: host_input,
                connected: true,
            },
        );
        confirmed(
            &mut client,
            &mut connection,
            Command::SetMixerRoute {
                source: output,
                destination,
                connected: true,
            },
        );
        confirmed(
            &mut client,
            &mut connection,
            Command::Global(GlobalControlAction::SetSync(false)),
        );
        confirmed(
            &mut client,
            &mut connection,
            Command::Loop {
                track,
                loop_id,
                action: LoopAction::RecordClicked,
            },
        );
        poll_until(&mut client, &mut connection, |c| {
            c.snapshot().unwrap().tracks[0].loops[0].mode == LoopMode::Recording
        });
        let first = client.snapshot().unwrap().status.processed_frames;
        poll_until(&mut client, &mut connection, |c| {
            c.snapshot().unwrap().status.processed_frames > first + 1024
        });
        confirmed(
            &mut client,
            &mut connection,
            Command::Loop {
                track,
                loop_id,
                action: LoopAction::PlayClicked,
            },
        );
        poll_until(&mut client, &mut connection, |c| {
            c.snapshot().unwrap().tracks[0].loops[0].mode == LoopMode::Playing
        });
        poll_until(&mut client, &mut connection, |c| {
            let s = c.snapshot().unwrap();
            s.status.output_peak > 0.001 && s.status.memory.is_some()
        });
        let snapshot = client.snapshot().unwrap();
        assert_eq!(snapshot.status.audio_backend, AudioBackendKind::FpgaRtlSim);
        assert_eq!(
            snapshot
                .audio_drivers
                .active
                .as_ref()
                .unwrap()
                .configured
                .kind(),
            AudioDriverKind::Simulated
        );
        assert!(!snapshot.audio_drivers.supported);
        assert!(snapshot.tracks[0].loops[0].length_frames > 0);
        let result = command(
            &mut client,
            &mut connection,
            Command::AddTrack(TrackSpec {
                name: "Outside profile".into(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 1,
                    midi: false,
                },
                latency: Default::default(),
                initial_output_bus_names: vec!["Master".into()],
                creation_request_id: None,
            }),
        );
        assert!(matches!(result, Outcome::Refused { .. }), "{result:?}");
        confirmed(
            &mut client,
            &mut connection,
            Command::Loop {
                track,
                loop_id,
                action: LoopAction::StopClicked,
            },
        );
        confirmed(
            &mut client,
            &mut connection,
            Command::Global(GlobalControlAction::ClearRecordings { include_sync: true }),
        );
        poll_until(&mut client, &mut connection, |c| {
            c.snapshot().unwrap().tracks[0].loops[0].empty
        });
        assert_eq!(
            client.snapshot().unwrap().status.audio_driver,
            AudioDriverState::Running
        );
    }
}

#[shoop_test]
fn rtl_host_rejects_a_real_time_driver_before_opening_devices() {
    let root = tempfile::tempdir().unwrap();
    let output = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
        .args([
            "--audio-backend",
            "fpga_rtl_sim",
            "--audio-driver",
            "cpal",
            "--driver-preferences",
        ])
        .arg(root.path().join("preferences.json"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("requires the Simulated clock driver"),
        "{error}"
    );
}

#[shoop_test]
fn cold_companion_readiness_uses_the_normal_framed_protocol() {
    let root = tempfile::tempdir().unwrap();
    let stderr = root.path().join("stderr.log");
    let mut host = Host(
        Process::new(env!("CARGO_BIN_EXE_shoop_host"))
            .arg("--launch-stdin")
            .env("SHOOP_FPGA_CACHE", root.path().join("cold-cache"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()))
            .spawn()
            .unwrap(),
    );
    let mut options = shoop_host::serve::StartOptions::new().unwrap();
    options.driver_preferences = Some(root.path().join("preferences.json"));
    let startup = shoop_host::launch::Startup {
        config: HostConfig {
            settings: HostSettings {
                backend: AudioBackendKind::FpgaRtlSim,
                driver: AudioDriverConfig::Simulated(Default::default()),
                ..Default::default()
            },
            ..Default::default()
        },
        options,
    };
    shoop_host::launch::write_message(host.0.stdin.take().unwrap(), &startup).unwrap();
    let stdout = host.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let _ = send.send(shoop_host::launch::read_message::<Target>(stdout));
    });
    let target = receive
        .recv_timeout(Duration::from_secs(180))
        .unwrap_or_else(|error| panic!("{error}: {}", std::fs::read_to_string(&stderr).unwrap()))
        .unwrap_or_else(|error| panic!("{error:#}: {}", std::fs::read_to_string(&stderr).unwrap()));
    reader.join().unwrap();
    let mut connection = connection::connect(&target, Duration::from_secs(3)).unwrap();
    let mut client = Client::new();
    connection.send(client.attach().unwrap()).unwrap();
    poll_until(&mut client, &mut connection, |client| {
        client.state() == State::Ready
    });
    assert_eq!(
        client.snapshot().unwrap().status.audio_backend,
        AudioBackendKind::FpgaRtlSim
    );
}
