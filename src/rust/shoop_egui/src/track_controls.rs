use crate::{
    colors, dial::paint_dial, meter_ballistics::PeakMeterAnimation,
    optimistic_value::OptimisticValue, GlobalControlState, TrackControlState, TrackWidgetAction,
    MAX_TRACK_GAIN_DB, MIN_TRACK_GAIN_DB,
};
use egui_material_icons::icons::{ICON_HEARING, ICON_VOLUME_MUTE, ICON_VOLUME_UP};

const METER_MIN_DB: f32 = -50.0;
const MIDI_ACTIVITY_WIDTH: f32 = 7.0;
const MIDI_ACTIVITY_GAP: f32 = 2.0;

#[derive(Debug, Default)]
pub struct TrackControls {
    output_gain: OptimisticValue<f32>,
    output_gain_dragging: bool,
    output_balance: OptimisticValue<f32>,
    output_balance_drag_start: Option<f32>,
    input_gain: OptimisticValue<f32>,
    input_gain_dragging: bool,
    input_balance: OptimisticValue<f32>,
    input_balance_drag_start: Option<f32>,
    output_peak_left: PeakMeterAnimation,
    output_peak_right: PeakMeterAnimation,
    input_peak_left: PeakMeterAnimation,
    input_peak_right: PeakMeterAnimation,
    #[cfg(test)]
    test_rects: TestTrackControlRects,
}

#[derive(Clone, Copy, Debug)]
enum TestTrackControl {
    OutputMeter,
    OutputMute,
    OutputGain,
    OutputBalance,
    InputMeter,
    InputMonitoring,
    InputGain,
    InputBalance,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TestTrackControlRects {
    output_meter: Option<egui::Rect>,
    output_mute: Option<egui::Rect>,
    output_gain: Option<egui::Rect>,
    output_balance: Option<egui::Rect>,
    input_meter: Option<egui::Rect>,
    input_monitoring: Option<egui::Rect>,
    input_gain: Option<egui::Rect>,
    input_balance: Option<egui::Rect>,
}

fn input_monitoring_tooltip(
    state: &TrackControlState,
    global_controls: &GlobalControlState,
) -> &'static str {
    if state.input_monitoring {
        "Mute input"
    } else if global_controls.auto_mute_other_track_inputs {
        "Unmute (and mute others)"
    } else {
        "Unmute"
    }
}

impl TrackControls {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &TrackControlState) -> Vec<TrackWidgetAction> {
        self.show_with_global_controls(ui, state, &GlobalControlState::default())
    }

    pub fn show_with_global_controls(
        &mut self,
        ui: &mut egui::Ui,
        state: &TrackControlState,
        global_controls: &GlobalControlState,
    ) -> Vec<TrackWidgetAction> {
        let mut actions = Vec::new();

        if state.has_output {
            let (left_db, right_db) = animated_peaks(
                ui,
                &mut self.output_peak_left,
                &mut self.output_peak_right,
                state.output_peak_left_db,
                state.output_peak_right_db,
            );
            let (response, mut row) = meter_row(
                ui,
                state.output_stereo,
                left_db,
                right_db,
                state.output_midi_activity,
            );
            self.record_rect(TestTrackControl::OutputMeter, &response);
            let ui = &mut row;
            let icon = if state.output_muted {
                ICON_VOLUME_MUTE
            } else {
                ICON_VOLUME_UP
            };
            let color = if state.output_muted {
                colors::MUTED_FOREGROUND
            } else {
                colors::FOREGROUND
            };
            let response = ui
                .add(egui::Button::new(icon.rich_text().size(16.0).color(color)).frame(false))
                .on_hover_text("Mute/unmute output");
            self.record_rect(TestTrackControl::OutputMute, &response);
            if response.clicked() {
                actions.push(TrackWidgetAction::OutputMuteChanged(!state.output_muted));
            }

            let (gain_width, balance_size) =
                control_sizes(ui, state.has_output_audio, state.output_stereo);
            if state.has_output_audio {
                let mut gain = self
                    .output_gain
                    .resolve(state.output_gain_db, self.output_gain_dragging);
                let fill = if state.output_muted {
                    colors::MUTED_SLIDER_FILL
                } else {
                    colors::COLORED_HIGHLIGHT
                };
                let response = gain_slider(ui, &mut gain, gain_width, fill)
                    .on_hover_text(format!("Output gain: {gain:.1} dB"));
                self.record_rect(TestTrackControl::OutputGain, &response);
                if response.drag_started() || response.dragged() {
                    self.output_gain_dragging = true;
                }
                if response.changed() {
                    self.output_gain.set(gain);
                    actions.push(TrackWidgetAction::OutputGainChanged(gain));
                }
                if response.drag_stopped() {
                    self.output_gain_dragging = false;
                }
            }

            if state.output_stereo {
                let response = balance_control(
                    ui,
                    state.output_balance,
                    balance_size,
                    &mut self.output_balance,
                    &mut self.output_balance_drag_start,
                    TrackWidgetAction::OutputBalanceChanged,
                    &mut actions,
                );
                self.record_rect(TestTrackControl::OutputBalance, &response);
            }
        }

        if state.has_input {
            let (left_db, right_db) = animated_peaks(
                ui,
                &mut self.input_peak_left,
                &mut self.input_peak_right,
                state.input_peak_left_db,
                state.input_peak_right_db,
            );
            let (response, mut row) = meter_row(
                ui,
                state.input_stereo,
                left_db,
                right_db,
                state.input_midi_activity,
            );
            self.record_rect(TestTrackControl::InputMeter, &response);
            let ui = &mut row;
            let color = if state.input_monitoring {
                colors::FOREGROUND
            } else {
                colors::MUTED_FOREGROUND
            };
            let response = ui
                .add(
                    egui::Button::new(ICON_HEARING.rich_text().size(16.0).color(color))
                        .frame(false),
                )
                .on_hover_text(input_monitoring_tooltip(state, global_controls));
            self.record_rect(TestTrackControl::InputMonitoring, &response);
            if response.clicked() {
                actions.push(TrackWidgetAction::InputMonitoringChanged {
                    enabled: !state.input_monitoring,
                    respect_auto_mute: true,
                });
            }

            let (gain_width, balance_size) =
                control_sizes(ui, state.has_input_audio, state.input_stereo);
            if state.has_input_audio {
                let mut gain = self
                    .input_gain
                    .resolve(state.input_gain_db, self.input_gain_dragging);
                let fill = if state.input_monitoring {
                    colors::COLORED_HIGHLIGHT
                } else {
                    colors::MUTED_SLIDER_FILL
                };
                let response = gain_slider(ui, &mut gain, gain_width, fill)
                    .on_hover_text(format!("Input gain: {gain:.1} dB"));
                self.record_rect(TestTrackControl::InputGain, &response);
                if response.drag_started() || response.dragged() {
                    self.input_gain_dragging = true;
                }
                if response.changed() {
                    self.input_gain.set(gain);
                    actions.push(TrackWidgetAction::InputGainChanged(gain));
                }
                if response.drag_stopped() {
                    self.input_gain_dragging = false;
                }
            }

            if state.input_stereo {
                let response = balance_control(
                    ui,
                    state.input_balance,
                    balance_size,
                    &mut self.input_balance,
                    &mut self.input_balance_drag_start,
                    TrackWidgetAction::InputBalanceChanged,
                    &mut actions,
                );
                self.record_rect(TestTrackControl::InputBalance, &response);
            }
        }

        actions
    }

    #[cfg(test)]
    fn record_rect(&mut self, control: TestTrackControl, response: &egui::Response) {
        let target = match control {
            TestTrackControl::OutputMeter => &mut self.test_rects.output_meter,
            TestTrackControl::OutputMute => &mut self.test_rects.output_mute,
            TestTrackControl::OutputGain => &mut self.test_rects.output_gain,
            TestTrackControl::OutputBalance => &mut self.test_rects.output_balance,
            TestTrackControl::InputMeter => &mut self.test_rects.input_meter,
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
            TestTrackControl::OutputMeter => self.test_rects.output_meter,
            TestTrackControl::OutputMute => self.test_rects.output_mute,
            TestTrackControl::OutputGain => self.test_rects.output_gain,
            TestTrackControl::OutputBalance => self.test_rects.output_balance,
            TestTrackControl::InputMeter => self.test_rects.input_meter,
            TestTrackControl::InputMonitoring => self.test_rects.input_monitoring,
            TestTrackControl::InputGain => self.test_rects.input_gain,
            TestTrackControl::InputBalance => self.test_rects.input_balance,
        }
    }
}

fn animated_peaks(
    ui: &egui::Ui,
    left: &mut PeakMeterAnimation,
    right: &mut PeakMeterAnimation,
    target_left_db: f32,
    target_right_db: f32,
) -> (f32, f32) {
    let now = ui.input(|input| input.time);
    let left = left.update(target_left_db, METER_MIN_DB, now);
    let right = right.update(target_right_db, METER_MIN_DB, now);
    if left.animating || right.animating {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    }
    (left.db, right.db)
}

fn gain_slider(
    ui: &mut egui::Ui,
    gain: &mut f32,
    width: f32,
    fill: egui::Color32,
) -> egui::Response {
    let slider_width = ui.spacing().slider_width;
    ui.spacing_mut().slider_width = width;
    let response = ui
        .scope(|ui| {
            ui.visuals_mut().selection.bg_fill = fill;
            ui.add(
                egui::Slider::new(gain, MIN_TRACK_GAIN_DB..=MAX_TRACK_GAIN_DB)
                    .show_value(false)
                    .trailing_fill(true),
            )
        })
        .inner;
    ui.spacing_mut().slider_width = slider_width;
    response
}

fn control_sizes(ui: &egui::Ui, gain: bool, balance: bool) -> (f32, f32) {
    let available = ui.available_width().max(0.0);
    let gap = if gain && balance {
        ui.spacing().item_spacing.x.min(available)
    } else {
        0.0
    };
    let usable = (available - gap).max(0.0);
    let balance_size = if balance { usable.min(18.0) } else { 0.0 };
    let gain_width = if gain {
        (usable - balance_size).max(0.0)
    } else {
        0.0
    };
    (gain_width, balance_size)
}

fn balance_control(
    ui: &mut egui::Ui,
    authoritative: f32,
    size: f32,
    optimistic: &mut OptimisticValue<f32>,
    drag_start: &mut Option<f32>,
    action: impl FnOnce(f32) -> TrackWidgetAction,
    actions: &mut Vec<TrackWidgetAction>,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());
    let value = optimistic.resolve(authoritative, drag_start.is_some());
    if response.drag_started() {
        *drag_start = Some(value);
    }
    let mut balance = value;
    if response.dragged() {
        balance = (drag_start.unwrap_or(value)
            - response.total_drag_delta().unwrap_or_default().y / 50.0)
            .clamp(-1.0, 1.0);
    }
    if response.double_clicked() {
        balance = 0.0;
    }
    if (balance - value).abs() > f32::EPSILON {
        optimistic.set(balance);
        actions.push(action(balance));
    }
    if response.drag_stopped() {
        *drag_start = None;
    }
    paint_dial(ui, &response, rect, (balance + 1.0) / 2.0, "B");
    response.on_hover_text(format!("Stereo balance: {balance:.2}"))
}

fn meter_row(
    ui: &mut egui::Ui,
    stereo: bool,
    left_db: f32,
    right_db: f32,
    midi_activity: bool,
) -> (egui::Response, egui::Ui) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, colors::CONTROL_BACKGROUND);

    let normalized = |db: f32| ((db - METER_MIN_DB) / -METER_MIN_DB).clamp(0.0, 1.0);
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
            colors::METER_LEVEL,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(center, rect.top()),
                egui::pos2(center + right_width, rect.bottom()),
            ),
            0.0,
            colors::METER_LEVEL,
        );
    } else {
        let width = normalized(left_db.max(right_db)) * rect.width();
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height())),
            2.0,
            colors::METER_LEVEL,
        );
    }

    let midi_rect = egui::Rect::from_min_max(
        egui::pos2(rect.right() - MIDI_ACTIVITY_WIDTH, rect.top() + 2.0),
        egui::pos2(rect.right(), rect.bottom() - 2.0),
    );
    if midi_activity {
        painter.rect_filled(midi_rect, 1.0, colors::MIDI_ACTIVITY);
    }
    let controls_rect = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(midi_rect.left() - MIDI_ACTIVITY_GAP, rect.bottom()),
    );
    let row = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("meter_controls", rect.min.y.to_bits()))
            .max_rect(controls_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    (response, row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at_width(
        context: &egui::Context,
        controls: &mut TrackControls,
        state: &TrackControlState,
        width: f32,
        events: Vec<egui::Event>,
    ) -> Vec<TrackWidgetAction> {
        let mut actions = Vec::new();
        let mut ignored_output_0 = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 120.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = controls.show(ui, state),
        );
        ignored_output_0.textures_delta.clear();
        actions
    }

    fn frame(
        context: &egui::Context,
        controls: &mut TrackControls,
        state: &TrackControlState,
        events: Vec<egui::Event>,
    ) -> Vec<TrackWidgetAction> {
        frame_at_width(context, controls, state, 220.0, events)
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

    #[shoop_wasm_test_support::shoop_test]
    fn input_monitoring_tooltip_describes_exclusive_unmute_behavior() {
        let mut state = TrackControlState::default();
        let mut globals = GlobalControlState::default();
        assert_eq!(input_monitoring_tooltip(&state, &globals), "Unmute");

        globals.auto_mute_other_track_inputs = true;
        assert_eq!(
            input_monitoring_tooltip(&state, &globals),
            "Unmute (and mute others)"
        );

        state.input_monitoring = true;
        assert_eq!(input_monitoring_tooltip(&state, &globals), "Mute input");
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn meters_gain_sliders_and_balance_dials_fit_the_available_width() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackControlState {
            has_output: true,
            has_output_audio: true,
            output_stereo: true,
            ..Default::default()
        };
        let mut controls = TrackControls::default();
        frame_at_width(&context, &mut controls, &state, 80.0, Vec::new());
        let narrow_meter = controls.test_rect(TestTrackControl::OutputMeter).unwrap();
        let narrow_gain = controls.test_rect(TestTrackControl::OutputGain).unwrap();
        let narrow_balance = controls.test_rect(TestTrackControl::OutputBalance).unwrap();
        assert_eq!(narrow_meter.x_range(), 0.0..=80.0);
        assert!(
            narrow_gain.right() <= narrow_meter.right(),
            "meter={narrow_meter:?}, gain={narrow_gain:?}, balance={narrow_balance:?}"
        );
        assert!(
            narrow_balance.right() <= narrow_meter.right(),
            "meter={narrow_meter:?}, gain={narrow_gain:?}, balance={narrow_balance:?}"
        );

        frame_at_width(&context, &mut controls, &state, 220.0, Vec::new());
        let wide_meter = controls.test_rect(TestTrackControl::OutputMeter).unwrap();
        let wide_gain = controls.test_rect(TestTrackControl::OutputGain).unwrap();
        let wide_balance = controls.test_rect(TestTrackControl::OutputBalance).unwrap();
        assert_eq!(wide_meter.x_range(), 0.0..=220.0);
        assert!(wide_gain.width() > narrow_gain.width());
        assert!(wide_balance.width() >= narrow_balance.width());
        assert!(wide_balance.right() <= wide_meter.right());
    }

    #[shoop_wasm_test_support::shoop_test]
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
        let output_balance = controls.test_rect(TestTrackControl::OutputBalance).unwrap();
        assert!((output_balance.width() - output_balance.height()).abs() < f32::EPSILON);
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
            vec![TrackWidgetAction::InputMonitoringChanged {
                enabled: true,
                respect_auto_mute: true,
            }]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dragged_gain_stays_optimistic_while_authoritative_state_is_stale() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackControlState {
            has_output: true,
            has_output_audio: true,
            output_gain_db: 0.0,
            ..Default::default()
        };
        let mut controls = TrackControls::default();
        frame(&context, &mut controls, &state, Vec::new());
        let slider = controls.test_rect(TestTrackControl::OutputGain).unwrap();
        let press = slider.center();
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(press),
                egui::Event::PointerButton {
                    pos: press,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let target = egui::pos2(slider.left() + 2.0, slider.center().y);
        let actions = frame(
            &context,
            &mut controls,
            &state,
            vec![egui::Event::PointerMoved(target)],
        );
        let TrackWidgetAction::OutputGainChanged(dragged_gain) = actions[0] else {
            panic!("drag should change output gain");
        };
        assert_ne!(dragged_gain, state.output_gain_db);
        assert_eq!(
            controls
                .output_gain
                .resolve(state.output_gain_db, controls.output_gain_dragging),
            dragged_gain
        );
        frame(
            &context,
            &mut controls,
            &state,
            vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            controls.output_gain.resolve(state.output_gain_db, false),
            dragged_gain
        );
        assert_eq!(
            controls.output_gain.resolve(dragged_gain, false),
            dragged_gain
        );
        assert_eq!(
            controls.output_gain.resolve(state.output_gain_db, false),
            state.output_gain_db
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn balance_dial_uses_total_delta_across_drag_frames() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackControlState {
            has_output: true,
            output_stereo: true,
            ..Default::default()
        };
        let mut controls = TrackControls::default();
        frame(&context, &mut controls, &state, Vec::new());
        let center = controls
            .test_rect(TestTrackControl::OutputBalance)
            .unwrap()
            .center();
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(center),
                egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let first = frame(
            &context,
            &mut controls,
            &state,
            vec![egui::Event::PointerMoved(center - egui::vec2(0.0, 10.0))],
        );
        let second = frame(
            &context,
            &mut controls,
            &state,
            vec![egui::Event::PointerMoved(center - egui::vec2(0.0, 20.0))],
        );
        let TrackWidgetAction::OutputBalanceChanged(first) = first[0] else {
            panic!("first drag frame should change balance");
        };
        let TrackWidgetAction::OutputBalanceChanged(second) = second[0] else {
            panic!("second drag frame should change balance");
        };
        assert!((first - 0.2).abs() < f32::EPSILON);
        assert!((second - 0.4).abs() < f32::EPSILON);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn balance_dial_double_click_resets_to_center() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = TrackControlState {
            has_output: true,
            has_output_audio: true,
            output_stereo: true,
            output_balance: 0.5,
            ..Default::default()
        };
        let mut controls = TrackControls::default();
        let _ = click(
            &context,
            &mut controls,
            &state,
            TestTrackControl::OutputBalance,
        );
        assert_eq!(
            click(
                &context,
                &mut controls,
                &state,
                TestTrackControl::OutputBalance,
            ),
            vec![TrackWidgetAction::OutputBalanceChanged(0.0)]
        );
    }
}
