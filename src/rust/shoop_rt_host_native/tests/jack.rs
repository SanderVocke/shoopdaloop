#![cfg(feature = "jack")]

mod support;

use jack::{AudioIn, AudioOut, MidiIn, MidiOut, Port};
use shoop_rt_host_native::{start, DriverConfig, JackConfig, NativeConfig};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Peer {
    audio_out: Port<AudioOut>,
    audio_in: Port<AudioIn>,
    midi_out: Port<MidiOut>,
    midi_in: Port<MidiIn>,
    samples: Arc<AtomicUsize>,
    messages: Arc<AtomicUsize>,
    resets: Arc<AtomicUsize>,
}
impl jack::ProcessHandler for Peer {
    fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
        assert_no_heap(|| {
            self.audio_out.as_mut_slice(scope).fill(0.25);
            let mut writer = self.midi_out.writer(scope);
            for offset in [0, 31, 64, scope.n_frames() - 1] {
                if offset < scope.n_frames() {
                    writer
                        .write(&jack::RawMidi {
                            time: offset,
                            bytes: &[0xb0, 1, (offset % 128) as u8],
                        })
                        .unwrap();
                }
            }
            if self
                .audio_in
                .as_slice(scope)
                .iter()
                .all(|sample| *sample == 0.25)
            {
                self.samples.fetch_add(1, Ordering::Relaxed);
            }
            for event in self.midi_in.iter(scope) {
                if event.bytes == [0xb0, 1, (event.time % 128) as u8] {
                    self.messages.fetch_add(1, Ordering::Relaxed);
                }
                if event.bytes.len() == 3 && matches!(event.bytes[1], 120 | 123) {
                    self.resets.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        jack::Control::Continue
    }
}
#[track_caller]
fn wait(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "JACK workflow timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn server() -> Option<jack::Client> {
    let name = format!(
        "shoop-jack-peer-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    match jack::Client::new(&name, jack::ClientOptions::NO_START_SERVER) {
        Ok((client, _)) => Some(client),
        Err(error) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => {
            eprintln!("JACK unavailable: {error}");
            None
        }
        Err(error) => panic!("required JACK server unavailable: {error}"),
    }
}

#[shoop_test(no_wasm = "real JACK audio and MIDI")]
fn native_jack_production_callback_routes_audio_and_timestamped_midi_and_recovers_on_stop() {
    let Some(peer) = server() else { return };
    let client_name = format!(
        "shoop-jack-host-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let samples = Arc::new(AtomicUsize::new(0));
    let messages = Arc::new(AtomicUsize::new(0));
    let resets = Arc::new(AtomicUsize::new(0));
    let audio_out = peer.register_port("source", AudioOut::default()).unwrap();
    let audio_in = peer.register_port("sink", AudioIn::default()).unwrap();
    let midi_out = peer.register_port("keys", MidiOut::default()).unwrap();
    let midi_in = peer.register_port("synth", MidiIn::default()).unwrap();
    let external = |short: &str| format!("{}:{short}", peer.name());
    let config = JackConfig {
        client_name: client_name.clone(),
        audio_inputs: vec!["capture".into()],
        audio_outputs: vec!["playback".into()],
        midi_inputs: vec!["midi_in".into()],
        midi_outputs: vec!["midi_out".into()],
        connections: vec![
            (external("source"), "capture".into()),
            ("playback".into(), external("sink")),
            (external("keys"), "midi_in".into()),
            ("midi_out".into(), external("synth")),
        ],
    };
    let peer = peer
        .activate_async(
            (),
            Peer {
                audio_out,
                audio_in,
                midi_out,
                midi_in,
                samples: samples.clone(),
                messages: messages.clone(),
                resets: resets.clone(),
            },
        )
        .unwrap();
    let started = start(&NativeConfig {
        driver: DriverConfig::Jack(config),
        engine: description(),
    })
    .unwrap();
    let (host, mut client) = (started.host, started.client);
    assert_eq!(host.instance_name(), client_name);
    assert_eq!(host.layout().ports.audio_inputs[0].as_str(), "jack:capture");
    assert_eq!(host.layout().master_connections, [(0, 0)]);
    let mut received = Received::default();
    connected(&mut client, &mut received);
    assert_eq!(host.status().driver.kind, DriverKind::Jack);
    let strip = strip(0);
    request_done(
        &mut client,
        &mut received,
        Request::CreateStrip {
            strip,
            shape: shape(1, true),
        },
    );
    for (kind, id) in [
        (PortKind::AudioIn, 0),
        (
            PortKind::AudioOut,
            host.layout().ports.audio_inputs.len() as u32,
        ),
        (
            PortKind::MidiIn,
            (host.layout().ports.audio_inputs.len() + host.layout().ports.audio_outputs.len())
                as u32,
        ),
        (
            PortKind::MidiOut,
            (host.layout().ports.audio_inputs.len()
                + host.layout().ports.audio_outputs.len()
                + host.layout().ports.midi_inputs.len()) as u32,
        ),
    ] {
        request_done(&mut client, &mut received, connect(strip, kind, id));
    }
    request_done(&mut client, &mut received, monitoring(strip));
    wait(|| samples.load(Ordering::Relaxed) >= 3 && messages.load(Ordering::Relaxed) >= 12);
    assert!(host.status().stats.split_callbacks > 0);
    assert_eq!(host.status().stats.midi_out_failed, 0);
    let resets_before = resets.load(Ordering::Relaxed);
    drop(client);
    let graph = host.graph_control().unwrap();
    let report = host.stop();
    assert_eq!(
        graph.snapshot().status,
        shoop_port_graph::Status::Unavailable
    );
    assert!(report.midi_reset);
    assert!(report.stopped.is_ok());
    wait(|| resets.load(Ordering::Relaxed) >= resets_before + 32);
    drop(peer);
}

#[shoop_test(no_wasm = "real JACK client lifecycle")]
fn native_jack_rejects_bad_ports_and_rolls_back_failed_startup_connections() {
    let mut config = JackConfig::default();
    config.midi_inputs = config.audio_inputs.clone();
    assert!(config.validate().is_err());
    let Some(peer) = server() else { return };
    config = JackConfig {
        client_name: format!("rollback-{}", peer.name()),
        connections: vec![("missing-client:source".into(), "audio_in_1".into())],
        ..Default::default()
    };
    let result = start(&NativeConfig {
        driver: DriverConfig::Jack(config.clone()),
        engine: description(),
    });
    assert!(result.is_err());
    assert!(peer
        .ports(
            Some(&format!("{}:", config.client_name)),
            None,
            jack::PortFlags::empty()
        )
        .is_empty());
}

#[shoop_test(no_wasm = "real JACK naming and buffer notifications")]
fn native_jack_resolves_name_collisions_and_splits_changed_server_buffers() {
    let Some(peer) = server() else { return };
    let config = JackConfig {
        client_name: format!("collision-{}", peer.name()),
        ..Default::default()
    };
    let first = start(&NativeConfig {
        driver: DriverConfig::Jack(config.clone()),
        engine: description(),
    })
    .unwrap();
    let second = start(&NativeConfig {
        driver: DriverConfig::Jack(config),
        engine: description(),
    })
    .unwrap();
    assert_ne!(first.host.instance_name(), second.host.instance_name());
    assert_eq!(first.host.layout().ports, second.host.layout().ports);
    if std::env::var_os("SHOOP_JACK_TEST_BUFFER_CHANGES").is_some() {
        struct Restore<'a>(&'a jack::Client, u32);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                let _ = self.0.set_buffer_size(self.1);
            }
        }
        let original = peer.buffer_size();
        let _restore = Restore(&peer, original);
        peer.set_buffer_size(original * 2).unwrap();
        wait(|| {
            first.host.status().driver.buffer_frames == original * 2
                && first.host.status().stats.split_callbacks > 0
        });
    }
    drop(first.client);
    assert!(first.host.stop().stopped.is_ok());
    drop(second.client);
    assert!(second.host.stop().stopped.is_ok());
}

#[shoop_test(no_wasm = "native JACK server-loss child")]
fn native_jack_server_loss_peer() {
    let Some(ready) = std::env::var_os("SHOOP_JACK_LOSS_READY") else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let started = loop {
        match start(&NativeConfig {
            driver: DriverConfig::Jack(JackConfig::default()),
            engine: description(),
        }) {
            Ok(started) => break started,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "server never became available: {error}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    };
    std::fs::write(ready, "ready").unwrap();
    wait(|| started.host.status().driver.state == DriverRunState::Failed);
    assert_eq!(
        started.host.status().failure,
        Some(shoop_rt_host_native::StreamFailure::DeviceNotAvailable)
    );
    drop(started.client);
    let begun = Instant::now();
    let graph = started.host.graph_control().unwrap();
    wait(|| graph.snapshot().status == shoop_port_graph::Status::Unavailable);
    let report = started.host.stop();
    assert!(begun.elapsed() < Duration::from_secs(2));
    assert!(
        !report.midi_reset,
        "server loss cannot confirm delivery of MIDI recovery"
    );
}

#[shoop_test(no_wasm = "real JACK server-loss lifecycle")]
fn native_jack_server_loss_is_reported_and_cleanup_does_not_hang() {
    if std::env::var_os("SHOOP_JACK_TEST_SERVER_LOSS").is_none() {
        return;
    }
    let root = std::env::temp_dir().join(format!("shoop-jack-loss-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let _ = std::fs::remove_file(&ready);
    let server_name = format!("shoop-jack-loss-{}", std::process::id());
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let log = std::fs::File::create(root.join("server.log")).unwrap();
    let mut server = Child(
        std::process::Command::new("jackd")
            .args([
                "--no-realtime",
                "--name",
                &server_name,
                "-d",
                "dummy",
                "-r",
                "48000",
                "-p",
                "128",
            ])
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let mut peer = Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "native_jack_server_loss_peer", "--nocapture"])
            .env("JACK_DEFAULT_SERVER", server_name)
            .env("SHOOP_JACK_LOSS_READY", &ready)
            .spawn()
            .unwrap(),
    );
    wait(|| {
        assert!(
            peer.0.try_wait().unwrap().is_none(),
            "server-loss fixture exited before readiness"
        );
        ready.exists()
    });
    server.0.kill().unwrap();
    server.0.wait().unwrap();
    let mut result = None;
    wait(|| {
        result = peer.0.try_wait().unwrap();
        result.is_some()
    });
    assert!(result.unwrap().success());
    std::fs::remove_dir_all(root).unwrap();
}

#[shoop_test(no_wasm = "native JACK unavailable facility child")]
fn native_jack_unavailable_peer() {
    if std::env::var_os("SHOOP_JACK_UNAVAILABLE_CHILD").is_none() {
        return;
    }
    let result = start(&NativeConfig {
        driver: DriverConfig::Jack(JackConfig::default()),
        engine: description(),
    });
    let error = result
        .err()
        .expect("unavailable JACK facilities must refuse startup");
    assert!(
        matches!(error, shoop_rt_host_native::StartError::Device(_)),
        "{error:?}"
    );
}

#[cfg(target_os = "linux")]
#[shoop_test(no_wasm = "native JACK dynamic loader and missing server")]
fn native_jack_missing_library_and_missing_server_are_fallible_without_autostart() {
    let root = std::env::temp_dir().join(format!("shoop-jack-unavailable-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("libjack.so.0"), b"invalid shared library").unwrap();
    for missing_library in [true, false] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "native_jack_unavailable_peer", "--nocapture"])
            .env("SHOOP_JACK_UNAVAILABLE_CHILD", "1")
            .env(
                "JACK_DEFAULT_SERVER",
                format!("nonexistent-{}", std::process::id()),
            );
        if missing_library {
            command.env("LD_LIBRARY_PATH", &root);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[shoop_test(no_wasm = "real JACK startup readiness")]
fn native_jack_server_readiness() {
    if std::env::var_os("SHOOP_JACK_TEST_SERVER_LOSS").is_none() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match jack::Client::new("shoop-readiness", jack::ClientOptions::NO_START_SERVER) {
            Ok((client, _)) => {
                drop(client);
                break;
            }
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "JACK readiness timed out: {error}"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[shoop_test(no_wasm = "real JACK provider conformance")]
fn native_jack_implements_the_shared_port_graph_contract() {
    use shoop_port_graph::{testing::connection_contract, *};
    let Some(peer) = server() else {
        return;
    };
    let sink = peer
        .register_port("contract_sink", jack::AudioIn::default())
        .unwrap();
    let incompatible = peer
        .register_port("contract_midi", jack::MidiIn::default())
        .unwrap();
    let foreign = peer
        .register_port("contract_foreign", jack::AudioOut::default())
        .unwrap();
    let _active_peer = peer
        .activate_async(
            (),
            jack::contrib::ClosureProcessHandler::new(
                |_: &jack::Client, _: &jack::ProcessScope| jack::Control::Continue,
            ),
        )
        .unwrap();
    let started = start(&NativeConfig {
        driver: DriverConfig::Jack(JackConfig::default()),
        engine: description(),
    })
    .unwrap();
    let mut graph = started.host.graph_control().unwrap();
    wait(|| graph.snapshot().status == Status::Ready);
    let snapshot = graph.snapshot();
    let local = snapshot
        .ports
        .iter()
        .find(|p| {
            p.local.is_some() && p.data_type == DataType::Audio && p.direction == Direction::Output
        })
        .unwrap()
        .id;
    let endpoint = |port: &jack::Port<jack::Unowned>| {
        snapshot
            .ports
            .iter()
            .find(|p| p.name == port.name().unwrap())
            .unwrap()
            .id
    };
    connection_contract(
        &mut graph,
        local,
        endpoint(&sink.clone_unowned()),
        endpoint(&incompatible.clone_unowned()),
        endpoint(&foreign.clone_unowned()),
        || std::thread::sleep(Duration::from_millis(2)),
    );
    drop(started.client);
    assert!(started.host.stop().stopped.is_ok());
}

#[shoop_test(no_wasm = "JACK registration incarnations")]
fn native_jack_ids_survive_unrelated_hotplug_and_rename_but_detect_unobserved_replacement() {
    use shoop_port_graph::*;
    let Some(peer) = server() else {
        return;
    };
    let mut input = peer
        .register_port("incarnation", jack::AudioIn::default())
        .unwrap();
    let name = input.name().unwrap();
    let peer = peer
        .activate_async(
            (),
            jack::contrib::ClosureProcessHandler::new(
                |_: &jack::Client, _: &jack::ProcessScope| jack::Control::Continue,
            ),
        )
        .unwrap();
    let started = start(&NativeConfig {
        driver: DriverConfig::Jack(JackConfig::default()),
        engine: description(),
    })
    .unwrap();
    let control = started.host.graph_control().unwrap();
    wait(|| control.snapshot().status == Status::Ready);
    let snapshot = control.snapshot();
    let before = snapshot.ports.iter().find(|p| p.name == name).unwrap().id;
    let local = snapshot
        .ports
        .iter()
        .find(|p| {
            p.local.is_some() && p.direction == Direction::Output && p.data_type == DataType::Audio
        })
        .unwrap()
        .id;
    let unrelated = peer
        .as_client()
        .register_port("unrelated", jack::MidiOut::default())
        .unwrap();
    input.set_name("renamed").unwrap();
    let renamed = input.name().unwrap();
    wait(|| {
        control
            .snapshot()
            .ports
            .iter()
            .any(|p| p.id == before && p.name == renamed)
    });
    let id = control
        .submit(Request {
            epoch: snapshot.epoch,
            operation: Operation::Connect {
                local,
                peer: before,
                connected: true,
            },
        })
        .unwrap();
    let mut result = None;
    wait(|| {
        result = control.take_result();
        result.is_some()
    });
    assert_eq!(
        result.unwrap(),
        Completion {
            id,
            result: Ok(Outcome::Connected)
        }
    );
    peer.as_client().unregister_port(input).unwrap();
    // Do not wait for a snapshot of the absent port before reusing the same native slot/name.
    let replacement = peer
        .as_client()
        .register_port("renamed", jack::AudioIn::default())
        .unwrap();
    wait(|| {
        control
            .snapshot()
            .ports
            .iter()
            .any(|p| p.name == renamed && p.id != before)
    });
    assert_eq!(
        control
            .submit(Request {
                epoch: snapshot.epoch,
                operation: Operation::Connect {
                    local,
                    peer: before,
                    connected: true
                }
            })
            .unwrap_err()
            .kind,
        ErrorKind::StalePort
    );
    drop(replacement);
    drop(unrelated);
    drop(started.client);
    assert!(started.host.stop().stopped.is_ok());
}

#[shoop_test(no_wasm = "real JACK dynamic publication, signal routing and capacity")]
fn native_jack_publications_route_beyond_64_slots_and_retire_midi_safely() {
    use shoop_port_graph::{self as graph, Controller};
    let Some(peer) = server() else {
        return;
    };
    let samples = Arc::new(AtomicUsize::new(0));
    let messages = Arc::new(AtomicUsize::new(0));
    let resets = Arc::new(AtomicUsize::new(0));
    let audio_out = peer.register_port("source", AudioOut::default()).unwrap();
    let audio_in = peer.register_port("sink", AudioIn::default()).unwrap();
    let midi_out = peer.register_port("keys", MidiOut::default()).unwrap();
    let midi_in = peer.register_port("synth", MidiIn::default()).unwrap();
    let peer_name = peer.name().to_owned();
    let peer = peer
        .activate_async(
            (),
            Peer {
                audio_out,
                audio_in,
                midi_out,
                midi_in,
                samples: samples.clone(),
                messages: messages.clone(),
                resets: resets.clone(),
            },
        )
        .unwrap();
    let started = start(&NativeConfig {
        engine: description(),
        driver: DriverConfig::Jack(JackConfig {
            audio_inputs: Vec::new(),
            audio_outputs: Vec::new(),
            midi_inputs: Vec::new(),
            midi_outputs: Vec::new(),
            ..Default::default()
        }),
    })
    .unwrap();
    let host = started.host;
    let mut client = started.client;
    let graph = host.graph_control().unwrap();
    wait(|| graph.snapshot().status == graph::Status::Ready);
    let mut received = Received::default();
    connected(&mut client, &mut received);
    let perform = |operation| {
        let id = graph
            .submit(graph::Request {
                epoch: graph.snapshot().epoch,
                operation,
            })
            .unwrap();
        let mut result = None;
        wait(|| {
            result = graph.take_result();
            result.is_some()
        });
        let completion = result.unwrap();
        assert_eq!(completion.id, id);
        completion.result
    };
    let capacity = host.layout().ports.audio_outputs.len();
    assert!(capacity > 64);
    for index in 0..capacity {
        perform(graph::Operation::Publish {
            key: graph::LocalPortKey(1000 + index as u64),
            spec: graph::PortSpec {
                name: format!("dynamic_out_{index}"),
                data_type: graph::DataType::Audio,
                direction: graph::Direction::Output,
            },
        })
        .unwrap();
    }
    assert_eq!(
        perform(graph::Operation::Publish {
            key: graph::LocalPortKey(2000),
            spec: graph::PortSpec {
                name: "overflow".into(),
                data_type: graph::DataType::Audio,
                direction: graph::Direction::Output
            }
        })
        .unwrap_err()
        .kind,
        graph::ErrorKind::Capacity
    );
    for (key, name, kind, direction) in [
        (
            3000,
            "dynamic_in",
            graph::DataType::Audio,
            graph::Direction::Input,
        ),
        (
            3001,
            "dynamic_keys",
            graph::DataType::Midi,
            graph::Direction::Input,
        ),
        (
            3002,
            "dynamic_synth",
            graph::DataType::Midi,
            graph::Direction::Output,
        ),
    ] {
        perform(graph::Operation::Publish {
            key: graph::LocalPortKey(key),
            spec: graph::PortSpec {
                name: name.into(),
                data_type: kind,
                direction,
            },
        })
        .unwrap();
    }
    let strip = strip(0);
    request_done(
        &mut client,
        &mut received,
        Request::CreateStrip {
            strip,
            shape: shape(1, true),
        },
    );
    for (kind, key) in [
        (PortKind::AudioIn, 3000),
        (PortKind::AudioOut, 1000 + capacity as u64 - 1),
        (PortKind::MidiIn, 3001),
        (PortKind::MidiOut, 3002),
    ] {
        let binding = graph.io_binding(graph::LocalPortKey(key)).unwrap();
        let ports = &host.layout().ports;
        let id = ports
            .audio_inputs
            .iter()
            .chain(&ports.audio_outputs)
            .chain(&ports.midi_inputs)
            .chain(&ports.midi_outputs)
            .position(|port| port.as_str() == binding.port)
            .unwrap() as u32;
        request_done(&mut client, &mut received, connect(strip, kind, id));
    }
    request_done(&mut client, &mut received, monitoring(strip));
    let local = |name: &str| format!("{}:{name}", host.instance_name());
    for (source, destination) in [
        (format!("{peer_name}:source"), local("dynamic_in")),
        (
            local(&format!("dynamic_out_{}", capacity - 1)),
            format!("{peer_name}:sink"),
        ),
        (format!("{peer_name}:keys"), local("dynamic_keys")),
        (local("dynamic_synth"), format!("{peer_name}:synth")),
    ] {
        peer.as_client()
            .connect_ports_by_name(&source, &destination)
            .unwrap();
    }
    wait(|| samples.load(Ordering::Relaxed) > 3 && messages.load(Ordering::Relaxed) > 12);
    struct RestorePeriod<'a>(&'a jack::Client, u32);
    impl Drop for RestorePeriod<'_> {
        fn drop(&mut self) {
            let _ = self.0.set_buffer_size(self.1);
        }
    }
    let _restore = RestorePeriod(peer.as_client(), peer.as_client().buffer_size());
    if std::env::var_os("SHOOP_JACK_TEST_BUFFER_CHANGES").is_some() {
        peer.as_client().set_buffer_size(32).unwrap();
    }
    let before = resets.load(Ordering::Relaxed);
    let id = graph
        .submit(graph::Request {
            epoch: graph.snapshot().epoch,
            operation: graph::Operation::Unpublish {
                key: graph::LocalPortKey(3002),
            },
        })
        .unwrap();
    let mut result = None;
    wait(|| {
        result = graph.take_result();
        result.is_some()
    });
    assert_eq!(
        result.unwrap(),
        graph::Completion {
            id,
            result: Ok(graph::Outcome::Unpublished)
        }
    );
    wait(|| resets.load(Ordering::Relaxed) >= before + 32);
    assert!(peer
        .as_client()
        .port_by_name(&local("dynamic_synth"))
        .is_none());
    assert!(graph.io_binding(graph::LocalPortKey(3002)).is_none());
    drop(client);
    assert!(host.stop().stopped.is_ok());
}
