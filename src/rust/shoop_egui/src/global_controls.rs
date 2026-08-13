use egui_material_icons::icons::{
    ICON_ARROW_DOWNWARD, ICON_BORDER_CLEAR, ICON_DELETE, ICON_FIBER_MANUAL_RECORD, ICON_HEARING,
    ICON_HOURGLASS_EMPTY, ICON_MENU, ICON_PLAY_ARROW, ICON_STOP,
};

use crate::{
    colors, optimistic_value::OptimisticValue, DefaultRecordingAction, GlobalControlAction,
    GlobalControlState,
};

const CONTROL_BUTTON_SIZE: [f32; 2] = [34.0, 28.0];

#[derive(Debug, Default)]
pub struct GlobalControls {
    connections_requested: bool,
    save_session_requested: bool,
    load_session_requested: bool,
    settings_requested: bool,
    apply_n_cycles: OptimisticValue<u32>,
    apply_n_cycles_dragging: bool,
    #[cfg(test)]
    test_rects: TestGlobalControlRects,
}

#[derive(Clone, Copy, Debug)]
enum TestGlobalControl {
    MainMenu,
    Connections,
    SaveSession,
    LoadSession,
    Settings,
    StopAll,
    DeselectAll,
    Clear,
    ClearRecordings,
    DefaultRecordingAction,
    PlayAfterRecord,
    Sync,
    Solo,
    AutoMuteOtherTrackInputs,
    ApplyNCycles,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TestGlobalControlRects {
    main_menu: Option<egui::Rect>,
    connections: Option<egui::Rect>,
    save_session: Option<egui::Rect>,
    load_session: Option<egui::Rect>,
    settings: Option<egui::Rect>,
    stop_all: Option<egui::Rect>,
    deselect_all: Option<egui::Rect>,
    clear: Option<egui::Rect>,
    clear_recordings: Option<egui::Rect>,
    default_recording_action: Option<egui::Rect>,
    play_after_record: Option<egui::Rect>,
    sync: Option<egui::Rect>,
    solo: Option<egui::Rect>,
    auto_mute_other_track_inputs: Option<egui::Rect>,
    apply_n_cycles: Option<egui::Rect>,
}

impl GlobalControls {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &GlobalControlState,
    ) -> Vec<GlobalControlAction> {
        self.connections_requested = false;
        self.save_session_requested = false;
        self.load_session_requested = false;
        self.settings_requested = false;
        let mut actions = Vec::new();
        ui.horizontal(|ui| {
            let response = ui
                .menu_button(ICON_MENU.rich_text().size(20.0), |ui| {
                    let connections = ui.button("Connections");
                    self.record_rect(TestGlobalControl::Connections, &connections);
                    if connections.clicked() {
                        self.connections_requested = true;
                        ui.close();
                    }
                    ui.separator();
                    let save_session = ui.button("Save session…");
                    self.record_rect(TestGlobalControl::SaveSession, &save_session);
                    if save_session.clicked() {
                        self.save_session_requested = true;
                        ui.close();
                    }
                    let load_session = ui.button("Load session…");
                    self.record_rect(TestGlobalControl::LoadSession, &load_session);
                    if load_session.clicked() {
                        self.load_session_requested = true;
                        ui.close();
                    }
                    let settings = ui.button("Settings");
                    self.record_rect(TestGlobalControl::Settings, &settings);
                    if settings.clicked() {
                        self.settings_requested = true;
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Main menu");
            self.record_rect(TestGlobalControl::MainMenu, &response);
            ui.separator();

            let response = icon_button(ui, ICON_STOP, "Stop all loops");
            self.record_rect(TestGlobalControl::StopAll, &response);
            if response.clicked() {
                actions.push(GlobalControlAction::StopAll);
            }
            let response = icon_button(ui, ICON_BORDER_CLEAR, "Deselect all loops");
            self.record_rect(TestGlobalControl::DeselectAll, &response);
            if response.clicked() {
                actions.push(GlobalControlAction::DeselectAll);
            }
            let response = ui
                .menu_button(ICON_DELETE.rich_text().size(20.0), |ui| {
                    let response = ui.button("Clear recordings");
                    self.record_rect(TestGlobalControl::ClearRecordings, &response);
                    if response.clicked() {
                        actions.push(GlobalControlAction::ClearRecordings { include_sync: true });
                        ui.close();
                    }
                    if ui.button("Clear recordings except sync").clicked() {
                        actions.push(GlobalControlAction::ClearRecordings {
                            include_sync: false,
                        });
                        ui.close();
                    }
                    if ui.button("Clear all").clicked() {
                        actions.push(GlobalControlAction::ClearAll { include_sync: true });
                        ui.close();
                    }
                    if ui.button("Clear all except sync").clicked() {
                        actions.push(GlobalControlAction::ClearAll {
                            include_sync: false,
                        });
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Clear multiple loops");
            self.record_rect(TestGlobalControl::Clear, &response);

            ui.separator();
            let recording_is_record =
                state.default_recording_action == DefaultRecordingAction::Record;
            let recording_icons = if recording_is_record {
                [
                    LayeredIcon {
                        icon: ICON_ARROW_DOWNWARD,
                        offset: egui::vec2(-4.0, 3.0),
                        color: colors::MUTED_FOREGROUND,
                        size: 17.0,
                    },
                    LayeredIcon {
                        icon: ICON_FIBER_MANUAL_RECORD,
                        offset: egui::vec2(4.0, -3.0),
                        color: colors::RECORD_ACTION,
                        size: 20.0,
                    },
                ]
            } else {
                [
                    LayeredIcon {
                        icon: ICON_FIBER_MANUAL_RECORD,
                        offset: egui::vec2(4.0, -3.0),
                        color: colors::MUTED_FOREGROUND,
                        size: 17.0,
                    },
                    LayeredIcon {
                        icon: ICON_ARROW_DOWNWARD,
                        offset: egui::vec2(-4.0, 3.0),
                        color: colors::RECORD_ACTION,
                        size: 20.0,
                    },
                ]
            };
            let response = layered_icon_button(
                ui,
                recording_icons,
                "Default recording action: click to switch between record and grab",
            );
            self.record_rect(TestGlobalControl::DefaultRecordingAction, &response);
            if response.clicked() {
                let next = match state.default_recording_action {
                    DefaultRecordingAction::Record => DefaultRecordingAction::Grab,
                    DefaultRecordingAction::Grab => DefaultRecordingAction::Record,
                };
                actions.push(GlobalControlAction::SetDefaultRecordingAction(next));
            }

            let play_after_color = |enabled_color| {
                if state.play_after_record {
                    enabled_color
                } else {
                    colors::MUTED_FOREGROUND
                }
            };
            let response = layered_icon_button(
                ui,
                [
                    LayeredIcon {
                        icon: ICON_FIBER_MANUAL_RECORD,
                        offset: egui::vec2(-4.0, 3.0),
                        color: play_after_color(colors::RECORD_ACTION),
                        size: 18.0,
                    },
                    LayeredIcon {
                        icon: ICON_PLAY_ARROW,
                        offset: egui::vec2(4.0, -3.0),
                        color: play_after_color(colors::PLAYING_STATE),
                        size: 20.0,
                    },
                ],
                "Play after recording",
            );
            self.record_rect(TestGlobalControl::PlayAfterRecord, &response);
            if response.clicked() {
                actions.push(GlobalControlAction::SetPlayAfterRecord(
                    !state.play_after_record,
                ));
            }

            let response = control_button(
                ui,
                ICON_HOURGLASS_EMPTY
                    .rich_text()
                    .size(20.0)
                    .color(if state.sync {
                        colors::COLORED_HIGHLIGHT
                    } else {
                        colors::MUTED_FOREGROUND
                    }),
                "Synchronized actions",
            );
            self.record_rect(TestGlobalControl::Sync, &response);
            if response.clicked() {
                actions.push(GlobalControlAction::SetSync(!state.sync));
            }
            let response = control_button(
                ui,
                egui::RichText::new("S").size(18.0).color(if state.solo {
                    colors::COLORED_HIGHLIGHT
                } else {
                    colors::MUTED_FOREGROUND
                }),
                if state.solo {
                    "Play and stop others in same track(s)"
                } else {
                    "Allow multiple loops to play in same track(s)"
                },
            );
            self.record_rect(TestGlobalControl::Solo, &response);
            if response.clicked() {
                actions.push(GlobalControlAction::SetSolo(!state.solo));
            }
            let input_color = if state.auto_mute_other_track_inputs {
                colors::COLORED_HIGHLIGHT
            } else {
                colors::MUTED_FOREGROUND
            };
            let response = exclusive_input_button(
                ui,
                input_color,
                "One track at a time: mute other track inputs when enabling monitoring",
            );
            self.record_rect(TestGlobalControl::AutoMuteOtherTrackInputs, &response);
            if response.clicked() {
                actions.push(GlobalControlAction::SetAutoMuteOtherTrackInputs(
                    !state.auto_mute_other_track_inputs,
                ));
            }

            let mut cycles = self
                .apply_n_cycles
                .resolve(state.apply_n_cycles, self.apply_n_cycles_dragging);
            let mut step_changed = false;
            let framed = egui::Frame::new()
                .fill(colors::CONTROL_BACKGROUND)
                .stroke(egui::Stroke::new(1.0, colors::MUTED_FOREGROUND))
                .corner_radius(3)
                .inner_margin(egui::Margin::symmetric(4, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("−").clicked() {
                            cycles = cycles.saturating_sub(1);
                            step_changed = true;
                        }
                        let value = ui
                            .add(
                                egui::DragValue::new(&mut cycles)
                                    .range(0..=i32::MAX as u32)
                                    .custom_formatter(cycles_text),
                            )
                            .on_hover_text(
                                "Recording length in sync cycles; infinity means unlimited",
                            );
                        if ui.small_button("+").clicked() {
                            cycles = cycles.saturating_add(1).min(i32::MAX as u32);
                            step_changed = true;
                        }
                        value
                    })
                    .inner
                });
            let response = framed.inner;
            self.record_rect(TestGlobalControl::ApplyNCycles, &framed.response);
            if response.drag_started() || response.dragged() {
                self.apply_n_cycles_dragging = true;
            }
            if response.changed() || step_changed {
                self.apply_n_cycles.set(cycles);
                actions.push(GlobalControlAction::SetApplyNCycles(cycles));
            }
            if response.drag_stopped() {
                self.apply_n_cycles_dragging = false;
            }
        });
        actions
    }

    pub fn take_connections_requested(&mut self) -> bool {
        std::mem::take(&mut self.connections_requested)
    }

    pub fn take_save_session_requested(&mut self) -> bool {
        std::mem::take(&mut self.save_session_requested)
    }

    pub fn take_load_session_requested(&mut self) -> bool {
        std::mem::take(&mut self.load_session_requested)
    }

    pub fn take_settings_requested(&mut self) -> bool {
        std::mem::take(&mut self.settings_requested)
    }

    #[cfg(test)]
    fn record_rect(&mut self, control: TestGlobalControl, response: &egui::Response) {
        let target = match control {
            TestGlobalControl::MainMenu => &mut self.test_rects.main_menu,
            TestGlobalControl::Connections => &mut self.test_rects.connections,
            TestGlobalControl::SaveSession => &mut self.test_rects.save_session,
            TestGlobalControl::LoadSession => &mut self.test_rects.load_session,
            TestGlobalControl::Settings => &mut self.test_rects.settings,
            TestGlobalControl::StopAll => &mut self.test_rects.stop_all,
            TestGlobalControl::DeselectAll => &mut self.test_rects.deselect_all,
            TestGlobalControl::Clear => &mut self.test_rects.clear,
            TestGlobalControl::ClearRecordings => &mut self.test_rects.clear_recordings,
            TestGlobalControl::DefaultRecordingAction => {
                &mut self.test_rects.default_recording_action
            }
            TestGlobalControl::PlayAfterRecord => &mut self.test_rects.play_after_record,
            TestGlobalControl::Sync => &mut self.test_rects.sync,
            TestGlobalControl::Solo => &mut self.test_rects.solo,
            TestGlobalControl::AutoMuteOtherTrackInputs => {
                &mut self.test_rects.auto_mute_other_track_inputs
            }
            TestGlobalControl::ApplyNCycles => &mut self.test_rects.apply_n_cycles,
        };
        *target = Some(response.rect);
    }

    #[cfg(not(test))]
    fn record_rect(&mut self, _control: TestGlobalControl, _response: &egui::Response) {}

    #[cfg(test)]
    fn test_rect(&self, control: TestGlobalControl) -> Option<egui::Rect> {
        match control {
            TestGlobalControl::MainMenu => self.test_rects.main_menu,
            TestGlobalControl::Connections => self.test_rects.connections,
            TestGlobalControl::SaveSession => self.test_rects.save_session,
            TestGlobalControl::LoadSession => self.test_rects.load_session,
            TestGlobalControl::Settings => self.test_rects.settings,
            TestGlobalControl::StopAll => self.test_rects.stop_all,
            TestGlobalControl::DeselectAll => self.test_rects.deselect_all,
            TestGlobalControl::Clear => self.test_rects.clear,
            TestGlobalControl::ClearRecordings => self.test_rects.clear_recordings,
            TestGlobalControl::DefaultRecordingAction => self.test_rects.default_recording_action,
            TestGlobalControl::PlayAfterRecord => self.test_rects.play_after_record,
            TestGlobalControl::Sync => self.test_rects.sync,
            TestGlobalControl::Solo => self.test_rects.solo,
            TestGlobalControl::AutoMuteOtherTrackInputs => {
                self.test_rects.auto_mute_other_track_inputs
            }
            TestGlobalControl::ApplyNCycles => self.test_rects.apply_n_cycles,
        }
    }
}

#[derive(Clone, Copy)]
struct LayeredIcon {
    icon: egui_material_icons::MaterialIcon,
    offset: egui::Vec2,
    color: egui::Color32,
    size: f32,
}

fn layered_icon_button(
    ui: &mut egui::Ui,
    icons: [LayeredIcon; 2],
    tooltip: &str,
) -> egui::Response {
    let response = ui.add_sized(CONTROL_BUTTON_SIZE, egui::Button::new(""));
    for layer in icons {
        ui.painter().text(
            response.rect.center() + layer.offset,
            egui::Align2::CENTER_CENTER,
            layer.icon.codepoint,
            egui::FontId::new(layer.size, layer.icon.font_family()),
            layer.color,
        );
    }
    response.on_hover_text(tooltip)
}

fn exclusive_input_button(
    ui: &mut egui::Ui,
    color: egui::Color32,
    tooltip: &str,
) -> egui::Response {
    let response = ui.add_sized(CONTROL_BUTTON_SIZE, egui::Button::new(""));
    let center = response.rect.center();
    ui.painter().text(
        center + egui::vec2(-2.0, -1.0),
        egui::Align2::CENTER_CENTER,
        ICON_HEARING.codepoint,
        egui::FontId::new(20.0, ICON_HEARING.font_family()),
        color,
    );
    ui.painter().text(
        center + egui::vec2(7.0, 4.0),
        egui::Align2::CENTER_CENTER,
        "1",
        egui::FontId::proportional(11.0),
        color,
    );
    response.on_hover_text(tooltip)
}

fn control_button(
    ui: &mut egui::Ui,
    content: impl Into<egui::WidgetText>,
    tooltip: &str,
) -> egui::Response {
    ui.add_sized(CONTROL_BUTTON_SIZE, egui::Button::new(content))
        .on_hover_text(tooltip)
}

fn cycles_text(value: f64, _decimals: std::ops::RangeInclusive<usize>) -> String {
    if value.round() == 0.0 {
        "∞".to_owned()
    } else {
        format!("{value:.0}")
    }
}

fn icon_button(
    ui: &mut egui::Ui,
    icon: egui_material_icons::MaterialIcon,
    tooltip: &str,
) -> egui::Response {
    ui.add(egui::Button::new(icon.rich_text().size(20.0)))
        .on_hover_text(tooltip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        context: &egui::Context,
        controls: &mut GlobalControls,
        state: &GlobalControlState,
        events: Vec<egui::Event>,
    ) -> Vec<GlobalControlAction> {
        let mut actions = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 100.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = controls.show(ui, state),
        );
        actions
    }

    fn click(
        context: &egui::Context,
        controls: &mut GlobalControls,
        state: &GlobalControlState,
        control: TestGlobalControl,
    ) -> Vec<GlobalControlAction> {
        frame(context, controls, state, Vec::new());
        let position = controls.test_rect(control).unwrap().center();
        frame(
            context,
            controls,
            state,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            context,
            controls,
            state,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
    }

    #[test]
    fn zero_cycles_are_displayed_as_infinity() {
        assert_eq!(cycles_text(0.0, 0..=0), "∞");
        assert_eq!(cycles_text(3.0, 0..=0), "3");
    }

    #[test]
    fn mode_controls_share_one_hover_target_size() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = GlobalControlState::default();
        let mut controls = GlobalControls::default();
        frame(&context, &mut controls, &state, Vec::new());

        assert_eq!(
            controls
                .test_rect(TestGlobalControl::ApplyNCycles)
                .unwrap()
                .height(),
            CONTROL_BUTTON_SIZE[1]
        );
        for control in [
            TestGlobalControl::DefaultRecordingAction,
            TestGlobalControl::PlayAfterRecord,
            TestGlobalControl::Sync,
            TestGlobalControl::Solo,
            TestGlobalControl::AutoMuteOtherTrackInputs,
        ] {
            assert_eq!(
                controls.test_rect(control).unwrap().size(),
                egui::vec2(CONTROL_BUTTON_SIZE[0], CONTROL_BUTTON_SIZE[1])
            );
        }
    }

    #[test]
    fn buttons_generate_typed_global_actions_and_main_menu_has_no_business_intent() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = GlobalControlState::default();
        let mut controls = GlobalControls::default();

        assert!(click(&context, &mut controls, &state, TestGlobalControl::MainMenu).is_empty());
        assert!(click(
            &context,
            &mut controls,
            &state,
            TestGlobalControl::Connections
        )
        .is_empty());
        assert!(controls.take_connections_requested());
        assert!(click(&context, &mut controls, &state, TestGlobalControl::MainMenu).is_empty());
        assert!(click(
            &context,
            &mut controls,
            &state,
            TestGlobalControl::SaveSession
        )
        .is_empty());
        assert!(controls.take_save_session_requested());
        assert!(click(&context, &mut controls, &state, TestGlobalControl::MainMenu).is_empty());
        assert!(click(
            &context,
            &mut controls,
            &state,
            TestGlobalControl::LoadSession
        )
        .is_empty());
        assert!(controls.take_load_session_requested());
        assert!(click(&context, &mut controls, &state, TestGlobalControl::MainMenu).is_empty());
        assert!(click(&context, &mut controls, &state, TestGlobalControl::Settings).is_empty());
        assert!(controls.take_settings_requested());
        assert_eq!(
            click(&context, &mut controls, &state, TestGlobalControl::StopAll),
            vec![GlobalControlAction::StopAll]
        );
        assert_eq!(
            click(
                &context,
                &mut controls,
                &state,
                TestGlobalControl::DeselectAll
            ),
            vec![GlobalControlAction::DeselectAll]
        );
        assert_eq!(
            click(
                &context,
                &mut controls,
                &state,
                TestGlobalControl::DefaultRecordingAction
            ),
            vec![GlobalControlAction::SetDefaultRecordingAction(
                DefaultRecordingAction::Grab
            )]
        );
        assert_eq!(
            click(
                &context,
                &mut controls,
                &state,
                TestGlobalControl::PlayAfterRecord
            ),
            vec![GlobalControlAction::SetPlayAfterRecord(false)]
        );
        assert_eq!(
            click(&context, &mut controls, &state, TestGlobalControl::Sync),
            vec![GlobalControlAction::SetSync(false)]
        );
        assert_eq!(
            click(&context, &mut controls, &state, TestGlobalControl::Solo),
            vec![GlobalControlAction::SetSolo(true)]
        );
    }

    #[test]
    fn clear_menu_generates_a_clear_action() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = GlobalControlState::default();
        let mut controls = GlobalControls::default();

        assert!(click(&context, &mut controls, &state, TestGlobalControl::Clear).is_empty());
        frame(&context, &mut controls, &state, Vec::new());
        assert_eq!(
            click(
                &context,
                &mut controls,
                &state,
                TestGlobalControl::ClearRecordings
            ),
            vec![GlobalControlAction::ClearRecordings { include_sync: true }]
        );
    }
}
