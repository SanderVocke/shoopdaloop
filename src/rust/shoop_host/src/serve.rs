use crate::{
    config::AppTransport,
    config::HostConfig,
    connection::Port,
    engine,
    remote::{identity, RemoteProfile, Target},
};
use anyhow::{anyhow, ensure, Result};
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_protocol::{codec, *};
use shoop_rt_backend::{RtBackend, Sleep};
use shoop_rt_host_native::{DriverConfig, NativeConfig, NativeHost};
use shoop_transport::{RecordPort, MAX_RECORD_BYTES};
use shoop_transport_socket::setup::Listener;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StartOptions {
    pub endpoint: SocketAddr,
    pub management_endpoint: SocketAddr,
    pub lan: bool,
    pub advertised_endpoint: Option<String>,
    pub advertised_management: Option<String>,
    pub export_profile: Option<PathBuf>,
    pub network: crate::remote::NetworkOptions,
    pub log_file: Option<PathBuf>,

    pub name: String,
}
impl StartOptions {
    pub fn new() -> Result<Self> {
        Ok(Self {
            endpoint: "127.0.0.1:0".parse()?,
            management_endpoint: "127.0.0.1:0".parse()?,
            lan: false,
            advertised_endpoint: None,
            advertised_management: None,
            export_profile: None,
            network: Default::default(),
            log_file: None,

            name: "ShoopDaLoop".into(),
        })
    }
}

struct Acceptor {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    ports: Receiver<shoop_transport_socket::Port>,
}
impl Acceptor {
    fn new(
        listener: Listener,
        lan: bool,
        target: &Target,
        available: Arc<AtomicBool>,
        endpoint: crate::connection::Endpoint,
    ) -> Result<Self> {
        use shoop_transport::{
            handshake::{Handshake, Intent},
            stream::ByteIo,
        };
        crate::launch::write_message(std::io::sink(), target)?;
        let target = target.clone();
        let admission = crate::connection::admission_header(endpoint);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let (sender, ports) = mpsc::sync_channel(4);
        let join = thread::Builder::new()
            .name("shoop-accept".into())
            .spawn(move || {
                struct Pending {
                    io: shoop_transport_socket::NativeStream,
                    hello: Handshake,
                    written: usize,
                    description: Vec<u8>,
                    admission: [u8; 3],
                    read: usize,
                    deadline: Instant,
                }
                let mut pending = Vec::<Pending>::with_capacity(16);
                let mut last = Instant::now();
                while !stopping.load(Ordering::Acquire) {
                    let now = Instant::now();
                    let elapsed = now.saturating_duration_since(last);
                    last = now;
                    for _ in 0..4 {
                        match listener.accept_stream() {
                            Ok(Some(stream)) if pending.len() < 16 => pending.push(Pending {
                                io: shoop_transport_socket::NativeStream(stream),
                                hello: Handshake::new(true, Duration::from_secs(2)),
                                written: 0,
                                description: Vec::new(),
                                admission: [0; 3],
                                read: 0,
                                deadline: now + Duration::from_secs(2),
                            }),
                            Ok(Some(_)) => {}
                            Ok(None) => break,
                            Err(_) => return,
                        }
                    }
                    let mut index = 0;
                    while index < pending.len() {
                        if now >= pending[index].deadline {
                            pending.swap_remove(index);
                            continue;
                        }
                        let item = &mut pending[index];
                        let mut remove = false;
                        match item.hello.advance(&mut item.io, elapsed) {
                            Ok(false) => {}
                            Ok(true) if item.hello.intent() == Intent::Inspect => {
                                if item.description.is_empty() {
                                    let mut target = target.clone();
                                    target.can_attach = available.load(Ordering::Acquire);
                                    if crate::launch::write_message(&mut item.description, &target)
                                        .is_err()
                                    {
                                        pending.swap_remove(index);
                                        continue;
                                    }
                                }
                                match item.io.try_write(&item.description[item.written..]) {
                                    Ok(0) => remove = true,
                                    Ok(count) => {
                                        item.written += count;
                                        remove = item.written == item.description.len();
                                    }
                                    Err(
                                        shoop_transport::stream::IoError::WouldBlock
                                        | shoop_transport::stream::IoError::Interrupted,
                                    ) => {}
                                    Err(_) => remove = true,
                                }
                            }
                            Ok(true) => {
                                match item.io.try_read(&mut item.admission[item.read..]) {
                                    Ok(0) => remove = true,
                                    Ok(count) => item.read += count,
                                    Err(
                                        shoop_transport::stream::IoError::WouldBlock
                                        | shoop_transport::stream::IoError::Interrupted,
                                    ) => {}
                                    Err(_) => remove = true,
                                }
                                if remove
                                    || (item.read == admission.len() && item.admission != admission)
                                {
                                    pending.swap_remove(index);
                                    continue;
                                }
                                if item.read != admission.len() {
                                    index += 1;
                                    continue;
                                }
                                let item = pending.swap_remove(index);
                                let config = if lan {
                                    shoop_transport_socket::Config {
                                        reliable: 8,
                                        telemetry: 1,
                                        bulk: 8,
                                    }
                                } else {
                                    Default::default()
                                };
                                if let Ok(port) =
                                    shoop_transport_socket::Port::from_stream(item.io.0, config)
                                {
                                    let _ = sender.try_send(port);
                                }
                                continue;
                            }
                            Err(_) => remove = true,
                        }
                        if remove {
                            pending.swap_remove(index);
                        } else {
                            index += 1;
                        }
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            })?;
        Ok(Self {
            stop,
            join: Some(join),
            ports,
        })
    }
}
impl Drop for Acceptor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub fn run(config: HostConfig, options: StartOptions) -> Result<()> {
    run_with_ready(config, options, |_| Ok(()))
}

pub fn run_with_ready(
    config: HostConfig,
    options: StartOptions,
    ready: impl FnOnce(Target) -> Result<()>,
) -> Result<()> {
    let _span = tracing::info_span!("host.session").entered();
    config.validate()?;
    options.network.validate()?;
    ensure!(
        !options.lan || config.transport == AppTransport::Socket,
        "LAN hosts require socket delivery"
    );
    ensure!(options.name.len() <= 128, "invalid host identity or name");
    let (mut native, mut warnings) = engine::native_config(
        &config.driver,
        &config.pools,
        &engine::inventory(&config.driver),
    );
    #[cfg(feature = "testing")]
    if config.test_signal {
        match &mut native.driver {
            DriverConfig::Dummy(dummy) => dummy.input = Some(|_, _| 0.25),
            #[cfg(feature = "cpal")]
            DriverConfig::Cpal(_) => {}
        }
    }
    #[cfg(not(feature = "testing"))]
    let _ = &mut native;
    let started = match shoop_rt_host_native::start(&native) {
        Ok(started) => started,
        Err(error) if !matches!(native.driver, DriverConfig::Dummy(_)) => {
            warnings.push(format!(
                "Could not start the configured audio driver: {error}; using dummy"
            ));
            let dummy = shoop_app_api::DummyAudioDriverConfig::default();
            shoop_rt_host_native::start(&NativeConfig {
                driver: DriverConfig::Dummy(engine::dummy_config(&dummy)),
                engine: engine::engine_description(&config.pools, dummy.buffer_size),
            })?
        }
        Err(error) => return Err(error.into()),
    };
    warnings.extend(started.host.warnings().iter().cloned());
    for warning in &warnings {
        tracing::warn!("{warning}");
    }
    let mut host = Some(started.host);
    let backend = RtBackend::connect(started.client, Sleep::default())?;
    let mut service = Service::new(
        Box::new(backend),
        ServiceOptions {
            master_auto_connect: config.master_auto_connect,
            loop_smoothing_ms: config.loop_smoothing_ms,
        },
    )?;
    service.set_host_settings(config.settings.clone(), warnings.clone())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !service.is_ready() {
        service.tick(Duration::from_millis(2));
        ensure!(
            service.failure().is_none(),
            "session initialization failed: {:?}",
            service.failure()
        );
        ensure!(
            Instant::now() < deadline,
            "session initialization timed out"
        );
        thread::sleep(Duration::from_millis(2));
    }
    let bind = if options.lan {
        Listener::bind_lan
    } else {
        Listener::bind_with
    };
    let data = bind(options.endpoint)?;
    let management = bind(options.management_endpoint)?;
    let descriptor = if config.transport == AppTransport::SharedMemory {
        Some(shoop_transport_shared_memory::Descriptor::new([
            512, 32, 32,
        ])?)
    } else {
        None
    };
    let mut shared = descriptor
        .map(|descriptor| {
            shoop_transport_shared_memory::Port::create(
                descriptor,
                shoop_transport_shared_memory::Side::Server,
            )
        })
        .transpose()?;
    let local_endpoint = concrete(data.address()?).to_string();
    let local_management = concrete(management.address()?).to_string();
    let target = Target {
        can_attach: true,
        local_endpoint: Some(local_endpoint.clone()),
        local_management: Some(local_management.clone()),
        process: shoop_host_processes::identity(std::process::id())?,
        instance: identity()?,
        version: VERSION,
        name: options.name.clone(),
        endpoint: options
            .advertised_endpoint
            .clone()
            .unwrap_or(local_endpoint),
        management: options
            .advertised_management
            .clone()
            .unwrap_or(local_management),
        transport: config.transport,
        shared_memory: descriptor.map(|descriptor| descriptor.encode().to_vec()),
        warnings,
        network: options.lan.then_some(options.network),
        log_file: options
            .log_file
            .as_ref()
            .map(|path| path.canonicalize())
            .transpose()?,
    };
    if let Some(path) = &options.export_profile {
        let profile = RemoteProfile {
            version: VERSION,
            name: target.name.clone(),
            endpoint: target.endpoint.clone(),
            management: target.management.clone(),
            network: options.network,
            local: !options.lan,
            transport: target.transport,
            log_file: if options.lan {
                None
            } else {
                target.log_file.clone()
            },
        };
        if path.exists() {
            let existing = RemoteProfile::read(path)?;
            ensure!(
                existing.endpoint == profile.endpoint
                    && existing.management == profile.management
                    && existing.transport == profile.transport,
                "existing profile selects different endpoints"
            );
        } else {
            profile.write(path)?;
        }
    }
    tracing::info!(
        "Host ready: data={}, management={}",
        target.endpoint,
        target.management
    );
    let available = Arc::new(AtomicBool::new(true));
    let data = Acceptor::new(
        data,
        options.lan,
        &target,
        Arc::clone(&available),
        crate::connection::Endpoint::Data,
    )?;
    let management = Acceptor::new(
        management,
        options.lan,
        &target,
        Arc::clone(&available),
        crate::connection::Endpoint::Management,
    )?;
    ready(target)?;
    let mut clients = Vec::new();
    let mut managers = Vec::new();

    let mut last = Instant::now();
    let mut finished = None;
    let mut cleanup_error = None;
    loop {
        for _ in 0..4 {
            let Ok(mut port) = data.ports.try_recv() else {
                break;
            };
            if clients.len() >= 4 {
                continue;
            }
            let shared_port = if config.transport == AppTransport::SharedMemory {
                match shared.take() {
                    Some(shared) => {
                        available.store(false, Ordering::Release);
                        Some(shared.with_liveness(port.stream().try_clone()?)?)
                    }
                    None => {
                        let _ = send(&mut port, &WireMessage::Rejected(Refusal::Unsupported));
                        continue;
                    }
                }
            } else {
                None
            };
            if send(&mut port, &WireMessage::CarrierReady).is_err() {
                continue;
            }
            clients.push(Data::new(
                ConnectionId(identity()?),
                port,
                shared_port,
                options.lan.then_some(options.network),
            )?);
        }
        for _ in 0..4 {
            match management.ports.try_recv() {
                Ok(port) if managers.len() < 4 => {
                    managers.push(shoop_app_connection::Management::new(port))
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let now = Instant::now();
        let elapsed = now.duration_since(last);
        for client in &mut clients {
            client.core.advance(elapsed);
            client.receive(&mut service);
        }
        service.tick(elapsed);
        last = now;
        if service.shutdown_ready() {
            service
                .begin_host_cleanup()
                .map_err(|reason| anyhow!("host cleanup refused: {reason:?}"))?;
            let result = match host.take() {
                Some(host) => stop(host),
                None => Err(cleanup_error
                    .clone()
                    .unwrap_or_else(|| "host cleanup could not be completed".into())),
            };
            if let Err(error) = &result {
                cleanup_error = Some(error.clone());
            }
            if result.is_ok() {
                finished = Some(Instant::now() + Duration::from_secs(2));
            }
            service.complete_shutdown(result);
        }
        managers.retain_mut(|manager| manager.pump(&mut service, elapsed));
        for client in &mut clients {
            client.core.write(&mut service);
        }
        clients.retain_mut(|client| {
            let alive = client.core.alive(&service);
            if !alive {
                service.detach(client.core.id);
            }
            alive
        });
        if finished.is_some_and(|deadline| Instant::now() >= deadline) {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    drop(service);
    drop(host);
    Ok(())
}

fn concrete(mut address: SocketAddr) -> SocketAddr {
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    address
}
fn stop(host: NativeHost) -> Result<(), String> {
    let report = host.stop();
    let mut errors = Vec::new();
    if !report.midi_reset {
        errors.push("MIDI reset was not confirmed".to_owned());
    }
    if report.phase == shoop_rt_host_native::RenderPhase::Faulted {
        errors.push("rendering faulted".to_owned());
    }
    if let Err(error) = report.stopped {
        errors.push(format!("engine cleanup failed: {error}"));
    }
    if report.stats.midi_out_failed > 0 {
        errors.push("MIDI output cleanup failed".to_owned());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
fn send(port: &mut impl RecordPort, message: &WireMessage) -> Result<()> {
    let mut bytes = [0; MAX_RECORD_BYTES];
    let length =
        codec::encode(message, &mut bytes).map_err(|_| anyhow!("invalid host response"))?;
    port.post(&bytes[..length])
        .map_err(|error| anyhow!("host response delivery failed: {error:?}"))
}
struct Data {
    core: shoop_app_connection::Session<Port>,
    shared: Option<shoop_transport_shared_memory::Port>,
}
impl Data {
    fn new(
        id: ConnectionId,
        port: shoop_transport_socket::Port,
        shared: Option<shoop_transport_shared_memory::Port>,
        network: Option<crate::remote::NetworkOptions>,
    ) -> Result<Self> {
        let core = shoop_app_connection::Session::new(id, Port::Socket(port));
        let core = match network {
            Some(network) => core
                .with_network(
                    network.heartbeat(),
                    (crate::remote::identity()? as u64).max(1),
                    network.snapshot_hz,
                )
                .map_err(|_| anyhow!("invalid heartbeat settings"))?,
            None => core,
        };
        Ok(Self { core, shared })
    }
    fn receive(&mut self, service: &mut Service) {
        if self.shared.is_some() {
            if let Port::Socket(port) = &mut self.core.port {
                let _ = port.flush();
                if !port.outgoing_empty() {
                    return;
                }
            }
            self.core.port = Port::Shared(self.shared.take().unwrap());
        }
        self.core.receive(service);
    }
}
