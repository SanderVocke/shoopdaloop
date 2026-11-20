use super::discovery;
use anyhow::{ensure, Context, Result};
use eframe::egui;
use shoop_host::launch;
use shoop_host_processes::Identity;
use shoop_remote_fx::{describe, Description};
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read},
    net::SocketAddr,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

type Reply = std::result::Result<Vec<Service>, String>;

struct Service {
    address: SocketAddr,
    identity: Option<Identity>,
    description: std::result::Result<Description, String>,
}

pub(super) struct Panel {
    pub(super) open: bool,
    bind: String,
    lan: bool,
    manual: String,
    addresses: Vec<SocketAddr>,
    services: Vec<Service>,
    next: Instant,
    pending: Option<Receiver<Reply>>,
    error: Option<String>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            open: false,
            bind: "127.0.0.1:0".into(),
            lan: false,
            manual: String::new(),
            addresses: Vec::new(),
            services: Vec::new(),
            next: Instant::now(),
            pending: None,
            error: None,
        }
    }
}

impl Panel {
    pub(super) fn show(&mut self, context: &egui::Context) -> Option<SocketAddr> {
        if let Some(pending) = &self.pending {
            match pending.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    match result {
                        Ok(services) => self.services = services,
                        Err(error) => self.error = Some(error),
                    }
                    self.next = Instant::now() + Duration::from_secs(2);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.error = Some("FX service inspection stopped".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.open {
            return None;
        }
        context.request_repaint_after(Duration::from_millis(100));
        if self.pending.is_none() && Instant::now() >= self.next {
            self.refresh(|| Ok(()));
        }
        let mut selected = None;
        let mut open = self.open;
        let mut stop = None;
        egui::Window::new("FX services")
            .open(&mut open)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Listen address");
                    ui.text_edit_singleline(&mut self.bind);
                    ui.checkbox(&mut self.lan, "LAN access");
                    if ui
                        .add_enabled(self.pending.is_none(), egui::Button::new("Start service"))
                        .clicked()
                    {
                        let bind = self.bind.clone();
                        let lan = self.lan;
                        self.refresh(move || {
                            let address =
                                bind.parse().context("Invalid FX service listen address")?;
                            launch_service(address, lan)?;
                            Ok(())
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Service address");
                    ui.text_edit_singleline(&mut self.manual);
                    if ui.button("Inspect address").clicked() {
                        match self.manual.trim().parse() {
                            Ok(address) if !self.addresses.contains(&address) => {
                                self.addresses.push(address);
                                self.next = Instant::now();
                            }
                            Ok(_) => self.next = Instant::now(),
                            Err(_) => self.error = Some("Enter an IP address and port".into()),
                        }
                    }
                });
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                for service in &self.services {
                    ui.separator();
                    ui.strong(service.address.to_string());
                    match &service.description {
                        Ok(description) => {
                            ui.label(format!(
                                "{} active · {} of {} slots reserved",
                                description.active, description.reserved, description.slots,
                            ));
                            ui.label(format!(
                                "Supported chains: {}",
                                description
                                    .kinds
                                    .iter()
                                    .map(|kind| match kind {
                                        shoop_remote_fx::Kind::Rack => "Rack",
                                        shoop_remote_fx::Kind::Patchbay => "Patchbay",
                                        shoop_remote_fx::Kind::Patchbay16x => "Patchbay 16x",
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                            ui.horizontal(|ui| {
                                if ui.button("Use for new host…").clicked() {
                                    selected = Some(service.address);
                                }
                                if let Some(identity) = &service.identity {
                                    if ui
                                        .add_enabled(
                                            self.pending.is_none(),
                                            egui::Button::new("Stop service"),
                                        )
                                        .clicked()
                                    {
                                        stop = Some((
                                            service.address,
                                            identity.clone(),
                                            description.instance.clone(),
                                        ));
                                    }
                                }
                            });
                        }
                        Err(error) => {
                            ui.colored_label(egui::Color32::LIGHT_RED, error);
                        }
                    }
                }
            });
        if let Some((address, identity, instance)) = stop {
            self.refresh(move || stop_service(address, &identity, instance));
        }
        self.open = open && selected.is_none();
        selected
    }

    fn refresh(&mut self, first: impl FnOnce() -> Result<()> + Send + 'static) {
        let addresses = self.addresses.clone();
        let (reply, pending) = mpsc::channel();
        self.pending = Some(pending);
        self.error = None;
        thread::spawn(move || {
            let result = first()
                .and_then(|()| scan(addresses))
                .map_err(|error| format!("{error:#}"));
            let _ = reply.send(result);
        });
    }
}

fn scan(addresses: Vec<SocketAddr>) -> Result<Vec<Service>> {
    let mut services = Vec::new();
    let mut instances = HashSet::new();
    for process in shoop_host_processes::snapshot()?.processes {
        for address in process.addresses.into_iter().map(discovery::local_address) {
            if let Ok(description) = describe(address) {
                if description.pid == process.identity.pid
                    && instances.insert(description.instance.clone())
                {
                    services.push(Service {
                        address,
                        identity: Some(process.identity.clone()),
                        description: Ok(description),
                    });
                }
            }
        }
    }
    for address in addresses {
        let description =
            describe(address).map_err(|error| format!("FX service unavailable: {error}"));
        if description
            .as_ref()
            .is_ok_and(|value| !instances.insert(value.instance.clone()))
        {
            continue;
        }
        services.push(Service {
            address,
            identity: None,
            description,
        });
    }
    Ok(services)
}

fn launch_service(address: SocketAddr, lan: bool) -> Result<(SocketAddr, Description)> {
    ensure!(
        lan || address.ip().is_loopback(),
        "Enable LAN access for a non-loopback service"
    );
    let executable = launch::companion_path(&std::env::current_exe()?)?;
    let mut command = Command::new(executable);
    command
        .arg("--fx-service")
        .arg("--listen")
        .arg(address.to_string());
    if lan {
        command.arg("--lan");
    }
    launch::independent(&mut command);
    command.stdout(Stdio::piped());
    let mut child = command.spawn().context("Start desktop FX service")?;
    let stdout = child
        .stdout
        .take()
        .context("FX service readiness pipe unavailable")?;
    let (sender, ready) = mpsc::channel();
    thread::spawn(move || {
        let result = BufReader::new(stdout.take(4096))
            .lines()
            .map_while(std::result::Result::ok)
            .find_map(|line| line.strip_prefix("FX service listening on ")?.parse().ok());
        let _ = sender.send(result);
    });
    let result = ready
        .recv_timeout(Duration::from_secs(5))
        .ok()
        .flatten()
        .context(
            "FX service did not start. Use a Carla-enabled companion with its runtime installed.",
        )
        .and_then(|address| {
            let address = discovery::local_address(address);
            let description = describe(address).context("Read FX service capabilities")?;
            ensure!(
                child.id() == description.pid,
                "FX service readiness identity changed"
            );
            Ok((address, description))
        });
    if result.is_err() {
        let _ = child.kill();
    }
    launch::reap(child);
    result
}

fn stop_service(address: SocketAddr, identity: &Identity, instance: String) -> Result<()> {
    ensure!(
        shoop_host_processes::identity(identity.pid)?.as_ref() == Some(identity),
        "FX service identity changed"
    );
    shoop_remote_fx::shutdown(address, instance)?;
    Ok(())
}

pub(super) fn probe() -> Result<()> {
    let (address, started) = launch_service("127.0.0.1:0".parse()?, false)?;
    let identity =
        shoop_host_processes::identity(started.pid)?.context("FX service locally unavailable")?;
    let result = scan(vec![address]).and_then(|services| {
        let count = services
            .iter()
            .filter(|service| {
                service
                    .description
                    .as_ref()
                    .is_ok_and(|service| service.instance == started.instance)
            })
            .count();
        ensure!(
            count == 1,
            "FX discovery did not reconcile its local and explicit addresses"
        );
        let target = super::start_host::Dialog::probe_remote(address)?;
        let result = describe(address)
            .map_err(anyhow::Error::from)
            .and_then(|status| {
                ensure!(
                    status.reserved == 2,
                    "Native host did not reserve the remote Carla banks"
                );
                Ok(())
            });
        let shutdown = shoop_host::connection::shutdown_ownerless(&target, Duration::from_secs(10));
        result.and(shutdown)
    });
    let stopped = stop_service(address, &identity, started.instance);
    result.and(stopped)
}
