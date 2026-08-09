use crate::{AppIntent, TrackProcessorDescriptor, TrackState, TrackWidget};
use egui_material_icons::icons::ICON_ADD;

#[derive(Debug, Default)]
pub struct TracksWidgetResponse {
    pub intents: Vec<AppIntent>,
    pub add_track_requested: bool,
    pub connection_track_requested: Option<crate::TrackId>,
}

#[derive(Debug, Default)]
pub struct TracksWidget {
    track_widgets: Vec<TrackWidget>,
    #[cfg(test)]
    test_empty_prompt_shown: bool,
}

impl TracksWidget {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        tracks: &[TrackState],
        processors: &[TrackProcessorDescriptor],
    ) -> TracksWidgetResponse {
        self.track_widgets
            .resize_with(tracks.len(), TrackWidget::default);
        let mut result = TracksWidgetResponse::default();
        #[cfg(test)]
        {
            self.test_empty_prompt_shown = tracks.is_empty();
        }
        let control_height = 82.0;
        egui::ScrollArea::horizontal()
            .id_salt("main_tracks_horizontal")
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    let loop_height = (ui.available_height() - control_height).max(80.0);
                    egui::ScrollArea::vertical()
                        .id_salt("main_tracks_loops_vertical")
                        .max_height(loop_height)
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                for (track, widget) in tracks.iter().zip(&mut self.track_widgets) {
                                    ui.push_id(track.id, |ui| {
                                        let processor = track.fx.as_ref().and_then(|fx| {
                                            processors
                                                .iter()
                                                .find(|candidate| candidate.id == fx.processor_type)
                                        });
                                        let response = widget.show_content_with_processor(
                                            ui, track, processor, true,
                                        );
                                        collect_response(&mut result, track, response);
                                    });
                                }
                                if tracks.is_empty() {
                                    ui.add_sized(
                                        [190.0, 40.0],
                                        egui::Label::new(
                                            "No tracks yet — use + to add your first track",
                                        ),
                                    );
                                }
                                let add = ui
                                    .add_sized(
                                        [32.0, 40.0],
                                        egui::Button::new(ICON_ADD.rich_text().size(20.0)),
                                    )
                                    .on_hover_text("Create a new track");
                                result.add_track_requested = add.clicked();
                            });
                        });
                    ui.separator();
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for (track, widget) in tracks.iter().zip(&mut self.track_widgets) {
                            ui.push_id((track.id, "controls"), |ui| {
                                result.intents.extend(
                                    widget.show_controls(ui, &track.controls).into_iter().map(
                                        |action| AppIntent::Track {
                                            track_id: track.id,
                                            action,
                                        },
                                    ),
                                );
                            });
                        }
                    });
                });
            });
        result
    }
}

fn collect_response(
    result: &mut TracksWidgetResponse,
    track: &TrackState,
    response: crate::TrackWidgetResponse,
) {
    result.intents.extend(response.io_intents.iter().cloned());
    result
        .intents
        .extend(
            response
                .loop_actions
                .into_iter()
                .map(|(loop_id, action)| AppIntent::Loop {
                    track_id: track.id,
                    loop_id,
                    action,
                }),
        );
    result
        .intents
        .extend(response.actions.into_iter().map(|action| AppIntent::Track {
            track_id: track.id,
            action,
        }));
    if response.add_loop_requested {
        result
            .intents
            .push(AppIntent::AddLoop { track_id: track.id });
    }
    if response.connections_requested {
        result.connection_track_requested = Some(track.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LoopId, LoopWidgetAction, SelectionModifiers, TrackId};

    #[test]
    fn empty_main_tracks_show_first_track_instruction_only() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut widget = TracksWidget::default();
        for (tracks, expected) in [
            (Vec::<TrackState>::new(), true),
            (
                vec![TrackState {
                    id: TrackId::from_raw(1),
                    name: "Track".to_owned(),
                    ..Default::default()
                }],
                false,
            ),
        ] {
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    widget.show(ui, &tracks, &[]);
                },
            );
            assert_eq!(widget.test_empty_prompt_shown, expected);
        }
    }

    #[test]
    fn response_routes_loop_and_add_actions_by_stable_id() {
        let track = TrackState {
            id: TrackId::from_raw(7),
            ..Default::default()
        };
        let loop_id = LoopId::from_raw(11);
        let response = crate::TrackWidgetResponse {
            loop_actions: vec![(
                loop_id,
                LoopWidgetAction::IconClicked(SelectionModifiers { additive: true }),
            )],
            add_loop_requested: true,
            connections_requested: true,
            ..Default::default()
        };
        let mut result = TracksWidgetResponse::default();
        collect_response(&mut result, &track, response);
        assert_eq!(result.connection_track_requested, Some(track.id));
        assert_eq!(
            result.intents,
            vec![
                AppIntent::Loop {
                    track_id: track.id,
                    loop_id,
                    action: LoopWidgetAction::IconClicked(SelectionModifiers { additive: true }),
                },
                AppIntent::AddLoop { track_id: track.id },
            ]
        );
    }
}
