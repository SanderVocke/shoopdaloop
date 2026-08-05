use crate::{LoopId, LoopWidget, LoopWidgetAction, TrackControls, TrackState, TrackWidgetAction};
use egui_material_icons::icons::{ICON_ADD, ICON_MORE_VERT};

#[derive(Debug, Default)]
pub struct TrackWidgetResponse {
    pub actions: Vec<TrackWidgetAction>,
    pub loop_actions: Vec<(LoopId, LoopWidgetAction)>,
    pub add_loop_requested: bool,
}

#[derive(Debug, Default)]
pub struct TrackWidget {
    name_edit: String,
    source_name: String,
    loop_widgets: Vec<LoopWidget>,
    controls: TrackControls,
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
            .resize_with(state.loops.len(), LoopWidget::default);
        let mut result = TrackWidgetResponse::default();
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(85, 85, 85))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(180.0);
                self.show_header(ui, state, &mut result);
                ui.add_space(2.0);
                for (loop_state, widget) in state.loops.iter().zip(&mut self.loop_widgets) {
                    ui.push_id(loop_state.id, |ui| {
                        let size = egui::vec2(ui.available_width(), 26.0);
                        let response = widget.show(ui, loop_state, size);
                        result.loop_actions.extend(
                            response
                                .actions
                                .into_iter()
                                .map(|action| (loop_state.id, action)),
                        );
                    });
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
                self.controls.show(ui, state)
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
            let _ = ui
                .add(egui::Button::new(ICON_MORE_VERT.rich_text().size(17.0)).frame(false))
                .on_hover_text("Track options (not implemented)");
        });
    }
}
