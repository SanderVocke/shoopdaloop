use crate::app_args::AppArgs;
use anyhow::{Context, Result};
use eframe::egui;
use shoop_app_protocol::HostReport;
use shoop_host::{
    connection, logging,
    registry::{AppTransport, HostEntry, Registry},
    remote::{RemoteProfile, Target},
};
use std::{
    path::PathBuf,
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct Source {
    path: PathBuf,
    remote: bool,
}
impl Source {
    fn target(&self) -> Result<Target> {
        if self.remote {
            Ok(RemoteProfile::read(&self.path)?.into())
        } else {
            Ok(HostEntry::read(&self.path)?.into())
        }
    }
    fn ui_command(&self) -> Result<Command> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--ui-only")
            .arg(if self.remote {
                "--remote-profile"
            } else {
                "--host-file"
            })
            .arg(&self.path);
        Ok(command)
    }
    fn log_path(&self) -> Option<PathBuf> {
        if self.remote {
            return None;
        }
        let process = self.path.file_stem()?.to_str()?.parse().ok()?;
        Some(logging::path(self.path.parent()?, process))
    }
}

struct Host {
    source: Source,
    name: String,
    endpoint: String,
    transport: AppTransport,
    report: Result<HostReport, String>,
    checked: Instant,
}
impl Host {
    fn query(source: Source) -> Self {
        let target = source.target();
        let (name, endpoint, transport) = match &target {
            Ok(target) => (
                target.name.clone(),
                target.endpoint.clone(),
                target.transport,
            ),
            Err(_) => (
                source
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                String::new(),
                AppTransport::Socket,
            ),
        };
        let report = target
            .and_then(|target| connection::host_report(&target, Duration::from_secs(2)))
            .map_err(|error| format!("{error:#}"));
        Self {
            source,
            name,
            endpoint,
            transport,
            report,
            checked: Instant::now(),
        }
    }
    fn state(&self) -> (&'static str, egui::Color32) {
        match &self.report {
            Err(_) => ("Unreachable", egui::Color32::from_rgb(235, 180, 85)),
            Ok(report) if report.closed => ("Closed", egui::Color32::GRAY),
            Ok(report) if !report.ready => ("Not ready", egui::Color32::from_rgb(235, 180, 85)),
            Ok(report)
                if !report.diagnostics.is_empty()
                    || report.effective.audio_driver
                        != shoop_app_api::AudioDriverState::Running =>
            {
                ("Warning", egui::Color32::from_rgb(235, 180, 85))
            }
            Ok(_) => ("Running", egui::Color32::from_rgb(100, 210, 165)),
        }
    }
}

enum Event {
    Scanned(Vec<Host>, Option<String>),
    Action(Result<String, String>),
    Logs(PathBuf, Result<String, String>),
}
struct Manager {
    registry: PathBuf,
    sources: Vec<Source>,
    hosts: Vec<Host>,
    selected: Option<Source>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    scanning: bool,
    action_pending: bool,
    next_scan: Instant,
    notice: Option<String>,
    discovery_error: Option<String>,
    confirm_stop: Option<Source>,
    logs: Option<(PathBuf, String)>,
}
impl Manager {
    fn new(registry: PathBuf, remote: Option<PathBuf>) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            registry,
            sources: remote
                .into_iter()
                .map(|path| Source { path, remote: true })
                .collect(),
            hosts: Vec::new(),
            selected: None,
            sender,
            receiver,
            scanning: false,
            action_pending: false,
            next_scan: Instant::now(),
            notice: None,
            discovery_error: None,
            confirm_stop: None,
            logs: None,
        }
    }
    fn scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        let mut sources = self.sources.clone();
        let registry = self.registry.clone();
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let discovered = (|| -> Result<()> {
                if !registry.exists() {
                    return Ok(());
                }
                Registry::open(&registry)?;
                // Inspect entries separately so a damaged registration cannot hide healthy hosts.
                for item in std::fs::read_dir(&registry)? {
                    let path = item?.path();
                    if path
                        .extension()
                        .is_some_and(|extension| extension == "json")
                    {
                        let source = Source {
                            path,
                            remote: false,
                        };
                        if !sources.contains(&source) && sources.len() < 64 {
                            sources.push(source);
                        }
                    }
                }
                Ok(())
            })();
            sources.sort_by(|a, b| a.path.cmp(&b.path));
            let hosts = sources.into_iter().map(Host::query).collect();
            let _ = sender.send(Event::Scanned(
                hosts,
                discovered.err().map(|error| format!("{error:#}")),
            ));
        });
    }
    fn action(&mut self, operation: impl FnOnce() -> Result<String> + Send + 'static) {
        self.action_pending = true;
        self.notice = None;
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let _ = sender.send(Event::Action(
                operation().map_err(|error| format!("{error:#}")),
            ));
        });
    }
    fn read_logs(&mut self, path: PathBuf) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = logging::tail(&path).map_err(|error| format!("{error:#}"));
            let _ = sender.send(Event::Logs(path, result));
        });
    }
    fn poll(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Scanned(mut hosts, error) => {
                    for host in &mut hosts {
                        if host.endpoint.is_empty() {
                            if let Some(previous) = self
                                .hosts
                                .iter()
                                .find(|previous| previous.source == host.source)
                            {
                                host.name = previous.name.clone();
                                host.endpoint = previous.endpoint.clone();
                                host.transport = previous.transport;
                            }
                        }
                    }
                    for host in &hosts {
                        if !self.sources.contains(&host.source) {
                            self.sources.push(host.source.clone());
                        }
                    }
                    if self.selected.is_none() {
                        self.selected = hosts.first().map(|host| host.source.clone());
                    }
                    self.hosts = hosts;
                    self.discovery_error = error;
                    self.scanning = false;
                    self.next_scan = Instant::now() + Duration::from_secs(2);
                }
                Event::Action(result) => {
                    self.action_pending = false;
                    self.notice =
                        Some(result.unwrap_or_else(|error| format!("Action failed: {error}")));
                    self.next_scan = Instant::now();
                }
                Event::Logs(path, result) => {
                    self.logs = Some((
                        path,
                        result.unwrap_or_else(|error| format!("Could not read logs: {error}")),
                    ))
                }
            }
        }
    }
    fn show(&mut self, ui: &mut egui::Ui) {
        self.poll();
        if Instant::now() >= self.next_scan {
            self.scan();
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.heading("Host Manager");
            if self.scanning {
                ui.spinner();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!self.scanning, egui::Button::new("Refresh"))
                    .clicked()
                {
                    self.scan();
                }
                if ui.button("Open host log…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_directory(self.registry.join("logs"))
                        .pick_file()
                    {
                        self.read_logs(path);
                    }
                }
                if ui.button("Add remote profile…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Host profile", &["json"])
                        .pick_file()
                    {
                        let source = Source { path, remote: true };
                        if !self.sources.contains(&source) && self.sources.len() >= 64 {
                            self.notice = Some("This manager can monitor up to 64 hosts. Open another manager for additional profiles.".into());
                        } else {
                            if !self.sources.contains(&source) { self.sources.push(source.clone()); }
                            self.selected = Some(source);
                            self.next_scan = Instant::now();
                        }
                    }
                }
            });
        });
        ui.label(
            egui::RichText::new(
                "Monitor independent audio hosts. Closing this window keeps hosts running.",
            )
            .weak(),
        );
        ui.add_space(12.0);
        if let Some(error) = &self.discovery_error {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("Host discovery failed: {error}"),
            );
        }
        if let Some(notice) = &self.notice {
            ui.label(notice);
            ui.add_space(8.0);
        }
        if self.hosts.is_empty() {
            ui.group(|ui| {
                ui.heading("No hosts found");
                ui.label("Start an independent host, or add a remote connection profile.");
                ui.monospace("shoop_host --config host.json");
                ui.label("Local hosts appear automatically. An empty host.json ({}) uses the dummy driver.");
            });
        } else {
            egui::ScrollArea::vertical()
                .id_salt("hosts")
                .max_height(210.0)
                .show(ui, |ui| {
                    egui::Grid::new("hosts")
                        .num_columns(5)
                        .spacing([24.0, 12.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for title in ["HOST", "STATUS", "AUDIO", "CONTROLLER", "LOCATION"] {
                                ui.strong(title);
                            }
                            ui.end_row();
                            for host in &self.hosts {
                                if ui
                                    .selectable_label(
                                        self.selected.as_ref() == Some(&host.source),
                                        &host.name,
                                    )
                                    .clicked()
                                {
                                    self.selected = Some(host.source.clone());
                                }
                                let (state, color) = host.state();
                                ui.colored_label(color, state);
                                match &host.report {
                                    Ok(report) => {
                                        ui.label(format!(
                                            "{} Hz / {} frames",
                                            report.effective.sample_rate,
                                            report.effective.buffer_size
                                        ));
                                        ui.label(if report.busy {
                                            "Connected"
                                        } else {
                                            "Disconnected"
                                        });
                                    }
                                    Err(_) => {
                                        ui.label("Unavailable");
                                        ui.label("Unknown");
                                    }
                                }
                                ui.label(if host.source.remote {
                                    "Remote profile"
                                } else {
                                    "This computer"
                                });
                                ui.end_row();
                            }
                        });
                });
            ui.add_space(12.0);
            ui.separator();
            if let Some(host) = self
                .hosts
                .iter()
                .find(|host| Some(&host.source) == self.selected.as_ref())
            {
                let source = host.source.clone();
                let can_open = host.transport == AppTransport::Socket
                    && host
                        .report
                        .as_ref()
                        .is_ok_and(|report| report.ready && !report.busy && !report.closed);
                let can_stop = host
                    .report
                    .as_ref()
                    .is_ok_and(|report| !report.busy && !report.closed);
                let diagnostics = diagnostic_text(host);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.heading(&host.name);
                    ui.label(
                        egui::RichText::new(format!(
                            "Queried {}s ago",
                            host.checked.elapsed().as_secs()
                        ))
                        .weak(),
                    );
                });
                ui.label(format!("Endpoint: {}", host.endpoint));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            can_open && !self.action_pending,
                            egui::Button::new("Open UI"),
                        )
                        .clicked()
                    {
                        let source = source.clone();
                        self.action(move || {
                            let mut command = source.ui_command()?;
                            shoop_host::launch::independent(&mut command);
                            command.spawn().context("Start looper UI")?;
                            Ok("Looper UI launched.".into())
                        });
                    }
                    if ui
                        .add_enabled(
                            can_stop && !self.action_pending,
                            egui::Button::new("Stop host…"),
                        )
                        .clicked()
                    {
                        self.confirm_stop = Some(source.clone());
                    }
                    if ui.button("Copy diagnostics").clicked() {
                        ui.ctx().copy_text(diagnostics.clone());
                    }
                    if ui
                        .add_enabled(source.log_path().is_some(), egui::Button::new("View logs"))
                        .clicked()
                    {
                        self.read_logs(source.log_path().unwrap());
                    }
                });
                if let Some(host) = self.hosts.iter().find(|host| host.source == source) {
                    if host.report.as_ref().is_ok_and(|report| report.busy) {
                        ui.label("A UI controls this host. Close it or use its “Close UI and host” action to stop.");
                    }
                    if host.transport == AppTransport::SharedMemory {
                        ui.colored_label(egui::Color32::YELLOW, "Shared-memory delivery cannot reattach a UI. Use socket delivery for recovery.");
                    }
                    egui::ScrollArea::vertical().id_salt("details").show(ui, |ui| {
                        match &host.report {
                            Ok(report) => show_report(ui, report),
                            Err(error) => { ui.colored_label(egui::Color32::YELLOW, error); ui.label("The endpoint could not be queried. It may be stopped, unreachable, or incompatible."); }
                        }
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(format!("{}: {}", if source.remote { "Profile" } else { "Registration" }, source.path.display())).small().weak());
                        if source.remote { ui.label("Logs are stored on the host computer."); }
                    });
                }
            }
        }
        if let Some(source) = self.confirm_stop.clone() {
            egui::Window::new("Stop host?")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    if let Some(host) = self.hosts.iter().find(|host| host.source == source) {
                        ui.strong(&host.name);
                    }
                    ui.label(
                        "Stopping ends playback and recording and discards the in-memory session.",
                    );
                    ui.label("Save the session in the looper UI first if you need to keep it.");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.confirm_stop = None;
                        }
                        if ui.button("Stop host").clicked() {
                            self.confirm_stop = None;
                            self.action(move || {
                                connection::shutdown_ownerless(
                                    &source.target()?,
                                    Duration::from_secs(15),
                                )?;
                                Ok("Host cleanup confirmed. Playback and devices have stopped."
                                    .into())
                            });
                        }
                    });
                });
        }
        if let Some((path, contents)) = self.logs.clone() {
            let mut open = true;
            egui::Window::new("Host logs")
                .open(&mut open)
                .default_size([760.0, 400.0])
                .show(ui.ctx(), |ui| {
                    ui.label(path.display().to_string());
                    ui.label("Latest 64 KiB. Logs remain available after host shutdown.");
                    ui.horizontal(|ui| {
                        if ui.button("Refresh logs").clicked() {
                            self.read_logs(path.clone());
                        }
                        if ui.button("Copy logs").clicked() {
                            ui.ctx().copy_text(contents.clone());
                        }
                    });
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.monospace(contents);
                    });
                });
            if !open {
                self.logs = None;
            }
        }
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }
}
fn show_report(ui: &mut egui::Ui, report: &HostReport) {
    ui.add_space(10.0);
    ui.strong("Audio health");
    ui.label(format!("Driver: {:?}", report.effective.audio_driver));
    ui.label(format!(
        "DSP load: {:.1}%    Xruns: {}    Host operations: {}",
        report.effective.dsp_load_percent, report.effective.xruns, report.pending_operations
    ));
    if report.effective.audio_driver == shoop_app_api::AudioDriverState::Dummy {
        ui.colored_label(
            egui::Color32::from_rgb(235, 180, 85),
            "Dummy driver: no physical audio input or output.",
        );
    }
    for diagnostic in &report.diagnostics {
        ui.colored_label(egui::Color32::YELLOW, diagnostic);
    }
    if let Some(settings) = &report.requested {
        ui.add_space(10.0);
        ui.strong("Startup configuration");
        ui.label(format!(
            "Requested driver: {}    Capacity: {} tracks / {} loops / {} channels / {} MiB",
            settings.driver.kind().id(),
            settings.pools.tracks,
            settings.pools.loops,
            settings.pools.channels,
            settings.pools.loop_memory_mb
        ));
        ui.label(format!(
            "Current loop smoothing: {} ms    Master auto-connect: {}",
            report.loop_smoothing_ms, report.master_auto_connect
        ));
        ui.label(egui::RichText::new("Driver and capacity changes require a host restart. Restarting creates a fresh session.").weak());
    }
}
fn diagnostic_text(host: &Host) -> String {
    let state = match &host.report {
        Ok(report) => serde_json::to_string_pretty(report).unwrap_or_default(),
        Err(error) => error.clone(),
    };
    format!(
        "Host: {}\nEndpoint: {}\nSource: {}\n{state}",
        host.name,
        host.endpoint,
        host.source.path.display()
    )
}
impl eframe::App for Manager {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(20))
            .show(ui, |ui| self.show(ui));
    }
}
pub fn launch() -> Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--manage-hosts");
    shoop_host::launch::independent(&mut command);
    command.spawn().context("Open Host Manager")?;
    Ok(())
}
pub fn run(args: &AppArgs) -> Result<()> {
    let registry = args
        .host_registry
        .clone()
        .map(Ok)
        .unwrap_or_else(Registry::default_path)?;
    let manager = Manager::new(registry, args.remote_profile.clone());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ShoopDaLoop Host Manager")
            .with_icon(crate::application_icon())
            .with_inner_size([960.0, 660.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ShoopDaLoop Host Manager",
        options,
        Box::new(move |context| {
            context.egui_ctx.set_visuals(egui::Visuals::dark());
            context.egui_ctx.style_mut_of(egui::Theme::Dark, |style| {
                style.spacing.item_spacing = egui::vec2(10.0, 8.0);
                style.spacing.button_padding = egui::vec2(10.0, 5.0);
                style
                    .text_styles
                    .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
                style
                    .text_styles
                    .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
                style
                    .text_styles
                    .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
            });
            Ok(Box::new(manager))
        }),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn unreachable_profiles_and_dummy_audio_have_explicit_states() {
        let source = Source {
            path: PathBuf::from("missing-profile.json"),
            remote: true,
        };
        let mut host = Host::query(source.clone());
        assert_eq!(host.state().0, "Unreachable");
        assert!(source.log_path().is_none());
        host.report = Ok(HostReport {
            ready: true,
            busy: true,
            closed: false,
            master_auto_connect: true,
            loop_smoothing_ms: 3,
            requested: None,
            effective: Default::default(),
            diagnostics: vec![],
            pending_operations: 0,
            restart_required_fields: vec![],
        });
        assert_eq!(host.state().0, "Warning");
        let report = host.report.as_mut().unwrap();
        report.effective.audio_driver = shoop_app_api::AudioDriverState::Running;
        assert_eq!(host.state().0, "Running");
        host.report.as_mut().unwrap().effective.audio_driver =
            shoop_app_api::AudioDriverState::Failed;
        assert_eq!(host.state().0, "Warning");
        let report = host.report.as_mut().unwrap();
        report.diagnostics.push("Audio fallback".into());
        assert_eq!(host.state().0, "Warning");
        assert!(diagnostic_text(&host).contains("Audio fallback"));
        assert!(!diagnostic_text(&host).contains("credential"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn manager_cli_is_exclusive_and_supports_remote_profiles() {
        use clap::Parser;
        assert!(
            AppArgs::try_parse_from([
                "shoopdaloop",
                "--manage-hosts",
                "--remote-profile",
                "remote.json"
            ])
            .unwrap()
            .manage_hosts
        );
        for option in [
            "--host-only",
            "--ui-only",
            "--shutdown-host",
            "--list-hosts",
            "--tracing",
        ] {
            assert!(AppArgs::try_parse_from(["shoopdaloop", "--manage-hosts", option]).is_err());
        }
    }
}
