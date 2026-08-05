use crate::{IndexedLoopAction, IndexedTrackAction, TrackState, TrackWidget, TrackWidgetAction};

#[derive(Debug, Default)]
pub struct TracksWidgetResponse {
    pub loop_actions: Vec<IndexedLoopAction>,
    pub track_actions: Vec<IndexedTrackAction>,
}

#[derive(Debug, Default)]
pub struct TracksWidget {
    track_widgets: Vec<TrackWidget>,
}

impl TracksWidget {
    pub fn show(&mut self, ui: &mut egui::Ui, tracks: &[TrackState]) -> TracksWidgetResponse {
        self.track_widgets
            .resize_with(tracks.len(), TrackWidget::default);

        let mut result = TracksWidgetResponse::default();
        egui::ScrollArea::both().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for (track_index, (track, widget)) in
                    tracks.iter().zip(&mut self.track_widgets).enumerate()
                {
                    ui.push_id(track_index, |ui| {
                        let response = widget.show(ui, track);
                        result
                            .loop_actions
                            .extend(response.loop_actions.into_iter().map(
                                |(loop_index, action)| IndexedLoopAction {
                                    track_index,
                                    loop_index,
                                    action,
                                },
                            ));
                        result
                            .track_actions
                            .extend(response.actions.into_iter().map(
                                |action: TrackWidgetAction| IndexedTrackAction {
                                    track_index,
                                    action,
                                },
                            ));
                    });
                }
            });
        });
        result
    }
}
