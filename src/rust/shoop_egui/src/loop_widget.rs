use egui_material_icons::icons::{
    ICON_ARROW_DOWNWARD, ICON_BORDER_CLEAR, ICON_EDIT_NOTE, ICON_FIBER_MANUAL_RECORD, ICON_HELP,
    ICON_HOURGLASS_EMPTY, ICON_PLAY_ARROW, ICON_STAR, ICON_STOP, ICON_VIEW_LIST,
};
use egui_material_icons::MaterialIcon;

use crate::{
    colors, composite_loop_widget::LoopDragPayload, dial::paint_dial,
    meter_ballistics::PeakMeterAnimation, optimistic_value::OptimisticValue, AppIntent,
    CompositeKind, GlobalControlState, LoopAudioExportFormat, LoopMidiExportFormat, LoopMode,
    LoopState, LoopWidgetAction, SelectionModifiers,
};

const TOUCH_MODE_ID: &str = "shoop_touch_mode";

pub(crate) fn set_touch_mode(context: &egui::Context, enabled: bool) {
    context.data_mut(|data| data.insert_temp(egui::Id::new(TOUCH_MODE_ID), enabled));
}

#[derive(Debug, Default)]
pub struct LoopWidgetResponse {
    pub actions: Vec<LoopWidgetAction>,
    pub io_intents: Vec<AppIntent>,
    pub click_track_requested: bool,
    pub(crate) hover_active: bool,
    close_context_menu: bool,
}

#[derive(Debug, Default)]
pub struct LoopWidget {
    gain: OptimisticValue<f32>,
    gain_drag_start: Option<f32>,
    balance: OptimisticValue<f32>,
    balance_drag_start: Option<f32>,
    play_popup_until: f64,
    record_popup_until: f64,
    balance_popup_until: f64,
    peak_left: PeakMeterAnimation,
    peak_right: PeakMeterAnimation,
    name_edit: String,
    source_name: String,
    pending_raw_export: Option<AppIntent>,
    #[cfg(test)]
    test_name_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_play_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_play_popup_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_play_popup_button_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_record_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_record_popup_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_gain_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_balance_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_duplicate_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_convert_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_drag_preview_rect: Option<egui::Rect>,
}

fn paint_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    icon: MaterialIcon,
    size: f32,
    color: egui::Color32,
) {
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        icon.codepoint,
        egui::FontId::new(size, icon.font_family()),
        color,
    );
}

fn icon_for_state(
    mode: LoopMode,
    empty: bool,
    regular_composite: bool,
    script_composite: bool,
) -> (MaterialIcon, egui::Color32, bool) {
    if empty {
        return (ICON_BORDER_CLEAR, colors::MUTED_FOREGROUND, false);
    }

    match mode {
        LoopMode::Playing => (
            ICON_PLAY_ARROW,
            if script_composite {
                colors::FOREGROUND
            } else {
                colors::PLAYING_STATE
            },
            false,
        ),
        LoopMode::PlayingDryThroughWet => (ICON_PLAY_ARROW, colors::DRY_THROUGH_WET, true),
        LoopMode::Recording => (ICON_FIBER_MANUAL_RECORD, colors::RECORD_ACTION, false),
        LoopMode::RecordingDryIntoWet => (ICON_FIBER_MANUAL_RECORD, colors::DRY_THROUGH_WET, true),
        LoopMode::Stopped if regular_composite => (ICON_VIEW_LIST, colors::DARK_BACKGROUND, false),
        LoopMode::Stopped if script_composite => (ICON_EDIT_NOTE, colors::DARK_BACKGROUND, false),
        LoopMode::Stopped => (ICON_STOP, colors::MUTED_FOREGROUND, false),
        _ => (ICON_HELP, colors::MUTED_FOREGROUND, false),
    }
}

fn paint_drag_preview(
    ui: &egui::Ui,
    response: &egui::Response,
    state: &LoopState,
    size: egui::Vec2,
    background: egui::Color32,
    border_color: egui::Color32,
) -> Option<egui::Rect> {
    if !response.dragged()
        || egui::DragAndDrop::payload::<LoopDragPayload>(ui.ctx())
            .is_none_or(|payload| payload.loop_id != state.id)
    {
        return None;
    }
    let pointer = ui.ctx().pointer_interact_pos()?;
    let rect = egui::Rect::from_center_size(pointer, size);
    let layer_id = egui::LayerId::new(
        egui::Order::Tooltip,
        ui.id().with(("loop_drag_preview", state.id)),
    );
    let mut painter = ui.ctx().layer_painter(layer_id);
    painter.set_opacity(0.62);
    painter.rect_filled(rect, 2.0, background);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(2.0, border_color),
        egui::StrokeKind::Inside,
    );
    let icon_rect = egui::Rect::from_min_size(rect.min, egui::vec2(24.0, 24.0));
    let (icon, color, fx) = icon_for_state(
        state.mode,
        state.empty,
        state.composite_kind == CompositeKind::Regular,
        state.composite_kind == CompositeKind::Script,
    );
    paint_icon(&painter, icon_rect.center(), icon, 21.0, color);
    if fx {
        painter.text(
            icon_rect.right_bottom() - egui::vec2(1.0, 1.0),
            egui::Align2::RIGHT_BOTTOM,
            "FX",
            egui::FontId::proportional(7.0),
            colors::FOREGROUND,
        );
    }
    let name_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 24.0, rect.top()),
        egui::pos2(rect.right() - 6.0, rect.bottom()),
    );
    painter.with_clip_rect(name_rect).text(
        name_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        &state.name,
        egui::FontId::proportional(11.0),
        colors::FOREGROUND,
    );
    Some(rect)
}

fn loop_icon_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id_salt: &'static str,
    icon: MaterialIcon,
    icon_size: f32,
    color: egui::Color32,
    tooltip: &str,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(id_salt), egui::Sense::click());
    paint_loop_button_background(ui, &response, rect);
    paint_icon(ui.painter(), rect.center(), icon, icon_size, color);
    tooltip_above(response, tooltip)
}

fn popup_icon_button(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    icon: MaterialIcon,
    icon_size: f32,
    color: egui::Color32,
    background: egui::Color32,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    ui.painter().rect_filled(rect, 0.0, background);
    paint_loop_button_background(ui, &response, rect);
    paint_icon(ui.painter(), rect.center(), icon, icon_size, color);
    tooltip_above(response, tooltip)
}

fn paint_loop_button_background(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect) {
    if response.hovered() || response.is_pointer_button_down_on() || response.has_focus() {
        ui.painter().rect_filled(
            rect.shrink2(egui::vec2(0.0, 2.0)),
            0.0,
            colors::LOOP_CONTROL_HOVER,
        );
    }
}

fn action_timing(sync: bool) -> &'static str {
    if sync {
        "synced"
    } else {
        "unsynced"
    }
}

fn action_tooltip(
    action: &str,
    controls: &GlobalControlState,
    records: bool,
    solo: bool,
) -> String {
    let mut description = action.to_owned();
    if records && controls.apply_n_cycles > 0 {
        let cycles = controls.apply_n_cycles;
        description.push_str(&format!(
            " for {cycles} {}",
            if cycles == 1 { "cycle" } else { "cycles" }
        ));
    }
    if records && controls.play_after_record {
        description.push_str(" then play");
    }
    if solo && controls.solo {
        description.push_str(" and stop others in same track(s)");
    }
    format!("{description} ({})", action_timing(controls.sync))
}

fn tooltip_above(response: egui::Response, tooltip: &str) -> egui::Response {
    let mut popup = egui::Tooltip::for_enabled(&response);
    popup.popup = popup
        .popup
        .align(egui::RectAlign::TOP)
        .align_alternatives(&[]);
    popup.show(|ui| {
        ui.label(tooltip);
    });
    response
}

fn peak_fraction(db: f32, minimum_db: f32) -> f32 {
    ((db - minimum_db) / -minimum_db).clamp(0.0, 1.0)
}

fn can_generate_click_track(state: &LoopState) -> bool {
    state.composite_kind == CompositeKind::None && (state.has_audio || state.has_midi)
}

fn loop_border_color(state: &LoopState) -> egui::Color32 {
    if state.targeted {
        colors::LOOP_TARGET_EDGE
    } else if state.selected {
        colors::LOOP_SELECTED_EDGE
    } else if state.selected_composite_kind != CompositeKind::None {
        colors::LOOP_COMPOSITE_REFERENCE_EDGE
    } else if state.empty {
        colors::MUTED_FOREGROUND
    } else {
        colors::LOOP_CONTENT_EDGE
    }
}

fn can_convert_to_composite(state: &LoopState) -> bool {
    state.composite_kind == CompositeKind::None
}

fn generated_loop_name(name: &str) -> bool {
    name.strip_prefix('(')
        .and_then(|name| name.strip_suffix(')'))
        .is_some_and(|name| {
            !name.is_empty() && name.chars().all(|character| character.is_ascii_digit())
        })
}

impl LoopWidget {
    fn show_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        state: &LoopState,
        result: &mut LoopWidgetResponse,
    ) {
        ui.label("Name");
        let name_id = ui.make_persistent_id("loop_name");
        let has_focus = ui.memory(|memory| memory.has_focus(name_id));
        if !has_focus && self.source_name != state.name {
            self.name_edit.clone_from(&state.name);
            self.source_name.clone_from(&state.name);
        }
        let name = ui.add(
            egui::TextEdit::singleline(&mut self.name_edit)
                .id(name_id)
                .desired_width(180.0),
        );
        #[cfg(test)]
        {
            self.test_name_rect = Some(name.rect);
        }
        let enter_pressed = has_focus && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if enter_pressed || name.lost_focus() {
            if self.name_edit != state.name {
                self.source_name.clone_from(&self.name_edit);
                result
                    .actions
                    .push(LoopWidgetAction::NameChanged(self.name_edit.clone()));
            }
            if enter_pressed {
                name.surrender_focus();
                result.close_context_menu = true;
            }
        }
        ui.separator();
        if !state.sync {
            let duplicate = ui.button("Clone");
            #[cfg(test)]
            {
                self.test_duplicate_rect = Some(duplicate.rect);
            }
            if duplicate.clicked() {
                result.actions.push(LoopWidgetAction::Duplicate);
                ui.close();
            }
            ui.separator();
        }
        if can_convert_to_composite(state) {
            let convert = ui.button("Convert to composite");
            #[cfg(test)]
            {
                self.test_convert_rect = Some(convert.rect);
            }
            if convert.clicked() {
                result.actions.push(LoopWidgetAction::ConvertToComposite);
                ui.close();
            }
            ui.separator();
        }
        if can_generate_click_track(state) {
            if ui.button("Generate click track…").clicked() {
                result.click_track_requested = true;
                ui.close();
            }
            ui.separator();
        }
        if state.has_audio {
            if ui.button("Save audio…").clicked() {
                result.io_intents.push(AppIntent::RequestLoopAudioExport {
                    loop_id: state.id,
                    format: LoopAudioExportFormat::Exact,
                });
                ui.close();
            }
            if ui.button("Save float WAV…").clicked() {
                result.io_intents.push(AppIntent::RequestLoopAudioExport {
                    loop_id: state.id,
                    format: LoopAudioExportFormat::FloatWav,
                });
                ui.close();
            }
            if ui
                .button("Save raw exact audio (includes retained margins)…")
                .clicked()
            {
                self.pending_raw_export = Some(AppIntent::RequestLoopAudioExport {
                    loop_id: state.id,
                    format: LoopAudioExportFormat::RawExact,
                });
                ui.close();
            }
            if ui
                .button("Save raw float WAV (includes retained margins)…")
                .clicked()
            {
                self.pending_raw_export = Some(AppIntent::RequestLoopAudioExport {
                    loop_id: state.id,
                    format: LoopAudioExportFormat::RawFloatWav,
                });
                ui.close();
            }
            if ui.button("Load audio…").clicked() {
                result
                    .io_intents
                    .push(AppIntent::RequestLoopAudioImportPicker { loop_id: state.id });
                ui.close();
            }
        }
        if state.has_recorded_fx_state && ui.button("Restore recorded FX state").clicked() {
            result
                .actions
                .push(LoopWidgetAction::RestoreRecordedFxState);
            ui.close();
        }
        if state.has_midi {
            if ui.button("Save exact MIDI…").clicked() {
                result.io_intents.push(AppIntent::RequestLoopMidiExport {
                    loop_id: state.id,
                    format: LoopMidiExportFormat::Exact,
                });
                ui.close();
            }
            if ui.button("Save standard MIDI…").clicked() {
                result.io_intents.push(AppIntent::RequestLoopMidiExport {
                    loop_id: state.id,
                    format: LoopMidiExportFormat::Standard,
                });
                ui.close();
            }
            if ui
                .button("Save raw standard MIDI (includes retained margins)…")
                .clicked()
            {
                self.pending_raw_export = Some(AppIntent::RequestLoopMidiExport {
                    loop_id: state.id,
                    format: LoopMidiExportFormat::RawStandard,
                });
                ui.close();
            }
            if ui.button("Load MIDI…").clicked() {
                result
                    .io_intents
                    .push(AppIntent::RequestLoopMidiImportPicker { loop_id: state.id });
                ui.close();
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &LoopState,
        size: egui::Vec2,
    ) -> LoopWidgetResponse {
        self.show_with_global_controls(ui, state, size, &GlobalControlState::default())
    }

    pub fn show_with_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &LoopState,
        size: egui::Vec2,
        controls: &GlobalControlState,
    ) -> LoopWidgetResponse {
        self.show_with_hover(ui, state, size, true, controls)
    }

    pub(crate) fn show_with_hover(
        &mut self,
        ui: &mut egui::Ui,
        state: &LoopState,
        size: egui::Vec2,
        hover_allowed: bool,
        controls: &GlobalControlState,
    ) -> LoopWidgetResponse {
        let mut result = LoopWidgetResponse::default();
        #[cfg(test)]
        {
            self.test_name_rect = None;
            self.test_duplicate_rect = None;
            self.test_convert_rect = None;
            self.test_drag_preview_rect = None;
        }
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        response.dnd_set_drag_payload(LoopDragPayload { loop_id: state.id });
        let popup_id = egui::Popup::default_response_id(&response);
        let touch_mode = ui.ctx().data(|data| {
            data.get_temp::<bool>(egui::Id::new(TOUCH_MODE_ID))
                .unwrap_or(false)
        });
        let hover_allowed = hover_allowed && !touch_mode;
        if touch_mode {
            self.play_popup_until = 0.0;
            self.record_popup_until = 0.0;
            self.balance_popup_until = 0.0;
        }
        let mut context_requested = response.secondary_clicked();
        let loop_visible = ui.clip_rect().intersect(rect).is_positive();
        if !loop_visible {
            self.play_popup_until = 0.0;
            self.record_popup_until = 0.0;
            self.balance_popup_until = 0.0;
        }
        let hovered = hover_allowed
            && loop_visible
            && response.contains_pointer()
            && ui.rect_contains_pointer(rect);
        let rounding = egui::CornerRadius::same(2);
        let background = if matches!(
            state.mode,
            LoopMode::Recording | LoopMode::RecordingDryIntoWet
        ) {
            colors::LOOP_RECORDING_BACKGROUND
        } else if state.composite_kind == CompositeKind::Regular {
            colors::LOOP_REGULAR_COMPOSITE
        } else if state.composite_kind == CompositeKind::Script {
            colors::LOOP_SCRIPT_COMPOSITE
        } else if !state.empty {
            colors::LOOP_AUDIO_BACKGROUND
        } else {
            colors::DARK_BACKGROUND
        };
        ui.painter().rect_filled(rect, rounding, background);

        if state.position > 0.0 {
            let progress_color = match state.mode {
                LoopMode::Playing => colors::LOOP_PROGRESS_PLAYING,
                LoopMode::PlayingDryThroughWet => colors::LOOP_PROGRESS_PLAYING_DRY,
                LoopMode::Recording => colors::LOOP_PROGRESS_RECORDING,
                LoopMode::RecordingDryIntoWet => colors::LOOP_PROGRESS_RECORDING_DRY,
                _ => colors::LOOP_PROGRESS_OTHER,
            };
            let progress_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(2.0, 2.0),
                egui::vec2(
                    (rect.width() - 4.0).max(0.0) * state.position,
                    (rect.height() - 4.0).max(0.0),
                ),
            );
            ui.painter().rect_filled(progress_rect, 0.0, progress_color);
        }

        if state.midi_activity {
            let midi_rect = egui::Rect::from_min_max(
                egui::pos2(
                    (rect.right() - 10.0).max(rect.left() + 2.0),
                    rect.top() + 2.0,
                ),
                egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0),
            );
            ui.painter()
                .rect_filled(midi_rect, 0.0, colors::MIDI_ACTIVITY);
        }

        let meter_color = colors::AUDIO_ACTIVITY;
        let meter_top = (rect.bottom() - 5.0).max(rect.top());
        let meter_bottom = (rect.bottom() - 2.0).max(meter_top);
        let now = ui.input(|input| input.time);
        let minimum_db = if state.stereo { -50.0 } else { -30.0 };
        let peak_left = self.peak_left.update(state.peak_left_db, minimum_db, now);
        let peak_right = self.peak_right.update(state.peak_right_db, minimum_db, now);
        if peak_left.animating || peak_right.animating {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        if state.stereo {
            let center = rect.center().x;
            let half_width = (rect.width() - 4.0).max(0.0) / 2.0;
            let left_width = half_width * peak_fraction(peak_left.db, minimum_db);
            let right_width = half_width * peak_fraction(peak_right.db, minimum_db);
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(center - left_width, meter_top),
                    egui::pos2(center, meter_bottom),
                ),
                0.0,
                meter_color,
            );
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(center, meter_top),
                    egui::pos2(center + right_width, meter_bottom),
                ),
                0.0,
                meter_color,
            );
        } else {
            let meter_width =
                (rect.width() - 4.0).max(0.0) * peak_fraction(peak_left.db, minimum_db);
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 2.0, meter_top),
                    egui::vec2(meter_width, meter_bottom - meter_top),
                ),
                0.0,
                meter_color,
            );
        }

        let border_color = loop_border_color(state);
        ui.painter().rect_stroke(
            rect,
            rounding,
            egui::Stroke::new(2.0, border_color),
            egui::StrokeKind::Inside,
        );

        let icon_rect = egui::Rect::from_min_size(rect.min, egui::vec2(24.0, 24.0));
        let has_transition = state.next_transition_delay.is_some() && state.mode != state.next_mode;
        if has_transition {
            let transition_delay = state.next_transition_delay.unwrap_or_default();
            if transition_delay > 0 {
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    (transition_delay + 1).to_string(),
                    egui::FontId::proportional(12.0),
                    colors::FOREGROUND,
                );
            } else {
                paint_icon(
                    ui.painter(),
                    icon_rect.center(),
                    ICON_HOURGLASS_EMPTY,
                    20.0,
                    colors::FOREGROUND,
                );
            }
            let (next_icon, next_color, next_fx) =
                icon_for_state(state.next_mode, false, false, false);
            let next_rect = egui::Rect::from_min_size(
                egui::pos2(icon_rect.right() - 15.0, icon_rect.bottom() - 15.0),
                egui::vec2(15.0, 15.0),
            );
            paint_icon(
                ui.painter(),
                next_rect.center(),
                next_icon,
                13.0,
                next_color,
            );
            if next_fx {
                ui.painter().text(
                    next_rect.right_bottom(),
                    egui::Align2::RIGHT_BOTTOM,
                    "FX",
                    egui::FontId::proportional(5.0),
                    colors::FOREGROUND,
                );
            }
        } else {
            let (icon, color, fx) = icon_for_state(
                state.mode,
                state.empty,
                state.composite_kind == CompositeKind::Regular,
                state.composite_kind == CompositeKind::Script,
            );
            paint_icon(ui.painter(), icon_rect.center(), icon, 21.0, color);
            if fx {
                ui.painter().text(
                    icon_rect.right_bottom() - egui::vec2(1.0, 1.0),
                    egui::Align2::RIGHT_BOTTOM,
                    "FX",
                    egui::FontId::proportional(7.0),
                    colors::FOREGROUND,
                );
            }
        }

        if state.sync {
            paint_icon(
                ui.painter(),
                egui::pos2(rect.left() + 6.0, rect.top() + 6.0),
                ICON_STAR,
                10.0,
                colors::LOOP_SYNC_MARKER,
            );
        }

        let icon_click_rect = if hovered {
            egui::Rect::from_min_max(
                icon_rect.min,
                egui::pos2(icon_rect.left() + 20.0, icon_rect.bottom()),
            )
        } else {
            icon_rect
        };
        let icon_response = ui.interact(
            icon_click_rect,
            ui.id().with("loop_state_icon"),
            egui::Sense::click(),
        );
        context_requested |= icon_response.secondary_clicked();
        if icon_response.double_clicked() {
            result.actions.push(LoopWidgetAction::IconDoubleClicked);
        } else if icon_response.clicked() {
            result
                .actions
                .push(LoopWidgetAction::IconClicked(SelectionModifiers {
                    additive: ui.input(|input| input.modifiers.command),
                }));
        }

        let dial_rect = state.show_gain.then(|| {
            egui::Rect::from_center_size(
                egui::pos2(rect.right() - 14.0, rect.center().y),
                egui::vec2(18.0, 18.0),
            )
        });
        let controls_left = rect.left() + 25.0;
        let controls_right = dial_rect
            .map(|dial| dial.left() - 2.0)
            .unwrap_or(rect.right() - 2.0);
        let gap = 1.0;
        let button_width = ((controls_right - controls_left - gap * 2.0) / 3.0).clamp(1.0, 20.0);
        let button_height = rect.height().min(26.0).max(1.0);
        let play_rect = egui::Rect::from_min_size(
            egui::pos2(controls_left, rect.center().y - button_height / 2.0),
            egui::vec2(button_width, button_height),
        );
        let record_rect = play_rect.translate(egui::vec2(button_width + gap, 0.0));
        let stop_rect = if state.composite_kind == CompositeKind::Script {
            record_rect
        } else {
            record_rect.translate(egui::vec2(button_width + gap, 0.0))
        };
        let play_popup_rect = play_rect.translate(egui::vec2(0.0, button_height + gap));
        let record_popup_rect = egui::Rect::from_min_size(
            record_rect.min + egui::vec2(0.0, button_height + gap),
            egui::vec2(button_width, button_height * 2.0),
        );
        let balance_rect =
            dial_rect.map(|dial| dial.translate(egui::vec2(dial.width() + 4.0, 0.0)));
        #[cfg(test)]
        {
            self.test_play_rect = Some(play_rect);
            self.test_play_popup_rect = Some(play_popup_rect);
            self.test_record_rect = Some(record_rect);
            self.test_record_popup_rect = Some(record_popup_rect);
            self.test_gain_rect = dial_rect;
            self.test_balance_rect = balance_rect;
        }
        let pointer = ui.input(|input| input.pointer.hover_pos());
        let non_script = state.composite_kind != CompositeKind::Script;
        let play_hovered = hovered && pointer.is_some_and(|pointer| play_rect.contains(pointer));
        let record_hovered =
            hovered && pointer.is_some_and(|pointer| record_rect.contains(pointer));
        if non_script && play_hovered {
            self.play_popup_until = now + 0.08;
        }
        if non_script && record_hovered {
            self.record_popup_until = now + 0.08;
        }
        let show_play_popup = loop_visible
            && hover_allowed
            && non_script
            && (play_hovered || now < self.play_popup_until);
        let show_record_popup = loop_visible
            && hover_allowed
            && non_script
            && (record_hovered || now < self.record_popup_until);
        let controls_visible = touch_mode || hovered || show_play_popup || show_record_popup;
        let icon_size = button_width.min(button_height) * 0.95;
        let play_tooltip = action_tooltip("Play", controls, false, true);
        let record_tooltip = action_tooltip("Record", controls, true, true);
        let stop_tooltip = action_tooltip("Stop", controls, false, false);
        let play_dry_tooltip =
            action_tooltip("Play dry through live effects", controls, false, true);
        let grab_tooltip = action_tooltip("Grab always-on recording", controls, true, true);

        if controls_visible {
            let play_response = loop_icon_button(
                ui,
                play_rect,
                "play",
                ICON_PLAY_ARROW,
                icon_size,
                if non_script {
                    colors::PLAY_ACTION
                } else {
                    colors::FOREGROUND
                },
                &play_tooltip,
            );
            context_requested |= play_response.secondary_clicked();
            if play_response.clicked() {
                result.actions.push(LoopWidgetAction::PlayClicked);
            }
            if non_script {
                let record_response =
                    ui.interact(record_rect, ui.id().with("record"), egui::Sense::click());
                paint_loop_button_background(ui, &record_response, record_rect);
                paint_icon(
                    ui.painter(),
                    record_rect.center(),
                    ICON_FIBER_MANUAL_RECORD,
                    icon_size,
                    colors::RECORD_ACTION,
                );
                if state.play_after_record {
                    paint_icon(
                        &ui.painter().with_clip_rect(egui::Rect::from_min_max(
                            egui::pos2(record_rect.center().x, record_rect.top()),
                            record_rect.max,
                        )),
                        record_rect.center(),
                        ICON_FIBER_MANUAL_RECORD,
                        icon_size,
                        colors::PLAY_ACTION,
                    );
                }
                let record_response = tooltip_above(record_response, &record_tooltip);
                context_requested |= record_response.secondary_clicked();
                if record_response.clicked() {
                    result.actions.push(LoopWidgetAction::RecordClicked);
                }
            }
            let stop_response = loop_icon_button(
                ui,
                stop_rect,
                "stop",
                ICON_STOP,
                icon_size,
                colors::FOREGROUND,
                &stop_tooltip,
            );
            context_requested |= stop_response.secondary_clicked();
            if stop_response.clicked() {
                result.actions.push(LoopWidgetAction::StopClicked);
            }
        } else {
            let name_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 24.0, rect.top()),
                egui::pos2(rect.right() - 6.0, rect.bottom()),
            );
            ui.painter().with_clip_rect(name_rect).text(
                name_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                &state.name,
                egui::FontId::proportional(11.0),
                if generated_loop_name(&state.name) {
                    colors::MUTED_FOREGROUND
                } else {
                    colors::FOREGROUND
                },
            );
        }

        if show_play_popup {
            let popup = egui::Area::new(ui.id().with("play_dry_popup"))
                .order(egui::Order::Foreground)
                .fixed_pos(play_popup_rect.min)
                .constrain(false)
                .show(ui.ctx(), |ui| {
                    let response = popup_icon_button(
                        ui,
                        play_popup_rect.size(),
                        ICON_PLAY_ARROW,
                        icon_size,
                        colors::DRY_THROUGH_WET,
                        background,
                        &play_dry_tooltip,
                    );
                    #[cfg(test)]
                    {
                        self.test_play_popup_button_rect = Some(response.rect);
                    }
                    if response.clicked() {
                        result.actions.push(LoopWidgetAction::PlayDryClicked);
                    }
                    let contains_pointer = response.contains_pointer();
                    let secondary_clicked = response.secondary_clicked()
                        || (contains_pointer
                            && ui.input(|input| {
                                input
                                    .pointer
                                    .button_released(egui::PointerButton::Secondary)
                            }));
                    (contains_pointer, secondary_clicked)
                });
            context_requested |= popup.inner.1;
            if hover_allowed && popup.inner.0 {
                self.play_popup_until = now + 0.08;
            }
        }
        if show_record_popup {
            let popup = egui::Area::new(ui.id().with("record_variants_popup"))
                .order(egui::Order::Foreground)
                .fixed_pos(record_popup_rect.min)
                .constrain(false)
                .show(ui.ctx(), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let grab = popup_icon_button(
                        ui,
                        egui::vec2(button_width, button_height),
                        ICON_ARROW_DOWNWARD,
                        icon_size,
                        colors::RECORD_ACTION,
                        background,
                        &grab_tooltip,
                    );
                    if state.play_after_record {
                        paint_icon(
                            &ui.painter().with_clip_rect(egui::Rect::from_min_max(
                                egui::pos2(grab.rect.left(), grab.rect.center().y),
                                grab.rect.max,
                            )),
                            grab.rect.center(),
                            ICON_ARROW_DOWNWARD,
                            icon_size,
                            colors::PLAY_ACTION,
                        );
                    }
                    if grab.clicked() {
                        result.actions.push(LoopWidgetAction::GrabClicked);
                    }
                    let rerecord = popup_icon_button(
                        ui,
                        egui::vec2(button_width, button_height),
                        ICON_FIBER_MANUAL_RECORD,
                        icon_size,
                        colors::DRY_THROUGH_WET,
                        background,
                        "Re-record dry through live effects for one loop cycle",
                    );
                    if rerecord.clicked() {
                        result.actions.push(LoopWidgetAction::RerecordClicked);
                    }
                    let contains_pointer = grab.contains_pointer() || rerecord.contains_pointer();
                    let secondary_clicked = grab.secondary_clicked()
                        || rerecord.secondary_clicked()
                        || (contains_pointer
                            && ui.input(|input| {
                                input
                                    .pointer
                                    .button_released(egui::PointerButton::Secondary)
                            }));
                    (contains_pointer, secondary_clicked)
                });
            context_requested |= popup.inner.1;
            if hover_allowed && popup.inner.0 {
                self.record_popup_until = now + 0.08;
            }
        }

        if let Some(dial_rect) = dial_rect {
            let dial_response = ui.interact(
                dial_rect,
                ui.id().with("loop_gain"),
                egui::Sense::click_and_drag(),
            );
            context_requested |= dial_response.secondary_clicked();
            let displayed_gain = self
                .gain
                .resolve(state.gain, self.gain_drag_start.is_some());
            if dial_response.drag_started() {
                self.gain_drag_start = Some(displayed_gain);
            }
            let mut gain = displayed_gain;
            if dial_response.dragged() {
                let start = self.gain_drag_start.unwrap_or(displayed_gain);
                gain = (start - dial_response.total_drag_delta().unwrap_or_default().y / 100.0)
                    .clamp(0.0, 1.0);
            }
            if dial_response.double_clicked() {
                gain = 0.6;
            }
            if (gain - displayed_gain).abs() > f32::EPSILON {
                self.gain.set(gain);
                result.actions.push(LoopWidgetAction::GainChanged(gain));
            }
            if dial_response.drag_stopped() {
                self.gain_drag_start = None;
            }
            paint_dial(ui, &dial_response, dial_rect, gain, "V");

            if state.stereo {
                let balance_rect = balance_rect.expect("stereo gain has a balance rectangle");
                let balance_hovered = hover_allowed
                    && dial_response.contains_pointer()
                    && ui.rect_contains_pointer(dial_rect);
                if balance_hovered
                    || self.gain_drag_start.is_some()
                    || self.balance_drag_start.is_some()
                {
                    self.balance_popup_until = now + 0.08;
                }
                if loop_visible
                    && hover_allowed
                    && (balance_hovered || now < self.balance_popup_until)
                {
                    let popup = egui::Area::new(ui.id().with("loop_balance_popup"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(balance_rect.min)
                        .constrain(false)
                        .show(ui.ctx(), |ui| {
                            let (allocated, response) = ui.allocate_exact_size(
                                balance_rect.size(),
                                egui::Sense::click_and_drag(),
                            );
                            let displayed_balance = self
                                .balance
                                .resolve(state.balance, self.balance_drag_start.is_some());
                            if response.drag_started() {
                                self.balance_drag_start = Some(displayed_balance);
                            }
                            let mut balance = displayed_balance;
                            if response.dragged() {
                                let start = self.balance_drag_start.unwrap_or(displayed_balance);
                                balance = (start
                                    - response.total_drag_delta().unwrap_or_default().y / 50.0)
                                    .clamp(-1.0, 1.0);
                            }
                            if response.double_clicked() {
                                balance = 0.0;
                            }
                            if (balance - displayed_balance).abs() > f32::EPSILON {
                                self.balance.set(balance);
                                result
                                    .actions
                                    .push(LoopWidgetAction::BalanceChanged(balance));
                            }
                            if response.drag_stopped() {
                                self.balance_drag_start = None;
                            }
                            paint_dial(ui, &response, allocated, (balance + 1.0) / 2.0, "B");
                            let contains_pointer = response.contains_pointer();
                            let secondary_clicked = response.secondary_clicked()
                                || (contains_pointer
                                    && ui.input(|input| {
                                        input
                                            .pointer
                                            .button_released(egui::PointerButton::Secondary)
                                    }));
                            (
                                response
                                    .on_hover_text("Loop stereo balance")
                                    .contains_pointer(),
                                secondary_clicked,
                            )
                        });
                    context_requested |= popup.inner.1;
                    if popup.inner.0 {
                        self.balance_popup_until = now + 0.08;
                    }
                }
            }
        }
        egui::Popup::context_menu(&response)
            .open_memory(context_requested.then_some(egui::SetOpenCommand::Bool(true)))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| self.show_context_menu(ui, state, &mut result));
        if result.close_context_menu {
            egui::Popup::close_id(ui.ctx(), popup_id);
        }

        self.show_raw_export_confirmation(ui.ctx(), state, &mut result);

        let drag_preview_rect =
            paint_drag_preview(ui, &response, state, size, background, border_color);
        #[cfg(test)]
        {
            self.test_drag_preview_rect = drag_preview_rect;
        }
        #[cfg(not(test))]
        let _ = drag_preview_rect;

        result.hover_active = hover_allowed
            && loop_visible
            && (controls_visible
                || now < self.balance_popup_until
                || self.gain_drag_start.is_some()
                || self.balance_drag_start.is_some());
        if result.hover_active {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }

        result
    }

    fn show_raw_export_confirmation(
        &mut self,
        context: &egui::Context,
        state: &LoopState,
        result: &mut LoopWidgetResponse,
    ) {
        let Some(intent) = self.pending_raw_export.clone() else {
            return;
        };
        let belongs_to_loop = match &intent {
            AppIntent::RequestLoopAudioExport { loop_id, .. }
            | AppIntent::RequestLoopMidiExport { loop_id, .. } => *loop_id == state.id,
            _ => false,
        };
        if !belongs_to_loop {
            self.pending_raw_export = None;
            return;
        }
        let mut open = true;
        egui::Window::new("Export retained raw media?")
            .id(egui::Id::new(("raw_export_confirmation", state.id)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(
                    "Raw export includes retained pre/post material and may expose frames outside the logical loop. It does not mutate or consolidate the take.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Export raw media").clicked() {
                        result.io_intents.push(intent.clone());
                        self.pending_raw_export = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_raw_export = None;
                    }
                });
            });
        if !open {
            self.pending_raw_export = None;
        }
    }

    #[cfg(test)]
    pub(crate) fn test_record_rect(&self) -> Option<egui::Rect> {
        self.test_record_rect
    }

    #[cfg(test)]
    pub(crate) fn test_record_popup_rect(&self) -> Option<egui::Rect> {
        self.test_record_popup_rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LoopId;

    fn frame(
        context: &egui::Context,
        widget: &mut LoopWidget,
        state: &LoopState,
        time: f64,
        events: Vec<egui::Event>,
    ) -> LoopWidgetResponse {
        let mut response = LoopWidgetResponse::default();
        let mut ignored_output_0 = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 200.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| response = widget.show(ui, state, egui::vec2(180.0, 26.0)),
        );
        ignored_output_0.textures_delta.clear();
        response
    }

    fn clipped_frame(
        context: &egui::Context,
        widget: &mut LoopWidget,
        state: &LoopState,
        time: f64,
        events: Vec<egui::Event>,
    ) -> LoopWidgetResponse {
        let mut response = LoopWidgetResponse::default();
        let mut ignored_output_1 = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 200.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_clip_rect(egui::Rect::from_min_max(
                    egui::pos2(0.0, 50.0),
                    egui::pos2(400.0, 200.0),
                ));
                response = widget.show(ui, state, egui::vec2(180.0, 26.0));
            },
        );
        ignored_output_1.textures_delta.clear();
        response
    }

    fn pointer(position: egui::Pos2) -> egui::Event {
        egui::Event::PointerMoved(position)
    }

    fn click(
        context: &egui::Context,
        widget: &mut LoopWidget,
        state: &LoopState,
        position: egui::Pos2,
        time: f64,
    ) -> LoopWidgetResponse {
        let _ = frame(
            context,
            widget,
            state,
            time,
            vec![
                pointer(position),
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
            time + 0.01,
            vec![
                pointer(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
    }

    fn secondary_click(
        context: &egui::Context,
        widget: &mut LoopWidget,
        state: &LoopState,
        position: egui::Pos2,
        time: f64,
    ) -> LoopWidgetResponse {
        let _ = frame(
            context,
            widget,
            state,
            time,
            vec![
                pointer(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            context,
            widget,
            state,
            time + 0.01,
            vec![
                pointer(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
    }

    fn context_menu_frame(
        context: &egui::Context,
        widget: &mut LoopWidget,
        state: &LoopState,
        time: f64,
        events: Vec<egui::Event>,
    ) -> LoopWidgetResponse {
        let mut response = LoopWidgetResponse::default();
        let mut ignored_output_2 = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| widget.show_context_menu(ui, state, &mut response),
        );
        ignored_output_2.textures_delta.clear();
        response
    }

    fn state() -> LoopState {
        LoopState {
            id: LoopId::from_raw(1),
            show_gain: true,
            stereo: true,
            balance: 0.5,
            ..Default::default()
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn action_tooltips_describe_active_global_behavior() {
        let mut controls = GlobalControlState {
            sync: true,
            play_after_record: true,
            solo: false,
            apply_n_cycles: 0,
            ..Default::default()
        };
        assert_eq!(
            action_tooltip("Record", &controls, true, true),
            "Record then play (synced)"
        );
        assert_eq!(
            action_tooltip("Stop", &controls, false, false),
            "Stop (synced)"
        );

        controls.sync = false;
        controls.play_after_record = false;
        assert_eq!(
            action_tooltip("Record", &controls, true, true),
            "Record (unsynced)"
        );

        controls.solo = true;
        controls.apply_n_cycles = 2;
        assert_eq!(
            action_tooltip("Record", &controls, true, true),
            "Record for 2 cycles and stop others in same track(s) (unsynced)"
        );
        assert_eq!(
            action_tooltip("Play", &controls, false, true),
            "Play and stop others in same track(s) (unsynced)"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn raw_export_is_gated_by_an_explicit_confirmation_window() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = LoopState {
            id: LoopId::from_raw(77),
            name: "Raw take".to_owned(),
            has_audio: true,
            ..Default::default()
        };
        let mut widget = LoopWidget {
            pending_raw_export: Some(AppIntent::RequestLoopAudioExport {
                loop_id: state.id,
                format: LoopAudioExportFormat::RawExact,
            }),
            ..Default::default()
        };
        let response = frame(&context, &mut widget, &state, 0.0, Vec::new());
        assert!(response.io_intents.is_empty());
        assert!(widget.pending_raw_export.is_some());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn action_buttons_use_the_full_row_height_without_covering_the_state_icon() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut widget = LoopWidget::default();
        let _ = frame(
            &context,
            &mut widget,
            &state,
            1.0,
            vec![pointer(egui::pos2(100.0, 13.0))],
        );

        let play = widget.test_play_rect.unwrap();
        assert!(play.left() >= 24.0);
        assert_eq!(play.y_range(), 0.0..=26.0);
        assert!(widget.test_record_rect.unwrap().left() > play.right());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn clipped_loop_does_not_activate_hover_overlays() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut widget = LoopWidget::default();
        let _ = frame(&context, &mut widget, &state, 1.0, Vec::new());

        let play = widget.test_play_rect.unwrap().center();
        let response = clipped_frame(&context, &mut widget, &state, 1.1, vec![pointer(play)]);
        assert!(!response.hover_active);
        assert_eq!(widget.play_popup_until, 0.0);
        assert_eq!(widget.record_popup_until, 0.0);
        assert!(widget.test_play_popup_button_rect.is_none());

        let gain = widget.test_gain_rect.unwrap().center();
        let response = clipped_frame(&context, &mut widget, &state, 1.2, vec![pointer(gain)]);
        assert!(!response.hover_active);
        assert_eq!(widget.balance_popup_until, 0.0);

        let _ = frame(&context, &mut widget, &state, 1.9, Vec::new());
        let _ = frame(&context, &mut widget, &state, 2.0, vec![pointer(play)]);
        assert!(widget.play_popup_until > 2.0);
        let popup = widget.test_play_popup_rect.unwrap().center();
        let response = clipped_frame(&context, &mut widget, &state, 2.02, vec![pointer(popup)]);
        assert!(!response.hover_active);
        assert_eq!(widget.play_popup_until, 0.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn touch_mode_shows_direct_controls_without_hover_variants() {
        let context = egui::Context::default();
        crate::initialize(&context);
        set_touch_mode(&context, true);
        let state = state();
        let mut widget = LoopWidget::default();
        widget.gain_drag_start = Some(state.gain);
        let response = frame(&context, &mut widget, &state, 1.0, Vec::new());
        assert!(!response.hover_active);
        assert_eq!(widget.gain_drag_start, Some(state.gain));
        widget.gain_drag_start = None;

        let play = widget.test_play_rect.unwrap().center();
        let response = click(&context, &mut widget, &state, play, 1.1);
        assert_eq!(response.actions, [LoopWidgetAction::PlayClicked]);
        assert_eq!(widget.play_popup_until, 0.0);
        assert!(widget.test_play_popup_button_rect.is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn hover_overlays_extend_outside_the_row_and_retain_child_hover() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut widget = LoopWidget::default();
        let _ = frame(&context, &mut widget, &state, 1.0, Vec::new());
        let _ = frame(
            &context,
            &mut widget,
            &state,
            1.05,
            vec![pointer(egui::pos2(100.0, 13.0))],
        );
        let play = widget.test_play_rect.unwrap().center();
        let _ = frame(&context, &mut widget, &state, 1.1, vec![pointer(play)]);
        let play_popup = widget.test_play_popup_rect.unwrap();
        assert!(play_popup.top() >= widget.test_play_rect.unwrap().bottom());
        assert!(widget.test_play_popup_button_rect.is_some());
        let _ = frame(
            &context,
            &mut widget,
            &state,
            1.12,
            vec![pointer(play_popup.center())],
        );
        assert!(widget.play_popup_until > 1.12);

        let record = widget.test_record_rect.unwrap().center();
        let _ = frame(&context, &mut widget, &state, 2.0, vec![pointer(record)]);
        let record_popup = widget.test_record_popup_rect.unwrap();
        assert!(record_popup.top() >= widget.test_record_rect.unwrap().bottom());
        let _ = frame(
            &context,
            &mut widget,
            &state,
            2.02,
            vec![pointer(record_popup.center())],
        );
        assert!(widget.record_popup_until > 2.02);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn context_menu_opens_from_state_actions_dial_and_dropped_controls() {
        for target in [
            "state", "play", "record", "stop", "dial", "dropped", "balance",
        ] {
            let context = egui::Context::default();
            crate::initialize(&context);
            let state = state();
            let mut widget = LoopWidget::default();
            let _ = frame(
                &context,
                &mut widget,
                &state,
                1.0,
                vec![pointer(egui::pos2(100.0, 13.0))],
            );
            let position = match target {
                "state" => egui::pos2(10.0, 13.0),
                "play" => widget.test_play_rect.unwrap().center(),
                "record" => widget.test_record_rect.unwrap().center(),
                "stop" => widget
                    .test_record_rect
                    .unwrap()
                    .translate(egui::vec2(
                        widget.test_record_rect.unwrap().width() + 1.0,
                        0.0,
                    ))
                    .center(),
                "dial" => widget.test_gain_rect.unwrap().center(),
                "dropped" => {
                    let record = widget.test_record_rect.unwrap().center();
                    let _ = frame(&context, &mut widget, &state, 1.05, vec![pointer(record)]);
                    widget.test_record_popup_rect.unwrap().center_top()
                        + egui::vec2(0.0, widget.test_record_rect.unwrap().height() / 2.0)
                }
                "balance" => {
                    let gain = widget.test_gain_rect.unwrap().center();
                    let _ = frame(&context, &mut widget, &state, 1.05, vec![pointer(gain)]);
                    widget.test_balance_rect.unwrap().center()
                }
                _ => unreachable!(),
            };
            let _ = secondary_click(&context, &mut widget, &state, position, 1.1);
            assert!(
                widget.test_convert_rect.is_some(),
                "context menu did not open from {target}"
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn context_menu_edits_the_loop_name_inline() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = LoopState {
            name: String::new(),
            ..state()
        };
        let mut widget = LoopWidget::default();
        let _ = frame(&context, &mut widget, &state, 1.0, Vec::new());
        let _ = secondary_click(&context, &mut widget, &state, egui::pos2(100.0, 13.0), 1.1);
        let _ = frame(&context, &mut widget, &state, 1.15, Vec::new());
        let name_rect = widget.test_name_rect.unwrap();
        let name = name_rect.center();
        let _ = click(&context, &mut widget, &state, name, 1.2);
        assert!(
            widget.test_name_rect.is_some(),
            "context menu closed when the name field was focused"
        );
        let _ = frame(
            &context,
            &mut widget,
            &state,
            1.3,
            vec![egui::Event::Text("Verse".to_owned())],
        );
        assert_eq!(widget.name_edit, "Verse", "name field was not focused");
        let response = frame(
            &context,
            &mut widget,
            &state,
            1.4,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            response.actions,
            [LoopWidgetAction::NameChanged("Verse".to_owned())]
        );
        assert!(response.close_context_menu);
        assert!(!egui::Popup::is_any_open(&context));
        let _ = frame(&context, &mut widget, &state, 1.5, Vec::new());
        assert!(
            widget.test_name_rect.is_none(),
            "context menu stayed open after accepting the name"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn context_menu_labels_clone_and_requests_duplication_for_non_sync_loops() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut widget = LoopWidget::default();
        let state = state();
        let mut initial_response = LoopWidgetResponse::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                time: Some(1.0),
                ..Default::default()
            },
            |ui| widget.show_context_menu(ui, &state, &mut initial_response),
        );
        output.textures_delta.clear();
        let painted_text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(painted_text.contains(&"Clone"));
        assert!(!painted_text.contains(&"Duplicate"));
        let duplicate = widget.test_duplicate_rect.expect("clone menu item");
        let _ = context_menu_frame(
            &context,
            &mut widget,
            &state,
            1.1,
            vec![
                pointer(duplicate.center()),
                egui::Event::PointerButton {
                    pos: duplicate.center(),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = context_menu_frame(
            &context,
            &mut widget,
            &state,
            1.2,
            vec![
                pointer(duplicate.center()),
                egui::Event::PointerButton {
                    pos: duplicate.center(),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(response.actions, [LoopWidgetAction::Duplicate]);

        widget.test_duplicate_rect = None;
        let sync = LoopState {
            sync: true,
            ..state
        };
        let _ = context_menu_frame(&context, &mut widget, &sync, 2.0, Vec::new());
        assert!(widget.test_duplicate_rect.is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn context_menu_routes_conversion_only_for_primitive_loops() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut widget = LoopWidget::default();
        let primitive = state();
        let _ = context_menu_frame(&context, &mut widget, &primitive, 1.0, Vec::new());
        let convert = widget.test_convert_rect.expect("conversion menu item");
        let _ = context_menu_frame(
            &context,
            &mut widget,
            &primitive,
            1.1,
            vec![
                pointer(convert.center()),
                egui::Event::PointerButton {
                    pos: convert.center(),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = context_menu_frame(
            &context,
            &mut widget,
            &primitive,
            1.2,
            vec![
                pointer(convert.center()),
                egui::Event::PointerButton {
                    pos: convert.center(),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(response
            .actions
            .contains(&LoopWidgetAction::ConvertToComposite));

        let composite = LoopState {
            composite_kind: CompositeKind::Regular,
            ..primitive
        };
        widget.test_convert_rect = None;
        let _ = context_menu_frame(&context, &mut widget, &composite, 2.0, Vec::new());
        assert!(widget.test_convert_rect.is_none());
        assert!(!can_convert_to_composite(&composite));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dragging_a_loop_sets_and_releases_its_stable_payload() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut widget = LoopWidget::default();
        let start = egui::pos2(90.0, 13.0);
        let _ = frame(&context, &mut widget, &state, 1.0, Vec::new());
        let _ = frame(
            &context,
            &mut widget,
            &state,
            1.1,
            vec![
                pointer(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let _ = frame(
            &context,
            &mut widget,
            &state,
            1.2,
            vec![pointer(start + egui::vec2(30.0, 20.0))],
        );
        assert_eq!(
            egui::DragAndDrop::payload::<LoopDragPayload>(&context).as_deref(),
            Some(&LoopDragPayload { loop_id: state.id })
        );
        assert_eq!(
            widget.test_drag_preview_rect.unwrap().center(),
            start + egui::vec2(30.0, 20.0)
        );
        let _ = frame(
            &context,
            &mut widget,
            &state,
            1.3,
            vec![egui::Event::PointerButton {
                pos: start + egui::vec2(30.0, 20.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(egui::DragAndDrop::payload::<LoopDragPayload>(&context).is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn click_track_context_action_applies_only_to_primitive_media_loops() {
        assert!(can_generate_click_track(&LoopState {
            has_audio: true,
            ..Default::default()
        }));
        assert!(can_generate_click_track(&LoopState {
            has_midi: true,
            ..Default::default()
        }));
        assert!(!can_generate_click_track(&LoopState::default()));
        assert!(!can_generate_click_track(&LoopState {
            has_audio: true,
            composite_kind: CompositeKind::Regular,
            ..Default::default()
        }));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn composite_reference_border_yields_to_selection_and_targeting() {
        let referenced = LoopState {
            selected_composite_kind: CompositeKind::Regular,
            ..Default::default()
        };
        assert_eq!(
            loop_border_color(&referenced),
            colors::LOOP_COMPOSITE_REFERENCE_EDGE
        );
        assert_eq!(
            loop_border_color(&LoopState {
                selected: true,
                ..referenced.clone()
            }),
            colors::LOOP_SELECTED_EDGE
        );
        assert_eq!(
            loop_border_color(&LoopState {
                targeted: true,
                selected: true,
                ..referenced
            }),
            colors::LOOP_TARGET_EDGE
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn script_composites_do_not_open_record_or_dry_variants() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = LoopState {
            id: LoopId::from_raw(2),
            composite_kind: CompositeKind::Script,
            ..Default::default()
        };
        let mut widget = LoopWidget::default();
        let _ = frame(&context, &mut widget, &state, 1.0, Vec::new());
        let play = widget.test_play_rect.unwrap().center();
        let record = widget.test_record_rect.unwrap().center();
        let _ = frame(&context, &mut widget, &state, 1.1, vec![pointer(play)]);
        let _ = frame(&context, &mut widget, &state, 1.2, vec![pointer(record)]);
        assert_eq!(widget.play_popup_until, 0.0);
        assert_eq!(widget.record_popup_until, 0.0);
        assert!(widget.test_play_popup_button_rect.is_none());
        assert!(widget.test_gain_rect.is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn stereo_balance_popup_is_outside_gain_and_double_click_resets() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut widget = LoopWidget::default();
        let _ = frame(&context, &mut widget, &state, 1.0, Vec::new());
        let gain = widget.test_gain_rect.unwrap();
        let _ = frame(
            &context,
            &mut widget,
            &state,
            1.1,
            vec![pointer(gain.center())],
        );
        let balance = widget.test_balance_rect.unwrap();
        assert!(balance.left() > gain.right());
        let _ = click(&context, &mut widget, &state, balance.center(), 1.12);
        let response = click(&context, &mut widget, &state, balance.center(), 1.2);
        assert!(response
            .actions
            .iter()
            .any(|action| *action == LoopWidgetAction::BalanceChanged(0.0)));
    }
}
