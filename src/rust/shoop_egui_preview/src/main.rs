use std::sync::Arc;

use eframe::egui;
use shoop_app_api::{
    AppIntent, AppSnapshot, ConnectionErrorKind, ConnectionErrorState, ConnectionViewState,
    ExternalPortConnectionState, LocalPortConnectionState, LoopId, LoopState, PortDataType,
    PortDirection, PortId, PortRole, StatusState, TrackControlState, TrackId, TrackState,
};
use shoop_egui::{
    register_settings, AppWidget, ConnectionScope, SettingsPersistenceState,
    SettingsRegistryBuilder, SettingsViewState,
};

#[cfg(any(target_arch = "wasm32", test))]
const WEB_CANVAS_ID: &str = "shoop_canvas";

struct PreviewApp {
    widget: AppWidget,
    snapshot: AppSnapshot,
    settings: SettingsViewState,
    last_intent: String,
    churn_endpoint_visible: bool,
}

impl Default for PreviewApp {
    fn default() -> Self {
        let mut settings_builder = SettingsRegistryBuilder::default();
        register_settings(&mut settings_builder).expect("preview settings must be valid");
        let settings_registry = Arc::new(settings_builder.finish());
        let widget = AppWidget::new(Arc::clone(&settings_registry));
        #[cfg(target_arch = "wasm32")]
        let widget = {
            let mut widget = widget;
            if let Some(search) =
                web_sys::window().and_then(|window| window.location().search().ok())
            {
                if search.contains("scope=all") {
                    widget.open_connections(ConnectionScope::AllTracks);
                } else if search.contains("scope=track") {
                    widget.open_connections(ConnectionScope::Track(TrackId::from_raw(2)));
                }
            }
            widget
        };
        Self {
            widget,
            snapshot: representative_snapshot(),
            settings: SettingsViewState {
                active: Arc::new(settings_registry.defaults(1)),
                diagnostics: Arc::from([]),
                storage_location: "preview fixture (not persisted)".to_owned(),
                recovery_required: false,
                persistence: SettingsPersistenceState::Idle,
            },
            last_intent: "Open Connections from the main or track menu".to_owned(),
            churn_endpoint_visible: true,
        }
    }
}

impl eframe::App for PreviewApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let response = self.widget.show(ui, &self.snapshot, &self.settings, None);
        for intent in response.app_actions {
            self.last_intent = format!("{intent:?}");
            self.apply(intent);
        }
        for action in response.settings_actions {
            self.last_intent = format!("{action:?}");
        }
        egui::Area::new(egui::Id::new("connection_preview_controls"))
            .anchor(egui::Align2::RIGHT_TOP, [-8.0, 8.0])
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new("Connection preview").strong());
                    ui.horizontal(|ui| {
                        if ui.small_button("All scope").clicked() {
                            self.widget.open_connections(ConnectionScope::AllTracks);
                        }
                        if ui.small_button("Track scope").clicked() {
                            self.widget
                                .open_connections(ConnectionScope::Track(TrackId::from_raw(2)));
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Ready").clicked() {
                            self.snapshot.connections = representative_connections();
                        }
                        if ui.small_button("Loading").clicked() {
                            let mut state = (*self.snapshot.connections).clone();
                            state.loading = true;
                            self.snapshot.connections = Arc::new(state);
                        }
                        if ui.small_button("Unavailable").clicked() {
                            let mut state = (*self.snapshot.connections).clone();
                            state.loading = false;
                            state.backend_available = false;
                            self.snapshot.connections = Arc::new(state);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Confirm pending").clicked() {
                            self.resolve_pending(true);
                        }
                        if ui.small_button("Fail pending").clicked() {
                            self.resolve_pending(false);
                        }
                        if ui.small_button("Endpoint churn").clicked() {
                            self.toggle_churn_endpoint();
                        }
                    });
                    ui.label(egui::RichText::new(&self.last_intent).small());
                });
            });
        #[cfg(target_arch = "wasm32")]
        set_preview_status(&self.widget, &self.snapshot);
    }
}

impl PreviewApp {
    fn apply(&mut self, intent: AppIntent) {
        if let AppIntent::SetPortConnected {
            port_id,
            external_port,
            connected,
        } = intent
        {
            let mut view = (*self.snapshot.connections).clone();
            let mut ports = view.ports.to_vec();
            if let Some(port) = ports.iter_mut().find(|port| port.id == port_id) {
                let mut candidates = port.candidates.to_vec();
                if let Some(candidate) = candidates
                    .iter_mut()
                    .find(|candidate| candidate.full_name == external_port)
                {
                    candidate.pending = Some(connected);
                    candidate.error = None;
                }
                port.candidates = candidates.into();
            }
            view.revision = view.revision.wrapping_add(1);
            view.ports = ports.into();
            self.snapshot.connections = Arc::new(view);
        }
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
    }

    fn resolve_pending(&mut self, succeed: bool) {
        let mut view = (*self.snapshot.connections).clone();
        let mut ports = view.ports.to_vec();
        let mut errors = view.errors.to_vec();
        for port in &mut ports {
            let mut candidates = port.candidates.to_vec();
            for candidate in &mut candidates {
                let Some(desired) = candidate.pending.take() else {
                    continue;
                };
                if succeed {
                    candidate.connected = desired;
                    candidate.error = None;
                } else {
                    candidate.error = Some("Injected preview failure".to_owned());
                    errors.push(ConnectionErrorState {
                        port_id: Some(port.id),
                        external_port: Some(candidate.full_name.clone()),
                        kind: ConnectionErrorKind::BackendRejected,
                        message: "Injected preview connection failure".to_owned(),
                    });
                }
            }
            port.candidates = candidates.into();
        }
        view.revision = view.revision.wrapping_add(1);
        view.loading = false;
        view.backend_available = true;
        view.ports = ports.into();
        view.errors = errors.into();
        self.snapshot.connections = Arc::new(view);
    }

    fn toggle_churn_endpoint(&mut self) {
        self.churn_endpoint_visible = !self.churn_endpoint_visible;
        let mut view = (*self.snapshot.connections).clone();
        let mut ports = view.ports.to_vec();
        for port in &mut ports {
            if port.data_type != PortDataType::Audio || port.direction != PortDirection::Input {
                continue;
            }
            let mut candidates = port.candidates.to_vec();
            candidates.retain(|candidate| candidate.full_name != "hotplug:output");
            if self.churn_endpoint_visible {
                candidates.push(candidate("hotplug:output", true, false, None, None));
                candidates.sort_by(|left, right| left.full_name.cmp(&right.full_name));
            }
            port.candidates = candidates.into();
        }
        view.revision = view.revision.wrapping_add(1);
        view.ports = ports.into();
        self.snapshot.connections = Arc::new(view);
    }
}

fn candidate(
    full_name: &str,
    eligible: bool,
    connected: bool,
    pending: Option<bool>,
    error: Option<&str>,
) -> ExternalPortConnectionState {
    ExternalPortConnectionState {
        full_name: full_name.to_owned(),
        eligible,
        connected,
        pending,
        error: error.map(str::to_owned),
    }
}

fn port(
    id: u64,
    track_id: u64,
    name: &str,
    data_type: PortDataType,
    direction: PortDirection,
    role: PortRole,
    candidates: Vec<ExternalPortConnectionState>,
) -> LocalPortConnectionState {
    LocalPortConnectionState {
        id: PortId::from_raw(id),
        track_id: TrackId::from_raw(track_id),
        name: name.to_owned(),
        data_type,
        direction,
        role,
        candidates: candidates.into(),
    }
}

fn representative_connections() -> Arc<ConnectionViewState> {
    Arc::new(ConnectionViewState {
        revision: 1,
        loading: false,
        backend_available: true,
        ports: vec![
            port(
                1,
                1,
                "sync_loop_direct_in",
                PortDataType::Audio,
                PortDirection::Input,
                PortRole::AudioInput,
                vec![
                    candidate("system:capture_1", true, true, None, None),
                    candidate("plain-endpoint", true, false, None, None),
                    candidate("hotplug:output", true, false, None, None),
                ],
            ),
            port(
                2,
                2,
                "stereo_direct_in_1",
                PortDataType::Audio,
                PortDirection::Input,
                PortRole::AudioInput,
                vec![
                    candidate("system:capture_1", true, false, Some(true), None),
                    candidate(
                        "studio-interface:output_1",
                        true,
                        false,
                        None,
                        Some("Previous request was rejected"),
                    ),
                    candidate("hotplug:output", true, false, None, None),
                ],
            ),
            port(
                3,
                2,
                "stereo_direct_out_1",
                PortDataType::Audio,
                PortDirection::Output,
                PortRole::AudioOutput,
                vec![
                    candidate("system:playback_1", true, false, None, None),
                    candidate("recorder:input", true, true, None, None),
                ],
            ),
            port(
                4,
                2,
                "stereo_send_1",
                PortDataType::Audio,
                PortDirection::Output,
                PortRole::AudioSend,
                vec![candidate("fx:input", true, false, None, None)],
            ),
            port(
                5,
                2,
                "stereo_return_1",
                PortDataType::Audio,
                PortDirection::Input,
                PortRole::AudioReturn,
                vec![candidate("fx:output", true, true, None, None)],
            ),
            port(
                6,
                2,
                "stereo_midi_in",
                PortDataType::Midi,
                PortDirection::Input,
                PortRole::MidiInput,
                vec![candidate("controller:midi_out", true, false, None, None)],
            ),
            port(
                7,
                2,
                "stereo_midi_out",
                PortDataType::Midi,
                PortDirection::Output,
                PortRole::MidiOutput,
                vec![candidate("synth:midi_in", true, true, None, None)],
            ),
            port(
                8,
                2,
                "stereo_midi_send",
                PortDataType::Midi,
                PortDirection::Output,
                PortRole::MidiSend,
                vec![candidate("lighting:midi_in", true, false, None, None)],
            ),
            port(
                9,
                3,
                "mono_direct_in",
                PortDataType::Audio,
                PortDirection::Input,
                PortRole::AudioInput,
                vec![candidate("system:capture_2", true, false, None, None)],
            ),
        ]
        .into(),
        errors: vec![ConnectionErrorState {
            port_id: Some(PortId::from_raw(2)),
            external_port: Some("studio-interface:output_1".to_owned()),
            kind: ConnectionErrorKind::BackendRejected,
            message: "Representative backend rejection".to_owned(),
        }]
        .into(),
    })
}

fn controls(audio_channels: u8, midi: bool) -> TrackControlState {
    TrackControlState {
        has_output: audio_channels > 0 || midi,
        has_output_audio: audio_channels > 0,
        output_stereo: audio_channels == 2,
        output_midi_activity: midi,
        has_input: audio_channels > 0 || midi,
        has_input_audio: audio_channels > 0,
        input_stereo: audio_channels == 2,
        input_midi_activity: midi,
        ..Default::default()
    }
}

fn loops(base: u64, count: u64, stereo: bool) -> Vec<LoopState> {
    (0..count)
        .map(|index| LoopState {
            id: LoopId::from_raw(base + index),
            name: format!("({})", index + 1),
            stereo,
            show_gain: true,
            ..Default::default()
        })
        .collect()
}

fn representative_snapshot() -> AppSnapshot {
    AppSnapshot {
        tracks: vec![
            TrackState {
                id: TrackId::from_raw(1),
                name: "Sync".to_owned(),
                is_sync: true,
                loops: vec![LoopState {
                    id: LoopId::from_raw(1),
                    name: "sync loop".to_owned(),
                    sync: true,
                    show_gain: true,
                    ..Default::default()
                }],
                controls: controls(1, false),
                port_ids: Arc::from([PortId::from_raw(1)]),
            },
            TrackState {
                id: TrackId::from_raw(2),
                name: "Stereo + MIDI + roles".to_owned(),
                loops: loops(2, 8, true),
                controls: controls(2, true),
                port_ids: (2..=8).map(PortId::from_raw).collect::<Vec<_>>().into(),
                ..Default::default()
            },
            TrackState {
                id: TrackId::from_raw(3),
                name: "Mono".to_owned(),
                loops: loops(20, 8, false),
                controls: controls(1, false),
                port_ids: Arc::from([PortId::from_raw(9)]),
                ..Default::default()
            },
        ],
        status: StatusState {
            version: "connection preview".to_owned(),
            dsp_load_percent: 12.5,
            buffer_size: 256,
            sample_rate: 48_000,
            ..Default::default()
        },
        connections: representative_connections(),
        ..Default::default()
    }
}

#[cfg(target_arch = "wasm32")]
fn set_preview_status(widget: &AppWidget, snapshot: &AppSnapshot) {
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    else {
        return;
    };
    let scope = match widget.open_connection_scope() {
        Some(ConnectionScope::AllTracks) => "all",
        Some(ConnectionScope::Track(_)) => "track",
        None => "closed",
    };
    let _ = element.set_attribute("data-connection-scope", scope);
    let _ = element.set_attribute(
        "data-connection-port-count",
        &snapshot.connections.ports.len().to_string(),
    );
    let _ = element.set_attribute(
        "data-connection-revision",
        &snapshot.connections.revision.to_string(),
    );
}

fn create_app(
    context: &eframe::CreationContext<'_>,
) -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
    shoop_egui::initialize(&context.egui_ctx);
    Ok(Box::new(PreviewApp::default()))
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ShoopDaLoop egui connection preview")
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([360.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ShoopDaLoop egui connection preview",
        options,
        Box::new(create_app),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("browser window is unavailable")
            .document()
            .expect("browser document is unavailable");
        let canvas = document
            .get_element_by_id(WEB_CANVAS_ID)
            .expect("missing preview canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("preview element is not a canvas");
        eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(create_app))
            .await
            .expect("failed to start connection preview");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_covers_scopes_roles_and_required_states_without_backend_dependencies() {
        let snapshot = representative_snapshot();
        assert!(snapshot.tracks.iter().any(|track| track.is_sync));
        assert!(snapshot.tracks.iter().any(|track| !track.is_sync));
        for role in PortRole::ORDERED {
            assert!(snapshot
                .connections
                .ports
                .iter()
                .any(|port| port.role == role));
        }
        let candidates: Vec<_> = snapshot
            .connections
            .ports
            .iter()
            .flat_map(|port| port.candidates.iter())
            .collect();
        assert!(candidates.iter().any(|candidate| candidate.connected));
        assert!(candidates.iter().any(|candidate| !candidate.connected));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.pending.is_some()));
        assert!(candidates.iter().any(|candidate| candidate.error.is_some()));
        assert!(!snapshot.connections.errors.is_empty());
    }

    #[test]
    fn preview_captures_desired_state_and_can_confirm_or_fail_it() {
        let mut preview = PreviewApp::default();
        preview.apply(AppIntent::SetPortConnected {
            port_id: PortId::from_raw(3),
            external_port: "system:playback_1".to_owned(),
            connected: true,
        });
        let pending = preview
            .snapshot
            .connections
            .ports
            .iter()
            .find(|port| port.id == PortId::from_raw(3))
            .unwrap()
            .candidates
            .iter()
            .find(|candidate| candidate.full_name == "system:playback_1")
            .unwrap()
            .pending;
        assert_eq!(pending, Some(true));
        preview.resolve_pending(true);
        let candidate = preview
            .snapshot
            .connections
            .ports
            .iter()
            .find(|port| port.id == PortId::from_raw(3))
            .unwrap()
            .candidates
            .iter()
            .find(|candidate| candidate.full_name == "system:playback_1")
            .unwrap();
        assert!(candidate.connected);
        assert_eq!(candidate.pending, None);
    }

    #[test]
    fn web_shell_targets_preview_canvas() {
        let html = include_str!("../index.html");
        assert!(html.contains("data-trunk"));
        assert!(html.contains(&format!("id=\"{WEB_CANVAS_ID}\"")));
    }
}
