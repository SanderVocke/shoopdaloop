use std::collections::BTreeMap;

use crate::{
    AppIntent, FxLifecycle, LoopId, LoopWidget, LoopWidgetAction, TrackControls,
    TrackProcessorDescriptor, TrackState, TrackWidgetAction,
};
use egui_material_icons::icons::{ICON_ADD, ICON_MORE_VERT};

#[derive(Debug, Default)]
pub struct TrackWidgetResponse {
    pub actions: Vec<TrackWidgetAction>,
    pub loop_actions: Vec<(LoopId, LoopWidgetAction)>,
    pub io_intents: Vec<AppIntent>,
    pub add_loop_requested: bool,
    pub connections_requested: bool,
}

#[derive(Debug, Default)]
pub struct TrackWidget {
    name_edit: String,
    source_name: String,
    loop_widgets: BTreeMap<LoopId, LoopWidget>,
    hovered_loop: Option<LoopId>,
    controls: TrackControls,
    fx_logs_open: bool,
    #[cfg(test)]
    test_loop_rects: Vec<egui::Rect>,
    #[cfg(test)]
    test_options_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_connections_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_fx_rect: Option<egui::Rect>,
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
        self.show_content_with_processor(ui, state, None, show_add_loop)
    }

    pub fn show_content_with_processor(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
        show_add_loop: bool,
    ) -> TrackWidgetResponse {
        self.loop_widgets
            .retain(|id, _| state.loops.iter().any(|loop_state| loop_state.id == *id));
        if self
            .hovered_loop
            .is_some_and(|id| !self.loop_widgets.contains_key(&id))
        {
            self.hovered_loop = None;
        }
        let mut result = TrackWidgetResponse::default();
        #[cfg(test)]
        self.test_loop_rects.clear();
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(85, 85, 85))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(180.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    self.show_header(ui, state, processor, &mut result);
                    ui.add_space(2.0);
                    for loop_state in &state.loops {
                        let hover_allowed = self
                            .hovered_loop
                            .is_none_or(|hovered| hovered == loop_state.id);
                        let widget = self.loop_widgets.entry(loop_state.id).or_default();
                        let loop_response = ui.push_id(loop_state.id, |ui| {
                            let size = egui::vec2(ui.available_width(), 26.0);
                            widget.show_with_hover(ui, loop_state, size, hover_allowed)
                        });
                        if loop_response.inner.hover_active {
                            self.hovered_loop = Some(loop_state.id);
                        } else if self.hovered_loop == Some(loop_state.id) {
                            self.hovered_loop = None;
                        }
                        result.loop_actions.extend(
                            loop_response
                                .inner
                                .actions
                                .into_iter()
                                .map(|action| (loop_state.id, action)),
                        );
                        result.io_intents.extend(loop_response.inner.io_intents);
                        #[cfg(test)]
                        self.test_loop_rects.push(loop_response.response.rect);
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
        self.show_fx_logs(ui.ctx(), state, processor, &mut result);
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
        processor: Option<&TrackProcessorDescriptor>,
        result: &mut TrackWidgetResponse,
    ) {
        ui.horizontal(|ui| {
            let fx_width = if state.fx.is_some() { 34.0 } else { 0.0 };
            if state.is_sync {
                ui.add_sized(
                    [(ui.available_width() - 22.0 - fx_width).max(40.0), 24.0],
                    egui::Label::new(&state.name),
                );
            } else {
                let name_id = ui.make_persistent_id("name");
                let has_focus = ui.memory(|memory| memory.has_focus(name_id));
                if !has_focus && self.source_name != state.name {
                    self.name_edit.clone_from(&state.name);
                    self.source_name.clone_from(&state.name);
                }
                let available = (ui.available_width() - 22.0 - fx_width).max(40.0);
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
            if let Some(fx) = &state.fx {
                let features = processor.map(|value| value.features).unwrap_or_default();
                let color = match fx.lifecycle {
                    FxLifecycle::Running if fx.active => egui::Color32::LIGHT_GREEN,
                    FxLifecycle::Running => egui::Color32::GRAY,
                    FxLifecycle::Starting | FxLifecycle::Restarting => egui::Color32::YELLOW,
                    FxLifecycle::Crashed | FxLifecycle::Unavailable => egui::Color32::LIGHT_RED,
                    FxLifecycle::Stopped => egui::Color32::GRAY,
                };
                let controllable = features.external_ui || features.recovery;
                let fx_button = ui
                    .add_enabled(
                        controllable,
                        egui::Button::new(egui::RichText::new("FX").color(color)),
                    )
                    .on_hover_text(format!(
                        "{}: {:?}{}",
                        fx.processor_type,
                        fx.lifecycle,
                        fx.crash_summary
                            .as_deref()
                            .map(|summary| format!(" — {summary}"))
                            .unwrap_or_default()
                    ));
                #[cfg(test)]
                {
                    self.test_fx_rect = Some(fx_button.rect);
                }
                if fx_button.clicked() {
                    result.actions.push(TrackWidgetAction::FxToggleOrRecover);
                }
                fx_button.context_menu(|ui| {
                    if ui
                        .add_enabled(features.logs, egui::Button::new("Process logs..."))
                        .clicked()
                    {
                        self.fx_logs_open = true;
                        ui.close();
                    }
                });
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

    fn show_fx_logs(
        &mut self,
        context: &egui::Context,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
        result: &mut TrackWidgetResponse,
    ) {
        let Some(fx) = &state.fx else {
            self.fx_logs_open = false;
            return;
        };
        if !processor.is_some_and(|value| value.features.logs) || !self.fx_logs_open {
            return;
        }
        let mut open = self.fx_logs_open;
        egui::Window::new(format!("{} FX logs", state.name))
            .id(egui::Id::new(("track_fx_logs", state.id)))
            .open(&mut open)
            .resizable(true)
            .show(context, |ui| {
                ui.label(format!(
                    "Lifecycle: {:?} · generation {}",
                    fx.lifecycle, fx.generation
                ));
                if let Some(summary) = &fx.crash_summary {
                    ui.colored_label(egui::Color32::LIGHT_RED, summary);
                }
                ui.horizontal(|ui| {
                    if ui.button("Clear").clicked() {
                        result.actions.push(TrackWidgetAction::FxClearLogs);
                    }
                    if ui.button("Copy all").clicked() {
                        let text = fx
                            .logs
                            .iter()
                            .map(|log| {
                                format!(
                                    "generation {}\nstdout:\n{}\nstderr:\n{}",
                                    log.generation, log.stdout, log.stderr
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        ui.ctx().copy_text(text);
                    }
                });
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for log in fx.logs.iter() {
                        ui.collapsing(format!("Generation {}", log.generation), |ui| {
                            ui.label(format!(
                                "Dropped stdout/stderr bytes: {}/{}",
                                log.dropped_stdout_bytes, log.dropped_stderr_bytes
                            ));
                            ui.monospace(format!("stdout:\n{}", log.stdout));
                            ui.monospace(format!("stderr:\n{}", log.stderr));
                        });
                    }
                    if fx.logs.is_empty() {
                        ui.label("No process logs are available.");
                    }
                });
            });
        self.fx_logs_open = open;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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

    fn processor_frame(
        context: &egui::Context,
        widget: &mut TrackWidget,
        state: &TrackState,
        processor: &TrackProcessorDescriptor,
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
            |ui| response = widget.show_content_with_processor(ui, state, Some(processor), false),
        );
        response
    }

    #[test]
    fn processor_facets_render_status_controls_and_logs_without_affecting_direct_tracks() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let processor = TrackProcessorDescriptor {
            id: crate::TrackProcessorTypeId::new("synthetic_fx"),
            label: "Synthetic FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: Default::default(),
            features: crate::TrackProcessorFeatures {
                state: true,
                external_ui: true,
                recovery: true,
                logs: true,
            },
        };
        let state = TrackState {
            id: TrackId::from_raw(9),
            name: "Processed".to_owned(),
            fx: Some(crate::TrackFxState {
                processor_type: processor.id.clone(),
                active: true,
                visible: false,
                lifecycle: FxLifecycle::Crashed,
                generation: 3,
                crash_summary: Some("worker exited".to_owned()),
                logs: Arc::from([crate::FxGenerationLogState {
                    generation: 3,
                    stdout: Arc::from("out"),
                    stderr: Arc::from("err"),
                    dropped_stdout_bytes: 1,
                    dropped_stderr_bytes: 2,
                }]),
            }),
            ..Default::default()
        };
        let mut widget = TrackWidget::default();
        let _ = processor_frame(&context, &mut widget, &state, &processor, Vec::new());
        let fx = widget.test_fx_rect.unwrap().center();
        let _ = processor_frame(
            &context,
            &mut widget,
            &state,
            &processor,
            vec![
                egui::Event::PointerMoved(fx),
                egui::Event::PointerButton {
                    pos: fx,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = processor_frame(
            &context,
            &mut widget,
            &state,
            &processor,
            vec![
                egui::Event::PointerMoved(fx),
                egui::Event::PointerButton {
                    pos: fx,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(response.actions, [TrackWidgetAction::FxToggleOrRecover]);
        widget.fx_logs_open = true;
        let _ = processor_frame(&context, &mut widget, &state, &processor, Vec::new());

        let direct = TrackState {
            id: TrackId::from_raw(10),
            ..Default::default()
        };
        let mut direct_widget = TrackWidget::default();
        let _ = frame(&context, &mut direct_widget, &direct, Vec::new());
        assert!(direct_widget.test_fx_rect.is_none());
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
    fn loop_widget_presentation_state_follows_stable_ids_across_reordering() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let first = LoopId::from_raw(1);
        let second = LoopId::from_raw(2);
        let mut state = TrackState {
            id: TrackId::from_raw(1),
            loops: vec![
                LoopState {
                    id: first,
                    ..Default::default()
                },
                LoopState {
                    id: second,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut widget = TrackWidget::default();
        let _ = frame(&context, &mut widget, &state, Vec::new());
        let first_widget = &widget.loop_widgets[&first] as *const LoopWidget;
        state.loops.swap(0, 1);
        let _ = frame(&context, &mut widget, &state, Vec::new());
        assert_eq!(
            first_widget,
            &widget.loop_widgets[&first] as *const LoopWidget
        );
    }

    #[test]
    fn popup_hover_keeps_the_source_loop_as_the_only_hover_owner() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let first = LoopId::from_raw(1);
        let state = TrackState {
            id: TrackId::from_raw(1),
            loops: vec![
                LoopState {
                    id: first,
                    ..Default::default()
                },
                LoopState {
                    id: LoopId::from_raw(2),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut widget = TrackWidget::default();
        let _ = frame(&context, &mut widget, &state, Vec::new());
        let second_rect = widget.test_loop_rects[1];
        let record = widget.loop_widgets[&first]
            .test_record_rect()
            .unwrap()
            .center();
        let _ = frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerMoved(record)],
        );
        assert_eq!(widget.hovered_loop, Some(first));

        let popup_over_second = widget.loop_widgets[&first]
            .test_record_popup_rect()
            .unwrap()
            .center();
        assert!(second_rect.contains(popup_over_second));
        let _ = frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerMoved(popup_over_second)],
        );
        assert_eq!(widget.hovered_loop, Some(first));
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
