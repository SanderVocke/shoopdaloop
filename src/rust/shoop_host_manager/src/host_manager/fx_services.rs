use super::{discovery, Host, Source, AMBER, MUTED};
use anyhow::{ensure, Context, Result};
use eframe::egui;
use shoop_host::launch;
use shoop_host_processes::Identity;
use shoop_remote_fx::{describe, ChainDescription, Description, Lifecycle};
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read},
    net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

pub(super) struct Settings {
    bind: String,
    lan: bool,
    isolated: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:0".into(),
            lan: false,
            isolated: false,
        }
    }
}

impl Settings {
    pub(super) fn fields(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("new_fx_host")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Listen address");
                ui.text_edit_singleline(&mut self.bind);
                ui.end_row();
                ui.label("Network");
                if ui.checkbox(&mut self.lan, "LAN access").changed()
                    && matches!(self.bind.as_str(), "127.0.0.1:0" | "0.0.0.0:0")
                {
                    self.bind = if self.lan { "0.0.0.0:0" } else { "127.0.0.1:0" }.into();
                }
                ui.end_row();
                ui.label("Carla isolation");
                ui.checkbox(&mut self.isolated, "One process per chain");
                ui.end_row();
            });
    }

    pub(super) fn request(&self) -> Result<Request> {
        let address: SocketAddr = self
            .bind
            .trim()
            .parse()
            .context("Invalid FX host listen address")?;
        ensure!(
            self.lan || address.ip().is_loopback(),
            "Enable LAN access for a non-loopback FX host"
        );
        Ok(Request {
            address,
            lan: self.lan,
            isolated: self.isolated,
        })
    }
}

pub(super) struct Request {
    address: SocketAddr,
    lan: bool,
    isolated: bool,
}

impl Request {
    pub(super) fn start(self) -> Result<Host> {
        let (address, description) = launch_service(self.address, self.lan, self.isolated)?;
        let identity = shoop_host_processes::identity(description.pid)?
            .context("FX host process identity unavailable")?;
        Ok(Host::from_fx(
            Source::Process(identity),
            address,
            description,
        ))
    }
}

fn chain_status(chain: &ChainDescription) -> (&'static str, egui::Color32) {
    if !chain.attached {
        return ("Waiting for owner", MUTED);
    }
    match chain.status.lifecycle {
        Lifecycle::Starting => ("Starting", MUTED),
        Lifecycle::Restarting => ("Restarting", AMBER),
        Lifecycle::Degraded => ("Degraded", AMBER),
        Lifecycle::Crashed => ("Crashed", AMBER),
        Lifecycle::Unavailable => ("Unavailable", AMBER),
        Lifecycle::Running if chain.status.active => ("Active", super::ACCENT),
        Lifecycle::Running | Lifecycle::Stopped => ("Idle", MUTED),
    }
}

pub(super) fn has_chain_failure(description: &Description) -> bool {
    description.chains.iter().any(|chain| {
        chain.attached
            && matches!(
                chain.status.lifecycle,
                Lifecycle::Degraded | Lifecycle::Crashed | Lifecycle::Unavailable
            )
    })
}

pub(super) fn show_overview(
    ui: &mut egui::Ui,
    description: &Description,
    can_edit: bool,
) -> Option<(String, bool)> {
    let active = description
        .chains
        .iter()
        .filter(|chain| chain.attached && chain.status.active)
        .count();
    ui.columns(2, |columns| {
        super::metric(
            &mut columns[0],
            "CHAINS",
            description.chains.len().to_string(),
        );
        super::metric(&mut columns[1], "ACTIVE", active.to_string());
    });
    if description.chains.is_empty() {
        ui.add_space(6.0);
        ui.label(egui::RichText::new("No chains assigned").color(MUTED));
        return None;
    }
    ui.add_space(6.0);
    let mut editor = None;
    egui::Grid::new("fx_chains").striped(true).show(ui, |ui| {
        for chain in &description.chains {
            let id = chain.id.split('-').next().unwrap_or(&chain.id);
            ui.label(format!("{} · {id}", chain.kind.label()))
                .on_hover_text(&chain.id);
            ui.label(format!(
                "{} in / {} out",
                chain.config.input.channels, chain.config.output.channels
            ));
            let (label, color) = chain_status(chain);
            ui.colored_label(color, label);
            if let Some(visible) = chain.status.editor_visible {
                let label = if visible {
                    "Close editor"
                } else {
                    "Open editor"
                };
                if ui
                    .add_enabled(can_edit, egui::Button::new(label))
                    .on_disabled_hover_text(
                        "Editor controls are available on the computer running this FX host.",
                    )
                    .clicked()
                {
                    editor = Some((chain.id.clone(), !visible));
                }
            }
            ui.end_row();
        }
    });
    editor
}

pub fn discover_fx_services() -> Vec<SocketAddr> {
    let lan = shoop_remote_fx::discover_lan().unwrap_or_default();
    let local = shoop_host_processes::snapshot()
        .ok()
        .into_iter()
        .flat_map(|snapshot| snapshot.processes)
        .flat_map(|process| {
            process.addresses.into_iter().map(move |address| {
                (
                    discovery::local_address(address),
                    Some(process.identity.pid),
                )
            })
        });
    let mut instances = HashSet::new();
    lan.into_iter()
        .map(|address| (address, None))
        .chain(local)
        .filter_map(|(address, pid)| {
            let description = describe(address).ok()?;
            let matches = pid.is_none_or(|pid| pid == description.pid);
            (matches && instances.insert(description.instance)).then_some(address)
        })
        .collect()
}

fn route_to_session(endpoint: &str) -> Result<SocketAddr> {
    for peer in endpoint
        .to_socket_addrs()
        .with_context(|| format!("Resolve session host address {endpoint}"))?
    {
        let bind = if peer.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let Ok(socket) = UdpSocket::bind(bind) else {
            continue;
        };
        if socket.connect(peer).is_err() {
            continue;
        }
        if let Ok(mut local) = socket.local_addr() {
            let ip = local.ip();
            if ip.is_unspecified()
                || ip.is_loopback()
                || matches!(ip, IpAddr::V6(ip) if ip.is_unicast_link_local())
            {
                continue;
            }
            local.set_port(0);
            return Ok(local);
        }
    }
    anyhow::bail!("Could not choose a desktop network address for {endpoint}. Select an FX host or enter its address.")
}

/// Finds a desktop FX host on the route to the session or starts one on that interface.
pub fn prepare_desktop_fx_host(session_endpoint: &str) -> Result<SocketAddr> {
    let interface = route_to_session(session_endpoint)?;
    if let Ok(snapshot) = shoop_host_processes::snapshot() {
        for process in snapshot.processes {
            for listener in process.addresses {
                let mut address = interface;
                address.set_port(listener.port());
                if (listener == address
                    || (listener.is_ipv4() == address.is_ipv4() && listener.ip().is_unspecified()))
                    && describe(address).is_ok_and(|fx| fx.pid == process.identity.pid)
                {
                    return Ok(address);
                }
            }
        }
    }
    let (address, _) = launch_service(interface, true, false)
        .with_context(|| format!("Start FX host on {}", interface.ip()))?;
    Ok(address)
}

fn launch_service(
    address: SocketAddr,
    lan: bool,
    isolated: bool,
) -> Result<(SocketAddr, Description)> {
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
    if isolated {
        command.arg("--isolated");
    }
    launch::independent(&mut command);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().context("Start desktop FX service")?;
    let stdout = child
        .stdout
        .take()
        .context("FX service readiness pipe unavailable")?;
    let mut stderr = child
        .stderr
        .take()
        .context("FX service diagnostic pipe unavailable")?;
    let (diagnostic, failure) = mpsc::channel();
    thread::spawn(move || {
        let mut tail = Vec::new();
        let mut buffer = [0; 4096];
        while let Ok(count) = stderr.read(&mut buffer) {
            if count == 0 {
                break;
            }
            tail.extend_from_slice(&buffer[..count]);
            let excess = tail.len().saturating_sub(buffer.len());
            tail.drain(..excess);
        }
        let _ = diagnostic.send(tail);
    });
    let (sender, ready) = mpsc::channel();
    thread::spawn(move || {
        let result = BufReader::new(stdout.take(4096))
            .lines()
            .map_while(std::result::Result::ok)
            .find_map(|line| line.strip_prefix("FX service listening on ")?.parse().ok());
        let _ = sender.send(result);
    });
    let mut result = ready
        .recv_timeout(Duration::from_secs(5))
        .ok()
        .flatten()
        .context("FX service did not report readiness")
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
        if let Ok(stderr) = failure.recv_timeout(Duration::from_millis(250)) {
            let stderr = String::from_utf8_lossy(&stderr);
            if !stderr.trim().is_empty() {
                result = result.context(stderr.trim().to_owned());
            }
        }
    }
    launch::reap(child);
    result
}

fn verify_identity(identity: &Identity) -> Result<()> {
    ensure!(
        shoop_host_processes::identity(identity.pid)?.as_ref() == Some(identity),
        "FX host process identity changed"
    );
    Ok(())
}

pub(super) fn stop_service(address: SocketAddr, identity: &Identity, instance: &str) -> Result<()> {
    verify_identity(identity)?;
    shoop_remote_fx::shutdown(address, instance.to_owned())?;
    Ok(())
}

pub(super) fn set_editor(
    address: SocketAddr,
    identity: &Identity,
    instance: &str,
    chain: String,
    visible: bool,
) -> Result<()> {
    verify_identity(identity)?;
    shoop_remote_fx::set_editor(address, instance.to_owned(), chain, visible)?;
    Ok(())
}

pub(super) fn probe() -> Result<()> {
    if let Ok(address) = std::env::var("SHOOP_FX_PROBE_DISCOVER_ADDRESS") {
        let address = address.parse()?;
        let expected = describe(address)?;
        ensure!(
            shoop_remote_fx::discover_lan()?.contains(&address),
            "FX host at {address} did not answer LAN discovery"
        );
        ensure!(
            discovery::scan(vec![], vec![]).hosts.iter().any(|host| {
                matches!(&host.details, super::HostDetails::Fx { report: Ok(report), .. }
                    if report.instance == expected.instance)
            }),
            "Network FX host at {address} is missing from the Manager list"
        );
        ensure!(
            discover_fx_services().contains(&address),
            "The Add Track picker could not discover the network FX host"
        );
        return Ok(());
    }
    const LAUNCH_ONLY: &str = "SHOOP_FX_PROBE_LAUNCH_ONLY";
    if std::env::var_os(LAUNCH_ONLY).is_some() {
        if let Ok(endpoint) = std::env::var("SHOOP_FX_PROBE_SESSION_ENDPOINT") {
            println!("{}", prepare_desktop_fx_host(&endpoint)?);
            return Ok(());
        }
        let host = Settings::default().request()?.start()?;
        println!("{}", host.endpoint);
        return Ok(());
    }
    let launcher = Command::new(std::env::current_exe()?)
        .arg("--probe-fx-service")
        .env(LAUNCH_ONLY, "1")
        .output()?;
    ensure!(
        launcher.status.success(),
        "FX service launcher failed: {}",
        String::from_utf8_lossy(&launcher.stderr)
    );
    let address = std::str::from_utf8(&launcher.stdout)?.trim().parse()?;
    let started = describe(address).context("FX service did not survive its manager")?;
    let identity =
        shoop_host_processes::identity(started.pid)?.context("FX service locally unavailable")?;
    let result = (|| {
        ensure!(
            discover_fx_services().contains(&address),
            "The Add Track picker could not discover the local FX service"
        );
        let scan = discovery::scan(vec![address.to_string()], vec![]);
        let mut matches = scan.hosts.iter().filter(|host| {
            matches!(&host.details, super::HostDetails::Fx { report: Ok(report), .. }
                if report.instance == started.instance)
        });
        let host = matches
            .next()
            .context("Local FX host is missing from the Manager list")?;
        let local = Source::Process(identity.clone());
        ensure!(
            host.source == local
                && matches.next().is_none()
                && scan
                    .aliases
                    .contains(&(Source::Address(address.to_string()), local)),
            "Manager discovery did not reconcile the local FX host with its explicit address"
        );
        let target = super::start_host::Dialog::probe_remote(address)?;
        let result = describe(address)
            .map_err(anyhow::Error::from)
            .and_then(|status| {
                ensure!(
                    status.chains.is_empty(),
                    "Native host opened a remote Carla chain before assignment"
                );
                Ok(())
            });
        let shutdown = shoop_host::connection::shutdown_ownerless(&target, Duration::from_secs(10));
        result.and(shutdown)
    })();
    let stopped = Host::from_fx(Source::Process(identity), address, started).stop();
    result.and(stopped.map(|_| ()))
}
