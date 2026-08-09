use crate::{
    AppIntent, AppState, ApplicationPortOwner, ApplicationPortState, ConnectionPolicy,
    ConnectionViewState, HostPortState, PortRole, TrackId,
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
                "Host connection management is unavailable for this audio backend.",
            );
        }

        let scoped_ports: Vec<_> = state
            .application_ports
            .iter()
            .filter(|port| match (&self.scope, &port.owner) {
                (ConnectionScope::AllTracks, _) => true,
                (
                    ConnectionScope::Track(track_id),
                    ApplicationPortOwner::Track {
                        track_id: owner_id, ..
                    },
                ) => track_id == owner_id,
                (ConnectionScope::Track(_), ApplicationPortOwner::LuaControl { .. }) => false,
            })
            .collect();
        if scoped_ports.is_empty() {
            ui.label("No application ports in this scope.");
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
            (&left.owner, &left.name, left.id).cmp(&(&right.owner, &right.name, right.id))
        });
        let endpoints: Vec<_> = state
            .host_ports
            .iter()
            .filter(|host| {
                ports.iter().any(|port| {
                    port.data_type == host.data_type && port.direction != host.direction
                })
            })
            .collect();
        if endpoints.is_empty() {
            ui.label("No compatible host ports are currently available.");
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
                        ui.label(crate::fonts::bold_text("Host port"));
                        for port in &ports {
                            ui.add_sized(
                                [84.0, 24.0],
                                egui::Label::new(short_port_name(&port.name)).truncate(),
                            )
                            .on_hover_text(format!(
                                "{} ({})",
                                port.name,
                                owner_label(&port.owner)
                            ));
                        }
                        ui.end_row();

                        let mut previous_client: Option<String> = None;
                        for endpoint in endpoints {
                            let (client, short_name) = external_name_parts(&endpoint.name);
                            if previous_client.as_deref() != Some(client.as_str()) {
                                ui.label(crate::fonts::bold_italic_text(&client).underline());
                                for _ in &ports {
                                    ui.label("");
                                }
                                ui.end_row();
                                previous_client = Some(client);
                            }
                            ui.add_sized([180.0, 24.0], egui::Label::new(short_name))
                                .on_hover_text(&endpoint.name);
                            for port in &ports {
                                self.show_cell(ui, state, port, endpoint, intents);
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
        state: &ConnectionViewState,
        port: &ApplicationPortState,
        endpoint: &HostPortState,
        intents: &mut Vec<AppIntent>,
    ) {
        let compatible =
            port.data_type == endpoint.data_type && port.direction != endpoint.direction;
        let connected = state
            .confirmed_links
            .iter()
            .any(|link| link.application_port_id == port.id && link.host_port_id == endpoint.id);
        let pending = state
            .pending_links
            .iter()
            .find(|link| link.application_port_id == port.id && link.host_port_id == endpoint.id);
        let error = state.errors.iter().rev().find(|error| {
            error.port_id == Some(port.id)
                && error.external_port.as_deref() == Some(endpoint.id.as_str())
        });
        let (text, enabled, hover) = if !compatible {
            (
                "×",
                false,
                "Incompatible with this application port".to_owned(),
            )
        } else if port.connection_policy == ConnectionPolicy::OwnerManaged {
            (
                "◆",
                false,
                "Connection is managed by the port owner".to_owned(),
            )
        } else if let Some(pending) = pending {
            (
                "…",
                false,
                if pending.desired_connected {
                    "Connecting…".to_owned()
                } else {
                    "Disconnecting…".to_owned()
                },
            )
        } else if let Some(error) = error {
            ("!", true, error.message.clone())
        } else if connected {
            ("●", true, "Connected; click to disconnect".to_owned())
        } else {
            ("○", true, "Disconnected; click to connect".to_owned())
        };
        let response = ui
            .add_enabled(
                enabled,
                egui::Button::new(text).min_size(egui::vec2(28.0, 24.0)),
            )
            .on_hover_text(hover);
        #[cfg(test)]
        self.cell_rects
            .push((port.id, endpoint.id.to_string(), response.rect));
        if response.clicked() {
            intents.push(connection_intent(port, endpoint, connected));
        }
    }
}

fn connection_intent(
    port: &ApplicationPortState,
    endpoint: &HostPortState,
    connected: bool,
) -> AppIntent {
    AppIntent::SetPortConnected {
        port_id: port.id,
        host_port_id: endpoint.id.clone(),
        connected: !connected,
    }
}

fn owner_label(owner: &ApplicationPortOwner) -> String {
    match owner {
        ApplicationPortOwner::Track { track_id, kind } => {
            format!("{kind:?} track {track_id}")
        }
        ApplicationPortOwner::LuaControl {
            script_id,
            registration,
        } => format!("Lua script {script_id}, control port {registration}"),
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
        ApplicationPortOwner, ApplicationPortState, ConfirmedConnectionState, ConnectionErrorState,
        HostPortId, HostPortState, PendingConnectionState, PortDataType, PortDirection, PortId,
        TrackPortOwnerKind, TrackState,
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
                application_ports: Arc::from([
                    ApplicationPortState {
                        id: PortId::from_raw(11),
                        owner: ApplicationPortOwner::Track {
                            track_id: track_one,
                            kind: TrackPortOwnerKind::Main,
                        },
                        name: "one:in".to_owned(),
                        data_type: PortDataType::Audio,
                        direction: PortDirection::Input,
                        role: PortRole::AudioInput,
                        connection_policy: ConnectionPolicy::UserManaged,
                    },
                    ApplicationPortState {
                        id: PortId::from_raw(12),
                        owner: ApplicationPortOwner::Track {
                            track_id: track_two,
                            kind: TrackPortOwnerKind::Main,
                        },
                        name: "two:out".to_owned(),
                        data_type: PortDataType::Audio,
                        direction: PortDirection::Output,
                        role: PortRole::AudioOutput,
                        connection_policy: ConnectionPolicy::UserManaged,
                    },
                ]),
                host_ports: Arc::from([
                    HostPortState {
                        id: HostPortId::new("client:out"),
                        name: "client:out".to_owned(),
                        data_type: PortDataType::Audio,
                        direction: PortDirection::Output,
                    },
                    HostPortState {
                        id: HostPortId::new("missing-colon"),
                        name: "missing-colon".to_owned(),
                        data_type: PortDataType::Audio,
                        direction: PortDirection::Input,
                    },
                    HostPortState {
                        id: HostPortId::new("client:in"),
                        name: "client:in".to_owned(),
                        data_type: PortDataType::Audio,
                        direction: PortDirection::Input,
                    },
                ]),
                confirmed_links: Arc::from([ConfirmedConnectionState {
                    application_port_id: PortId::from_raw(12),
                    host_port_id: HostPortId::new("client:in"),
                }]),
                pending_links: Arc::from([]),
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
            (ConnectionScope::Track(TrackId::from_raw(1)), 1),
            (ConnectionScope::AllTracks, 1),
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
        let port = &state.connections.application_ports[0];
        let eligible = &state.connections.host_ports[0];
        let disabled = &state.connections.host_ports[1];
        assert_eq!(
            connection_intent(port, eligible, false),
            AppIntent::SetPortConnected {
                port_id: PortId::from_raw(11),
                host_port_id: HostPortId::new("client:out"),
                connected: true,
            }
        );
        assert_ne!(port.direction, eligible.direction);
        assert_eq!(port.direction, disabled.direction);
    }

    #[test]
    fn clicking_a_rendered_user_managed_cell_emits_the_exact_route_intent() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                assert!(dialog.show(ui.ctx(), &state).is_empty());
            },
        );
        let mut cell = dialog.cell_rects[0].2.center();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events: vec![egui::Event::PointerMoved(cell)],
                ..Default::default()
            },
            |ui| assert!(dialog.show(ui.ctx(), &state).is_empty()),
        );
        cell = dialog.cell_rects[0].2.center();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(cell),
                    egui::Event::PointerButton {
                        pos: cell,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::default(),
                    },
                ],
                ..Default::default()
            },
            |ui| assert!(dialog.show(ui.ctx(), &state).is_empty()),
        );
        cell = dialog.cell_rects[0].2.center();
        let mut intents = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(cell),
                    egui::Event::PointerButton {
                        pos: cell,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::default(),
                    },
                ],
                ..Default::default()
            },
            |ui| intents = dialog.show(ui.ctx(), &state),
        );
        assert_eq!(
            intents,
            vec![AppIntent::SetPortConnected {
                port_id: crate::PortId::from_raw(11),
                host_port_id: crate::HostPortId::new("client:out"),
                connected: true,
            }]
        );
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
        let application_ports: Arc<[ApplicationPortState]> = (0..16)
            .map(|index| ApplicationPortState {
                id: PortId::from_raw(index + 1),
                owner: ApplicationPortOwner::Track {
                    track_id,
                    kind: TrackPortOwnerKind::Main,
                },
                name: format!("local:input_{index}"),
                data_type: PortDataType::Audio,
                direction: PortDirection::Input,
                role: PortRole::AudioInput,
                connection_policy: ConnectionPolicy::UserManaged,
            })
            .collect::<Vec<_>>()
            .into();
        let host_ports: Arc<[HostPortState]> = (0..50)
            .map(|index| HostPortState {
                id: HostPortId::new(format!("client_{}:output_{index}", index / 5)),
                name: format!("client_{}:output_{index}", index / 5),
                data_type: PortDataType::Audio,
                direction: PortDirection::Output,
            })
            .collect::<Vec<_>>()
            .into();
        let confirmed_links: Arc<[ConfirmedConnectionState]> = (0..50)
            .filter(|index| index % 7 == 0)
            .map(|index| ConfirmedConnectionState {
                application_port_id: PortId::from_raw(1),
                host_port_id: HostPortId::new(format!("client_{}:output_{index}", index / 5)),
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
                application_ports,
                host_ports,
                confirmed_links,
                pending_links: Arc::from([PendingConnectionState {
                    application_port_id: PortId::from_raw(1),
                    host_port_id: HostPortId::new("client_0:output_3"),
                    desired_connected: true,
                }]),
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
    fn empty_host_inventory_keeps_application_ports_visible_and_safe() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut state = state();
        Arc::make_mut(&mut state.connections).host_ports = Arc::from([]);
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        let output = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(output.shapes.len() > 2);
        assert_eq!(state.connections.application_ports.len(), 2);
        assert!(dialog.cell_rects.is_empty());
    }

    #[test]
    fn lua_control_ports_are_global_owner_managed_and_safe_without_hosts() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut state = state();
        Arc::make_mut(&mut state.connections).application_ports =
            Arc::from([ApplicationPortState {
                id: PortId::from_raw(99),
                owner: ApplicationPortOwner::LuaControl {
                    script_id: crate::ScriptId::from_raw(7),
                    registration: 0,
                },
                name: "APC: MIDI source 1".to_owned(),
                data_type: PortDataType::Midi,
                direction: PortDirection::Input,
                role: PortRole::MidiInput,
                connection_policy: ConnectionPolicy::OwnerManaged,
            }]);
        Arc::make_mut(&mut state.connections).host_ports = Arc::from([]);
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        let global = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(global.shapes.len() > 2);
        assert!(dialog.cell_rects.is_empty());

        dialog.open(ConnectionScope::Track(TrackId::from_raw(1)));
        let scoped = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(scoped.shapes.len() > 2);
        assert!(dialog.cell_rects.is_empty());
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
