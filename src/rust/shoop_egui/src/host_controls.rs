use crate::colors;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostConnectionState {
    Connecting,
    Synchronizing,
    Ready,
    Lost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostLocation {
    InProcess,
    LocalSocket,
    RemoteSocket,
    SharedMemory,
}

#[derive(Clone, Debug)]
pub struct HostUiState {
    pub name: String,
    pub endpoint: Option<String>,
    pub location: HostLocation,
    pub connection: HostConnectionState,
    pub error: Option<String>,
    pub shutdown_pending: bool,
    pub import_enabled: bool,
}

impl HostUiState {
    pub fn session_controls_enabled(&self) -> bool {
        self.connection == HostConnectionState::Ready && !self.shutdown_pending
    }

    fn independent(&self) -> bool {
        self.location != HostLocation::InProcess
    }

    fn can_reconnect(&self) -> bool {
        self.connection == HostConnectionState::Lost
            && matches!(
                self.location,
                HostLocation::LocalSocket | HostLocation::RemoteSocket
            )
            && !self.shutdown_pending
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostAction {
    OpenManager,
    ImportProfile,
    OpenSettings,
    CloseUi,
    CloseUiAndHost,
    Reconnect,
}

#[derive(Default)]
pub(crate) struct HostControls {
    pub state: Option<HostUiState>,
    pub actions: Vec<HostAction>,
    #[cfg(test)]
    pub status_rect: Option<egui::Rect>,
    #[cfg(test)]
    pub action_rects: Vec<(HostAction, egui::Rect)>,
    #[cfg(test)]
    pub menu_rect: Option<egui::Rect>,
}

impl HostControls {
    pub fn session_controls_enabled(&self) -> bool {
        self.state
            .as_ref()
            .is_none_or(HostUiState::session_controls_enabled)
    }

    pub fn audio_state_known(&self) -> bool {
        self.state
            .as_ref()
            .is_none_or(|state| state.connection == HostConnectionState::Ready)
    }

    fn button(&mut self, ui: &mut egui::Ui, label: &str, action: HostAction, enabled: bool) {
        let response = ui.add_enabled(enabled, egui::Button::new(label));
        #[cfg(test)]
        self.action_rects.push((action, response.rect));
        if response.clicked() {
            self.actions.push(action);
            ui.close();
        }
    }

    pub fn show_menu(&mut self, ui: &mut egui::Ui) {
        let Some(state) = self.state.clone() else {
            return;
        };
        ui.separator();
        let _menu = ui.menu_button("Host", |ui| {
            self.show_shortcuts(ui);
            ui.menu_button("Advanced", |ui| {
                self.button(
                    ui,
                    "Import host profile…",
                    HostAction::ImportProfile,
                    state.import_enabled,
                );
            });
        });
        #[cfg(test)]
        {
            self.menu_rect = Some(_menu.response.rect);
        }
    }

    pub fn show_exit_menu(&mut self, ui: &mut egui::Ui) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        if state.independent() {
            let stop_enabled = !state.shutdown_pending;
            ui.separator();
            self.button(
                ui,
                "Close UI — keep host running",
                HostAction::CloseUi,
                true,
            );
            self.button(
                ui,
                "Close UI and stop host…",
                HostAction::CloseUiAndHost,
                stop_enabled,
            );
        }
    }

    fn show_shortcuts(&mut self, ui: &mut egui::Ui) {
        self.button(ui, "Host Manager…", HostAction::OpenManager, true);
        if self.state.as_ref().is_some_and(HostUiState::independent) {
            self.button(
                ui,
                "Connected host settings…",
                HostAction::OpenSettings,
                true,
            );
        }
    }

    fn show_details(&self, ui: &mut egui::Ui) {
        if let Some(state) = &self.state {
            ui.label(format!("Host: {}", state.name));
            ui.label(match state.location {
                HostLocation::InProcess => "Location: in application process",
                HostLocation::LocalSocket => "Location: local independent host (socket)",
                HostLocation::RemoteSocket => "Location: remote independent host (socket)",
                HostLocation::SharedMemory => "Location: local independent host (shared memory)",
            });
            if let Some(endpoint) = &state.endpoint {
                ui.label(format!("Endpoint: {endpoint}"));
            }
            ui.label(match state.connection {
                HostConnectionState::Ready => "Connection: ready",
                HostConnectionState::Connecting => "Connection: connecting",
                HostConnectionState::Synchronizing => "Connection: synchronizing",
                HostConnectionState::Lost => "Connection lost; host may still be running",
            });
            ui.label(if state.independent() {
                "Closing only the UI keeps host playback and recording running."
            } else {
                "Audio stops when the application closes."
            });
            if state.location == HostLocation::SharedMemory {
                ui.label("Shared-memory mode cannot reattach after this UI exits.");
            }
            if let Some(error) = &state.error {
                ui.label(error);
            }
        } else {
            ui.label("Audio runs in this browser page and stops when it closes.");
        }
    }

    pub fn show_status(
        &mut self,
        ui: &mut egui::Ui,
        audio_color: egui::Color32,
        audio_details: impl Fn(&mut egui::Ui),
    ) {
        let known = self.audio_state_known();
        let response = ui
            .add_sized([28.0, 22.0], egui::Button::new(""))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                ui.is_enabled(),
                "Backend and host status",
            )
        });
        ui.painter().circle_filled(
            response.rect.center(),
            4.0,
            if known { audio_color } else { colors::WARNING },
        );
        #[cfg(test)]
        {
            self.status_rect = Some(response.rect);
        }
        let details = |ui: &mut egui::Ui, controls: &Self| {
            controls.show_details(ui);
            ui.separator();
            if known {
                audio_details(ui);
            } else {
                ui.label("Audio state: unknown while disconnected or synchronizing");
            }
        };
        response.clone().on_hover_ui(|ui| {
            details(ui, self);
            ui.label("Click for backend and host details");
        });
        egui::Popup::from_toggle_button_response(&response)
            .id(ui.make_persistent_id("backend_status_popup"))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_max_width(420.0);
                details(ui, self);
                if self.state.is_some() {
                    ui.separator();
                    self.show_shortcuts(ui);
                    self.show_reconnect(ui);
                }
            });
    }

    pub fn show_reconnect(&mut self, ui: &mut egui::Ui) {
        if self.state.as_ref().is_some_and(HostUiState::can_reconnect) {
            self.button(ui, "Reconnect to host", HostAction::Reconnect, true);
        }
    }

    pub fn show_connection_message(&mut self, ui: &mut egui::Ui) {
        let Some(state) = &self.state else { return };
        if state.shutdown_pending {
            ui.label("Waiting for host reset and cleanup…");
        } else {
            match state.connection {
                HostConnectionState::Lost => {
                    ui.heading("Host connection lost");
                    ui.label("The host may still be playing and recording. Reconnect to read its current state.");
                    if let Some(error) = &state.error {
                        ui.label(error);
                    }
                    self.show_reconnect(ui);
                }
                HostConnectionState::Synchronizing => {
                    ui.spinner();
                    ui.label("Receiving a coherent session snapshot…");
                }
                HostConnectionState::Connecting => {
                    ui.spinner();
                    ui.label("Attaching to session host…");
                }
                HostConnectionState::Ready => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(context: &egui::Context, host: &mut HostControls, events: Vec<egui::Event>) -> String {
        host.actions.clear();
        host.action_rects.clear();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                host.show_status(ui, colors::SUCCESS, |ui| {
                    ui.label("Driver status: Running");
                })
            },
        );
        fn text(shape: &egui::Shape, result: &mut String) {
            match shape {
                egui::Shape::Text(text) => {
                    result.push_str(text.galley.text());
                    result.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        text(shape, result);
                    }
                }
                _ => {}
            }
        }
        let mut result = String::new();
        for shape in &output.shapes {
            text(&shape.shape, &mut result);
        }
        output.textures_delta.clear();
        result
    }

    fn click(context: &egui::Context, host: &mut HostControls, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                context,
                host,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn status_popover_stays_open_and_reports_unknown_audio_until_connected() {
        for location in [
            HostLocation::InProcess,
            HostLocation::LocalSocket,
            HostLocation::RemoteSocket,
            HostLocation::SharedMemory,
        ] {
            let context = egui::Context::default();
            crate::initialize(&context);
            let mut host = HostControls {
                state: Some(HostUiState {
                    name: "Stage host".into(),
                    endpoint: Some("127.0.0.1:4321".into()),
                    location,
                    connection: HostConnectionState::Lost,
                    error: Some("Heartbeat timed out".into()),
                    shutdown_pending: false,
                    import_enabled: true,
                }),
                ..Default::default()
            };
            frame(&context, &mut host, Vec::new());
            let status = host.status_rect.unwrap();
            assert!(status.width() >= 28.0 && status.height() >= 22.0);
            click(&context, &mut host, status.center());
            let text = frame(
                &context,
                &mut host,
                vec![egui::Event::PointerMoved(egui::pos2(850.0, 550.0))],
            );
            assert!(text.contains("Host: Stage host"));
            assert!(text.contains("Connection lost; host may still be running"));
            assert!(text.contains("Audio state: unknown"));
            assert!(!text.contains("Driver status: Running"));
            assert!(text.contains("Heartbeat timed out"));
            assert_eq!(
                text.contains("Connected host settings…"),
                location != HostLocation::InProcess
            );
            assert_eq!(
                text.contains("cannot reattach"),
                location == HostLocation::SharedMemory
            );
            let reconnect = host
                .action_rects
                .iter()
                .find(|(action, _)| *action == HostAction::Reconnect)
                .map(|(_, rect)| rect.center());
            assert_eq!(
                reconnect.is_some(),
                matches!(
                    location,
                    HostLocation::LocalSocket | HostLocation::RemoteSocket
                )
            );
            if let Some(reconnect) = reconnect {
                click(&context, &mut host, reconnect);
                assert_eq!(host.actions, [HostAction::Reconnect]);
                click(&context, &mut host, status.center());
            }
            host.state.as_mut().unwrap().connection = HostConnectionState::Ready;
            let text = frame(&context, &mut host, Vec::new());
            assert!(text.contains("Connection: ready"));
            assert!(text.contains("Driver status: Running"));
            assert!(!text.contains("Audio state: unknown"));
            let manager = host
                .action_rects
                .iter()
                .find(|(action, _)| *action == HostAction::OpenManager)
                .unwrap()
                .1
                .center();
            click(&context, &mut host, manager);
            assert_eq!(host.actions, [HostAction::OpenManager]);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn browser_status_has_audio_details_without_native_host_actions() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut host = HostControls::default();
        frame(&context, &mut host, Vec::new());
        let status = host.status_rect.unwrap().center();
        click(&context, &mut host, status);
        let text = frame(&context, &mut host, Vec::new());
        assert!(text.contains("Audio runs in this browser page"));
        assert!(text.contains("Driver status: Running"));
        assert!(host.action_rects.is_empty());
    }
}
