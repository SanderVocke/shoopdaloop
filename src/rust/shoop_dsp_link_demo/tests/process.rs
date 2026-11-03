use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
};

const BIN: &str = env!("CARGO_BIN_EXE_shoop_dsp_link_demo");
struct Server(Child, String);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn server(key: &Path, processor: &str) -> Server {
    let mut child = Command::new(BIN)
        .args(["serve", "--processor", processor, "--credential-file"])
        .arg(key)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    let ready: Value = serde_json::from_str(&ready).unwrap();
    Server(child, ready["control_endpoint"].as_str().unwrap().into())
}
fn result(command: &mut Command, path: &Path) -> Value {
    let r = command.arg("--output").arg(path).output().unwrap();
    assert!(
        r.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    let report: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(report["correctness_errors"], 0);
    assert!(report["verified_samples"].as_u64().unwrap() > 0);
    assert!(report["node"]["processed"].as_u64().unwrap() > 0);
    assert_eq!(report["node"]["observation_dropped"], 0);
    assert_eq!(report["unverified_samples"], 0);
    report
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "native graph reference demo")]
fn reference_demo_has_no_udp_configuration_and_verifies_noninteger_q_b() {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(BIN);
    command.args([
        "run",
        "--backend",
        "reference",
        "--quantum",
        "3",
        "--dsp-frames",
        "5",
        "--callback-frames",
        "7",
        "--frames",
        "1000",
        "--delay",
        "17",
    ]);
    let report = result(&mut command, &root.path().join("reference.json"));
    assert_eq!(report["quantum"], 3);
    assert_eq!(report["dsp_frames"], 5);
    assert_eq!(report["node"]["fallback"], 0);
    assert_eq!(
        report["total_known_latency_frames"].as_u64().unwrap(),
        report["link_latency_frames"].as_u64().unwrap() + 17
    );
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "real graph UDP process pair")]
fn graph_udp_demo_handles_callbacks_below_and_above_the_wire_packet_size() {
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    shoop_transport_socket::private::create(&key, &[7; 32]).unwrap();
    let server = server(&key, "delay-copy");
    for callback in [7, 96] {
        let mut command = Command::new(BIN);
        command
            .args(["run", "--endpoint", &server.1, "--credential-file"])
            .arg(&key)
            .args([
                "--callback-frames",
                &callback.to_string(),
                "--frames",
                "4096",
                "--delay",
                "67",
            ]);
        let report = result(
            &mut command,
            &root.path().join(format!("udp-{callback}.json")),
        );
        assert_eq!(report["node"]["fallback"], 0);
        assert_eq!(report["algorithm_latency_frames"], 67);
        assert!(report["service"]["server"]["blocks"].as_u64().unwrap() > 0);
    }
}

#[cfg(feature = "carla")]
#[shoop_wasm_test_support::shoop_test(no_wasm = "required real Carla graph workflow")]
fn graph_carla_gain_and_state_restoration_use_the_same_node() {
    if std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_none() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    shoop_transport_socket::private::create(&key, &[9; 32]).unwrap();
    let project = root.path().join("gain.xml");
    let saved = root.path().join("saved.txt");
    let xml = include_str!("../../shoop_engine/test_data/carla_legacy_rack_loaded_project.xml")
        .replacen("<Value>1</Value>", "<Value>0.5</Value>", 1);
    std::fs::write(&project, xml).unwrap();
    let server = server(&key, "carla");
    for (index, state) in [&project, &saved].iter().enumerate() {
        let mut command = Command::new(BIN);
        command
            .args([
                "run",
                "--processor",
                "carla",
                "--expected-gain",
                "0.5",
                "--warmup-frames",
                "4096",
                "--frames",
                "8192",
                "--state-file",
            ])
            .arg(state)
            .args(["--endpoint", &server.1, "--credential-file"])
            .arg(&key);
        if index == 0 {
            command.arg("--state-output").arg(&saved);
        }
        let report = result(
            &mut command,
            &root.path().join(format!("carla-{index}.json")),
        );
        assert!(report["algorithm_latency_frames"].is_null());
        assert!(report["total_known_latency_frames"].is_null());
        assert_eq!(report["node"]["fallback"], 0);
    }
    assert!(std::fs::read_to_string(saved)
        .unwrap()
        .starts_with("shoop-carla-native-state:"));
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "native worker lifecycle and peer death")]
fn native_preparation_peer_death_and_fresh_sessions_preserve_ownership() {
    use shoop_dsp_link::{memory, Buffer, Endpoint, Header};
    use shoop_dsp_link_native::{
        control::Algorithm,
        polling::Polling,
        udp::{Adapter, Worker},
        Session,
    };
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    shoop_transport_socket::private::create(&key, &[5; 32]).unwrap();
    let mut old = server(&key, "delay-copy");
    let address = old.1.parse().unwrap();
    let config = shoop_remote_dsp::Config {
        start_frame: 128,
        allowance: 64,
        ..Default::default()
    };
    let session =
        Session::connect(address, &key, config, Algorithm::DelayCopy { delay: 0 }).unwrap();
    let first_epoch = session.epoch;
    let descriptor = session.descriptor(64, None, 32).unwrap();
    let wrong = shoop_dsp_link::Descriptor {
        generation: descriptor.generation.wrapping_add(1).max(1),
        ..descriptor
    };
    let (render, peer) = memory::pair(wrong).unwrap();
    assert!(Adapter::prepare(session, peer).is_err());
    assert!(!render.available());
    drop(render);
    let session =
        Session::connect(address, &key, config, Algorithm::DelayCopy { delay: 0 }).unwrap();
    assert_ne!(session.epoch, first_epoch);
    let descriptor = session.descriptor(64, None, 32).unwrap();
    let (mut render, peer) = memory::pair(descriptor).unwrap();
    let worker = Worker::spawn(Adapter::prepare(session, peer).unwrap(), Polling::Yield).unwrap();
    for step in 0..4 {
        let mut lease = render.acquire_input().unwrap();
        lease.set_header(Header {
            generation: descriptor.generation,
            frame: 128 + step * 16,
            ..Header::default()
        });
        lease.audio_mut().fill(1.);
        assert!(render.submit(lease).is_ok());
    }
    worker.wait_until(192, Duration::from_secs(5)).unwrap();
    let mut received = 0;
    while let Some(lease) = render.receive() {
        assert_eq!(lease.header().generation, descriptor.generation);
        received += 1;
        render.release_output(lease);
    }
    assert_eq!(received, 4);
    old.0.kill().unwrap();
    old.0.wait().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while render.available() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(worker.stop().is_err());
    let fresh = server(&key, "delay-copy");
    let session = Session::connect(
        fresh.1.parse().unwrap(),
        &key,
        config,
        Algorithm::DelayCopy { delay: 0 },
    )
    .unwrap();
    assert_ne!(session.epoch, descriptor.generation);
    let descriptor = session.descriptor(64, None, 32).unwrap();
    let (render, peer) = memory::pair(descriptor).unwrap();
    let adapter = Adapter::prepare(session, peer).unwrap();
    let worker = Worker::spawn(adapter, Polling::Yield).unwrap();
    let mut adapter = worker.stop().unwrap();
    assert!(!render.available());
    adapter.session.pause().unwrap();
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "native stalled data service")]
fn silent_or_repeated_data_cannot_keep_a_stalled_link_alive() {
    use shoop_dsp_link::{memory, Buffer, Endpoint, Header};
    use shoop_dsp_link_native::{
        control::{self, Algorithm, Reply, Request},
        polling::Polling,
        udp::{Adapter, Worker},
        Session,
    };
    use shoop_remote_dsp::wire;
    use std::{
        io::ErrorKind,
        net::{SocketAddr, TcpListener, UdpSocket},
        time::{Duration, Instant},
    };
    for repeated in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let key = root.path().join("key");
        let token = [6; 32];
        shoop_transport_socket::private::create(&key, &token).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let fake = std::thread::spawn(move || {
            let (tcp, _) = listener.accept().unwrap();
            let mut tcp = shoop_transport_socket::setup::authenticate(
                tcp,
                token,
                true,
                Duration::from_secs(2),
            )
            .unwrap();
            let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
            let request: Request = control::read(&mut tcp, Duration::from_secs(5), || {}).unwrap();
            let Request::Configure {
                config,
                epoch,
                udp_port,
                ..
            } = request
            else {
                panic!("expected configuration")
            };
            let peer = SocketAddr::new(address.ip(), udp_port);
            control::write(
                &mut tcp,
                &Reply::Ready {
                    version: control::VERSION,
                    udp_port: udp.local_addr().unwrap().port(),
                    latency_frames: Some(0),
                    receive_buffer: 1024,
                    send_buffer: 1024,
                },
            )
            .unwrap();
            let request: Request = control::read(&mut tcp, Duration::from_secs(5), || {}).unwrap();
            assert!(matches!(request, Request::Start));
            control::write(&mut tcp, &Reply::Ok).unwrap();
            let mut header = wire::Header::input(epoch, 0, config);
            header.kind = wire::OUTPUT;
            let mut raw = vec![0; config.max_packet_bytes()];
            let n = wire::encode(
                header,
                config,
                &vec![0.; config.packet_frames as usize * config.channels as usize],
                &[],
                &mut raw,
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                match tcp.peek(&mut [0]) {
                    Ok(0) => break,
                    Err(e)
                        if e.kind() != ErrorKind::WouldBlock
                            && e.kind() != ErrorKind::Interrupted =>
                    {
                        break
                    }
                    _ => {}
                }
                if repeated {
                    let _ = udp.send_to(&raw[..n], peer);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let config = shoop_remote_dsp::Config::default();
        let session =
            Session::connect(address, &key, config, Algorithm::DelayCopy { delay: 0 }).unwrap();
        let d = session.descriptor(64, None, 16).unwrap();
        let (mut render, peer) = memory::pair(d).unwrap();
        let worker =
            Worker::spawn(Adapter::prepare(session, peer).unwrap(), Polling::Yield).unwrap();
        for frame in [0, 16] {
            let mut input = render.acquire_input().unwrap();
            input.set_header(Header {
                generation: d.generation,
                frame,
                ..Header::default()
            });
            assert!(render.submit(input).is_ok());
        }
        let deadline = Instant::now() + Duration::from_secs(6);
        while render.available() {
            assert!(
                Instant::now() < deadline,
                "duplicate data must not refresh progress"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(worker.stop().is_err());
        fake.join().unwrap();
    }
}
