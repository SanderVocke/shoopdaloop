use crate::{TrackControlState, TrackWidgetAction, MAX_TRACK_GAIN_DB, MIN_TRACK_GAIN_DB};
use egui_material_icons::icons::{ICON_HEARING, ICON_VOLUME_MUTE, ICON_VOLUME_UP};

const METER_MIN_DB: f32 = -50.0;

#[derive(Debug, Default)]
pub struct TrackControls {
    #[cfg(test)]
    test_rects: TestTrackControlRects,
}

#[derive(Clone, Copy, Debug)]
enum TestTrackControl {
    OutputMute,
    OutputGain,
    OutputBalance,
    InputMonitoring,
    InputGain,
    InputBalance,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TestTrackControlRects {
    output_mute: Option<egui::Rect>,
    output_gain: Option<egui::Rect>,
    output_balance: Option<egui::Rect>,
    input_monitoring: Option<egui::Rect>,
    input_gain: Option<egui::Rect>,
    input_balance: Option<egui::Rect>,
}

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
                let response = ui
                    .add(egui::Button::new(icon.rich_text().size(16.0)).frame(false))
                    .on_hover_text("Mute/unmute output");
                self.record_rect(TestTrackControl::OutputMute, &response);
                if response.clicked() {
                    actions.push(TrackWidgetAction::OutputMuteChanged(!state.output_muted));
                }

                if state.has_output_audio {
                    let mut gain = state.output_gain_db;
                    let response = ui
                        .add(
                            egui::Slider::new(&mut gain, MIN_TRACK_GAIN_DB..=MAX_TRACK_GAIN_DB)
                                .show_value(false),
                        )
                        .on_hover_text(format!("Output gain: {gain:.1} dB"));
                    self.record_rect(TestTrackControl::OutputGain, &response);
                    if response.changed() {
                        actions.push(TrackWidgetAction::OutputGainChanged(gain));
                    }
                }

                if state.output_stereo {
                    let response = balance_control(
                        ui,
                        state.output_balance,
                        TrackWidgetAction::OutputBalanceChanged,
                        &mut actions,
                    );
                    self.record_rect(TestTrackControl::OutputBalance, &response);
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
                let response = ui
                    .add(
                        egui::Button::new(ICON_HEARING.rich_text().size(16.0).color(color))
                            .frame(false),
                    )
                    .on_hover_text("Enable/disable input monitoring");
                self.record_rect(TestTrackControl::InputMonitoring, &response);
                if response.clicked() {
                    actions.push(TrackWidgetAction::InputMonitoringChanged(
                        !state.input_monitoring,
                    ));
                }

                if state.has_input_audio {
                    let mut gain = state.input_gain_db;
                    let response = ui
                        .add(
                            egui::Slider::new(&mut gain, MIN_TRACK_GAIN_DB..=MAX_TRACK_GAIN_DB)
                                .show_value(false),
                        )
                        .on_hover_text(format!("Input gain: {gain:.1} dB"));
                    self.record_rect(TestTrackControl::InputGain, &response);
                    if response.changed() {
                        actions.push(TrackWidgetAction::InputGainChanged(gain));
                    }
                }

                if state.input_stereo {
                    let response = balance_control(
                        ui,
                        state.input_balance,
                        TrackWidgetAction::InputBalanceChanged,
                        &mut actions,
                    );
                    self.record_rect(TestTrackControl::InputBalance, &response);
                }
            });
        }

        actions
    }

    #[cfg(test)]
    fn record_rect(&mut self, control: TestTrackControl, response: &egui::Response) {
        let target = match control {
            TestTrackControl::OutputMute => &mut self.test_rects.output_mute,
            TestTrackControl::OutputGain => &mut self.test_rects.output_gain,
            TestTrackControl::OutputBalance => &mut self.test_rects.output_balance,
            TestTrackControl::InputMonitoring => &mut self.test_rects.input_monitoring,
            TestTrackControl::InputGain => &mut self.test_rects.input_gain,
            TestTrackControl::InputBalance => &mut self.test_rects.input_balance,
        };
        *target = Some(response.rect);
    }

    #[cfg(not(test))]
    fn record_rect(&mut self, _control: TestTrackControl, _response: &egui::Response) {}

    #[cfg(test)]
    fn test_rect(&self, control: TestTrackControl) -> Option<egui::Rect> {
        match control {
            TestTrackControl::OutputMute => self.test_rects.output_mute,
            TestTrackControl::OutputGain => self.test_rects.output_gain,
            TestTrackControl::OutputBalance => self.test_rects.output_balance,
            TestTrackControl::InputMonitoring => self.test_rects.input_monitoring,
            TestTrackControl::InputGain => self.test_rects.input_gain,
            TestTrackControl::InputBalance => self.test_rects.input_balance,
        }
    }
}

fn balance_control(
    ui: &mut egui::Ui,
    value: f32,
    action: impl FnOnce(f32) -> TrackWidgetAction,
    actions: &mut Vec<TrackWidgetAction>,
) -> egui::Response {
    let mut balance = value;
    let response = ui
        .add(
            egui::DragValue::new(&mut balance)
                .range(-1.0..=1.0)
                .speed(0.01)
                .prefix("B ")
                .max_decimals(2),
        )
        .on_hover_text(format!("Stereo balance: {balance:.2}"));
    if response.changed() {
        actions.push(action(balance));
    }
    response
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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        context: &egui::Context,
        controls: &mut TrackControls,
        state: &TrackControlState,
        events: Vec<egui::Event>,
    ) -> Vec<TrackWidgetAction> {
        let mut actions = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(220.0, 120.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = controls.show(ui, state),
        );
        actions
    }

    fn click(
        context: &egui::Context,
        controls: &mut TrackControls,
        state: &TrackControlState,
        control: TestTrackControl,
    ) -> Vec<TrackWidgetAction> {
        frame(context, controls, state, Vec::new());
        let position = controls.test_rect(control).unwrap().center();
        frame(
            context,
            controls,
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
            controls,
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

    #[test]
    fn inapplicable_track_controls_are_not_rendered() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut controls = TrackControls::default();
        frame(
            &context,
            &mut controls,
            &TrackControlState::default(),
            Vec::new(),
        );

        assert!(controls.test_rect(TestTrackControl::OutputMute).is_none());
        assert!(controls.test_rect(TestTrackControl::OutputGain).is_none());
        assert!(controls
            .test_rect(TestTrackControl::OutputBalance)
            .is_none());
        assert!(controls
            .test_rect(TestTrackControl::InputMonitoring)
            .is_none());
        assert!(controls.test_rect(TestTrackControl::InputGain).is_none());
        assert!(controls.test_rect(TestTrackControl::InputBalance).is_none());
    }

    #[test]
    fn applicable_controls_render_and_buttons_generate_typed_actions() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackControlState {
            has_output: true,
            has_output_audio: true,
            output_stereo: true,
            has_input: true,
            has_input_audio: true,
            input_stereo: true,
            ..Default::default()
        };
        let mut controls = TrackControls::default();
        frame(&context, &mut controls, &state, Vec::new());

        assert!(controls.test_rect(TestTrackControl::OutputGain).is_some());
        assert!(controls
            .test_rect(TestTrackControl::OutputBalance)
            .is_some());
        assert!(controls.test_rect(TestTrackControl::InputGain).is_some());
        assert!(controls.test_rect(TestTrackControl::InputBalance).is_some());
        assert_eq!(
            click(
                &context,
                &mut controls,
                &state,
                TestTrackControl::OutputMute
            ),
            vec![TrackWidgetAction::OutputMuteChanged(true)]
        );
        assert_eq!(
            click(
                &context,
                &mut controls,
                &state,
                TestTrackControl::InputMonitoring
            ),
            vec![TrackWidgetAction::InputMonitoringChanged(true)]
        );
    }
}
