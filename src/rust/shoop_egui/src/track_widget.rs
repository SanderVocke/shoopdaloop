use crate::{LoopWidget, LoopWidgetAction, TrackControls, TrackState, TrackWidgetAction};
use egui_material_icons::icons::ICON_MORE_VERT;

#[derive(Debug, Default)]
pub struct TrackWidgetResponse {
    pub actions: Vec<TrackWidgetAction>,
    pub loop_actions: Vec<(usize, LoopWidgetAction)>,
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
        self.loop_widgets
            .resize_with(state.loops.len(), LoopWidget::default);

        let mut result = TrackWidgetResponse::default();
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(85, 85, 85))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(180.0);
                ui.horizontal(|ui| {
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
                    if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        response.surrender_focus();
                    }
                    if response.lost_focus() && self.name_edit != state.name {
                        self.source_name.clone_from(&self.name_edit);
                        result
                            .actions
                            .push(TrackWidgetAction::NameChanged(self.name_edit.clone()));
                    }

                    let _ = ui
                        .add(egui::Button::new(ICON_MORE_VERT.rich_text().size(17.0)).frame(false))
                        .on_hover_text("Track options (not implemented)");
                });

                ui.add_space(2.0);
                for (loop_index, (loop_state, widget)) in
                    state.loops.iter().zip(&mut self.loop_widgets).enumerate()
                {
                    ui.push_id(loop_index, |ui| {
                        let size = egui::vec2(ui.available_width(), 26.0);
                        let response = widget.show(ui, loop_state, size);
                        result.loop_actions.extend(
                            response
                                .actions
                                .into_iter()
                                .map(|action| (loop_index, action)),
                        );
                    });
                    ui.add_space(2.0);
                }

                ui.add_space(2.0);
                result
                    .actions
                    .extend(self.controls.show(ui, &state.controls));
            });
        result
    }
}
