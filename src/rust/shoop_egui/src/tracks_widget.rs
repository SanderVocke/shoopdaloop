use crate::{LoopState, LoopWidget, LoopWidgetAction};

#[derive(Clone, Debug, Default)]
pub struct TrackState {
    pub name: String,
    pub loops: Vec<LoopState>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexedLoopAction {
    pub track_index: usize,
    pub loop_index: usize,
    pub action: LoopWidgetAction,
}

#[derive(Debug, Default)]
pub struct TracksWidget {
    loop_widgets: Vec<Vec<LoopWidget>>,
}

impl TracksWidget {
    pub fn show(&mut self, ui: &mut egui::Ui, tracks: &[TrackState]) -> Vec<IndexedLoopAction> {
        self.loop_widgets.resize_with(tracks.len(), Vec::new);
        for (track, widgets) in tracks.iter().zip(&mut self.loop_widgets) {
            widgets.resize_with(track.loops.len(), LoopWidget::default);
        }

        let mut actions = Vec::new();
        egui::ScrollArea::both().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for (track_index, track) in tracks.iter().enumerate() {
                    ui.push_id(track_index, |ui| {
                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(180.0);
                                ui.label(egui::RichText::new(&track.name).strong());
                                for (loop_index, state) in track.loops.iter().enumerate() {
                                    ui.push_id(loop_index, |ui| {
                                        let size = egui::vec2(ui.available_width(), 26.0);
                                        let response = self.loop_widgets[track_index][loop_index]
                                            .show(ui, state, size);
                                        actions.extend(response.actions.into_iter().map(
                                            |action| IndexedLoopAction {
                                                track_index,
                                                loop_index,
                                                action,
                                            },
                                        ));
                                    });
                                }
                            });
                        });
                    });
                }
            });
        });
        actions
    }
}
