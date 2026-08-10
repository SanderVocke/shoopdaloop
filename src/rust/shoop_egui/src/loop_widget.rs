use egui_material_icons::icons::{
    ICON_ARROW_DOWNWARD, ICON_BORDER_CLEAR, ICON_EDIT_NOTE, ICON_FIBER_MANUAL_RECORD, ICON_HELP,
    ICON_PLAY_ARROW, ICON_STAR, ICON_STOP, ICON_TIMER, ICON_VIEW_LIST,
};
use egui_material_icons::MaterialIcon;

use crate::{
    colors, dial::paint_dial, optimistic_value::OptimisticValue, AppIntent, CompositeKind,
    LoopAudioExportFormat, LoopMode, LoopState, LoopWidgetAction, SelectionModifiers,
};

#[derive(Debug, Default)]
pub struct LoopWidgetResponse {
    pub actions: Vec<LoopWidgetAction>,
    pub io_intents: Vec<AppIntent>,
    pub click_track_requested: bool,
    pub(crate) hover_active: bool,
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

fn popup_icon_button(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    icon: MaterialIcon,
    icon_size: f32,
    color: egui::Color32,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(2), visuals.bg_fill);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(2),
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    paint_icon(ui.painter(), rect.center(), icon, icon_size, color);
    response.on_hover_text(tooltip)
}

fn peak_fraction(db: f32, minimum_db: f32) -> f32 {
    ((db - minimum_db) / -minimum_db).clamp(0.0, 1.0)
}

fn can_generate_click_track(state: &LoopState) -> bool {
    state.composite_kind == CompositeKind::None && (state.has_audio || state.has_midi)
}

fn generated_loop_name(name: &str) -> bool {
    name.strip_prefix('(')
        .and_then(|name| name.strip_suffix(')'))
        .is_some_and(|name| {
            !name.is_empty() && name.chars().all(|character| character.is_ascii_digit())
        })
}

impl LoopWidget {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &LoopState,
        size: egui::Vec2,
    ) -> LoopWidgetResponse {
        self.show_with_hover(ui, state, size, true)
    }

    pub(crate) fn show_with_hover(
        &mut self,
        ui: &mut egui::Ui,
        state: &LoopState,
        size: egui::Vec2,
        hover_allowed: bool,
    ) -> LoopWidgetResponse {
        let mut result = LoopWidgetResponse::default();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        response.context_menu(|ui| {
            ui.label(&state.name);
            if can_generate_click_track(state) {
                if ui.button("Generate click track…").clicked() {
                    result.click_track_requested = true;
                    ui.close();
                }
                ui.separator();
            }
            if state.has_audio {
                if ui.button("Save exact audio…").clicked() {
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
                        standard: false,
                    });
                    ui.close();
                }
                if ui.button("Save standard MIDI…").clicked() {
                    result.io_intents.push(AppIntent::RequestLoopMidiExport {
                        loop_id: state.id,
                        standard: true,
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
        });
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
        let background = if state.composite_kind == CompositeKind::Regular {
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
        if state.stereo {
            let center = rect.center().x;
            let half_width = (rect.width() - 4.0).max(0.0) / 2.0;
            let left_width = half_width * peak_fraction(state.peak_left_db, -50.0);
            let right_width = half_width * peak_fraction(state.peak_right_db, -50.0);
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
                (rect.width() - 4.0).max(0.0) * peak_fraction(state.peak_left_db, -30.0);
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 2.0, meter_top),
                    egui::vec2(meter_width, meter_bottom - meter_top),
                ),
                0.0,
                meter_color,
            );
        }

        let border_color = if state.targeted {
            colors::LOOP_TARGET_EDGE
        } else if state.selected {
            colors::LOOP_SELECTED_EDGE
        } else if state.selected_composite_kind == CompositeKind::Regular {
            colors::LOOP_REGULAR_COMPOSITE
        } else if state.selected_composite_kind == CompositeKind::Script {
            colors::LOOP_SCRIPT_COMPOSITE
        } else if state.empty {
            colors::MUTED_FOREGROUND
        } else {
            colors::LOOP_CONTENT_EDGE
        };
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
                    ICON_TIMER,
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
        let controls_left = rect.left() + 20.0;
        let controls_right = dial_rect
            .map(|dial| dial.left() - 2.0)
            .unwrap_or(rect.right() - 2.0);
        let gap = 1.0;
        let button_width = ((controls_right - controls_left - gap * 2.0) / 3.0).clamp(1.0, 18.0);
        let button_height = (rect.height() - 4.0).min(22.0).max(1.0);
        let play_rect = egui::Rect::from_min_size(
            egui::pos2(controls_left, rect.top() + 2.0),
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
        let now = ui.input(|input| input.time);
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
        let controls_visible = hovered || show_play_popup || show_record_popup;
        let icon_size = button_width.min(button_height) * 0.9;

        if controls_visible {
            if ui
                .put(
                    play_rect,
                    egui::Button::new(ICON_PLAY_ARROW.rich_text().size(icon_size).color(
                        if non_script {
                            colors::PLAY_ACTION
                        } else {
                            colors::FOREGROUND
                        },
                    )),
                )
                .on_hover_text("Play")
                .clicked()
            {
                result.actions.push(LoopWidgetAction::PlayClicked);
            }
            if non_script {
                let record_response = ui.put(record_rect, egui::Button::new(""));
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
                if record_response.on_hover_text("Record").clicked() {
                    result.actions.push(LoopWidgetAction::RecordClicked);
                }
            }
            if ui
                .put(
                    stop_rect,
                    egui::Button::new(
                        ICON_STOP
                            .rich_text()
                            .size(icon_size)
                            .color(colors::FOREGROUND),
                    ),
                )
                .on_hover_text("Stop")
                .clicked()
            {
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
                        "Play dry through live effects",
                    );
                    #[cfg(test)]
                    {
                        self.test_play_popup_button_rect = Some(response.rect);
                    }
                    if response.clicked() {
                        result.actions.push(LoopWidgetAction::PlayDryClicked);
                    }
                    response.contains_pointer()
                });
            if hover_allowed && popup.inner {
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
                        "Grab always-on recording",
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
                        "Re-record dry through live effects",
                    );
                    if rerecord.clicked() {
                        result.actions.push(LoopWidgetAction::RerecordClicked);
                    }
                    grab.contains_pointer() || rerecord.contains_pointer()
                });
            if hover_allowed && popup.inner {
                self.record_popup_until = now + 0.08;
            }
        }

        if let Some(dial_rect) = dial_rect {
            let dial_response = ui.interact(
                dial_rect,
                ui.id().with("loop_gain"),
                egui::Sense::click_and_drag(),
            );
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
                            response
                                .on_hover_text("Loop stereo balance")
                                .contains_pointer()
                        });
                    if popup.inner {
                        self.balance_popup_until = now + 0.08;
                    }
                }
            }
        }
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
        let _ = context.run_ui(
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
        let _ = context.run_ui(
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

    fn state() -> LoopState {
        LoopState {
            id: LoopId::from_raw(1),
            show_gain: true,
            stereo: true,
            balance: 0.5,
            ..Default::default()
        }
    }

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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
