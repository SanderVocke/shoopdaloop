use anyhow::{bail, ensure, Context, Result};
use eframe::egui;
use shoop_app_api::{
    AudioDriverConfig, CpalAudioDriverConfig, DummyAudioDriverConfig, HostSettings, NewEnginePools,
};
use shoop_host::{
    config::{AppTransport, HostConfig},
    remote::RemoteProfile,
    serve::StartOptions,
};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

#[derive(Default)]
struct Devices {
    host: String,
    hosts: Vec<String>,
    inputs: Vec<String>,
    outputs: Vec<String>,
    midi_inputs: Vec<String>,
    midi_outputs: Vec<String>,
    error: Option<String>,
}
fn discover(host: String) -> Devices {
    #[cfg(feature = "cpal")]
    {
        let mut devices = Devices {
            host: host.clone(),
            hosts: shoop_rt_host_native::cpal_hosts(),
            ..Default::default()
        };
        match shoop_rt_host_native::discover((host != "default").then_some(host.as_str())) {
            Ok(inventory) => {
                devices.inputs = inventory
                    .inputs
                    .into_iter()
                    .map(|device| device.name)
                    .collect();
                devices.outputs = inventory
                    .outputs
                    .into_iter()
                    .map(|device| device.name)
                    .collect();
                devices.midi_inputs = inventory.midi_inputs;
                devices.midi_outputs = inventory.midi_outputs;
            }
            Err(error) => devices.error = Some(error.to_string()),
        }
        devices
    }
    #[cfg(not(feature = "cpal"))]
    {
        Devices {
            host,
            ..Default::default()
        }
    }
}

pub(super) struct Request {
    pub profile: PathBuf,
    pub credential: PathBuf,
    pub create_credential: bool,
    pub config: HostConfig,
    pub options: StartOptions,
}
impl Request {
    pub fn start(self) -> Result<PathBuf> {
        self.config.validate()?;
        ensure!(
            !self.profile.exists(),
            "Profile already exists; select a new path"
        );
        if self.create_credential {
            shoop_host::remote::create_credential(&self.credential).context("Create credential")?;
        }
        shoop_host::remote::credential(&self.credential).context("Read credential")?;
        let executable = shoop_host::launch::companion_path(&std::env::current_exe()?)?;
        let (mut child, _) = shoop_host::launch::spawn(&self.config, self.options, &executable)?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(self.profile)
    }
}

pub(super) struct Dialog {
    pub open: bool,
    pub pending: bool,
    pub error: Option<String>,
    name: String,
    physical_audio: bool,
    device_defaults: bool,
    sample_rate: u32,
    buffer_size: u32,
    cpal_host: String,
    input: String,
    output: String,
    input_channels: String,
    output_channels: String,
    midi_inputs: Vec<String>,
    midi_outputs: Vec<String>,
    pools: NewEnginePools,
    master_auto_connect: bool,
    smoothing: u32,
    lan: bool,
    transport: AppTransport,
    endpoint: String,
    management: String,
    advertised_host: String,
    profile: PathBuf,
    credential: PathBuf,
    new_credential: bool,
    log_enabled: bool,
    log_file: PathBuf,
    devices: Devices,
    device_receiver: Option<Receiver<Devices>>,
}
impl Default for Dialog {
    fn default() -> Self {
        Self {
            open: true,
            pending: false,
            error: None,
            name: "ShoopDaLoop".into(),
            physical_audio: cfg!(feature = "cpal"),
            device_defaults: true,
            sample_rate: 48000,
            buffer_size: 256,
            cpal_host: "default".into(),
            input: "default".into(),
            output: "default".into(),
            input_channels: "all".into(),
            output_channels: "all".into(),
            midi_inputs: vec!["all".into()],
            midi_outputs: vec!["all".into()],
            pools: NewEnginePools::default(),
            master_auto_connect: true,
            smoothing: 3,
            lan: false,
            transport: AppTransport::Socket,
            endpoint: "127.0.0.1:0".into(),
            management: "127.0.0.1:0".into(),
            advertised_host: String::new(),
            profile: PathBuf::new(),
            credential: PathBuf::new(),
            new_credential: true,
            log_enabled: false,
            log_file: PathBuf::new(),
            devices: Devices::default(),
            device_receiver: None,
        }
    }
}
impl Dialog {
    pub fn new() -> Self {
        let mut dialog = Self::default();
        if dialog.physical_audio {
            dialog.refresh_devices();
        }
        dialog
    }
    fn refresh_devices(&mut self) {
        if self.device_receiver.is_some() {
            return;
        }
        let host = self.cpal_host.clone();
        let (sender, receiver) = mpsc::channel();
        self.device_receiver = Some(receiver);
        std::thread::spawn(move || {
            let _ = sender.send(discover(host));
        });
    }
    fn poll_devices(&mut self) {
        if let Some(receiver) = self.device_receiver.take() {
            match receiver.try_recv() {
                Ok(devices) => {
                    if devices.host != self.cpal_host {
                        self.refresh_devices();
                    } else {
                        self.devices = devices;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => self.device_receiver = Some(receiver),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Device enumeration failed".into())
                }
            }
        }
    }
    fn request(&self) -> Result<Request> {
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 128,
            "Name required (max 128 bytes)"
        );
        let profile = destination(&self.profile).context("Profile path")?;
        let credential = destination(&self.credential).context("Credential path")?;
        let log_file = if self.log_enabled {
            Some(destination(&self.log_file).context("Log path")?)
        } else {
            None
        };
        ensure!(
            !profile.exists(),
            "Profile already exists; select a new path"
        );
        ensure!(
            !self.new_credential || !credential.exists(),
            "Credential exists; select Use existing or a new key path"
        );
        ensure!(
            profile != credential,
            "Profile and credential paths must differ"
        );
        ensure!(
            log_file
                .as_ref()
                .is_none_or(|path| path != &profile && path != &credential),
            "Log path must differ from the profile and credential paths"
        );
        let endpoint: SocketAddr = self.endpoint.parse().context("Invalid data bind address")?;
        let management: SocketAddr = self
            .management
            .parse()
            .context("Invalid management bind address")?;
        ensure!(
            endpoint.port() == 0 || endpoint != management,
            "Data and management listeners require different ports"
        );
        ensure!(
            self.lan || (endpoint.ip().is_loopback() && management.ip().is_loopback()),
            "Non-loopback bind addresses require LAN mode"
        );
        ensure!(
            !self.lan || self.transport == AppTransport::Socket,
            "LAN mode requires socket transport"
        );
        let mut options = StartOptions::new()?;
        options.name = self.name.clone();
        options.endpoint = endpoint;
        options.management_endpoint = management;
        options.lan = self.lan;
        options.credential = Some(credential.clone());
        options.export_credential = Some(credential.clone());
        options.export_profile = Some(profile.clone());
        options.log_file = log_file;
        if !self.advertised_host.trim().is_empty() {
            ensure!(
                endpoint.port() != 0 && management.port() != 0,
                "Advertised endpoints require fixed bind ports"
            );
            let host = self.advertised_host.trim();
            let host = if host.contains(':') && !host.starts_with('[') {
                format!("[{host}]")
            } else {
                host.into()
            };
            options.advertised_endpoint = Some(format!("{host}:{}", endpoint.port()));
            options.advertised_management = Some(format!("{host}:{}", management.port()));
            RemoteProfile {
                version: shoop_app_protocol::VERSION,
                name: self.name.clone(),
                endpoint: options.advertised_endpoint.clone().unwrap(),
                management: options.advertised_management.clone().unwrap(),
                credential: self.credential.clone(),
                network: options.network,
                local: !self.lan,
                transport: AppTransport::Socket,
                shared_memory: None,
                log_file: None,
            }
            .validate()?;
        } else if endpoint.ip().is_unspecified() || management.ip().is_unspecified() {
            bail!("Advertise host is required for wildcard bind addresses");
        }
        let driver = if self.physical_audio {
            ensure!(cfg!(feature = "cpal"), "CPAL is unavailable in this build");
            AudioDriverConfig::Cpal(CpalAudioDriverConfig {
                host: self.cpal_host.clone(),
                input_device: self.input.clone(),
                output_device: self.output.clone(),
                input_channels: self.input_channels.clone(),
                output_channels: self.output_channels.clone(),
                sample_rate: if self.device_defaults {
                    0
                } else {
                    self.sample_rate
                },
                buffer_size: if self.device_defaults {
                    0
                } else {
                    self.buffer_size
                },
                midi_inputs: midi_ports(&self.midi_inputs),
                midi_outputs: midi_ports(&self.midi_outputs),
                client_name: self.name.clone(),
                ..Default::default()
            })
        } else {
            AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                sample_rate: self.sample_rate,
                buffer_size: self.buffer_size,
            })
        };
        let config = HostConfig {
            settings: HostSettings {
                driver,
                pools: self.pools.clone(),
                master_auto_connect: self.master_auto_connect,
                loop_smoothing_ms: self.smoothing,
            },
            transport: self.transport,
            ..Default::default()
        };
        config.validate()?;
        Ok(Request {
            profile,
            credential,
            create_credential: self.new_credential,
            config,
            options,
        })
    }
    pub fn failed(&mut self, message: String) {
        self.pending = false;
        self.error = Some(message);
        if self.credential.exists() {
            self.new_credential = false;
        }
    }
    pub fn show(&mut self, context: &egui::Context) -> Option<Request> {
        self.poll_devices();
        let mut open = self.open;
        let mut request = None;
        egui::Window::new("New host")
            .open(&mut open)
            .default_width(540.0)
            .collapsible(false)
            .show(context, |ui| {
                ui.add_enabled_ui(!self.pending, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("new_host_form")
                        .max_height((context.viewport_rect().height() - 150.0).max(220.0))
                        .show(ui, |ui| {
                            self.fields(ui);
                        });
                });
                if let Some(error) = &self.error {
                    ui.colored_label(super::AMBER, error);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if self.pending {
                        ui.spinner();
                        ui.label("Starting host…");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(!self.pending, super::primary_button("Start host"))
                            .clicked()
                        {
                            match self.request() {
                                Ok(value) => {
                                    self.pending = true;
                                    self.error = None;
                                    request = Some(value);
                                }
                                Err(error) => self.error = Some(format!("{error:#}")),
                            }
                        }
                        if ui
                            .add_enabled(!self.pending, egui::Button::new("Cancel"))
                            .clicked()
                        {
                            self.open = false;
                        }
                    });
                });
            });
        self.open &= open;
        request
    }
    fn fields(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("new_host_audio")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut self.name);
                ui.end_row();
                ui.label("Audio driver");
                let previous = self.physical_audio;
                egui::ComboBox::from_id_salt("new_host_driver")
                    .selected_text(if self.physical_audio {
                        "System audio (CPAL)"
                    } else {
                        "Dummy"
                    })
                    .show_ui(ui, |ui| {
                        ui.add_enabled_ui(cfg!(feature = "cpal"), |ui| {
                            ui.selectable_value(
                                &mut self.physical_audio,
                                true,
                                "System audio (CPAL)",
                            );
                        });
                        ui.selectable_value(&mut self.physical_audio, false, "Dummy");
                    });
                if !previous && self.physical_audio {
                    self.refresh_devices();
                }
                ui.end_row();
                if self.physical_audio {
                    ui.label("Audio API");
                    let previous_host = self.cpal_host.clone();
                    selector(
                        ui,
                        "new_host_api",
                        &mut self.cpal_host,
                        &["default"],
                        &self.devices.hosts,
                    );
                    if previous_host != self.cpal_host {
                        self.input = "default".into();
                        self.output = "default".into();
                        self.refresh_devices();
                    }
                    ui.end_row();
                    ui.label("Output device");
                    selector(
                        ui,
                        "new_host_output",
                        &mut self.output,
                        &["default", "none"],
                        &self.devices.outputs,
                    );
                    ui.end_row();
                    ui.label("Input device");
                    selector(
                        ui,
                        "new_host_input",
                        &mut self.input,
                        &["default", "none"],
                        &self.devices.inputs,
                    );
                    ui.end_row();
                    ui.label("Rate / buffer");
                    ui.checkbox(&mut self.device_defaults, "Device defaults");
                    ui.end_row();
                }
                if !self.physical_audio || !self.device_defaults {
                    ui.label("Sample rate");
                    ui.add(
                        egui::DragValue::new(&mut self.sample_rate)
                            .range(8000..=384000)
                            .speed(100)
                            .suffix(" Hz"),
                    );
                    ui.end_row();
                    ui.label("Buffer size");
                    ui.add(
                        egui::DragValue::new(&mut self.buffer_size)
                            .range(1..=16384)
                            .suffix(" frames"),
                    );
                    ui.end_row();
                }
            });
        if self.physical_audio {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.device_receiver.is_none(),
                        egui::Button::new("Refresh devices").small(),
                    )
                    .clicked()
                {
                    self.refresh_devices();
                }
                if self.device_receiver.is_some() {
                    ui.spinner();
                }
                if let Some(error) = &self.devices.error {
                    ui.colored_label(super::MUTED, error);
                }
            });
        }
        ui.separator();
        let previous_profile = self.profile.clone();
        egui::Grid::new("new_host_files")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Profile file");
                path_field(ui, &mut self.profile, true, "host-profile.json");
                ui.end_row();
                if self.profile != previous_profile {
                    if self.credential.as_os_str().is_empty()
                        || self.credential == previous_profile.with_extension("key")
                    {
                        self.credential = self.profile.with_extension("key");
                    }
                    if self.log_file.as_os_str().is_empty()
                        || self.log_file == previous_profile.with_extension("log")
                    {
                        self.log_file = self.profile.with_extension("log");
                    }
                }
                ui.label("Credential");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.new_credential, true, "Create new");
                    ui.selectable_value(&mut self.new_credential, false, "Use existing");
                });
                ui.end_row();
                ui.label("Key file");
                path_field(ui, &mut self.credential, self.new_credential, "host.key");
                ui.end_row();
                ui.label("Disk logging");
                ui.checkbox(&mut self.log_enabled, "Enabled");
                ui.end_row();
                if self.log_enabled {
                    ui.label("Log file");
                    path_field(ui, &mut self.log_file, true, "host.log");
                    ui.end_row();
                }
            });
        ui.collapsing("Capacity and session defaults", |ui| {
            egui::Grid::new("new_host_capacity")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Tracks");
                    ui.add(egui::DragValue::new(&mut self.pools.tracks).range(1..=64));
                    ui.end_row();
                    ui.label("Loops");
                    ui.add(egui::DragValue::new(&mut self.pools.loops).range(1..=256));
                    ui.end_row();
                    ui.label("Channels");
                    ui.add(egui::DragValue::new(&mut self.pools.channels).range(1..=16));
                    ui.end_row();
                    ui.label("Loop memory");
                    ui.add(
                        egui::DragValue::new(&mut self.pools.loop_memory_mb)
                            .range(1..=1024)
                            .suffix(" MiB"),
                    );
                    ui.end_row();
                    ui.label("Loop smoothing");
                    ui.add(
                        egui::DragValue::new(&mut self.smoothing)
                            .range(0..=100)
                            .suffix(" ms"),
                    );
                    ui.end_row();
                    ui.label("Master auto-connect");
                    ui.checkbox(&mut self.master_auto_connect, "Enabled");
                    ui.end_row();
                });
        });
        ui.collapsing("Connection", |ui| {
            let was_lan = self.lan;
            ui.checkbox(&mut self.lan, "LAN access");
            if self.lan != was_lan {
                self.endpoint = if self.lan { "0.0.0.0:15400" } else { "127.0.0.1:0" }.into();
                self.management = if self.lan { "0.0.0.0:15401" } else { "127.0.0.1:0" }.into();
                if self.lan {
                    self.transport = AppTransport::Socket;
                }
            }
            egui::Grid::new("new_host_network")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Transport");
                    ui.add_enabled_ui(!self.lan, |ui| {
                        egui::ComboBox::from_id_salt("new_host_transport")
                            .selected_text(if self.transport == AppTransport::Socket {
                                "Socket"
                            } else {
                                "Shared memory"
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.transport, AppTransport::Socket, "Socket");
                                ui.selectable_value(&mut self.transport, AppTransport::SharedMemory, "Shared memory");
                            });
                    });
                    ui.end_row();
                    ui.label("Data bind");
                    ui.text_edit_singleline(&mut self.endpoint);
                    ui.end_row();
                    ui.label("Management bind");
                    ui.text_edit_singleline(&mut self.management);
                    ui.end_row();
                    if self.lan {
                        ui.label("Advertise host").on_hover_text("Client-facing hostname or IP. Required for wildcard listeners; bind ports must be fixed.");
                        ui.text_edit_singleline(&mut self.advertised_host);
                        ui.end_row();
                    }
                });
        });
        if self.physical_audio {
            ui.collapsing("Channels and MIDI", |ui| {
                egui::Grid::new("new_host_channels")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Output channels");
                        ui.text_edit_singleline(&mut self.output_channels)
                            .on_hover_text("all, or a channel count");
                        ui.end_row();
                        ui.label("Input channels");
                        ui.text_edit_singleline(&mut self.input_channels)
                            .on_hover_text("all, or a channel count");
                        ui.end_row();
                    });
                midi_selector(
                    ui,
                    "MIDI inputs",
                    &mut self.midi_inputs,
                    &self.devices.midi_inputs,
                );
                midi_selector(
                    ui,
                    "MIDI outputs",
                    &mut self.midi_outputs,
                    &self.devices.midi_outputs,
                );
            });
        }
    }
}
fn selector(
    ui: &mut egui::Ui,
    id: &str,
    selected: &mut String,
    defaults: &[&str],
    entries: &[String],
) {
    egui::ComboBox::from_id_salt(id)
        .width(260.0)
        .selected_text(selected.as_str())
        .show_ui(ui, |ui| {
            for value in defaults {
                ui.selectable_value(selected, value.to_string(), *value);
            }
            for value in entries {
                ui.selectable_value(selected, value.clone(), value);
            }
        });
}
fn path_field(ui: &mut egui::Ui, path: &mut PathBuf, save: bool, file_name: &str) {
    ui.horizontal(|ui| {
        let mut value = path.to_string_lossy().into_owned();
        if ui
            .add(egui::TextEdit::singleline(&mut value).desired_width(300.0))
            .changed()
        {
            *path = value.into();
        }
        if ui.small_button("Browse…").clicked() {
            let dialog = rfd::FileDialog::new().set_file_name(file_name);
            let selected = if save {
                dialog.save_file()
            } else {
                dialog.pick_file()
            };
            if let Some(selected) = selected {
                *path = selected;
            }
        }
    });
}
fn midi_selector(ui: &mut egui::Ui, label: &str, selected: &mut Vec<String>, entries: &[String]) {
    ui.push_id(label, |ui| {
        ui.label(label);
        let mut all = selected.iter().any(|port| port == "all");
        if ui.checkbox(&mut all, "All ports").changed() {
            *selected = if all { vec!["all".into()] } else { vec![] };
        }
        ui.add_enabled_ui(!all, |ui| {
            for port in entries {
                let mut enabled = selected.contains(port);
                if ui.checkbox(&mut enabled, port).changed() {
                    if enabled {
                        selected.push(port.clone());
                    } else {
                        selected.retain(|value| value != port);
                    }
                }
            }
        });
    });
}

fn destination(path: &std::path::Path) -> Result<PathBuf> {
    let name = path.file_name().context("File path required")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    Ok(parent
        .canonicalize()
        .context("Parent directory does not exist")?
        .join(name))
}
fn midi_ports(selected: &[String]) -> Vec<String> {
    if selected.is_empty() {
        vec!["none".into()]
    } else {
        selected.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(root: &std::path::Path) -> Dialog {
        Dialog {
            physical_audio: false,
            profile: root.join("profile.json"),
            credential: root.join("host.key"),
            ..Default::default()
        }
    }
    #[shoop_wasm_test_support::shoop_test]
    fn planning_a_host_uses_shared_config_without_writing_files() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        dialog.sample_rate = 44100;
        dialog.buffer_size = 128;
        dialog.pools.loop_memory_mb = 16;
        dialog.midi_inputs.clear();
        let request = dialog.request().unwrap();
        assert_eq!(
            request.config.driver,
            AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                sample_rate: 44100,
                buffer_size: 128
            })
        );
        assert_eq!(request.config.pools.loop_memory_mb, 16);
        assert!(request.options.log_file.is_none());
        assert_eq!(
            request.options.export_profile.as_ref(),
            Some(&request.profile)
        );
        assert_eq!(
            request.options.export_credential.as_ref(),
            Some(&request.credential)
        );
        assert!(request.create_credential);
        assert_eq!(midi_ports(&dialog.midi_inputs), ["none"]);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    #[shoop_wasm_test_support::shoop_test]
    fn existing_profiles_and_aliasing_output_paths_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        dialog.credential = root.path().join("./profile.json");
        assert!(dialog.request().is_err());
        dialog.credential = root.path().join("host.key");
        dialog.log_enabled = true;
        dialog.log_file = root.path().join("./host.key");
        assert!(dialog.request().is_err());
        dialog.log_enabled = false;
        std::fs::write(&dialog.profile, b"existing configuration").unwrap();
        assert!(dialog.request().is_err());
        assert_eq!(
            std::fs::read(&dialog.profile).unwrap(),
            b"existing configuration"
        );
    }
    #[shoop_wasm_test_support::shoop_test]
    fn lan_requires_client_addresses_and_socket_delivery() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        dialog.lan = true;
        dialog.endpoint = "0.0.0.0:15400".into();
        dialog.management = "0.0.0.0:15401".into();
        assert!(dialog.request().is_err());
        dialog.advertised_host = "studio.local".into();
        let request = dialog.request().unwrap();
        assert_eq!(
            request.options.advertised_endpoint.as_deref(),
            Some("studio.local:15400")
        );
        assert_eq!(
            request.options.advertised_management.as_deref(),
            Some("studio.local:15401")
        );
        dialog.transport = AppTransport::SharedMemory;
        assert!(dialog.request().is_err());
    }
}
