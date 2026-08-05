use egui_material_icons::icons::{
    ICON_BORDER_CLEAR, ICON_DELETE, ICON_EXCLAMATION, ICON_FIBER_MANUAL_RECORD, ICON_MENU,
    ICON_PLAY_ARROW, ICON_STOP, ICON_TIMER,
};

use crate::{DefaultRecordingAction, GlobalControlAction, GlobalControlState};

#[derive(Debug, Default)]
pub struct GlobalControls;

impl GlobalControls {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &GlobalControlState,
    ) -> Vec<GlobalControlAction> {
        let mut actions = Vec::new();
        ui.horizontal(|ui| {
            let _ = icon_button(ui, ICON_MENU, "Main menu (not implemented)");
            ui.separator();

            if icon_button(ui, ICON_STOP, "Stop all loops").clicked() {
                actions.push(GlobalControlAction::StopAll);
            }
            if icon_button(ui, ICON_BORDER_CLEAR, "Deselect all loops").clicked() {
                actions.push(GlobalControlAction::DeselectAll);
            }
            ui.menu_button(ICON_DELETE.rich_text().size(20.0), |ui| {
                if ui.button("Clear recordings").clicked() {
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

            ui.separator();
            let recording_label = match state.default_recording_action {
                DefaultRecordingAction::Record => "REC",
                DefaultRecordingAction::Grab => "GRAB",
            };
            if ui
                .button(recording_label)
                .on_hover_text("Default recording action")
                .clicked()
            {
                let next = match state.default_recording_action {
                    DefaultRecordingAction::Record => DefaultRecordingAction::Grab,
                    DefaultRecordingAction::Grab => DefaultRecordingAction::Record,
                };
                actions.push(GlobalControlAction::SetDefaultRecordingAction(next));
            }

            let play_after_text = egui::RichText::new(format!(
                "{}{}",
                ICON_FIBER_MANUAL_RECORD.codepoint, ICON_PLAY_ARROW.codepoint
            ))
            .family(ICON_FIBER_MANUAL_RECORD.font_family())
            .size(20.0)
            .color(if state.play_after_record {
                egui::Color32::WHITE
            } else {
                egui::Color32::GRAY
            });
            if ui
                .selectable_label(state.play_after_record, play_after_text)
                .on_hover_text("Play after recording")
                .clicked()
            {
                actions.push(GlobalControlAction::SetPlayAfterRecord(
                    !state.play_after_record,
                ));
            }

            let sync_icon = if state.sync {
                ICON_TIMER
            } else {
                ICON_EXCLAMATION
            };
            if ui
                .selectable_label(state.sync, sync_icon.rich_text().size(20.0))
                .on_hover_text("Synchronized actions")
                .clicked()
            {
                actions.push(GlobalControlAction::SetSync(!state.sync));
            }
            if ui
                .selectable_label(state.solo, egui::RichText::new("S").size(18.0))
                .on_hover_text("Solo within track")
                .clicked()
            {
                actions.push(GlobalControlAction::SetSolo(!state.solo));
            }

            let mut cycles = state.apply_n_cycles;
            if ui
                .add(
                    egui::DragValue::new(&mut cycles)
                        .range(0..=i32::MAX as u32)
                        .prefix("cycles "),
                )
                .on_hover_text("Recording length in sync cycles; 0 means infinite")
                .changed()
            {
                actions.push(GlobalControlAction::SetApplyNCycles(cycles));
            }
        });
        actions
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
