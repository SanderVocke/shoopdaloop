use crate::{TrackControlState, TrackWidgetAction, MAX_TRACK_GAIN_DB, MIN_TRACK_GAIN_DB};
use egui_material_icons::icons::{ICON_HEARING, ICON_VOLUME_MUTE, ICON_VOLUME_UP};

const METER_MIN_DB: f32 = -50.0;

#[derive(Debug, Default)]
pub struct TrackControls;

impl TrackControls {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &TrackControlState) -> Vec<TrackWidgetAction> {
        let mut actions = Vec::new();

        if state.has_output {
            meter(
                ui,
                state.output_stereo,
                state.output_peak_left_db,
                state.output_peak_right_db,
                state.output_midi_activity,
            );
            ui.horizontal(|ui| {
                let icon = if state.output_muted {
                    ICON_VOLUME_MUTE
                } else {
                    ICON_VOLUME_UP
                };
                if ui
                    .add(egui::Button::new(icon.rich_text().size(16.0)).frame(false))
                    .on_hover_text("Mute/unmute output")
                    .clicked()
                {
                    actions.push(TrackWidgetAction::OutputMuteChanged(!state.output_muted));
                }

                if state.has_output_audio {
                    let mut gain = state.output_gain_db;
                    if ui
                        .add(
                            egui::Slider::new(&mut gain, MIN_TRACK_GAIN_DB..=MAX_TRACK_GAIN_DB)
                                .show_value(false),
                        )
                        .on_hover_text(format!("Output gain: {gain:.1} dB"))
                        .changed()
                    {
                        actions.push(TrackWidgetAction::OutputGainChanged(gain));
                    }
                }

                if state.output_stereo {
                    balance_control(
                        ui,
                        state.output_balance,
                        TrackWidgetAction::OutputBalanceChanged,
                        &mut actions,
                    );
                }
            });
        }

        if state.has_input {
            meter(
                ui,
                state.input_stereo,
                state.input_peak_left_db,
                state.input_peak_right_db,
                state.input_midi_activity,
            );
            ui.horizontal(|ui| {
                let color = if state.input_monitoring {
                    ui.visuals().text_color()
                } else {
                    egui::Color32::GRAY
                };
                if ui
                    .add(
                        egui::Button::new(ICON_HEARING.rich_text().size(16.0).color(color))
                            .frame(false),
                    )
                    .on_hover_text("Enable/disable input monitoring")
                    .clicked()
                {
                    actions.push(TrackWidgetAction::InputMonitoringChanged(
                        !state.input_monitoring,
                    ));
                }

                if state.has_input_audio {
                    let mut gain = state.input_gain_db;
                    if ui
                        .add(
                            egui::Slider::new(&mut gain, MIN_TRACK_GAIN_DB..=MAX_TRACK_GAIN_DB)
                                .show_value(false),
                        )
                        .on_hover_text(format!("Input gain: {gain:.1} dB"))
                        .changed()
                    {
                        actions.push(TrackWidgetAction::InputGainChanged(gain));
                    }
                }

                if state.input_stereo {
                    balance_control(
                        ui,
                        state.input_balance,
                        TrackWidgetAction::InputBalanceChanged,
                        &mut actions,
                    );
                }
            });
        }

        actions
    }
}

fn balance_control(
    ui: &mut egui::Ui,
    value: f32,
    action: impl FnOnce(f32) -> TrackWidgetAction,
    actions: &mut Vec<TrackWidgetAction>,
) {
    let mut balance = value;
    if ui
        .add(
            egui::DragValue::new(&mut balance)
                .range(-1.0..=1.0)
                .speed(0.01)
                .prefix("B ")
                .max_decimals(2),
        )
        .on_hover_text(format!("Stereo balance: {balance:.2}"))
        .changed()
    {
        actions.push(action(balance));
    }
}

fn meter(ui: &mut egui::Ui, stereo: bool, left_db: f32, right_db: f32, midi_activity: bool) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 4.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(34, 34, 34));

    let normalized = |db: f32| ((db - METER_MIN_DB) / -METER_MIN_DB).clamp(0.0, 1.0);
    let color = egui::Color32::from_rgb(102, 102, 102);
    if stereo {
        let center = rect.center().x;
        let left_width = normalized(left_db) * rect.width() * 0.5;
        let right_width = normalized(right_db) * rect.width() * 0.5;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(center - left_width, rect.top()),
                egui::pos2(center, rect.bottom()),
            ),
            0.0,
            color,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(center, rect.top()),
                egui::pos2(center + right_width, rect.bottom()),
            ),
            0.0,
            color,
        );
    } else {
        let width = normalized(left_db.max(right_db)) * rect.width();
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height())),
            0.0,
            color,
        );
    }

    if midi_activity {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - 6.0, rect.top()),
                rect.right_bottom(),
            ),
            1.0,
            egui::Color32::CYAN,
        );
    }
}
