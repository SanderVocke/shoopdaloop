#![cfg(not(target_arch = "wasm32"))]
use shoop_host_processes::{force_stop, host_executable, identity, inspect};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Child, Command, Stdio},
};

struct Fixture {
    child: Child,
    addresses: Vec<std::net::SocketAddr>,
}
impl Fixture {
    fn start() -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "listening_child", "--nocapture"])
            .env("SHOOP_PROCESS_FIXTURE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            line.clear();
            assert!(
                reader.read_line(&mut line).unwrap() > 0,
                "fixture exited before readiness"
            );
            if let Some(value) = line.strip_prefix("SOCKETS=") {
                return Self {
                    child,
                    addresses: value
                        .trim()
                        .split(',')
                        .map(|s| s.parse().unwrap())
                        .collect(),
                };
            }
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[shoop_wasm_test_support::shoop_test(
    no_trace = "fixture owns stdin and stdout for process readiness"
)]
fn listening_child() {
    if std::env::var_os("SHOOP_PROCESS_FIXTURE").is_none() {
        return;
    }
    let sockets = [
        TcpListener::bind("127.0.0.1:0").unwrap(),
        TcpListener::bind("[::1]:0").unwrap(),
        TcpListener::bind("0.0.0.0:0").unwrap(),
    ];
    println!(
        "SOCKETS={}",
        sockets
            .iter()
            .map(|s| s.local_addr().unwrap().to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    std::io::stdout().flush().unwrap();
    let _ = std::io::stdin().read(&mut [0]);
}
#[shoop_wasm_test_support::shoop_test]
fn associates_real_child_listeners_and_exact_process_identity() {
    let first = Fixture::start();
    let second = Fixture::start();
    let snapshot = inspect(&[first.child.id(), second.child.id()]).unwrap();
    assert_eq!(snapshot.processes.len(), 2);
    for fixture in [&first, &second] {
        let process = snapshot
            .processes
            .iter()
            .find(|p| p.identity.pid == fixture.child.id())
            .unwrap();
        assert!(process.identity.started > 0);
        assert_eq!(
            process.identity.executable.canonicalize().unwrap(),
            std::env::current_exe().unwrap().canonicalize().unwrap()
        );
        for address in &fixture.addresses {
            assert!(
                process.addresses.contains(address),
                "missing {address}: {:?}; errors: {:?}",
                process.addresses,
                snapshot.errors
            );
        }
        assert_eq!(
            identity(fixture.child.id()).unwrap().as_ref(),
            Some(&process.identity)
        );
    }
}
#[shoop_wasm_test_support::shoop_test]
fn refuses_changed_identity_and_confirms_only_the_selected_child_exits() {
    let mut first = Fixture::start();
    let mut second = Fixture::start();
    let expected = identity(first.child.id()).unwrap().unwrap();
    let mut changed = expected.clone();
    changed.started ^= 1;
    assert!(force_stop(&changed).is_err());
    assert!(first.child.try_wait().unwrap().is_none());
    force_stop(&expected).unwrap();
    first.child.wait().unwrap();
    assert!(identity(expected.pid).unwrap().is_none());
    assert!(second.child.try_wait().unwrap().is_none());
    let snapshot = inspect(&[expected.pid, second.child.id(), u32::MAX]).unwrap();
    assert_eq!(snapshot.processes.len(), 1);
    assert_eq!(snapshot.processes[0].identity.pid, second.child.id());
}
#[shoop_wasm_test_support::shoop_test]
fn recognizes_installed_host_names_and_rejects_unrelated_programs() {
    for name in ["shoop_host", "shoop_host.exe", "SHOOP_HOST.EXE"] {
        assert!(host_executable(std::path::Path::new(name)));
    }
    for name in [
        "shoopdaloop",
        "shoop_host_processes",
        "other",
        "shoop_host.backup",
    ] {
        assert!(!host_executable(std::path::Path::new(name)));
    }
    assert!(identity(u32::MAX).unwrap().is_none());
}
