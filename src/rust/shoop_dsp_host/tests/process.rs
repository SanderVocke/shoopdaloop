use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};
const BIN: &str = env!("CARGO_BIN_EXE_shoop_dsp_host");
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "native process pair")]
fn process_pair_preserves_non_aligned_delay_across_aggregation_and_input_loss() {
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    assert!(Command::new(BIN)
        .args(["generate-credential"])
        .arg(&key)
        .status()
        .unwrap()
        .success());
    for (k, drop_every, polling) in [(1, 0, "yield"), (4, 7, "yield"), (4, 7, "spin")] {
        let mut server = Child(
            Command::new(BIN)
                .args(["serve", "--once", "--polling", polling, "--credential-file"])
                .arg(&key)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        BufReader::new(server.0.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let ready: Value = serde_json::from_str(&line).unwrap();
        let output = root.path().join("result.json");
        let result = Command::new(BIN)
            .args([
                "client",
                "--endpoint",
                ready["control_endpoint"].as_str().unwrap(),
                "--credential-file",
            ])
            .arg(&key)
            .args([
                "--polling",
                polling,
                "--seconds",
                "0.04",
                "--warmup",
                "0",
                "--allowance",
                "64",
                "--multiplier",
                &k.to_string(),
                "--delay",
                &(k * 16 + 3).to_string(),
                "--drop-every",
                &drop_every.to_string(),
                "--duplicate-every",
                "5",
                "--output",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(report["correctness_errors"], 0);
        assert!(report["verified_samples"].as_u64().unwrap() > 0);
        assert!(report["server"]["blocks"].as_u64().unwrap() > 0);
        if drop_every != 0 {
            assert!(report["server"]["concealed_packets"].as_u64().unwrap() > 0);
        }
    }
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "native TCP and complete-input UDP process pairs")]
fn complete_input_transports_preserve_non_aligned_delay_and_report_socket_policy() {
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    assert!(Command::new(BIN)
        .arg("generate-credential")
        .arg(&key)
        .status()
        .unwrap()
        .success());
    for (transport, polling) in [("tcp", "yield"), ("tcp", "spin"), ("udp", "yield")] {
        let mut server = Child(
            Command::new(BIN)
                .args(["serve", "--once", "--polling", polling, "--credential-file"])
                .arg(&key)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        BufReader::new(server.0.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let ready: Value = serde_json::from_str(&line).unwrap();
        let output = root.path().join("complete.json");
        let result = Command::new(BIN)
            .args([
                "client",
                "--endpoint",
                ready["control_endpoint"].as_str().unwrap(),
                "--credential-file",
            ])
            .arg(&key)
            .args([
                "--transport",
                transport,
                "--input-policy",
                "complete",
                "--polling",
                polling,
                "--catch-up",
                "--seconds",
                "0.05",
                "--warmup",
                "0",
                "--allowance",
                "64",
                "--multiplier",
                "4",
                "--delay",
                "67",
                "--output",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert_eq!(report["transport"], transport);
        assert_eq!(report["input_policy"], "complete");
        assert_eq!(report["correctness_errors"], 0);
        assert_eq!(report["unverified_samples"], 0);
        assert!(report["verified_samples"].as_u64().unwrap() > 0);
        assert_eq!(report["server"]["concealed_packets"], 0);
        if transport == "tcp" {
            assert_eq!(report["client_audio"]["nodelay"], true);
            assert_eq!(report["server_audio"]["nodelay"], true);
            assert!(report["transport_error"].is_null());
        }
    }
}

#[shoop_wasm_test_support::shoop_test(no_wasm = "native process pair")]
fn invalid_deadline_is_reported_as_a_refusal_without_contacting_a_server() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("refusal.json");
    let result = Command::new(BIN)
        .args([
            "client",
            "--endpoint",
            "127.0.0.1:1",
            "--credential-file",
            "nonexistent",
            "--multiplier",
            "4",
            "--allowance",
            "2",
            "--output",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(result.status.success());
    let report: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(report["status"], "rejected");
    assert_eq!(report["reason"], "remote DSP Deadline");
}

fn rpc(tcp: &mut std::net::TcpStream, value: Value) -> Value {
    use std::io::{Read, Write};
    let raw = serde_json::to_vec(&value).unwrap();
    tcp.write_all(&(raw.len() as u32).to_le_bytes()).unwrap();
    tcp.write_all(&raw).unwrap();
    let mut prefix = [0; 4];
    tcp.read_exact(&mut prefix).unwrap();
    let mut raw = vec![0; u32::from_le_bytes(prefix) as usize];
    tcp.read_exact(&mut raw).unwrap();
    serde_json::from_slice(&raw).unwrap()
}
#[shoop_wasm_test_support::shoop_test(no_wasm = "native control and UDP lifecycle")]
fn control_stays_responsive_and_new_epochs_fence_old_udp_then_disconnect_stops_worker() {
    use serde_json::json;
    use shoop_remote_dsp::{wire, Config};
    use std::{
        net::{TcpStream, UdpSocket},
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    assert!(Command::new(BIN)
        .arg("generate-credential")
        .arg(&key)
        .status()
        .unwrap()
        .success());
    let mut server = Child(
        Command::new(BIN)
            .args(["serve", "--once", "--credential-file"])
            .arg(&key)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut line = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let ready: Value = serde_json::from_str(&line).unwrap();
    let token = std::fs::read(&key).unwrap().try_into().unwrap();
    let tcp = TcpStream::connect(ready["control_endpoint"].as_str().unwrap()).unwrap();
    let mut tcp =
        shoop_transport_socket::setup::authenticate(tcp, token, false, Duration::from_secs(2))
            .unwrap();
    tcp.set_nonblocking(false).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let config = Config {
        packet_frames: 4,
        multiplier: 1,
        allowance: 4,
        ..Config::default()
    };
    let configure = |epoch| json!({"request":"configure","version":1,"config":config,"epoch":epoch,"udp_port":udp.local_addr().unwrap().port(),"algorithm":{"type":"delay-copy","delay":0}});
    let mut raw = [0; 1452];
    let mut output = [0; 1452];
    let mut wrong_version = configure(99);
    wrong_version["version"] = json!(999);
    assert_eq!(rpc(&mut tcp, wrong_version)["reply"], "error");
    for epoch in [1, 2] {
        let reply = rpc(&mut tcp, configure(epoch));
        assert_eq!(reply["reply"], "ready");
        let address = format!("127.0.0.1:{}", reply["udp_port"]);
        assert_eq!(rpc(&mut tcp, json!({"request":"start"}))["reply"], "ok");
        assert_eq!(rpc(&mut tcp, json!({"request":"ping"}))["reply"], "ok");
        if epoch == 2 {
            let n = wire::encode(
                wire::Header::input(1, 0, config),
                config,
                &[0.25; 8],
                &[],
                &mut raw,
            )
            .unwrap();
            udp.send_to(&raw[..n], &address).unwrap();
        }
        let n = wire::encode(
            wire::Header::input(epoch, 0, config),
            config,
            &[0.25; 8],
            &[],
            &mut raw,
        )
        .unwrap();
        udp.send_to(&raw[..n], &address).unwrap();
        let (n, _) = udp.recv_from(&mut output).unwrap();
        let packet = wire::decode(&output[..n], config).unwrap();
        assert_eq!(packet.header.epoch, epoch);
        assert_eq!(packet.sample(1, 3), 0.25);
        assert_eq!(rpc(&mut tcp, json!({"request":"save"}))["reply"], "error");
        assert_eq!(rpc(&mut tcp, json!({"request":"pause"}))["reply"], "ok");
        let stats = rpc(&mut tcp, json!({"request":"stats"}));
        assert_eq!(stats["next_sequence"], 1);
        if epoch == 2 {
            assert_eq!(stats["stats"]["stale"], 1);
        }
        assert_eq!(rpc(&mut tcp, configure(epoch))["reply"], "error");
    }
    assert_eq!(rpc(&mut tcp, json!({"request":"start"}))["reply"], "ok");
    drop(tcp);
    let deadline = Instant::now() + Duration::from_secs(6);
    while server.0.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "worker outlived control connection"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "native TCP data authentication and control lifecycle"
)]
fn tcp_partial_input_keeps_control_responsive_and_audio_uses_a_fresh_credential() {
    use serde_json::json;
    use shoop_remote_dsp::{wire, Config};
    use std::{
        io::{Read, Write},
        net::TcpStream,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    assert!(Command::new(BIN)
        .arg("generate-credential")
        .arg(&key)
        .status()
        .unwrap()
        .success());
    let mut server = Child(
        Command::new(BIN)
            .args(["serve", "--once", "--credential-file"])
            .arg(&key)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut line = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let ready: Value = serde_json::from_str(&line).unwrap();
    let control = TcpStream::connect(ready["control_endpoint"].as_str().unwrap()).unwrap();
    let mut control = shoop_transport_socket::setup::authenticate(
        control,
        std::fs::read(&key).unwrap().try_into().unwrap(),
        false,
        Duration::from_secs(3),
    )
    .unwrap();
    control.set_nonblocking(false).unwrap();
    control
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let config = Config {
        packet_frames: 16,
        multiplier: 4,
        allowance: 8,
        ..Config::default()
    };
    let configure = |epoch| {
        json!({"request":"configure","version":1,"config":config,"epoch":epoch,"udp_port":0,
        "transport":"tcp","input_policy":"complete","algorithm":{"type":"delay-copy","delay":0}})
    };
    let mut invalid = configure(1);
    invalid["input_policy"] = json!("gap");
    assert_eq!(rpc(&mut control, invalid)["reply"], "error");
    let ready = rpc(&mut control, configure(1));
    let token: [u8; 32] = serde_json::from_value(ready["tcp_token"].clone()).unwrap();
    let stream = TcpStream::connect(format!("127.0.0.1:{}", ready["tcp_port"])).unwrap();
    let starting = std::thread::spawn(move || {
        let reply = rpc(&mut control, json!({"request":"start"}));
        (control, reply)
    });
    let mut data =
        shoop_transport_socket::setup::authenticate(stream, token, false, Duration::from_secs(3))
            .unwrap();
    data.set_nonblocking(false).unwrap();
    data.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    data.set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (mut control, started) = starting.join().unwrap();
    assert_eq!(started["reply"], "ok");
    let send = |data: &mut TcpStream, sequence| {
        let mut raw = [0; 1452];
        let n = wire::encode(
            wire::Header::input(1, sequence, config),
            config,
            &[0.25; 32],
            &[],
            &mut raw,
        )
        .unwrap();
        for byte in (n as u32).to_le_bytes() {
            data.write_all(&[byte]).unwrap();
        }
        for chunk in raw[..n].chunks(3) {
            data.write_all(chunk).unwrap();
        }
    };
    send(&mut data, 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert_eq!(rpc(&mut control, json!({"request":"ping"}))["reply"], "ok");
        assert_eq!(rpc(&mut control, json!({"request":"pause"}))["reply"], "ok");
        let stats = rpc(&mut control, json!({"request":"stats"}));
        assert_eq!(stats["stats"]["blocks"], 0);
        assert_eq!(stats["stats"]["concealed_packets"], 0);
        assert_eq!(stats["next_sequence"], 0);
        assert_eq!(rpc(&mut control, json!({"request":"start"}))["reply"], "ok");
        if stats["stats"]["received"] == 1 {
            break;
        }
        assert!(Instant::now() < deadline, "audio input was not received");
        std::thread::sleep(Duration::from_millis(1));
    }
    for sequence in 1..4 {
        send(&mut data, sequence);
    }
    for sequence in 0..4 {
        let mut prefix = [0; 4];
        data.read_exact(&mut prefix).unwrap();
        let length = u32::from_le_bytes(prefix) as usize;
        assert!(length <= config.max_packet_bytes());
        let mut raw = vec![0; length];
        data.read_exact(&mut raw).unwrap();
        let packet = wire::decode(&raw, config).unwrap();
        assert_eq!(packet.header.sequence, sequence);
        assert_eq!(packet.header.missing_mask, 0);
        assert_eq!(packet.sample(1, 15), 0.25);
    }
    assert_eq!(rpc(&mut control, json!({"request":"pause"}))["reply"], "ok");
    let ready = rpc(&mut control, configure(2));
    assert_ne!(
        serde_json::from_value::<[u8; 32]>(ready["tcp_token"].clone()).unwrap(),
        token
    );
    let stream = TcpStream::connect(format!("127.0.0.1:{}", ready["tcp_port"])).unwrap();
    let starting = std::thread::spawn(move || {
        let reply = rpc(&mut control, json!({"request":"start"}));
        (control, reply)
    });
    assert!(shoop_transport_socket::setup::authenticate(
        stream,
        token,
        false,
        Duration::from_secs(3)
    )
    .is_err());
    let (mut control, reply) = starting.join().unwrap();
    assert_eq!(reply["reply"], "error");
    assert_eq!(rpc(&mut control, json!({"request":"ping"}))["reply"], "ok");
    assert_eq!(rpc(&mut control, json!({"request":"stop"}))["reply"], "ok");
}

#[cfg(feature = "carla")]
#[shoop_wasm_test_support::shoop_test(no_wasm = "real native Carla process pair")]
fn native_carla_network_gain_and_saved_state_are_verified_when_required() {
    if std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_none() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    assert!(Command::new(BIN)
        .arg("generate-credential")
        .arg(&key)
        .status()
        .unwrap()
        .success());
    let project = root.path().join("gain.xml");
    let saved = root.path().join("state.txt");
    let xml = include_str!("../../shoop_engine/test_data/carla_legacy_rack_loaded_project.xml")
        .replacen("<Value>1</Value>", "<Value>0.5</Value>", 1);
    std::fs::write(&project, xml).unwrap();
    for (index, state) in [&project, &saved].iter().enumerate() {
        let mut server = Child(
            Command::new(BIN)
                .args([
                    "serve",
                    "--processor",
                    "carla",
                    "--once",
                    "--credential-file",
                ])
                .arg(&key)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        BufReader::new(server.0.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let ready: Value = serde_json::from_str(&line).unwrap();
        let output = root.path().join(format!("phase-{index}.json"));
        let mut command = Command::new(BIN);
        command
            .args(["client", "--processor", "carla", "--state-file"])
            .arg(state)
            .args([
                "--transport",
                if index == 0 { "udp" } else { "tcp" },
                "--expected-gain",
                "0.5",
                "--endpoint",
                ready["control_endpoint"].as_str().unwrap(),
                "--credential-file",
            ])
            .arg(&key)
            .args([
                "--seconds",
                "0.05",
                "--warmup",
                "0.05",
                "--allowance",
                "64",
                "--output",
            ])
            .arg(&output);
        if index == 0 {
            command.arg("--state-output").arg(&saved);
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(report["correctness_errors"], 0);
        assert!(report["verified_samples"].as_u64().unwrap() > 0);
        assert!(report["algorithm_latency_frames"].is_null());
    }
    assert!(std::fs::read_to_string(saved)
        .unwrap()
        .starts_with("shoop-carla-native-state:"));
}
