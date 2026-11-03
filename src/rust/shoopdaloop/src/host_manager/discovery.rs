use super::{Host, Source};
use shoop_host::{connection, remote::Target};
use shoop_host_processes::{identity, Process};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

pub(super) struct Scan {
    pub hosts: Vec<Host>,
    pub aliases: Vec<(Source, Source)>,
    pub warnings: Vec<String>,
}

enum Job {
    Local(Process),
    Manual(String),
}
impl Job {
    fn query(self) -> Option<Host> {
        match self {
            Self::Local(process) => local(process),
            Self::Manual(address) => Some(Host::query(address)),
        }
    }
}
fn local_address(mut address: SocketAddr) -> SocketAddr {
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv4() {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        });
    }
    address
}
fn local(process: Process) -> Option<Host> {
    let source = Source::Process(process.identity.clone());
    let mut error =
        "No listening sockets available; host may be starting or socket inspection unavailable"
            .to_owned();
    let deadline = Instant::now() + Duration::from_secs(2);
    for (index, address) in process.addresses.iter().enumerate() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match connection::describe(
            &local_address(*address).to_string(),
            remaining / (process.addresses.len() - index) as u32,
        ) {
            Ok(mut target) => {
                if target.process.as_ref() != Some(&process.identity) {
                    error = "Host description does not match the owning OS process".into();
                    continue;
                }
                match identity(process.identity.pid) {
                    Ok(Some(current)) if current == process.identity => {
                        target.use_local_addresses();
                        return Some(Host::from_target(source, target));
                    }
                    Ok(_) => return None,
                    Err(failure) => {
                        error = format!("Process inspection unavailable: {failure:#}");
                        break;
                    }
                }
            }
            Err(failure) => error = format!("{failure:#}"),
        }
    }
    Some(Host::unavailable(source, error))
}
fn probes(jobs: Vec<Job>) -> (Vec<Host>, Vec<String>) {
    let count = jobs.len().min(4);
    let jobs = Mutex::new(jobs.into_iter());
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..count)
            .map(|_| {
                scope.spawn(|| {
                    let mut hosts = Vec::new();
                    loop {
                        let job = jobs.lock().unwrap().next();
                        let Some(job) = job else {
                            break;
                        };
                        if let Some(host) = job.query() {
                            hosts.push(host);
                        }
                    }
                    hosts
                })
            })
            .collect();
        let mut hosts = Vec::new();
        let mut warnings = Vec::new();
        for worker in workers {
            match worker.join() {
                Ok(found) => hosts.extend(found),
                Err(_) => warnings.push("Host inspection worker failed".into()),
            }
        }
        (hosts, warnings)
    })
}
fn deduplicate(scan: &mut Scan) {
    scan.hosts.sort_by_key(|host| match &host.source {
        Source::Process(process) => (0, process.pid, String::new()),
        Source::Address(address) => (1, 0, address.clone()),
    });
    let mut instances: HashMap<u128, Source> = HashMap::new();
    scan.hosts.retain(|host| {
        let Some(Target { instance, .. }) = &host.target else {
            return true;
        };
        if *instance == 0 {
            return true;
        }
        if let Some(source) = instances.get(instance) {
            scan.aliases.push((host.source.clone(), source.clone()));
            false
        } else {
            instances.insert(*instance, host.source.clone());
            true
        }
    });
}
pub(super) fn scan(manual: Vec<String>, previous: Vec<Host>) -> Scan {
    let mut scan = Scan {
        hosts: vec![],
        aliases: vec![],
        warnings: vec![],
    };
    let mut jobs = Vec::new();
    match shoop_host_processes::snapshot() {
        Ok(snapshot) => {
            if !snapshot.errors.is_empty() {
                for mut host in previous {
                    let Source::Process(expected) = &host.source else {
                        continue;
                    };
                    if snapshot
                        .processes
                        .iter()
                        .any(|process| process.identity == *expected)
                    {
                        continue;
                    }
                    match identity(expected.pid) {
                        Ok(Some(current)) if current == *expected => {}
                        Ok(_) => continue,
                        Err(_) => {}
                    }
                    host.report =
                        Err("OS inspection unavailable; process exit is unconfirmed".into());
                    host.checked = Instant::now();
                    scan.hosts.push(host);
                }
            }
            scan.warnings.extend(snapshot.errors);
            jobs.extend(snapshot.processes.into_iter().map(Job::Local));
        }
        Err(error) => {
            let error = format!("OS process inspection unavailable: {error:#}");
            scan.warnings.push(error.clone());
            for mut host in previous {
                if matches!(host.source, Source::Process(_)) {
                    host.report = Err(error.clone());
                    host.checked = Instant::now();
                    scan.hosts.push(host);
                }
            }
        }
    }
    jobs.extend(manual.into_iter().map(Job::Manual));
    let (hosts, warnings) = probes(jobs);
    scan.hosts.extend(hosts);
    scan.warnings.extend(warnings);
    deduplicate(&mut scan);
    scan
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_host::{
        config::{AppTransport, HostConfig},
        launch::Startup,
        serve::StartOptions,
    };
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Child, Command, Stdio},
    };

    struct Fixture {
        child: Child,
        target: Option<Target>,
        identity: shoop_host_processes::Identity,
        _root: tempfile::TempDir,
        _stdout: BufReader<std::process::ChildStdout>,
    }
    impl Fixture {
        fn start(transport: AppTransport, unresponsive: bool) -> Self {
            let root = tempfile::tempdir().unwrap();
            let folder = root.path().join("companion with spaces");
            std::fs::create_dir(&folder).unwrap();
            let executable = folder.join(if cfg!(windows) {
                "shoop_host.exe"
            } else {
                "shoop_host"
            });
            std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
            let mut command = Command::new(&executable);
            shoop_host::launch::independent(&mut command);
            command
                .args([
                    "--exact",
                    "host_manager::discovery::tests::host_child",
                    "--nocapture",
                ])
                .env(
                    "SHOOP_MANAGER_HOST_FIXTURE",
                    if unresponsive {
                        "unresponsive"
                    } else {
                        "ready"
                    },
                )
                .stdin(Stdio::piped())
                .stdout(Stdio::piped());
            let mut child = command.spawn().unwrap();
            let mut config = HostConfig::default();
            config.settings.pools = shoop_app_api::NewEnginePools {
                tracks: 3,
                loops: 24,
                channels: 2,
                loop_memory_mb: 8,
            };
            config.transport = transport;
            let mut options = StartOptions::new().unwrap();
            options.name = "Same host name".into();
            shoop_host::launch::write_message(
                child.stdin.take().unwrap(),
                &Startup { config, options },
            )
            .unwrap();
            let mut reader = BufReader::new(child.stdout.take().unwrap());
            let mut line = String::new();
            let target = loop {
                line.clear();
                assert!(
                    reader.read_line(&mut line).unwrap() > 0,
                    "host fixture exited before readiness"
                );
                if let Some(value) = line.strip_prefix("HOST_READY=") {
                    break serde_json::from_str(value.trim()).unwrap();
                }
            };
            let identity = identity(child.id()).unwrap().unwrap();
            Self {
                child,
                target,
                identity,
                _root: root,
                _stdout: reader,
            }
        }
        fn source(&self) -> Source {
            Source::Process(self.identity.clone())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
        }
    }
    #[shoop_wasm_test_support::shoop_test(
        no_trace = "child fixture uses stdout for readiness and stdin for configuration"
    )]
    fn host_child() {
        let Some(mode) = std::env::var_os("SHOOP_MANAGER_HOST_FIXTURE") else {
            return;
        };
        let startup: Startup = shoop_host::launch::read_message(std::io::stdin().lock()).unwrap();
        if mode == "unresponsive" {
            let _listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            println!("HOST_READY=null");
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        shoop_host::serve::run_with_ready(startup.config, startup.options, |target| {
            println!("HOST_READY={}", serde_json::to_string(&Some(target))?);
            std::io::stdout().flush()?;
            Ok(())
        })
        .unwrap();
    }
    #[shoop_wasm_test_support::shoop_test]
    fn discovers_real_hosts_with_duplicate_names_and_deduplicates_manual_aliases() {
        let socket = Fixture::start(AppTransport::Socket, false);
        let shared = Fixture::start(AppTransport::SharedMemory, false);
        let manual = socket.target.as_ref().unwrap().management.clone();
        let mut previous = vec![];
        for _ in 0..3 {
            let observed = scan(vec![manual.clone()], previous);
            for fixture in [&socket, &shared] {
                let related: Vec<_> = observed.hosts.iter().filter(|host| {
                    matches!(&host.source, Source::Process(process) if process.executable == fixture.identity.executable)
                }).collect();
                assert_eq!(
                    related.len(),
                    1,
                    "Host worker threads must not appear as independent hosts"
                );
                let matches: Vec<_> = observed
                    .hosts
                    .iter()
                    .filter(|host| host.source == fixture.source())
                    .collect();
                assert_eq!(matches.len(), 1);
                let host = matches[0];
                assert_eq!(host.name, "Same host name");
                assert!(host.report.as_ref().unwrap().ready);
                let target = host.target.as_ref().unwrap();
                assert!(target.can_attach);
                assert_eq!(target.instance, fixture.target.as_ref().unwrap().instance);
            }
            assert!(observed
                .aliases
                .contains(&(Source::Address(manual.clone()), socket.source())));
            previous = observed.hosts;
        }
        let fresh = scan(vec![], vec![]);
        for fixture in [&socket, &shared] {
            assert!(fresh
                .hosts
                .iter()
                .any(|host| host.source == fixture.source()));
            assert_eq!(std::fs::read_dir(fixture._root.path()).unwrap().count(), 1);
        }
        assert!(local_address("0.0.0.0:4521".parse().unwrap())
            .ip()
            .is_loopback());
        assert!(local_address("[::]:4521".parse().unwrap())
            .ip()
            .is_loopback());
    }
    #[shoop_wasm_test_support::shoop_test]
    fn unresponsive_hosts_remain_visible_and_changed_identity_never_stops_a_child() {
        let mut fixture = Fixture::start(AppTransport::Socket, true);
        assert!(fixture.target.is_none());
        let observed = scan(vec![], vec![]);
        let host = observed
            .hosts
            .iter()
            .find(|host| host.source == fixture.source())
            .unwrap();
        assert!(host.report.is_err() && host.target.is_none());
        let mut changed = fixture.identity.clone();
        changed.started ^= 1;
        assert!(shoop_host_processes::force_stop(&changed).is_err());
        assert!(fixture.child.try_wait().unwrap().is_none());
        let mut manager = super::super::Manager::new(None);
        manager.next_scan = Instant::now() + Duration::from_secs(3600);
        manager.hosts = observed.hosts.clone();
        manager.select(fixture.source());
        let mut screen = super::super::tests::Screen::new(eframe::egui::vec2(800.0, 500.0));
        screen.frame(vec![], |ui| manager.show(ui));
        screen.click("•••", |ui| manager.show(ui));
        screen.click("Force stop…", |ui| manager.show(ui));
        screen.click("Force stop", |ui| manager.show(ui));
        let deadline = Instant::now() + Duration::from_secs(10);
        while manager.action_pending {
            manager.poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(manager
            .notice
            .as_ref()
            .unwrap()
            .message
            .contains("Forcibly terminated PID"));
        fixture.child.wait().unwrap();
        let fresh = scan(vec![], observed.hosts);
        assert!(!fresh
            .hosts
            .iter()
            .any(|host| host.source == fixture.source()));
    }
    fn attach(target: &Target) -> (shoop_app_client::Client, shoop_host::connection::Connection) {
        let mut connection = connection::connect(target, Duration::from_secs(2)).unwrap();
        let mut client = shoop_app_client::Client::new();
        connection.send(client.attach().unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while client.state() != shoop_app_client::State::Ready {
            connection.poll(&mut client).unwrap();
            if let Some(message) = client.ready_message() {
                connection.send(message).unwrap();
                client.ready_sent().unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        (client, connection)
    }
    fn command(
        client: &mut shoop_app_client::Client,
        connection: &mut shoop_host::connection::Connection,
        command: shoop_app_protocol::Command,
    ) -> shoop_app_protocol::Outcome {
        let message = client.command(command, None).unwrap();
        let shoop_app_protocol::ClientMessage::Submit { operation, .. } = message else {
            unreachable!()
        };
        connection.send(message).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            connection.poll(client).unwrap();
            if let Some(outcome) = client.results().get(&operation) {
                return outcome.clone();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[shoop_wasm_test_support::shoop_test]
    fn fresh_managers_rediscover_the_same_recording_and_socket_ui_can_reattach() {
        use shoop_app_api::*;
        use shoop_app_protocol::{Command, Outcome};
        let mut fixture = Fixture::start(AppTransport::Socket, false);
        let target = fixture.target.as_ref().unwrap();
        let (mut client, mut connection) = attach(target);
        command(
            &mut client,
            &mut connection,
            Command::Global(GlobalControlAction::SetSync(false)),
        );
        let Outcome::TrackCreated { track, loops, .. } = command(
            &mut client,
            &mut connection,
            Command::AddTrack(TrackSpec {
                name: "Discovery keeps this take".into(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 1,
                    midi: true,
                },
                latency: Default::default(),
                initial_output_bus_names: vec!["Master".into()],
                creation_request_id: None,
            }),
        ) else {
            panic!("track creation failed");
        };
        command(
            &mut client,
            &mut connection,
            Command::Loop {
                track,
                loop_id: loops[0],
                action: LoopAction::RecordClicked,
            },
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while client.snapshot().unwrap().tracks[1].loops[0].mode != LoopMode::Recording {
            connection.poll(&mut client).unwrap();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let frames = client.snapshot().unwrap().status.processed_frames;
        for _ in 0..2 {
            super::super::verify_host_discovery(target, Some((true, true))).unwrap();
            assert!(connection::shutdown_ownerless(target, Duration::from_secs(2)).is_err());
        }
        assert!(fixture.child.try_wait().unwrap().is_none());
        while client.snapshot().unwrap().status.processed_frames <= frames {
            connection.poll(&mut client).unwrap();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            client.snapshot().unwrap().tracks[1].loops[0].mode,
            LoopMode::Recording
        );
        drop(connection);
        std::thread::sleep(Duration::from_millis(30));
        let (mut replacement, mut connection) = attach(target);
        assert_eq!(
            replacement.snapshot().unwrap().tracks[1].name,
            "Discovery keeps this take"
        );
        assert_eq!(
            replacement.snapshot().unwrap().tracks[1].loops[0].mode,
            LoopMode::Recording
        );
        command(
            &mut replacement,
            &mut connection,
            Command::Loop {
                track,
                loop_id: loops[0],
                action: LoopAction::PlayClicked,
            },
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while replacement.snapshot().unwrap().tracks[1].loops[0].mode != LoopMode::Playing {
            connection.poll(&mut replacement).unwrap();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        drop(connection);
        std::thread::sleep(Duration::from_millis(30));
        connection::shutdown_ownerless(target, Duration::from_secs(5)).unwrap();
        assert!(fixture.child.wait().unwrap().success());
        assert!(!scan(vec![], vec![])
            .hosts
            .iter()
            .any(|host| host.source == fixture.source()));
        assert_eq!(std::fs::read_dir(fixture._root.path()).unwrap().count(), 1);
    }
    #[shoop_wasm_test_support::shoop_test]
    fn keeps_every_candidate_and_prefers_os_sources_over_manual_targets() {
        let mut hosts: Vec<_> = (0..90)
            .map(|index| {
                let mut host = super::super::tests::host();
                let target = host.target.as_mut().unwrap();
                target.instance += index;
                target.process.as_mut().unwrap().pid += index as u32;
                host.source = Source::Process(target.process.clone().unwrap());
                host
            })
            .collect();
        let mut manual = hosts[0].clone();
        manual.source = Source::Address("localhost:15401".into());
        hosts.push(manual);
        let mut result = Scan {
            hosts,
            aliases: vec![],
            warnings: vec![],
        };
        deduplicate(&mut result);
        assert_eq!(result.hosts.len(), 90);
        assert_eq!(result.aliases.len(), 1);
        assert!(result
            .hosts
            .iter()
            .all(|host| matches!(host.source, Source::Process(_))));
    }
}
