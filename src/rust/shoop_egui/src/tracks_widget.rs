use std::collections::BTreeMap;

use crate::{
    AppIntent, GlobalControlState, TrackId, TrackProcessorDescriptor, TrackState, TrackWidget,
};
use egui_material_icons::icons::ICON_ADD;

#[derive(Debug, Default)]
pub struct TracksWidgetResponse {
    pub intents: Vec<AppIntent>,
    pub add_track_requested: bool,
    pub connection_track_requested: Option<crate::TrackId>,
    pub click_track_requested: Option<crate::LoopId>,
}

#[derive(Debug, Default)]
pub struct TracksWidget {
    track_widgets: Vec<TrackWidget>,
    track_centers: BTreeMap<TrackId, f32>,
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
        self.show_with_global_controls(ui, tracks, processors, &GlobalControlState::default())
    }

    pub fn show_with_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        tracks: &[TrackState],
        processors: &[TrackProcessorDescriptor],
        global_controls: &GlobalControlState,
    ) -> TracksWidgetResponse {
        let _span = tracing::trace_span!(
            "frontend.egui.tracks",
            track_count = tracks.len(),
            processor_count = processors.len()
        )
        .entered();
        self.track_widgets
            .resize_with(tracks.len(), TrackWidget::default);
        let mut track_centers = BTreeMap::new();
        let mut result = TracksWidgetResponse::default();
        #[cfg(test)]
        {
            self.test_empty_prompt_shown = tracks.is_empty();
        }
        let control_height = 82.0;
        egui::ScrollArea::horizontal()
            .id_salt("main_tracks_horizontal")
            .scroll_source(crate::control_safe_scroll_source())
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let loop_height = (ui.available_height() - control_height).max(80.0);
                    egui::ScrollArea::vertical()
                        .id_salt("main_tracks_loops_vertical")
                        .scroll_source(crate::control_safe_scroll_source())
                        .max_height(loop_height)
                        .auto_shrink([true, false])
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                for (track, widget) in tracks.iter().zip(&mut self.track_widgets) {
                                    let track_response = ui.push_id(track.id, |ui| {
                                        let processor = track.fx.as_ref().and_then(|fx| {
                                            processors
                                                .iter()
                                                .find(|candidate| candidate.id == fx.processor_type)
                                        });
                                        let response = widget
                                            .show_content_with_processor_min_height_and_global_controls(
                                                ui,
                                                track,
                                                processor,
                                                true,
                                                loop_height,
                                                global_controls,
                                            );
                                        collect_response(&mut result, track, response);
                                    });
                                    track_centers.insert(
                                        track.id,
                                        track_response.response.rect.center().x,
                                    );
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
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for (track, widget) in tracks.iter().zip(&mut self.track_widgets) {
                            ui.push_id((track.id, "controls"), |ui| {
                                result.intents.extend(
                                    widget
                                        .show_controls_with_global_controls(
                                            ui,
                                            &track.controls,
                                            global_controls,
                                        )
                                        .into_iter()
                                        .map(|action| AppIntent::Track {
                                            track_id: track.id,
                                            action,
                                        }),
                                );
                            });
                        }
                    });
                });
            });
        self.track_centers = track_centers;
        if result.add_track_requested || !result.intents.is_empty() {
            tracing::debug!(
                target: "Frontend.Egui",
                intent_count = result.intents.len(),
                add_track_requested = result.add_track_requested,
                "frontend.egui.tracks_interaction"
            );
        }
        result
    }

    pub fn track_centers(&self, track_ids: &[TrackId]) -> Vec<f32> {
        track_ids
            .iter()
            .filter_map(|id| self.track_centers.get(id).copied())
            .collect()
    }
}

fn collect_response(
    result: &mut TracksWidgetResponse,
    track: &TrackState,
    response: crate::TrackWidgetResponse,
) {
    result.intents.extend(response.io_intents.iter().cloned());
    if response.click_track_requested.is_some() {
        result.click_track_requested = response.click_track_requested;
    }
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
    use crate::{LoopId, LoopState, LoopWidgetAction, SelectionModifiers, TrackId};

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
    fn track_content_and_control_rows_have_matching_horizontal_bounds() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let tracks = (1..=3)
            .map(|id| TrackState {
                id: TrackId::from_raw(id),
                name: format!("Track {id}"),
                loops: vec![LoopState {
                    id: LoopId::from_raw(id),
                    ..Default::default()
                }],
                controls: crate::TrackControlState {
                    has_output: true,
                    has_output_audio: true,
                    output_stereo: true,
                    has_input: true,
                    has_input_audio: true,
                    input_stereo: true,
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let mut widget = TracksWidget::default();

        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700.0, 400.0),
                )),
                ..Default::default()
            },
            |ui| {
                widget.show(ui, &tracks, &[]);
            },
        );

        for track_widget in &widget.track_widgets {
            let (content, controls) = track_widget.test_layout_rects();
            assert_eq!(content.x_range(), controls.x_range());
            assert_eq!(content.bottom(), controls.top());
            assert_eq!(controls.bottom(), 400.0);
        }
        let centers = widget.track_centers(&[
            TrackId::from_raw(1),
            TrackId::from_raw(3),
            TrackId::from_raw(99),
        ]);
        assert_eq!(centers.len(), 2);
        assert!(centers[0] < centers[1]);
    }

    #[test]
    fn response_routes_loop_and_add_actions_by_stable_id() {
        let track = TrackState {
            id: TrackId::from_raw(7),
            ..Default::default()
        };
        let loop_id = LoopId::from_raw(11);
        let play_loop_id = LoopId::from_raw(12);
        let response = crate::TrackWidgetResponse {
            loop_actions: vec![
                (
                    loop_id,
                    LoopWidgetAction::IconClicked(SelectionModifiers { additive: true }),
                ),
                (play_loop_id, LoopWidgetAction::PlayClicked),
            ],
            click_track_requested: Some(loop_id),
            add_loop_requested: true,
            connections_requested: true,
            ..Default::default()
        };
        let mut result = TracksWidgetResponse::default();
        collect_response(&mut result, &track, response);
        assert_eq!(result.connection_track_requested, Some(track.id));
        assert_eq!(result.click_track_requested, Some(loop_id));
        assert_eq!(
            result.intents,
            vec![
                AppIntent::Loop {
                    track_id: track.id,
                    loop_id,
                    action: LoopWidgetAction::IconClicked(SelectionModifiers { additive: true }),
                },
                AppIntent::Loop {
                    track_id: track.id,
                    loop_id: play_loop_id,
                    action: LoopWidgetAction::PlayClicked,
                },
                AppIntent::AddLoop { track_id: track.id },
            ]
        );
    }
}
