use anyhow::{ensure, Context, Result};
use eframe::egui;
use shoop_app_api::{
    AudioDriverConfig, AudioDriverKind, CpalAudioDriverConfig, DummyAudioDriverConfig,
    HostSettings, JackAudioDriverConfig, NewEnginePools,
};
use shoop_host::{
    config::{AppTransport, HostConfig},
    remote::{RemoteProfile, Target},
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
    pub config: HostConfig,
    pub options: StartOptions,
}
impl Request {
    pub fn start(self) -> Result<Target> {
        self.config.validate()?;
        let executable = shoop_host::launch::companion_path(&std::env::current_exe()?)?;
        let (child, target) = shoop_host::launch::spawn(&self.config, self.options, &executable)?;
        shoop_host::launch::reap(child);
        Ok(target)
    }
}

pub(super) struct Dialog {
    pub open: bool,
    pub pending: bool,
    pub error: Option<String>,
    name: String,
    driver: AudioDriverKind,
    jack_client_name: String,
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
    processors: shoop_app_api::ProcessorSettings,
    master_auto_connect: bool,
    smoothing: u32,
    lan: bool,
    transport: AppTransport,
    endpoint: String,
    management: String,
    advertised_host: String,
    log_enabled: bool,
    log_file: PathBuf,
    devices: Devices,
    device_receiver: Option<Receiver<Devices>>,
}
impl Default for Dialog {
    fn default() -> Self {
        let jack = JackAudioDriverConfig::default();
        Self {
            open: true,
            pending: false,
            error: None,
            name: "ShoopDaLoop".into(),
            driver: if cfg!(feature = "cpal") {
                AudioDriverKind::Cpal
            } else if cfg!(feature = "jack") {
                AudioDriverKind::Jack
            } else {
                AudioDriverKind::Dummy
            },
            jack_client_name: jack.client_name,
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
            processors: Default::default(),
            master_auto_connect: true,
            smoothing: 3,
            lan: false,
            transport: AppTransport::Socket,
            endpoint: "127.0.0.1:0".into(),
            management: "127.0.0.1:0".into(),
            advertised_host: String::new(),
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
        if dialog.driver == AudioDriverKind::Cpal {
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
        let log_file = if self.log_enabled {
            Some(destination(&self.log_file).context("Log path")?)
        } else {
            None
        };
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
        options.log_file = log_file;
        if self.lan && !self.advertised_host.trim().is_empty() {
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
                credential: None,
                version: shoop_app_protocol::VERSION,
                name: self.name.clone(),
                endpoint: options.advertised_endpoint.clone().unwrap(),
                management: options.advertised_management.clone().unwrap(),
                network: options.network,
                local: !self.lan,
                transport: AppTransport::Socket,
                log_file: None,
            }
            .validate()?;
        }
        let driver = match self.driver {
            AudioDriverKind::Jack => {
                ensure!(cfg!(feature = "jack"), "JACK is unavailable in this build");
                let jack = JackAudioDriverConfig {
                    client_name: self.jack_client_name.clone(),
                };
                jack.validate().map_err(anyhow::Error::msg)?;
                AudioDriverConfig::Jack(jack)
            }
            AudioDriverKind::Cpal => {
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
            }
            AudioDriverKind::Dummy => AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                sample_rate: self.sample_rate,
                buffer_size: self.buffer_size,
            }),
            AudioDriverKind::WebAudio => anyhow::bail!("Web Audio is unavailable in native hosts"),
        };
        let config = HostConfig {
            settings: HostSettings {
                driver,
                pools: self.pools.clone(),
                processors: self.processors,
                master_auto_connect: self.master_auto_connect,
                loop_smoothing_ms: self.smoothing,
            },
            transport: self.transport,
            ..Default::default()
        };
        config.validate()?;
        Ok(Request { config, options })
    }
    pub fn failed(&mut self, message: String) {
        self.pending = false;
        self.error = Some(message);
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
                let previous = self.driver;
                egui::ComboBox::from_id_salt("new_host_driver")
                    .selected_text(match self.driver {
                        AudioDriverKind::Cpal => "System audio (CPAL)",
                        AudioDriverKind::Jack => "JACK",
                        _ => "Dummy",
                    })
                    .show_ui(ui, |ui| {
                        ui.add_enabled_ui(cfg!(feature = "cpal"), |ui| {
                            ui.selectable_value(
                                &mut self.driver,
                                AudioDriverKind::Cpal,
                                "System audio (CPAL)",
                            );
                        });
                        ui.add_enabled_ui(cfg!(feature = "jack"), |ui| {
                            ui.selectable_value(&mut self.driver, AudioDriverKind::Jack, "JACK");
                        })
                        .response
                        .on_disabled_hover_text("JACK is unavailable in this build");
                        ui.selectable_value(&mut self.driver, AudioDriverKind::Dummy, "Dummy");
                    });
                if previous != self.driver && self.driver == AudioDriverKind::Cpal {
                    self.refresh_devices();
                }
                ui.end_row();
                if self.driver == AudioDriverKind::Jack {
                    ui.label("JACK client name");
                    ui.text_edit_singleline(&mut self.jack_client_name);
                    ui.end_row();
                    ui.label("Rate / buffer");
                    ui.label("Set by JACK server");
                    ui.end_row();
                }
                if self.driver == AudioDriverKind::Cpal {
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
                if self.driver == AudioDriverKind::Dummy
                    || (self.driver == AudioDriverKind::Cpal && !self.device_defaults)
                {
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
        if self.driver == AudioDriverKind::Cpal {
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
        if self.driver == AudioDriverKind::Jack {
            ui.label("Audio and MIDI ports follow the tracks and buses in the session.");
        }
        ui.separator();
        ui.collapsing("Logging", |ui| {
            ui.checkbox(&mut self.log_enabled, "Disk logging");
            if self.log_enabled {
                ui.label("Log file");
                path_field(ui, &mut self.log_file, "host.log");
            }
        });
        ui.collapsing("Capacity and session defaults", |ui| {
            egui::Grid::new("new_host_capacity")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Buses");
                    ui.add(egui::DragValue::new(&mut self.pools.buses).range(1..=8));
                    ui.end_row();
                    ui.label("Bus channels");
                    ui.add(egui::DragValue::new(&mut self.pools.bus_channels).range(2..=8));
                    ui.end_row();
                    ui.label("Tracks");
                    ui.add(egui::DragValue::new(&mut self.pools.tracks).range(1..=64));
                    ui.end_row();
                    ui.label("Loops");
                    ui.add(egui::DragValue::new(&mut self.pools.loops).range(1..=256));
                    ui.end_row();
                    ui.label("Channels");
                    ui.add(egui::DragValue::new(&mut self.pools.channels).range(1..=8));
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
        ui.collapsing("Track processors", |ui| {
            ui.label("These settings apply when starting this host.");
            for (label, policy) in [
                ("Built-in FX", &mut self.processors.builtin_fx),
                ("Built-in Synth", &mut self.processors.oxisynth),
            ] {
                egui::ComboBox::from_id_salt(label)
                    .selected_text(match *policy {
                        shoop_app_api::ProcessorPolicy::Auto => "Auto",
                        shoop_app_api::ProcessorPolicy::On => "On",
                        shoop_app_api::ProcessorPolicy::Off => "Off",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(policy, shoop_app_api::ProcessorPolicy::Auto, "Auto");
                        ui.selectable_value(policy, shoop_app_api::ProcessorPolicy::On, "On");
                        ui.selectable_value(policy, shoop_app_api::ProcessorPolicy::Off, "Off");
                    });
            }
            ui.horizontal(|ui| {
                ui.label("FX instances");
                ui.add(egui::DragValue::new(&mut self.processors.fx_instances).range(0..=64));
            });
            ui.horizontal(|ui| {
                ui.label("Synth instances");
                ui.add(egui::DragValue::new(&mut self.processors.synth_instances).range(0..=64));
            });
            ui.horizontal(|ui| {
                ui.label("Processor memory");
                ui.add(
                    egui::DragValue::new(&mut self.processors.memory_mb)
                        .range(0..=4096)
                        .suffix(" MiB"),
                );
            });
        });
        ui.collapsing("Connection", |ui| {
            let was_lan = self.lan;
            ui.checkbox(&mut self.lan, "LAN access");
            if self.lan != was_lan {
                self.endpoint = if self.lan { "0.0.0.0:0" } else { "127.0.0.1:0" }.into();
                self.management = if self.lan { "0.0.0.0:0" } else { "127.0.0.1:0" }.into();
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
                                ui.selectable_value(
                                    &mut self.transport,
                                    AppTransport::Socket,
                                    "Socket",
                                );
                                ui.selectable_value(
                                    &mut self.transport,
                                    AppTransport::SharedMemory,
                                    "Shared memory",
                                );
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
                        ui.label("Advertise host").on_hover_text(
                            "Optional client-facing hostname or IP; requires fixed bind ports.",
                        );
                        ui.text_edit_singleline(&mut self.advertised_host);
                        ui.end_row();
                    }
                });
        });
        if self.driver == AudioDriverKind::Cpal {
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
fn path_field(ui: &mut egui::Ui, path: &mut PathBuf, file_name: &str) {
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
            let selected = dialog.save_file();
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
    use super::super::tests::Screen;
    use super::*;

    fn form(_root: &std::path::Path) -> Dialog {
        Dialog {
            driver: AudioDriverKind::Dummy,
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
        dialog.pools.buses = 4;
        dialog.pools.bus_channels = 6;
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
        assert_eq!(request.config.pools.buses, 4);
        assert_eq!(request.config.pools.bus_channels, 6);
        assert!(request.options.log_file.is_none());
        assert!(request.options.export_profile.is_none());
        assert_eq!(midi_ports(&dialog.midi_inputs), ["none"]);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[cfg(feature = "jack")]
    #[shoop_wasm_test_support::shoop_test]
    fn jack_selection_only_configures_client_name_and_explains_dynamic_ports() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        let mut screen = Screen::new(egui::vec2(800.0, 1000.0));
        screen.frame(vec![], |ui| dialog.fields(ui));
        screen.click("Dummy", |ui| dialog.fields(ui));
        screen.click("JACK", |ui| dialog.fields(ui));
        assert_eq!(dialog.driver, AudioDriverKind::Jack);
        assert!(screen.contains("JACK client name") && screen.contains("Set by JACK server"));
        assert!(screen.contains("Audio and MIDI ports follow the tracks and buses in the session."));
        for label in [
            "Sample rate",
            "Buffer size",
            "Audio API",
            "Channels and MIDI",
            "JACK ports and connections",
            "Startup connections",
            "Add connection",
        ] {
            assert!(!screen.contains(label), "obsolete JACK control: {label}");
        }
        dialog.jack_client_name = "Studio".into();
        let request = dialog.request().unwrap();
        let AudioDriverConfig::Jack(jack) = &request.config.driver else {
            panic!("direct JACK expected");
        };
        assert_eq!(jack.client_name, "Studio");
        let json = serde_json::to_value(&request.config).unwrap();
        assert_eq!(
            json["driver"]["Jack"],
            serde_json::json!({"client_name":"Studio"})
        );
        let decoded: HostConfig = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.settings, request.config.settings);
        dialog.jack_client_name = "bad:name".into();
        assert!(dialog.request().is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn unavailable_drivers_cannot_be_selected_or_launched() {
        for (kind, label, available) in [
            (AudioDriverKind::Jack, "JACK", cfg!(feature = "jack")),
            (
                AudioDriverKind::Cpal,
                "System audio (CPAL)",
                cfg!(feature = "cpal"),
            ),
        ] {
            if available {
                continue;
            }
            let mut dialog = Dialog {
                driver: AudioDriverKind::Dummy,
                ..Default::default()
            };
            let mut screen = Screen::new(egui::vec2(800.0, 700.0));
            screen.frame(vec![], |ui| dialog.fields(ui));
            screen.click("Dummy", |ui| dialog.fields(ui));
            screen.click(label, |ui| dialog.fields(ui));
            assert_eq!(dialog.driver, AudioDriverKind::Dummy);
            dialog.driver = kind;
            assert!(dialog.request().is_err());
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn optional_logging_validates_only_an_explicit_output() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        assert!(dialog.request().unwrap().options.log_file.is_none());
        dialog.log_enabled = true;
        assert!(dialog.request().is_err());
        dialog.log_file = root.path().join("host.log");
        assert_eq!(
            dialog.request().unwrap().options.log_file,
            Some(root.path().canonicalize().unwrap().join("host.log"))
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    #[shoop_wasm_test_support::shoop_test]
    fn lan_uses_assigned_ports_and_supports_optional_advertised_addresses() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        dialog.lan = true;
        dialog.endpoint = "0.0.0.0:0".into();
        dialog.management = "0.0.0.0:0".into();
        assert!(dialog.request().is_ok());
        dialog.advertised_host = "studio.local".into();
        assert!(dialog.request().is_err());
        dialog.endpoint = "0.0.0.0:15400".into();
        dialog.management = "0.0.0.0:15401".into();
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

    #[shoop_wasm_test_support::shoop_test]
    fn start_form_validates_before_launch_and_disables_duplicate_submissions() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        dialog.name.clear();
        let mut screen = Screen::new(egui::vec2(700.0, 400.0));
        for _ in 0..2 {
            screen.frame(vec![], |ui| {
                assert!(dialog.show(ui.ctx()).is_none());
            });
        }
        screen.click("Start host", |ui| {
            assert!(dialog.show(ui.ctx()).is_none());
        });
        assert!(dialog.error.as_deref().unwrap().contains("Name required"));
        assert!(!dialog.pending);
        dialog.name = "Studio".into();
        let mut request = None;
        screen.click("Start host", |ui| {
            if let Some(value) = dialog.show(ui.ctx()) {
                request = Some(value);
            }
        });
        assert!(dialog.pending);
        assert_eq!(request.unwrap().options.name, "Studio");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        screen.click("Start host", |ui| {
            assert!(dialog.show(ui.ctx()).is_none());
        });
        dialog.failed("Port already in use".into());
        screen.frame(vec![], |ui| {
            assert!(dialog.show(ui.ctx()).is_none());
        });
        assert!(screen.contains("Port already in use") && !dialog.pending);
        screen.click("Cancel", |ui| {
            assert!(dialog.show(ui.ctx()).is_none());
        });
        assert!(!dialog.open);
    }

    #[cfg(feature = "cpal")]
    #[shoop_wasm_test_support::shoop_test]
    fn audio_capacity_midi_and_lan_controls_feed_the_shared_configuration() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        dialog.driver = AudioDriverKind::Cpal;
        dialog.device_defaults = false;
        dialog.output = "Speakers".into();
        dialog.devices.outputs = vec!["Speakers".into(), "Headphones".into()];
        dialog.devices.midi_inputs = vec!["Controller keyboard".into()];
        dialog.sample_rate = 44100;
        dialog.buffer_size = 128;
        let mut screen = Screen::new(egui::vec2(800.0, 1400.0));
        screen.frame(vec![], |ui| dialog.fields(ui));
        screen.click("Speakers", |ui| dialog.fields(ui));
        screen.click("Headphones", |ui| dialog.fields(ui));
        assert_eq!(dialog.output, "Headphones");
        for label in [
            "Capacity and session defaults",
            "Connection",
            "Channels and MIDI",
            "Logging",
        ] {
            screen.click(label, |ui| dialog.fields(ui));
        }
        assert!(screen.contains("Loop memory") && screen.contains("Output channels"));
        assert!(screen.contains("Buses") && screen.contains("Bus channels"));
        screen.click("All ports", |ui| dialog.fields(ui));
        assert!(dialog.midi_inputs.is_empty());
        screen.click("Controller keyboard", |ui| dialog.fields(ui));
        assert_eq!(dialog.midi_inputs, ["Controller keyboard"]);
        screen.click("Controller keyboard", |ui| dialog.fields(ui));
        screen.click("LAN access", |ui| dialog.fields(ui));
        assert!(dialog.lan && screen.contains("Advertise host"));
        assert_eq!(dialog.endpoint, "0.0.0.0:0");
        dialog.log_enabled = true;
        dialog.log_file = root.path().join("studio.log");
        screen.frame(vec![], |ui| dialog.fields(ui));
        assert!(screen.contains("Log file"));
        let request = dialog.request().unwrap();
        let AudioDriverConfig::Cpal(ref driver) = request.config.driver else {
            panic!("CPAL selection was lost");
        };
        assert_eq!((driver.sample_rate, driver.buffer_size), (44100, 128));
        assert_eq!(driver.output_device, "Headphones");
        assert_eq!(driver.midi_inputs, ["none"]);
        assert_eq!(
            request.options.log_file,
            Some(root.path().canonicalize().unwrap().join("studio.log"))
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        screen.click("LAN access", |ui| dialog.fields(ui));
        assert!(!dialog.lan && dialog.endpoint == "127.0.0.1:0");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn device_results_and_failed_startup_leave_configuration_retryable() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        let (sender, receiver) = mpsc::channel();
        dialog.device_receiver = Some(receiver);
        dialog.poll_devices();
        assert!(dialog.device_receiver.is_some());
        sender
            .send(Devices {
                host: "default".into(),
                outputs: vec!["Speakers".into()],
                ..Default::default()
            })
            .unwrap();
        dialog.poll_devices();
        assert_eq!(dialog.devices.outputs, ["Speakers"]);
        let (sender, receiver) = mpsc::channel();
        dialog.device_receiver = Some(receiver);
        drop(sender);
        dialog.poll_devices();
        assert_eq!(dialog.error.as_deref(), Some("Device enumeration failed"));
        dialog.failed("Bind failed".into());
        assert!(dialog.request().is_ok());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn normal_start_form_has_no_file_requirements() {
        let root = tempfile::tempdir().unwrap();
        let mut dialog = form(root.path());
        let mut screen = Screen::new(egui::vec2(800.0, 700.0));
        screen.frame(vec![], |ui| dialog.fields(ui));
        assert!(screen.contains("Name") && screen.contains("Audio driver"));
        assert!(!screen.contains("Profile") && !screen.contains("Log file"));
        let request = dialog.request().unwrap();
        assert!(request.options.export_profile.is_none());
        assert!(request.options.log_file.is_none());
        assert_eq!(request.options.endpoint.port(), 0);
    }
}
