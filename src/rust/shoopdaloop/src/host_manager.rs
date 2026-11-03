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

#[derive(Clone)]
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
            Err(_) => ("Unreachable", MUTED),
            Ok(report) if report.closed => ("Closed", MUTED),
            Ok(report) if !report.ready => ("Not ready", AMBER),
            Ok(report) if audio_fallback(report) => ("Audio fallback", AMBER),
            Ok(report)
                if !report.diagnostics.is_empty()
                    || !matches!(
                        report.effective.audio_driver,
                        shoop_app_api::AudioDriverState::Running
                            | shoop_app_api::AudioDriverState::Dummy
                    ) =>
            {
                ("Needs attention", AMBER)
            }
            Ok(report)
                if report.effective.audio_driver == shoop_app_api::AudioDriverState::Dummy =>
            {
                ("Dummy audio", BLUE)
            }
            Ok(_) => ("Running", ACCENT),
        }
    }
}

const BACKGROUND: egui::Color32 = egui::Color32::from_rgb(20, 23, 29);
const SIDEBAR: egui::Color32 = egui::Color32::from_rgb(16, 19, 24);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(28, 33, 41);
const BORDER: egui::Color32 = egui::Color32::from_rgb(46, 54, 65);
const TEXT: egui::Color32 = egui::Color32::from_rgb(230, 235, 242);
const MUTED: egui::Color32 = egui::Color32::from_rgb(155, 166, 181);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(81, 211, 180);
const BLUE: egui::Color32 = egui::Color32::from_rgb(135, 181, 230);
const AMBER: egui::Color32 = egui::Color32::from_rgb(240, 189, 110);

fn audio_fallback(report: &HostReport) -> bool {
    report.effective.audio_driver == shoop_app_api::AudioDriverState::Dummy
        && report
            .requested
            .as_ref()
            .is_some_and(|settings| settings.driver.kind() != shoop_app_api::AudioDriverKind::Dummy)
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum Tab {
    #[default]
    Overview,
    Logs,
}
struct Notice {
    message: String,
    error: bool,
    created: Instant,
}
struct LogView {
    path: PathBuf,
    contents: Option<Result<String, String>>,
    loading: bool,
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
    notice: Option<Notice>,
    discovery_error: Option<String>,
    confirm_stop: Option<Source>,
    logs: Option<LogView>,
    tab: Tab,
    saved_log: bool,
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
            tab: Tab::Overview,
            saved_log: false,
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
        self.tab = Tab::Logs;
        if self
            .logs
            .as_ref()
            .is_some_and(|log| log.path == path && log.loading)
        {
            return;
        }
        let contents = self
            .logs
            .take()
            .filter(|log| log.path == path)
            .and_then(|log| log.contents);
        self.logs = Some(LogView {
            path: path.clone(),
            contents,
            loading: true,
        });
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = logging::tail(&path).map_err(|error| format!("{error:#}"));
            let _ = sender.send(Event::Logs(path, result));
        });
    }
    fn notify(&mut self, message: String, error: bool) {
        self.notice = Some(Notice {
            message,
            error,
            created: Instant::now(),
        });
    }
    fn select(&mut self, source: Source) {
        if self.selected.as_ref() != Some(&source) || self.saved_log {
            self.selected = Some(source);
            self.saved_log = false;
            self.tab = Tab::Overview;
            self.logs = None;
        }
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
                    if self.selected.is_none() && !self.saved_log {
                        self.selected = hosts.first().map(|host| host.source.clone());
                    }
                    self.hosts = hosts;
                    self.discovery_error = error;
                    self.scanning = false;
                    self.next_scan = Instant::now() + Duration::from_secs(2);
                }
                Event::Action(result) => {
                    self.action_pending = false;
                    match result {
                        Ok(message) => self.notify(message, false),
                        Err(error) => self.notify(format!("Action failed: {error}"), true),
                    }
                    self.next_scan = Instant::now();
                }
                Event::Logs(path, result) => {
                    if let Some(log) = self.logs.as_mut().filter(|log| log.path == path) {
                        log.contents = Some(result);
                        log.loading = false;
                    }
                }
            }
        }
    }
    fn add_host(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Host profile", &["json"])
            .pick_file()
        {
            let source = Source { path, remote: true };
            if !self.sources.contains(&source) && self.sources.len() >= 64 {
                self.notify("This manager can monitor up to 64 hosts.".into(), true);
            } else {
                if !self.sources.contains(&source) {
                    self.sources.push(source.clone());
                }
                self.select(source);
                self.next_scan = Instant::now();
            }
        }
    }
    fn open_saved_log(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(self.registry.join("logs"))
            .pick_file()
        {
            self.saved_log = true;
            self.selected = None;
            self.read_logs(path);
        }
    }
    fn show(&mut self, ui: &mut egui::Ui) {
        self.poll();
        if Instant::now() >= self.next_scan {
            self.scan();
        }
        egui::Panel::top("manager_header")
            .frame(
                egui::Frame::new()
                    .fill(SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Host Manager").size(20.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new("+ Add host").corner_radius(8))
                            .on_hover_text("Choose a remote connection profile")
                            .clicked()
                        {
                            self.add_host();
                        }
                    });
                });
            });
        egui::Panel::bottom("manager_footer")
            .frame(
                egui::Frame::new()
                    .fill(SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(24, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Hosts keep running when this window closes.")
                            .size(12.0)
                            .color(MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(!self.scanning, egui::Button::new("Refresh").frame(false))
                            .on_hover_text("Status refreshes automatically after each scan")
                            .clicked()
                        {
                            self.scan();
                        }
                        if self.scanning {
                            ui.spinner();
                        } else if let Some(host) = self
                            .hosts
                            .iter()
                            .find(|host| Some(&host.source) == self.selected.as_ref())
                        {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Updated {}s ago",
                                    host.checked.elapsed().as_secs()
                                ))
                                .size(12.0)
                                .color(MUTED),
                            );
                        }
                    });
                });
            });
        egui::Panel::left("host_sidebar")
            .resizable(false)
            .exact_size(232.0)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(14))
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("HOSTS")
                            .size(11.0)
                            .strong()
                            .color(MUTED),
                    );
                    ui.label(
                        egui::RichText::new(self.hosts.len().to_string())
                            .size(11.0)
                            .color(MUTED),
                    );
                });
                ui.add_space(8.0);
                let mut selected = None;
                egui::ScrollArea::vertical()
                    .id_salt("host_list")
                    .max_height((ui.available_height() - 84.0).max(80.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.hosts.is_empty() {
                            ui.label(egui::RichText::new("No hosts yet").color(MUTED));
                        }
                        for host in &self.hosts {
                            if host_row(ui, host, self.selected.as_ref() == Some(&host.source))
                                .clicked()
                            {
                                selected = Some(host.source.clone());
                            }
                        }
                    });
                if let Some(source) = selected {
                    self.select(source);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if ui
                        .add(egui::Button::new("Open saved log…").frame(false))
                        .on_hover_text("Browse logs from exited hosts and failed startups")
                        .clicked()
                    {
                        self.open_saved_log();
                    }
                    if let Some(error) = &self.discovery_error {
                        ui.colored_label(AMBER, "Discovery needs attention")
                            .on_hover_text(error);
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(28))
            .show(ui, |ui| {
                self.show_content(ui);
            });
        self.show_stop_dialog(ui.ctx());
        self.show_notice(ui.ctx());
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }
    fn show_content(&mut self, ui: &mut egui::Ui) {
        if self.saved_log {
            ui.heading("Saved host log");
            ui.add_space(12.0);
            self.show_logs(ui);
            return;
        }
        let Some(host) = self
            .hosts
            .iter()
            .find(|host| Some(&host.source) == self.selected.as_ref())
            .cloned()
        else {
            ui.add_space((ui.available_height() * 0.25).max(20.0));
            ui.vertical_centered(|ui| {
                ui.heading("Your hosts, in one place");
                ui.label(
                    egui::RichText::new(
                        "Local hosts appear automatically. Add a profile to connect remotely.",
                    )
                    .color(MUTED),
                );
                ui.add_space(12.0);
                if ui.add(primary_button("+ Add host")).clicked() {
                    self.add_host();
                }
                ui.add_space(20.0);
                ui.collapsing("Starting a local host", |ui| {
                    ui.monospace("shoop_host --config host.json");
                    ui.label("An empty host.json ({}) selects the dummy driver.");
                });
            });
            return;
        };
        let can_open = host.transport == AppTransport::Socket
            && host
                .report
                .as_ref()
                .is_ok_and(|report| report.ready && !report.busy && !report.closed);
        let can_stop = host
            .report
            .as_ref()
            .is_ok_and(|report| !report.busy && !report.closed);
        let busy = host
            .report
            .as_ref()
            .is_ok_and(|report| report.busy && !report.closed);
        ui.horizontal(|ui| {
            let title_width = (ui.available_width() - 196.0).max(100.0);
            ui.allocate_ui_with_layout(
                egui::vec2(title_width, 64.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&host.name).size(26.0).strong())
                            .truncate(),
                    )
                    .on_hover_text(&host.name);
                    ui.horizontal(|ui| {
                        let (label, color) = host.state();
                        status_chip(ui, label, color);
                        ui.label(
                            egui::RichText::new(if host.source.remote {
                                "Remote host"
                            } else {
                                "This computer"
                            })
                            .size(12.0)
                            .color(MUTED),
                        );
                    });
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("•••", |ui| {
                    if ui.button("Copy diagnostics").clicked() {
                        ui.ctx().copy_text(diagnostic_text(&host));
                        self.notify("Diagnostics copied".into(), false);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            can_stop && !self.action_pending,
                            egui::Button::new(
                                egui::RichText::new("Stop host…")
                                    .color(egui::Color32::from_rgb(245, 137, 137)),
                            ),
                        )
                        .on_disabled_hover_text(if busy {
                            "The connected UI controls shutdown. Close it to release control."
                        } else {
                            "This host is unavailable."
                        })
                        .clicked()
                    {
                        self.confirm_stop = Some(host.source.clone());
                        ui.close();
                    }
                });
                if busy {
                    ui.label(egui::RichText::new("UI connected").color(ACCENT))
                        .on_hover_text(
                        "Close the looper UI to release control, or stop the host from that UI.",
                    );
                } else if can_open {
                    if ui
                        .add_enabled(!self.action_pending, primary_button("Open UI"))
                        .clicked()
                    {
                        let source = host.source.clone();
                        self.action(move || {
                            let mut command = source.ui_command()?;
                            shoop_host::launch::independent(&mut command);
                            command.spawn().context("Start looper UI")?;
                            Ok("Looper UI launched".into())
                        });
                    }
                }
                if self.action_pending {
                    ui.spinner();
                }
            });
        });
        let previous_tab = self.tab;
        ui.horizontal(|ui| {
            if tab_button(ui, "Overview", self.tab == Tab::Overview).clicked() {
                self.tab = Tab::Overview;
            }
            if tab_button(ui, "Logs", self.tab == Tab::Logs).clicked() {
                self.tab = Tab::Logs;
            }
        });
        ui.separator();
        ui.add_space(12.0);
        if self.tab == Tab::Logs {
            if previous_tab != Tab::Logs {
                if let Some(path) = host.source.log_path() {
                    self.read_logs(path);
                }
            }
            if host.source.remote {
                ui.label(
                    egui::RichText::new("Logs are on the host computer")
                        .size(18.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new("Open the manager on that computer to view its logs.")
                        .color(MUTED),
                );
            } else {
                self.show_logs(ui);
            }
        } else {
            egui::ScrollArea::vertical().id_salt(("overview", &host.source.path)).auto_shrink([false, false]).show(ui, |ui| {
                match &host.report {
                    Ok(report) => show_overview(ui, report),
                    Err(_) => {
                        ui.label(egui::RichText::new("We couldn’t reach this host").size(18.0).strong());
                        ui.label(egui::RichText::new("It may be stopped or disconnected. Check the host and try again.").color(MUTED));
                        ui.add_space(8.0);
                        if ui.add_enabled(!self.scanning, egui::Button::new("Try again").corner_radius(8)).clicked() { self.scan(); }
                    }
                }
                if host.transport == AppTransport::SharedMemory {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("Shared-memory hosts cannot reattach a UI.").color(MUTED));
                }
                ui.add_space(16.0);
                egui::CollapsingHeader::new("Advanced details").id_salt(&host.source.path).show(ui, |ui| { show_advanced(ui, &host); });
            });
        }
    }
    fn show_logs(&mut self, ui: &mut egui::Ui) {
        let Some(log) = self.logs.as_ref() else {
            ui.label(egui::RichText::new("Select a host to view its logs.").color(MUTED));
            return;
        };
        let path = log.path.clone();
        let mut refresh = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Latest 64 KiB").size(12.0).color(MUTED))
                .on_hover_text(path.display().to_string());
            if log.loading {
                ui.spinner();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!log.loading, egui::Button::new("Refresh logs").frame(false))
                    .clicked()
                {
                    refresh = true;
                }
                if ui
                    .add_enabled(
                        log.contents
                            .as_ref()
                            .is_some_and(|contents| contents.is_ok()),
                        egui::Button::new("Copy logs").frame(false),
                    )
                    .clicked()
                {
                    if let Some(Ok(contents)) = &log.contents {
                        ui.ctx().copy_text(contents.clone());
                    }
                }
            });
        });
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(SIDEBAR)
            .stroke(egui::Stroke::new(1.0, BORDER))
            .corner_radius(10)
            .inner_margin(16)
            .show(ui, |ui| {
                let height = (ui.available_height() - 34.0).max(100.0);
                egui::ScrollArea::both()
                    .id_salt(("log_contents", &path))
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_height(height);
                        match &log.contents {
                            Some(Ok(contents)) if contents.is_empty() => {
                                ui.label(egui::RichText::new("No log entries yet.").color(MUTED));
                            }
                            Some(Ok(contents)) => {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(contents)
                                            .monospace()
                                            .size(12.0)
                                            .color(TEXT),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Extend),
                                );
                            }
                            Some(Err(error)) => {
                                ui.colored_label(AMBER, "Could not read this log");
                                ui.label(error);
                            }
                            None => {
                                ui.label(egui::RichText::new("Loading log…").color(MUTED));
                            }
                        }
                    });
            });
        if refresh {
            self.read_logs(path);
        }
    }
    fn show_stop_dialog(&mut self, context: &egui::Context) {
        let Some(source) = self.confirm_stop.clone() else {
            return;
        };
        egui::Window::new("Stop host?")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .default_width(400.0)
            .show(context, |ui| {
                if let Some(host) = self.hosts.iter().find(|host| host.source == source) {
                    ui.strong(&host.name);
                }
                ui.label(
                    "Stopping ends playback and recording and discards the in-memory session.",
                );
                ui.label(
                    egui::RichText::new(
                        "Save the session in the looper UI first if you need to keep it.",
                    )
                    .color(MUTED),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.confirm_stop = None;
                    }
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new("Stop host")
                                .color(egui::Color32::from_rgb(245, 137, 137)),
                        ))
                        .clicked()
                    {
                        self.confirm_stop = None;
                        self.action(move || {
                            connection::shutdown_ownerless(
                                &source.target()?,
                                Duration::from_secs(15),
                            )?;
                            Ok("Host stopped. Cleanup confirmed.".into())
                        });
                    }
                });
            });
    }
    fn show_notice(&mut self, context: &egui::Context) {
        if self.notice.as_ref().is_some_and(|notice| {
            !notice.error && notice.created.elapsed() >= Duration::from_secs(5)
        }) {
            self.notice = None;
        }
        let Some(notice) = &self.notice else {
            return;
        };
        let mut dismiss = false;
        egui::Area::new("manager_notice".into())
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-24.0, -56.0])
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .corner_radius(10)
                    .inner_margin(14)
                    .show(ui, |ui| {
                        ui.set_max_width(340.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&notice.message).color(if notice.error {
                                AMBER
                            } else {
                                TEXT
                            }));
                            if ui.small_button("×").clicked() {
                                dismiss = true;
                            }
                        });
                    });
            });
        if dismiss {
            self.notice = None;
        }
    }
}

fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(label)
            .strong()
            .color(egui::Color32::from_rgb(10, 33, 29)),
    )
    .fill(ACCENT)
    .stroke(egui::Stroke::NONE)
    .corner_radius(8)
}
fn status_chip(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(11.0).color(color));
        });
}
fn host_row(ui: &mut egui::Ui, host: &Host, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 82.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            &host.name,
        )
    });
    let fill = if selected {
        egui::Color32::from_rgb(30, 51, 53)
    } else if response.hovered() {
        SURFACE
    } else {
        SIDEBAR
    };
    ui.painter().rect_filled(rect, 8, fill);
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            8,
            egui::Stroke::new(1.0, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 14.0), egui::vec2(3.0, 54.0)),
            2,
            ACCENT,
        );
    }
    let painter = ui.painter().with_clip_rect(rect.shrink(12.0));
    let name = egui::WidgetText::from(host.name.as_str()).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        rect.width() - 28.0,
        egui::FontId::proportional(14.0),
    );
    painter.galley(rect.min + egui::vec2(14.0, 11.0), name, TEXT);
    let (label, color) = host.state();
    painter.circle_filled(rect.min + egui::vec2(18.0, 40.0), 3.0, color);
    painter.text(
        rect.min + egui::vec2(28.0, 32.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::proportional(12.0),
        color,
    );
    let location = if host.source.remote {
        "Remote"
    } else {
        "Local"
    };
    let subtitle = match &host.report {
        Ok(report) if !report.closed && report.ready => format!(
            "{location}  ·  {}",
            if report.busy { "In use" } else { "Available" }
        ),
        _ => location.to_owned(),
    };
    painter.text(
        rect.min + egui::vec2(14.0, 56.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        egui::FontId::proportional(11.0),
        MUTED,
    );
    response
        .on_hover_text(&host.name)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn tab_button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(86.0, 36.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(14.0),
        if selected || response.hovered() {
            TEXT
        } else {
            MUTED
        },
    );
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - 2.0),
                egui::vec2(rect.width(), 2.0),
            ),
            1,
            ACCENT,
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            3,
            egui::Stroke::new(1.0, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
fn metric(ui: &mut egui::Ui, label: &str, value: String) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(10)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
            ui.label(egui::RichText::new(label).size(11.0).color(MUTED));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(value).size(22.0).strong());
        });
}
fn show_overview(ui: &mut egui::Ui, report: &HostReport) {
    if audio_fallback(report) || !report.diagnostics.is_empty() {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(48, 39, 27))
            .corner_radius(8)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(if audio_fallback(report) {
                        "Audio device unavailable — using dummy audio"
                    } else {
                        "This host needs attention"
                    })
                    .color(AMBER)
                    .strong(),
                );
                if let Some(diagnostic) = report.diagnostics.first() {
                    ui.label(egui::RichText::new(diagnostic).size(13.0));
                }
            });
        ui.add_space(16.0);
    }
    ui.label(egui::RichText::new("Audio").size(18.0).strong());
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        let driver = if report.effective.audio_driver == shoop_app_api::AudioDriverState::Dummy {
            "Dummy driver".to_owned()
        } else {
            report
                .requested
                .as_ref()
                .map(|settings| settings.driver.kind().id().to_uppercase())
                .unwrap_or_else(|| format!("{:?}", report.effective.audio_driver))
        };
        ui.label(driver);
        ui.label(
            egui::RichText::new(
                if report.effective.audio_driver == shoop_app_api::AudioDriverState::Dummy {
                    "No physical audio I/O".to_owned()
                } else {
                    format!("{:?}", report.effective.audio_driver)
                },
            )
            .color(MUTED),
        );
    });
    ui.add_space(8.0);
    ui.columns(4, |columns| {
        metric(
            &mut columns[0],
            "SAMPLE RATE",
            format!("{} kHz", report.effective.sample_rate as f32 / 1000.0),
        );
        metric(
            &mut columns[1],
            "BUFFER",
            format!("{}", report.effective.buffer_size),
        );
        metric(
            &mut columns[2],
            "DSP LOAD",
            format!("{:.1}%", report.effective.dsp_load_percent),
        );
        metric(&mut columns[3], "XRUNS", report.effective.xruns.to_string());
    });
}
fn show_advanced(ui: &mut egui::Ui, host: &Host) {
    ui.add_space(8.0);
    egui::Grid::new("host_details")
        .num_columns(2)
        .spacing([20.0, 10.0])
        .show(ui, |ui| {
            let mut row = |label: &str, value: String| {
                ui.label(egui::RichText::new(label).color(MUTED));
                ui.label(value);
                ui.end_row();
            };
            row("Endpoint", host.endpoint.clone());
            if let Ok(report) = &host.report {
                row(
                    "Audio state",
                    format!("{:?}", report.effective.audio_driver),
                );
                row("Host operations", report.pending_operations.to_string());
                row("Loop smoothing", format!("{} ms", report.loop_smoothing_ms));
                row(
                    "Master auto-connect",
                    if report.master_auto_connect {
                        "On"
                    } else {
                        "Off"
                    }
                    .into(),
                );
                if let Some(settings) = &report.requested {
                    row("Requested driver", settings.driver.kind().id().into());
                    row(
                        "Capacity",
                        format!(
                            "{} tracks · {} loops · {} channels · {} MiB",
                            settings.pools.tracks,
                            settings.pools.loops,
                            settings.pools.channels,
                            settings.pools.loop_memory_mb
                        ),
                    );
                }
            }
        });
    if let Err(error) = &host.report {
        ui.add_space(8.0);
        ui.colored_label(AMBER, error);
    }
    if let Ok(report) = &host.report {
        for diagnostic in &report.diagnostics {
            ui.colored_label(AMBER, diagnostic);
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Driver and capacity changes require a restart, which creates a fresh session.",
            )
            .size(12.0)
            .color(MUTED),
        );
    }
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!(
            "{}: {}",
            if host.source.remote {
                "Profile"
            } else {
                "Registration"
            },
            host.source.path.display()
        ))
        .size(12.0)
        .color(MUTED),
    );
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
        self.show(ui);
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
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size([820.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ShoopDaLoop Host Manager",
        options,
        Box::new(move |context| {
            context.egui_ctx.set_visuals(egui::Visuals::dark());
            context.egui_ctx.style_mut_of(egui::Theme::Dark, |style| {
                style.spacing.item_spacing = egui::vec2(10.0, 10.0);
                style.spacing.button_padding = egui::vec2(14.0, 8.0);
                style.visuals.override_text_color = Some(TEXT);
                style.visuals.panel_fill = BACKGROUND;
                style.visuals.window_fill = SURFACE;
                style.visuals.faint_bg_color = SURFACE;
                style.visuals.extreme_bg_color = SIDEBAR;
                style.visuals.selection.bg_fill = egui::Color32::from_rgb(34, 69, 65);
                style.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
                style.visuals.widgets.noninteractive.bg_stroke.color = BORDER;
                style.visuals.widgets.inactive.weak_bg_fill = SURFACE;
                style.visuals.widgets.inactive.fg_stroke.color = TEXT;
                style.visuals.widgets.inactive.bg_stroke.color = BORDER;
                style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(40, 48, 58);
                style
                    .text_styles
                    .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
                style
                    .text_styles
                    .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
                style
                    .text_styles
                    .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
                style
                    .text_styles
                    .insert(egui::TextStyle::Heading, egui::FontId::proportional(26.0));
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
        assert_eq!(host.state().0, "Dummy audio");
        let report = host.report.as_mut().unwrap();
        report.effective.audio_driver = shoop_app_api::AudioDriverState::Running;
        assert_eq!(host.state().0, "Running");
        host.report.as_mut().unwrap().effective.audio_driver =
            shoop_app_api::AudioDriverState::Failed;
        assert_eq!(host.state().0, "Needs attention");
        let report = host.report.as_mut().unwrap();
        report.diagnostics.push("Audio fallback".into());
        assert_eq!(host.state().0, "Needs attention");
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
    #[shoop_wasm_test_support::shoop_test]
    fn intended_dummy_audio_and_physical_driver_fallback_are_distinct() {
        let mut host = Host::query(Source {
            path: PathBuf::from("missing.json"),
            remote: true,
        });
        let mut report = HostReport {
            ready: true,
            busy: false,
            closed: false,
            master_auto_connect: true,
            loop_smoothing_ms: 3,
            requested: Some(shoop_app_api::HostSettings::default()),
            effective: Default::default(),
            diagnostics: vec![],
            pending_operations: 0,
            restart_required_fields: vec![],
        };
        assert!(!audio_fallback(&report));
        host.report = Ok(report.clone());
        assert_eq!(host.state().0, "Dummy audio");
        report.requested.as_mut().unwrap().driver =
            shoop_app_api::AudioDriverConfig::Cpal(Default::default());
        assert!(audio_fallback(&report));
        host.report = Ok(report);
        assert_eq!(host.state().0, "Audio fallback");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn changing_host_clears_logs_and_ignores_departed_log_replies() {
        let mut manager = Manager::new(PathBuf::from("sessions"), None);
        let first = Source {
            path: PathBuf::from("sessions/1.json"),
            remote: false,
        };
        let second = Source {
            path: PathBuf::from("sessions/2.json"),
            remote: false,
        };
        let first_log = first.log_path().unwrap();
        manager.select(first.clone());
        manager.tab = Tab::Logs;
        manager.logs = Some(LogView {
            path: first_log.clone(),
            contents: Some(Ok("host one".into())),
            loading: false,
        });
        manager.select(second);
        assert!(manager.tab == Tab::Overview && manager.logs.is_none());
        manager
            .sender
            .send(Event::Logs(first_log, Ok("departed log reply".into())))
            .unwrap();
        manager.poll();
        assert!(manager.logs.is_none());

        let archive = PathBuf::from("archive.log");
        manager.saved_log = true;
        manager.selected = None;
        manager.logs = Some(LogView {
            path: archive.clone(),
            contents: None,
            loading: true,
        });
        manager
            .sender
            .send(Event::Scanned(vec![Host::query(first)], None))
            .unwrap();
        manager
            .sender
            .send(Event::Logs(archive, Ok("archived host".into())))
            .unwrap();
        manager.poll();
        assert!(manager.selected.is_none());
        let log = manager.logs.as_ref().unwrap();
        assert!(!log.loading);
        assert_eq!(
            log.contents.as_ref().unwrap().as_ref().unwrap(),
            "archived host"
        );
    }
}
