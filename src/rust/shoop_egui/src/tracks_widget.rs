use crate::{AppIntent, TrackState, TrackWidget};

#[derive(Debug, Default)]
pub struct TracksWidgetResponse {
    pub intents: Vec<AppIntent>,
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
                for (track, widget) in tracks.iter().zip(&mut self.track_widgets) {
                    ui.push_id(track.id, |ui| {
                        let response = widget.show(ui, track);
                        result.intents.extend(response.loop_actions.into_iter().map(
                            |(loop_id, action)| AppIntent::Loop {
                                track_id: track.id,
                                loop_id,
                                action,
                            },
                        ));
                        result
                            .intents
                            .extend(response.actions.into_iter().map(|action| AppIntent::Track {
                                track_id: track.id,
                                action,
                            }));
                    });
                }
            });
        });
        result
    }
}
