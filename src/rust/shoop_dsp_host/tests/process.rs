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
    for (k, drop_every) in [(1, 0), (4, 7)] {
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
