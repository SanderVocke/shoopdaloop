use std::collections::BTreeMap;

use crate::{LoopId, LoopWidget, LoopWidgetAction, TrackControls, TrackState, TrackWidgetAction};
use egui_material_icons::icons::{ICON_ADD, ICON_MORE_VERT};

#[derive(Debug, Default)]
pub struct TrackWidgetResponse {
    pub actions: Vec<TrackWidgetAction>,
    pub loop_actions: Vec<(LoopId, LoopWidgetAction)>,
    pub add_loop_requested: bool,
    pub connections_requested: bool,
}

#[derive(Debug, Default)]
pub struct TrackWidget {
    name_edit: String,
    source_name: String,
    loop_widgets: BTreeMap<LoopId, LoopWidget>,
    controls: TrackControls,
    #[cfg(test)]
    test_loop_rects: Vec<egui::Rect>,
    #[cfg(test)]
    test_options_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_connections_rect: Option<egui::Rect>,
}

impl TrackWidget {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &TrackState) -> TrackWidgetResponse {
        let mut response = self.show_content(ui, state, !state.is_sync);
        response
            .actions
            .extend(self.show_controls(ui, &state.controls));
        response
    }

    pub fn show_content(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        show_add_loop: bool,
    ) -> TrackWidgetResponse {
        self.loop_widgets
            .retain(|id, _| state.loops.iter().any(|loop_state| loop_state.id == *id));
        let mut result = TrackWidgetResponse::default();
        #[cfg(test)]
        self.test_loop_rects.clear();
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(85, 85, 85))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(180.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    self.show_header(ui, state, &mut result);
                    ui.add_space(2.0);
                    for loop_state in &state.loops {
                        let widget = self.loop_widgets.entry(loop_state.id).or_default();
                        let _loop_response = ui.push_id(loop_state.id, |ui| {
                            let size = egui::vec2(ui.available_width(), 26.0);
                            let response = widget.show(ui, loop_state, size);
                            result.loop_actions.extend(
                                response
                                    .actions
                                    .into_iter()
                                    .map(|action| (loop_state.id, action)),
                            );
                        });
                        #[cfg(test)]
                        self.test_loop_rects.push(_loop_response.response.rect);
                        ui.add_space(2.0);
                    }
                    if show_add_loop {
                        let response = ui
                            .add_sized(
                                [ui.available_width(), 28.0],
                                egui::Button::new(ICON_ADD.rich_text().size(18.0)),
                            )
                            .on_hover_text("Add a loop row");
                        result.add_loop_requested = response.clicked();
                    }
                });
            });
        result
    }

    pub fn show_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &crate::TrackControlState,
    ) -> Vec<TrackWidgetAction> {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(85, 85, 85))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(180.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    self.controls.show(ui, state)
                })
                .inner
            })
            .inner
    }

    fn show_header(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        result: &mut TrackWidgetResponse,
    ) {
        ui.horizontal(|ui| {
            if state.is_sync {
                ui.add_sized(
                    [(ui.available_width() - 22.0).max(40.0), 24.0],
                    egui::Label::new(&state.name),
                );
            } else {
                let name_id = ui.make_persistent_id("name");
                let has_focus = ui.memory(|memory| memory.has_focus(name_id));
                if !has_focus && self.source_name != state.name {
                    self.name_edit.clone_from(&state.name);
                    self.source_name.clone_from(&state.name);
                }
                let available = (ui.available_width() - 22.0).max(40.0);
                let response = ui.add_sized(
                    [available, 24.0],
                    egui::TextEdit::singleline(&mut self.name_edit).id(name_id),
                );
                if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    response.surrender_focus();
                }
                if response.lost_focus() && self.name_edit != state.name {
                    self.source_name.clone_from(&self.name_edit);
                    result
                        .actions
                        .push(TrackWidgetAction::NameChanged(self.name_edit.clone()));
                }
            }
            let menu = ui.menu_button(ICON_MORE_VERT.rich_text().size(17.0), |ui| {
                let connections = ui.button("Connections...");
                #[cfg(test)]
                {
                    self.test_connections_rect = Some(connections.rect);
                }
                if connections.clicked() {
                    result.connections_requested = true;
                    ui.close();
                }
                if !state.is_sync {
                    ui.add_enabled(false, egui::Button::new("Delete Track"));
                }
            });
            #[cfg(test)]
            {
                self.test_options_rect = Some(menu.response.rect);
            }
            menu.response.on_hover_text("Track options");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LoopState, TrackId};

    fn frame(
        context: &egui::Context,
        widget: &mut TrackWidget,
        state: &TrackState,
        events: Vec<egui::Event>,
    ) -> TrackWidgetResponse {
        let mut response = TrackWidgetResponse::default();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500.0, 400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| response = widget.show_content(ui, state, false),
        );
        response
    }

    fn click(
        context: &egui::Context,
        widget: &mut TrackWidget,
        state: &TrackState,
        position: egui::Pos2,
    ) -> TrackWidgetResponse {
        let _ = frame(
            context,
            widget,
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
            widget,
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
    fn track_and_sync_options_menus_request_connection_scope() {
        let context = egui::Context::default();
        crate::initialize(&context);
        for is_sync in [false, true] {
            let state = TrackState {
                id: TrackId::from_raw(if is_sync { 1 } else { 2 }),
                name: if is_sync { "Sync" } else { "Track" }.to_owned(),
                is_sync,
                ..Default::default()
            };
            let mut widget = TrackWidget::default();
            let _ = frame(&context, &mut widget, &state, Vec::new());
            let options = widget.test_options_rect.unwrap().center();
            assert!(!click(&context, &mut widget, &state, options).connections_requested);
            let _ = frame(&context, &mut widget, &state, Vec::new());
            let connections = widget.test_connections_rect.unwrap().center();
            assert!(click(&context, &mut widget, &state, connections).connections_requested);
        }
    }

    #[test]
    fn loops_stack_vertically_inside_a_horizontal_track_row() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackState {
            id: TrackId::from_raw(1),
            name: "Track".to_owned(),
            loops: (1..=3)
                .map(|id| LoopState {
                    id: LoopId::from_raw(id),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut widget = TrackWidget::default();

        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500.0, 400.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.horizontal_top(|ui| widget.show_content(ui, &state, true));
            },
        );

        assert_eq!(widget.test_loop_rects.len(), state.loops.len());
        for pair in widget.test_loop_rects.windows(2) {
            assert!((pair[0].left() - pair[1].left()).abs() < f32::EPSILON);
            assert!(pair[1].top() >= pair[0].bottom());
        }
    }
}
