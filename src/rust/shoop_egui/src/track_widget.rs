use std::collections::BTreeMap;

use crate::{
    colors, composite_loop_widget::LoopDragPayload, AppIntent, FxLifecycle, GlobalControlState,
    LoopId, LoopWidget, LoopWidgetAction, TrackControls, TrackProcessorDescriptor, TrackState,
    TrackWidgetAction,
};
use egui_material_icons::icons::{ICON_ADD, ICON_MORE_VERT};

use crate::tiny_synth_fx_editor::TinySynthFxEditor;

const DEFAULT_TRACK_WIDTH: f32 = 120.0;
const MIN_TRACK_WIDTH: f32 = 100.0;
const MAX_TRACK_WIDTH: f32 = 400.0;
const TRACK_CONTROLS_HEIGHT: f32 = 48.0;
const TRACK_CONTENT_MARGIN: egui::Margin = egui::Margin::same(4);
const TRACK_CONTROLS_MARGIN: egui::Margin = egui::Margin::same(4);
const RESIZE_HANDLE_RADIUS: f32 = 3.0;

#[derive(Debug, Default)]
pub struct TrackWidgetResponse {
    pub actions: Vec<TrackWidgetAction>,
    pub loop_actions: Vec<(LoopId, LoopWidgetAction)>,
    pub io_intents: Vec<AppIntent>,
    pub click_track_requested: Option<LoopId>,
    pub add_loop_requested: bool,
    pub connections_requested: bool,
}

#[derive(Debug)]
pub struct TrackWidget {
    name_edit: String,
    source_name: String,
    loop_widgets: BTreeMap<LoopId, LoopWidget>,
    hovered_loop: Option<LoopId>,
    pending_loop_drop: Option<(LoopId, LoopId)>,
    controls: TrackControls,
    fx_logs_open: bool,
    tiny_synth_fx_editor: TinySynthFxEditor,
    width: f32,
    rendered_content_width: f32,
    width_drag_start: Option<f32>,
    width_resizable: bool,
    #[cfg(test)]
    test_loop_rects: Vec<egui::Rect>,
    #[cfg(test)]
    test_content_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_controls_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_controls_clip_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_options_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_connections_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_delete_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_fx_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_click_track_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_drop_duplicate_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_drop_swap_rect: Option<egui::Rect>,
}

impl Default for TrackWidget {
    fn default() -> Self {
        Self {
            name_edit: String::new(),
            source_name: String::new(),
            loop_widgets: BTreeMap::new(),
            hovered_loop: None,
            pending_loop_drop: None,
            controls: TrackControls::default(),
            fx_logs_open: false,
            tiny_synth_fx_editor: TinySynthFxEditor::default(),
            width: DEFAULT_TRACK_WIDTH,
            rendered_content_width: DEFAULT_TRACK_WIDTH,
            width_drag_start: None,
            width_resizable: true,
            #[cfg(test)]
            test_loop_rects: Vec::new(),
            #[cfg(test)]
            test_content_rect: None,
            #[cfg(test)]
            test_controls_rect: None,
            #[cfg(test)]
            test_controls_clip_rect: None,
            #[cfg(test)]
            test_options_rect: None,
            #[cfg(test)]
            test_connections_rect: None,
            #[cfg(test)]
            test_delete_rect: None,
            #[cfg(test)]
            test_fx_rect: None,
            #[cfg(test)]
            test_click_track_rect: None,
            #[cfg(test)]
            test_drop_duplicate_rect: None,
            #[cfg(test)]
            test_drop_swap_rect: None,
        }
    }
}

fn track_background(state: &crate::TrackControlState) -> egui::Color32 {
    if state.input_monitoring {
        colors::INPUT_ACTIVE_BACKGROUND
    } else {
        colors::RAISED_BACKGROUND
    }
}

impl TrackWidget {
    pub(crate) fn set_width_resizable(&mut self, resizable: bool) {
        self.width_resizable = resizable;
        if !resizable {
            self.width = DEFAULT_TRACK_WIDTH;
            self.rendered_content_width = DEFAULT_TRACK_WIDTH;
            self.width_drag_start = None;
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &TrackState) -> TrackWidgetResponse {
        self.show_with_global_controls(ui, state, &GlobalControlState::default())
    }

    pub fn show_with_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        global_controls: &GlobalControlState,
    ) -> TrackWidgetResponse {
        let item_spacing_y = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut response =
            self.show_content_with_global_controls(ui, state, !state.is_sync, global_controls);
        response
            .actions
            .extend(self.show_controls_with_height_and_global_controls(
                ui,
                &state.controls,
                !state.is_sync,
                global_controls,
            ));
        ui.spacing_mut().item_spacing.y = item_spacing_y;
        response
    }

    pub fn show_content(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        show_add_loop: bool,
    ) -> TrackWidgetResponse {
        self.show_content_with_global_controls(
            ui,
            state,
            show_add_loop,
            &GlobalControlState::default(),
        )
    }

    pub fn show_content_with_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        show_add_loop: bool,
        global_controls: &GlobalControlState,
    ) -> TrackWidgetResponse {
        self.show_content_with_processor_and_global_controls(
            ui,
            state,
            None,
            show_add_loop,
            global_controls,
        )
    }

    pub fn show_content_with_processor(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
        show_add_loop: bool,
    ) -> TrackWidgetResponse {
        self.show_content_with_processor_and_global_controls(
            ui,
            state,
            processor,
            show_add_loop,
            &GlobalControlState::default(),
        )
    }

    pub fn show_content_with_processor_and_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
        show_add_loop: bool,
        global_controls: &GlobalControlState,
    ) -> TrackWidgetResponse {
        self.show_content_with_processor_min_height_and_global_controls(
            ui,
            state,
            processor,
            show_add_loop,
            0.0,
            global_controls,
        )
    }

    pub(crate) fn show_content_with_processor_min_height_and_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
        show_add_loop: bool,
        min_height: f32,
        global_controls: &GlobalControlState,
    ) -> TrackWidgetResponse {
        let _span = tracing::trace_span!(
            "frontend.egui.track",
            track_id = state.id.raw(),
            loop_count = state.loops.len(),
            is_sync = state.is_sync
        )
        .entered();
        self.loop_widgets
            .retain(|id, _| state.loops.iter().any(|loop_state| loop_state.id == *id));
        if self
            .hovered_loop
            .is_some_and(|id| !self.loop_widgets.contains_key(&id))
        {
            self.hovered_loop = None;
        }
        if self.pending_loop_drop.is_some_and(|(source, target)| {
            !state.loops.iter().any(|loop_state| loop_state.id == source)
                || !state.loops.iter().any(|loop_state| loop_state.id == target)
        }) {
            self.pending_loop_drop = None;
        }
        let mut result = TrackWidgetResponse::default();
        #[cfg(test)]
        {
            self.test_loop_rects.clear();
            self.test_drop_duplicate_rect = None;
            self.test_drop_swap_rect = None;
        }
        let rendered_width = self.width;
        let frame = egui::Frame::new()
            .fill(track_background(&state.controls))
            .inner_margin(TRACK_CONTENT_MARGIN)
            .show(ui, |ui| {
                ui.set_width(rendered_width);
                ui.set_min_height((min_height - TRACK_CONTENT_MARGIN.sum().y).max(0.0));
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    self.show_header(ui, state, processor, &mut result);
                    ui.add_space(2.0);
                    for loop_state in &state.loops {
                        let hover_allowed = self
                            .hovered_loop
                            .is_none_or(|hovered| hovered == loop_state.id);
                        let loop_response = ui.push_id(loop_state.id, |ui| {
                            let widget = self.loop_widgets.entry(loop_state.id).or_default();
                            let (drop_zone, dropped) =
                                ui.dnd_drop_zone::<LoopDragPayload, _>(egui::Frame::NONE, |ui| {
                                    let size = egui::vec2(ui.available_width(), 26.0);
                                    widget.show_with_hover(
                                        ui,
                                        loop_state,
                                        size,
                                        hover_allowed,
                                        global_controls,
                                    )
                                });
                            let dropped = dropped.filter(|payload| {
                                payload.loop_id != loop_state.id
                                    && state
                                        .loops
                                        .iter()
                                        .any(|candidate| candidate.id == payload.loop_id)
                            });
                            if let Some(payload) = dropped.as_ref() {
                                self.pending_loop_drop = Some((payload.loop_id, loop_state.id));
                            }
                            self.show_loop_drop_menu(
                                ui,
                                &drop_zone.response,
                                loop_state.id,
                                dropped.is_some(),
                                &mut result,
                            );
                            drop_zone.inner
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
                        if loop_response.inner.click_track_requested {
                            result.click_track_requested = Some(loop_state.id);
                        }
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
        self.rendered_content_width = rendered_width;
        #[cfg(test)]
        {
            self.test_content_rect = Some(frame.response.rect);
        }
        if self.width_resizable {
            self.show_width_resize_handle(ui, frame.response.rect, "content_width_resize");
        }
        self.show_fx_logs(ui.ctx(), state, processor, &mut result);
        result
            .actions
            .extend(self.tiny_synth_fx_editor.show(ui.ctx(), state, processor));
        if !result.actions.is_empty()
            || !result.loop_actions.is_empty()
            || !result.io_intents.is_empty()
            || result.add_loop_requested
        {
            tracing::debug!(
                target: "Frontend.Egui",
                track_id = state.id.raw(),
                track_action_count = result.actions.len(),
                loop_action_count = result.loop_actions.len(),
                io_intent_count = result.io_intents.len(),
                add_loop_requested = result.add_loop_requested,
                "frontend.egui.track_interaction"
            );
        }
        result
    }

    fn show_loop_drop_menu(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        anchor_loop: LoopId,
        open: bool,
        result: &mut TrackWidgetResponse,
    ) {
        let Some((source, target)) = self.pending_loop_drop else {
            return;
        };
        if target != anchor_loop {
            return;
        }
        let popup_id = ui.id().with(("loop_drop_menu", target));
        let mut action = None;
        egui::Popup::menu(response)
            .id(popup_id)
            .open_memory(open.then_some(egui::SetOpenCommand::Bool(true)))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let duplicate = ui.button("Duplicate");
                let swap = ui.button("Swap");
                #[cfg(test)]
                {
                    self.test_drop_duplicate_rect = Some(duplicate.rect);
                    self.test_drop_swap_rect = Some(swap.rect);
                }
                if duplicate.clicked() {
                    action = Some(LoopWidgetAction::DuplicateTo(target));
                    ui.close();
                } else if swap.clicked() {
                    action = Some(LoopWidgetAction::SwapWith(target));
                    ui.close();
                }
            });
        if let Some(action) = action {
            result.loop_actions.push((source, action));
            self.pending_loop_drop = None;
        } else if !egui::Popup::is_id_open(ui.ctx(), popup_id) {
            self.pending_loop_drop = None;
        }
    }

    pub fn show_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &crate::TrackControlState,
    ) -> Vec<TrackWidgetAction> {
        self.show_controls_with_global_controls(ui, state, &GlobalControlState::default())
    }

    pub fn show_controls_with_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &crate::TrackControlState,
        global_controls: &GlobalControlState,
    ) -> Vec<TrackWidgetAction> {
        self.show_controls_with_height_and_global_controls(ui, state, true, global_controls)
    }

    fn show_controls_with_height_and_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &crate::TrackControlState,
        fill_available: bool,
        global_controls: &GlobalControlState,
    ) -> Vec<TrackWidgetAction> {
        let frame = egui::Frame::new()
            .fill(track_background(state))
            .inner_margin(TRACK_CONTROLS_MARGIN);
        let background = ui.painter().add(egui::Shape::Noop);
        let total_margin = frame.total_margin();
        let outer_min = ui.next_widget_position();
        let frame_content_min = egui::pos2(
            outer_min.x + total_margin.left,
            outer_min.y + total_margin.top,
        );
        let frame_content_height = if fill_available {
            (ui.available_height() - total_margin.sum().y).max(TRACK_CONTROLS_HEIGHT)
        } else {
            TRACK_CONTROLS_HEIGHT
        };
        let controls_min = egui::pos2(
            frame_content_min.x,
            frame_content_min.y + frame_content_height - TRACK_CONTROLS_HEIGHT,
        );
        let controls_bounds = egui::Rect::from_min_size(
            controls_min,
            egui::vec2(self.rendered_content_width, TRACK_CONTROLS_HEIGHT),
        );
        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("track_controls_content")
                .max_rect(controls_bounds)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let horizontal_clip = egui::Rect::from_min_max(
            egui::pos2(outer_min.x, ui.clip_rect().top()),
            egui::pos2(
                outer_min.x + self.rendered_content_width + total_margin.sum().x,
                ui.clip_rect().bottom(),
            ),
        );
        content_ui.set_clip_rect(ui.clip_rect().intersect(horizontal_clip));
        #[cfg(test)]
        {
            self.test_controls_clip_rect = Some(content_ui.clip_rect());
        }
        content_ui.set_width(self.rendered_content_width);
        content_ui.set_min_height(TRACK_CONTROLS_HEIGHT);
        let actions =
            self.controls
                .show_with_global_controls(&mut content_ui, state, global_controls);
        let content_rect = egui::Rect::from_min_size(
            frame_content_min,
            egui::vec2(self.rendered_content_width, frame_content_height),
        );
        ui.painter().set(background, frame.paint(content_rect));
        let outer_rect = frame.outer_rect(content_rect);
        let response = ui.allocate_rect(outer_rect, egui::Sense::hover());
        #[cfg(test)]
        {
            self.test_controls_rect = Some(response.rect);
        }
        if self.width_resizable {
            self.show_width_resize_handle(ui, response.rect, "controls_width_resize");
        }
        actions
    }

    fn show_width_resize_handle(
        &mut self,
        ui: &mut egui::Ui,
        track_rect: egui::Rect,
        id_salt: &'static str,
    ) {
        let handle_rect = egui::Rect::from_min_max(
            egui::pos2(track_rect.right() - RESIZE_HANDLE_RADIUS, track_rect.top()),
            egui::pos2(
                track_rect.right() + RESIZE_HANDLE_RADIUS,
                track_rect.bottom(),
            ),
        );
        let response = ui
            .interact(
                handle_rect,
                ui.make_persistent_id(id_salt),
                egui::Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
            .on_hover_text("Drag to resize track");
        if response.drag_started() {
            self.width_drag_start = Some(self.width);
        }
        if response.dragged() {
            self.width = (self.width_drag_start.unwrap_or(self.width)
                + response.total_drag_delta().unwrap_or_default().x)
                .clamp(MIN_TRACK_WIDTH, MAX_TRACK_WIDTH);
        }
        if response.drag_stopped() {
            self.width_drag_start = None;
        }
        if response.hovered() || response.dragged() {
            ui.painter().vline(
                track_rect.right(),
                track_rect.y_range(),
                ui.visuals().widgets.hovered.fg_stroke,
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn test_layout_rects(&self) -> (egui::Rect, egui::Rect) {
        (
            self.test_content_rect.unwrap(),
            self.test_controls_rect.unwrap(),
        )
    }

    fn show_header(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
        result: &mut TrackWidgetResponse,
    ) {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 24.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
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
                        let delete = ui.button("Delete Track");
                        #[cfg(test)]
                        {
                            self.test_delete_rect = Some(delete.rect);
                        }
                        if delete.clicked() {
                            result.actions.push(TrackWidgetAction::Remove);
                            ui.close();
                        }
                    }
                });
                #[cfg(test)]
                {
                    self.test_options_rect = Some(menu.response.rect);
                }
                menu.response.on_hover_text("Track options");

                if state.is_sync {
                    let click_track = ui
                        .add(egui::Button::new(egui::RichText::new("C")))
                        .on_hover_text("Generate a click track");
                    #[cfg(test)]
                    {
                        self.test_click_track_rect = Some(click_track.rect);
                    }
                    if click_track.clicked() {
                        result.click_track_requested = state.loops.first().map(|loop_| loop_.id);
                    }
                } else if let Some(fx) = &state.fx {
                    let features = processor.map(|value| value.features).unwrap_or_default();
                    let color = match fx.lifecycle {
                        FxLifecycle::Running if fx.active => egui::Color32::LIGHT_GREEN,
                        FxLifecycle::Running => egui::Color32::GRAY,
                        FxLifecycle::Starting | FxLifecycle::Restarting => egui::Color32::YELLOW,
                        FxLifecycle::Crashed | FxLifecycle::Unavailable => egui::Color32::LIGHT_RED,
                        FxLifecycle::Stopped => egui::Color32::GRAY,
                    };
                    let controllable =
                        features.external_ui || features.embedded_ui || features.recovery;
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

                let available = ui.available_width();
                if state.is_sync {
                    ui.add_sized([available, 24.0], egui::Label::new(&state.name));
                } else {
                    let name_id = ui.make_persistent_id("name");
                    let has_focus = ui.memory(|memory| memory.has_focus(name_id));
                    if !has_focus && self.source_name != state.name {
                        self.name_edit.clone_from(&state.name);
                        self.source_name.clone_from(&state.name);
                    }
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
                }
            },
        );
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

    fn full_frame(
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
                    egui::vec2(700.0, 400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| response = widget.show(ui, state),
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

    #[tracy_nextest_capture::tracy_capture_test]
    fn monitored_input_accents_the_whole_track() {
        assert_eq!(
            track_background(&crate::TrackControlState {
                input_monitoring: true,
                ..Default::default()
            }),
            colors::INPUT_ACTIVE_BACKGROUND
        );
        assert_eq!(
            track_background(&crate::TrackControlState::default()),
            colors::RAISED_BACKGROUND
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
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
                embedded_ui: false,
                recovery: true,
                logs: true,
            },
            editor: None,
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
                editor: None,
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

    #[tracy_nextest_capture::tracy_capture_test]
    fn sync_header_click_track_shortcut_targets_the_sync_loop() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let loop_id = LoopId::from_raw(1);
        let sync = TrackState {
            id: TrackId::from_raw(1),
            name: "Sync".to_owned(),
            is_sync: true,
            loops: vec![LoopState {
                id: loop_id,
                has_audio: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut widget = TrackWidget::default();
        let _ = frame(&context, &mut widget, &sync, Vec::new());
        let shortcut = widget.test_click_track_rect.unwrap().center();
        assert_eq!(
            click(&context, &mut widget, &sync, shortcut).click_track_requested,
            Some(loop_id)
        );
        assert!(widget.test_fx_rect.is_none());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn track_options_menu_requests_deletion_for_main_tracks_only() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackState {
            id: TrackId::from_raw(2),
            name: "Track".to_owned(),
            ..Default::default()
        };
        let mut widget = TrackWidget::default();
        let _ = frame(&context, &mut widget, &state, Vec::new());
        let options = widget.test_options_rect.unwrap().center();
        assert!(click(&context, &mut widget, &state, options)
            .actions
            .is_empty());
        let _ = frame(&context, &mut widget, &state, Vec::new());
        let delete = widget.test_delete_rect.unwrap().center();
        assert_eq!(
            click(&context, &mut widget, &state, delete).actions,
            [TrackWidgetAction::Remove]
        );

        let sync = TrackState {
            id: TrackId::from_raw(1),
            name: "Sync".to_owned(),
            is_sync: true,
            ..Default::default()
        };
        let mut sync_widget = TrackWidget::default();
        let _ = frame(&context, &mut sync_widget, &sync, Vec::new());
        let options = sync_widget.test_options_rect.unwrap().center();
        let _ = click(&context, &mut sync_widget, &sync, options);
        let _ = frame(&context, &mut sync_widget, &sync, Vec::new());
        assert!(sync_widget.test_delete_rect.is_none());
    }

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
    fn dropping_a_loop_on_a_peer_offers_duplicate_and_swap_actions() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let source = LoopId::from_raw(1);
        let target = LoopId::from_raw(2);
        let state = TrackState {
            id: TrackId::from_raw(1),
            loops: vec![
                LoopState {
                    id: source,
                    ..Default::default()
                },
                LoopState {
                    id: target,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut widget = TrackWidget::default();
        let _ = frame(&context, &mut widget, &state, Vec::new());
        let target_center = widget.test_loop_rects[1].center();

        let drop_on_target = |context: &egui::Context, widget: &mut TrackWidget| {
            egui::DragAndDrop::set_payload(context, LoopDragPayload { loop_id: source });
            let _ = frame(
                context,
                widget,
                &state,
                vec![
                    egui::Event::PointerMoved(target_center),
                    egui::Event::PointerButton {
                        pos: target_center,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            frame(
                context,
                widget,
                &state,
                vec![egui::Event::PointerButton {
                    pos: target_center,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
        };

        let response = drop_on_target(&context, &mut widget);
        assert!(response.loop_actions.is_empty());
        let _ = frame(&context, &mut widget, &state, Vec::new());
        let duplicate = widget
            .test_drop_duplicate_rect
            .expect("duplicate drop action");
        let response = click(&context, &mut widget, &state, duplicate.center());
        assert_eq!(
            response.loop_actions,
            [(source, LoopWidgetAction::DuplicateTo(target))]
        );

        let _ = drop_on_target(&context, &mut widget);
        let _ = frame(&context, &mut widget, &state, Vec::new());
        let swap = widget.test_drop_swap_rect.expect("swap drop action");
        let response = click(&context, &mut widget, &state, swap.center());
        assert_eq!(
            response.loop_actions,
            [(source, LoopWidgetAction::SwapWith(target))]
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
    fn fixed_width_track_ignores_resize_drags() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackState {
            id: TrackId::from_raw(1),
            name: "Sync".to_owned(),
            is_sync: true,
            ..Default::default()
        };
        let mut widget = TrackWidget::default();
        widget.width = MAX_TRACK_WIDTH;
        widget.set_width_resizable(false);
        let _ = full_frame(&context, &mut widget, &state, Vec::new());
        assert!(widget.test_controls_rect.unwrap().height() < 100.0);
        let edge = widget.test_content_rect.unwrap().right_center();
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(edge),
                egui::Event::PointerButton {
                    pos: edge,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerMoved(edge + egui::vec2(100.0, 0.0))],
        );
        assert_eq!(widget.width, DEFAULT_TRACK_WIDTH);
        assert!(widget.width_drag_start.is_none());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn content_and_controls_share_a_bounded_drag_resizable_width() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackState {
            id: TrackId::from_raw(1),
            name: "Track".to_owned(),
            loops: vec![LoopState {
                id: LoopId::from_raw(1),
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
        };
        let mut widget = TrackWidget::default();

        let _ = full_frame(&context, &mut widget, &state, Vec::new());
        assert_eq!(widget.width, DEFAULT_TRACK_WIDTH);
        let content_rect = widget.test_content_rect.unwrap();
        let controls_rect = widget.test_controls_rect.unwrap();
        assert!(
            (content_rect.width() - controls_rect.width()).abs() < f32::EPSILON,
            "content: {content_rect:?}, controls: {controls_rect:?}"
        );
        assert!((content_rect.left() - controls_rect.left()).abs() < f32::EPSILON);
        assert_eq!(content_rect.bottom(), controls_rect.top());

        let content_edge = content_rect.right_center();
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(content_edge),
                egui::Event::PointerButton {
                    pos: content_edge,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerMoved(
                content_edge + egui::vec2(30.0, 0.0),
            )],
        );
        assert_eq!(widget.width, DEFAULT_TRACK_WIDTH + 30.0);
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerMoved(
                content_edge + egui::vec2(80.0, 0.0),
            )],
        );
        assert_eq!(widget.width, DEFAULT_TRACK_WIDTH + 80.0);
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerMoved(
                content_edge + egui::vec2(500.0, 0.0),
            )],
        );
        assert_eq!(widget.width, MAX_TRACK_WIDTH);
        let (content_rect, controls_rect) = widget.test_layout_rects();
        assert_eq!(content_rect.x_range(), controls_rect.x_range());
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerButton {
                pos: content_edge + egui::vec2(500.0, 0.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );

        let _ = full_frame(&context, &mut widget, &state, Vec::new());
        let content_rect = widget.test_content_rect.unwrap();
        let controls_rect = widget.test_controls_rect.unwrap();
        assert!((content_rect.width() - controls_rect.width()).abs() < f32::EPSILON);
        assert!((content_rect.left() - controls_rect.left()).abs() < f32::EPSILON);

        let context = egui::Context::default();
        crate::initialize(&context);
        let mut widget = TrackWidget::default();
        let _ = full_frame(&context, &mut widget, &state, Vec::new());
        let controls_edge = widget.test_controls_rect.unwrap().right_center();
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![
                egui::Event::PointerMoved(controls_edge),
                egui::Event::PointerButton {
                    pos: controls_edge,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let resized_pointer = controls_edge - egui::vec2(100.0, 0.0);
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerMoved(resized_pointer)],
        );
        assert_eq!(widget.width, MIN_TRACK_WIDTH);
        let _ = full_frame(
            &context,
            &mut widget,
            &state,
            vec![egui::Event::PointerButton {
                pos: resized_pointer,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );

        let _ = full_frame(&context, &mut widget, &state, Vec::new());
        let content_rect = widget.test_content_rect.unwrap();
        let controls_rect = widget.test_controls_rect.unwrap();
        assert!(
            (content_rect.width() - controls_rect.width()).abs() < f32::EPSILON,
            "content: {content_rect:?}, controls: {controls_rect:?}"
        );
        assert!((content_rect.left() - controls_rect.left()).abs() < f32::EPSILON);
        assert_eq!(
            widget.test_controls_clip_rect.unwrap().x_range(),
            controls_rect.x_range()
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
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
