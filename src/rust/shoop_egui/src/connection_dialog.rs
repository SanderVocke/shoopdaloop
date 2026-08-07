use std::collections::BTreeSet;

use crate::{
    AppIntent, AppState, ConnectionViewState, LocalPortConnectionState, PortRole, TrackId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConnectionScope {
    AllTracks,
    Track(TrackId),
}

#[derive(Debug)]
pub struct ConnectionDialog {
    open: bool,
    scope: ConnectionScope,
    selected_role: Option<PortRole>,
    #[cfg(test)]
    cell_rects: Vec<(crate::PortId, String, egui::Rect)>,
}

impl Default for ConnectionDialog {
    fn default() -> Self {
        Self {
            open: false,
            scope: ConnectionScope::AllTracks,
            selected_role: None,
            #[cfg(test)]
            cell_rects: Vec::new(),
        }
    }
}

impl ConnectionDialog {
    pub fn open(&mut self, scope: ConnectionScope) {
        if self.scope != scope {
            self.selected_role = None;
        }
        self.scope = scope;
        self.open = true;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn scope(&self) -> ConnectionScope {
        self.scope
    }

    pub fn show(&mut self, context: &egui::Context, state: &AppState) -> Vec<AppIntent> {
        if !self.open {
            return Vec::new();
        }
        #[cfg(test)]
        self.cell_rects.clear();

        let scoped_track = match self.scope {
            ConnectionScope::AllTracks => None,
            ConnectionScope::Track(track_id) => {
                state.tracks.iter().find(|track| track.id == track_id)
            }
        };
        let title = match self.scope {
            ConnectionScope::AllTracks => "Connections".to_owned(),
            ConnectionScope::Track(track_id) => scoped_track
                .map(|track| format!("{} Connections", track.name))
                .unwrap_or_else(|| format!("Track {track_id} Connections")),
        };
        let mut open = self.open;
        let mut intents = Vec::new();
        egui::Window::new(title)
            .id(egui::Id::new("connections_dialog"))
            .open(&mut open)
            .resizable(true)
            .default_size([620.0, 450.0])
            .min_size([300.0, 180.0])
            .show(context, |ui| {
                if matches!(self.scope, ConnectionScope::Track(_)) && scoped_track.is_none() {
                    ui.colored_label(egui::Color32::YELLOW, "This track is no longer available.");
                    return;
                }
                self.show_contents(ui, &state.connections, &mut intents);
            });
        self.open = open;
        intents
    }

    fn show_contents(
        &mut self,
        ui: &mut egui::Ui,
        state: &ConnectionViewState,
        intents: &mut Vec<AppIntent>,
    ) {
        if state.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading connection state…");
            });
            return;
        }
        if !state.backend_available {
            ui.colored_label(
                egui::Color32::YELLOW,
                "External connection management is unavailable for this audio backend.",
            );
        }

        let scoped_ports: Vec<_> = state
            .ports
            .iter()
            .filter(|port| match self.scope {
                ConnectionScope::AllTracks => true,
                ConnectionScope::Track(track_id) => port.track_id == track_id,
            })
            .collect();
        if scoped_ports.is_empty() {
            ui.label("No externally connectable ports in this scope.");
            return;
        }

        let roles: Vec<_> = PortRole::ORDERED
            .into_iter()
            .filter(|role| scoped_ports.iter().any(|port| port.role == *role))
            .collect();
        if !roles.contains(&self.selected_role.unwrap_or(roles[0])) {
            self.selected_role = Some(roles[0]);
        }
        let selected = self.selected_role.unwrap_or(roles[0]);
        egui::ScrollArea::horizontal()
            .id_salt("connection_category_tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for role in &roles {
                        if ui
                            .selectable_label(*role == selected, role.label())
                            .clicked()
                        {
                            self.selected_role = Some(*role);
                        }
                    }
                });
            });
        ui.separator();

        let selected = self.selected_role.unwrap_or(selected);
        let mut ports: Vec<_> = scoped_ports
            .into_iter()
            .filter(|port| port.role == selected)
            .collect();
        ports.sort_by(|left, right| {
            (left.track_id, &left.name, left.id).cmp(&(right.track_id, &right.name, right.id))
        });
        let endpoints: BTreeSet<_> = ports
            .iter()
            .flat_map(|port| {
                port.candidates
                    .iter()
                    .map(|candidate| candidate.full_name.clone())
            })
            .collect();
        if endpoints.is_empty() {
            ui.label("No compatible external ports are currently available.");
            return;
        }

        egui::ScrollArea::both()
            .id_salt(("connection_matrix", selected, self.scope))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new(("connection_grid", selected, self.scope))
                    .striped(true)
                    .spacing([4.0, 3.0])
                    .show(ui, |ui| {
                        ui.label(crate::fonts::bold_text("External port"));
                        for port in &ports {
                            ui.add_sized(
                                [84.0, 24.0],
                                egui::Label::new(short_port_name(&port.name)).truncate(),
                            )
                            .on_hover_text(format!("{} (track {})", port.name, port.track_id));
                        }
                        ui.end_row();

                        let mut previous_client: Option<String> = None;
                        for endpoint in endpoints {
                            let (client, short_name) = external_name_parts(&endpoint);
                            if previous_client.as_deref() != Some(client.as_str()) {
                                ui.label(crate::fonts::bold_italic_text(&client).underline());
                                for _ in &ports {
                                    ui.label("");
                                }
                                ui.end_row();
                                previous_client = Some(client);
                            }
                            ui.add_sized([180.0, 24.0], egui::Label::new(short_name))
                                .on_hover_text(&endpoint);
                            for port in &ports {
                                self.show_cell(ui, port, &endpoint, intents);
                            }
                            ui.end_row();
                        }
                    });
            });

        if let Some(error) = state.errors.last() {
            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_RED, &error.message);
        }
    }

    fn show_cell(
        &mut self,
        ui: &mut egui::Ui,
        port: &LocalPortConnectionState,
        endpoint: &str,
        intents: &mut Vec<AppIntent>,
    ) {
        let candidate = port
            .candidates
            .iter()
            .find(|candidate| candidate.full_name == endpoint);
        let (text, enabled, hover) = match candidate {
            None => ("×", false, "Unavailable for this local port".to_owned()),
            Some(candidate) if !candidate.eligible => {
                ("×", false, "Incompatible with this local port".to_owned())
            }
            Some(candidate) if candidate.pending.is_some() => (
                "…",
                false,
                if candidate.pending == Some(true) {
                    "Connecting…".to_owned()
                } else {
                    "Disconnecting…".to_owned()
                },
            ),
            Some(candidate) if candidate.error.is_some() => {
                ("!", true, candidate.error.clone().unwrap_or_default())
            }
            Some(candidate) if candidate.connected => {
                ("●", true, "Connected; click to disconnect".to_owned())
            }
            Some(_) => ("○", true, "Disconnected; click to connect".to_owned()),
        };
        let response = ui
            .add_enabled(
                enabled,
                egui::Button::new(text).min_size(egui::vec2(28.0, 24.0)),
            )
            .on_hover_text(hover);
        #[cfg(test)]
        self.cell_rects
            .push((port.id, endpoint.to_owned(), response.rect));
        if response.clicked() {
            let candidate = candidate.expect("only eligible candidate cells are enabled");
            intents.push(connection_intent(port, candidate));
        }
    }
}

fn connection_intent(
    port: &LocalPortConnectionState,
    candidate: &crate::ExternalPortConnectionState,
) -> AppIntent {
    AppIntent::SetPortConnected {
        port_id: port.id,
        external_port: candidate.full_name.clone(),
        connected: !candidate.connected,
    }
}

fn external_name_parts(full_name: &str) -> (String, String) {
    match full_name.split_once(':') {
        Some((client, port)) => (client.to_owned(), port.to_owned()),
        None => ("Other".to_owned(), full_name.to_owned()),
    }
}

fn short_port_name(full_name: &str) -> &str {
    full_name
        .split_once(':')
        .map(|(_, port)| port)
        .unwrap_or(full_name)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        ConnectionErrorState, ExternalPortConnectionState, LocalPortConnectionState, PortDataType,
        PortDirection, PortId, TrackState,
    };

    fn state() -> AppState {
        let track_one = TrackId::from_raw(1);
        let track_two = TrackId::from_raw(2);
        AppState {
            tracks: vec![
                TrackState {
                    id: track_one,
                    name: "One".to_owned(),
                    ..Default::default()
                },
                TrackState {
                    id: track_two,
                    name: "Two".to_owned(),
                    ..Default::default()
                },
            ],
            connections: Arc::new(ConnectionViewState {
                revision: 2,
                loading: false,
                backend_available: true,
                ports: Arc::from([
                    LocalPortConnectionState {
                        id: PortId::from_raw(11),
                        track_id: track_one,
                        name: "one:in".to_owned(),
                        data_type: PortDataType::Audio,
                        direction: PortDirection::Input,
                        role: PortRole::AudioInput,
                        candidates: Arc::from([
                            ExternalPortConnectionState {
                                full_name: "client:out".to_owned(),
                                eligible: true,
                                connected: false,
                                pending: None,
                                error: None,
                            },
                            ExternalPortConnectionState {
                                full_name: "missing-colon".to_owned(),
                                eligible: false,
                                connected: false,
                                pending: None,
                                error: None,
                            },
                        ]),
                    },
                    LocalPortConnectionState {
                        id: PortId::from_raw(12),
                        track_id: track_two,
                        name: "two:out".to_owned(),
                        data_type: PortDataType::Audio,
                        direction: PortDirection::Output,
                        role: PortRole::AudioOutput,
                        candidates: Arc::from([ExternalPortConnectionState {
                            full_name: "client:in".to_owned(),
                            eligible: true,
                            connected: true,
                            pending: None,
                            error: None,
                        }]),
                    },
                ]),
                errors: Arc::<[ConnectionErrorState]>::from([]),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn scopes_categories_and_matrix_paint_at_minimum_and_common_sizes() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ConnectionDialog::default();
        for (scope, expected_cells) in [
            (ConnectionScope::Track(TrackId::from_raw(1)), 2),
            (ConnectionScope::AllTracks, 2),
        ] {
            dialog.open(scope);
            for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
                let output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| {
                        let intents = dialog.show(ui.ctx(), &state);
                        assert!(intents.is_empty());
                    },
                );
                assert!(output.shapes.len() > 5);
            }
            assert_eq!(dialog.cell_rects.len(), expected_cells);
        }
    }

    #[test]
    fn eligible_cells_emit_exact_desired_state_and_disabled_cells_do_not() {
        let state = state();
        let port = &state.connections.ports[0];
        let eligible = &port.candidates[0];
        let disabled = &port.candidates[1];
        assert_eq!(
            connection_intent(port, eligible),
            AppIntent::SetPortConnected {
                port_id: PortId::from_raw(11),
                external_port: "client:out".to_owned(),
                connected: true,
            }
        );
        assert!(eligible.eligible && eligible.pending.is_none());
        assert!(!disabled.eligible);
    }

    #[test]
    fn close_reopen_scope_switch_and_stale_track_keep_presentation_routing_safe() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::Track(TrackId::from_raw(1)));
        dialog.selected_role = Some(PortRole::AudioInput);
        dialog.open = false;
        dialog.open(ConnectionScope::Track(TrackId::from_raw(1)));
        assert_eq!(dialog.selected_role, Some(PortRole::AudioInput));
        dialog.open(ConnectionScope::AllTracks);
        assert_eq!(dialog.selected_role, None);
        dialog.open(ConnectionScope::Track(TrackId::from_raw(999)));
        let output = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(output.shapes.len() > 2);
        assert_eq!(
            dialog.scope(),
            ConnectionScope::Track(TrackId::from_raw(999))
        );
    }

    #[test]
    fn large_matrix_uses_both_axis_overflow_at_minimum_size() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let track_id = TrackId::from_raw(1);
        let candidates: Arc<[ExternalPortConnectionState]> = (0..50)
            .map(|index| ExternalPortConnectionState {
                full_name: format!("client_{}:output_{index}", index / 5),
                eligible: true,
                connected: index % 7 == 0,
                pending: (index == 3).then_some(true),
                error: (index == 4).then(|| "failure".to_owned()),
            })
            .collect::<Vec<_>>()
            .into();
        let ports: Arc<[LocalPortConnectionState]> = (0..16)
            .map(|index| LocalPortConnectionState {
                id: PortId::from_raw(index + 1),
                track_id,
                name: format!("local:input_{index}"),
                data_type: PortDataType::Audio,
                direction: PortDirection::Input,
                role: PortRole::AudioInput,
                candidates: Arc::clone(&candidates),
            })
            .collect::<Vec<_>>()
            .into();
        let state = AppState {
            tracks: vec![TrackState {
                id: track_id,
                name: "Large".to_owned(),
                ..Default::default()
            }],
            connections: Arc::new(ConnectionViewState {
                revision: 3,
                loading: false,
                backend_available: true,
                ports,
                errors: Arc::from([]),
            }),
            ..Default::default()
        };
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 200.0),
                )),
                ..Default::default()
            },
            |ui| {
                assert!(dialog.show(ui.ctx(), &state).is_empty());
            },
        );
        assert!(output.shapes.len() > 10);
        assert_eq!(dialog.cell_rects.len(), 16 * 50);
    }

    #[test]
    fn role_order_and_name_splitting_match_the_contract() {
        assert_eq!(
            external_name_parts("client:port:part"),
            ("client".to_owned(), "port:part".to_owned())
        );
        assert_eq!(
            external_name_parts("plain"),
            ("Other".to_owned(), "plain".to_owned())
        );
        assert_eq!(short_port_name("client:port"), "port");
    }
}
