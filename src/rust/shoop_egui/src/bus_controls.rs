use crate::{
    colors, dial::paint_dial, meter_ballistics::PeakMeterAnimation,
    optimistic_value::OptimisticValue, BusAction, BusState, MAX_BUS_GAIN_DB, MIN_BUS_GAIN_DB,
};
use egui_material_icons::icons::{ICON_VOLUME_MUTE, ICON_VOLUME_UP};

const METER_MIN_DB: f32 = -50.0;
const CONTROL_HEIGHT: f32 = 24.0;
const BALANCE_SIZE: f32 = 18.0;

#[derive(Debug, Default)]
pub struct BusControls {
    gain: OptimisticValue<f32>,
    gain_dragging: bool,
    balance: OptimisticValue<f32>,
    balance_drag_start: Option<f32>,
    peaks: Vec<PeakMeterAnimation>,
    #[cfg(test)]
    test_rects: TestBusControlRects,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TestBusControlRects {
    block: Option<egui::Rect>,
    meter: Option<egui::Rect>,
    mute: Option<egui::Rect>,
    gain: Option<egui::Rect>,
    balance: Option<egui::Rect>,
}

impl BusControls {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &BusState) -> Vec<BusAction> {
        if state.control_error.is_some() {
            self.gain.clear();
            self.balance.clear();
        }
        let mut actions = Vec::new();
        let _response = egui::Frame::new()
            .fill(colors::RAISED_BACKGROUND)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let color = if state.control_error.is_some() {
                        colors::ERROR
                    } else {
                        colors::FOREGROUND
                    };
                    let response = ui.label(egui::RichText::new(&state.name).strong().color(color));
                    if let Some(error) = &state.control_error {
                        response.on_hover_text(error);
                    }
                    if state.control_pending {
                        ui.spinner();
                    }
                });
                self.show_control_row(ui, state, &mut actions);
            })
            .response;
        #[cfg(test)]
        {
            self.test_rects.block = Some(_response.rect);
        }
        actions
    }

    fn show_control_row(
        &mut self,
        ui: &mut egui::Ui,
        state: &BusState,
        actions: &mut Vec<BusAction>,
    ) {
        let (rect, _response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), CONTROL_HEIGHT),
            egui::Sense::hover(),
        );
        #[cfg(test)]
        {
            self.test_rects.meter = Some(_response.rect);
        }
        self.paint_meter(ui, rect, state);
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("bus_controls", state.id.raw()))
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let icon = if state.muted {
            ICON_VOLUME_MUTE
        } else {
            ICON_VOLUME_UP
        };
        let color = if state.muted {
            colors::MUTED_FOREGROUND
        } else {
            colors::FOREGROUND
        };
        let mute = row
            .add(egui::Button::new(icon.rich_text().size(16.0).color(color)).frame(false))
            .on_hover_text("Mute/unmute bus");
        #[cfg(test)]
        {
            self.test_rects.mute = Some(mute.rect);
        }
        if mute.clicked() {
            actions.push(BusAction::MuteChanged(!state.muted));
        }

        let stereo = state.stereo();
        let available = row.available_width().max(0.0);
        let gap = if stereo {
            row.spacing().item_spacing.x.min(available)
        } else {
            0.0
        };
        let balance_size = if stereo {
            BALANCE_SIZE.min((available - gap).max(0.0))
        } else {
            0.0
        };
        let gain_width = (available - gap - balance_size).max(0.0);
        let mut gain = self.gain.resolve(state.gain_db, self.gain_dragging);
        let slider_width = row.spacing().slider_width;
        row.spacing_mut().slider_width = gain_width;
        let fill = if state.muted {
            colors::MUTED_SLIDER_FILL
        } else {
            colors::COLORED_HIGHLIGHT
        };
        let gain_response = row
            .scope(|ui| {
                ui.visuals_mut().selection.bg_fill = fill;
                ui.add(
                    egui::Slider::new(&mut gain, MIN_BUS_GAIN_DB..=MAX_BUS_GAIN_DB)
                        .show_value(false)
                        .trailing_fill(true),
                )
            })
            .inner
            .on_hover_text(format!("Bus gain: {gain:.1} dB"));
        row.spacing_mut().slider_width = slider_width;
        #[cfg(test)]
        {
            self.test_rects.gain = Some(gain_response.rect);
        }
        if gain_response.drag_started() || gain_response.dragged() {
            self.gain_dragging = true;
        }
        if gain_response.changed() {
            self.gain.set(gain);
            actions.push(BusAction::GainChanged(gain));
        }
        if gain_response.drag_stopped() {
            self.gain_dragging = false;
        }

        if stereo {
            let (balance_rect, balance_response) = row.allocate_exact_size(
                egui::vec2(balance_size, balance_size),
                egui::Sense::click_and_drag(),
            );
            let value = self
                .balance
                .resolve(state.balance, self.balance_drag_start.is_some());
            if balance_response.drag_started() {
                self.balance_drag_start = Some(value);
            }
            let mut balance = value;
            if balance_response.dragged() {
                balance = (self.balance_drag_start.unwrap_or(value)
                    - balance_response.total_drag_delta().unwrap_or_default().y / 50.0)
                    .clamp(-1.0, 1.0);
            }
            if balance_response.double_clicked() {
                balance = 0.0;
            }
            if (balance - value).abs() > f32::EPSILON {
                self.balance.set(balance);
                actions.push(BusAction::BalanceChanged(balance));
            }
            if balance_response.drag_stopped() {
                self.balance_drag_start = None;
            }
            paint_dial(
                &row,
                &balance_response,
                balance_rect,
                (balance + 1.0) / 2.0,
                "B",
            );
            let _balance_response =
                balance_response.on_hover_text(format!("Stereo balance: {balance:.2}"));
            #[cfg(test)]
            {
                self.test_rects.balance = Some(_balance_response.rect);
            }
        } else {
            #[cfg(test)]
            {
                self.test_rects.balance = None;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn mute_rect(&self) -> Option<egui::Rect> {
        self.test_rects.mute
    }

    fn paint_meter(&mut self, ui: &egui::Ui, rect: egui::Rect, state: &BusState) {
        let channel_count = state.channels.len().max(1);
        self.peaks.resize_with(channel_count, Default::default);
        self.peaks.truncate(channel_count);
        ui.painter()
            .rect_filled(rect, 2.0, colors::CONTROL_BACKGROUND);
        let now = ui.input(|input| input.time);
        let segment_width = rect.width() / channel_count as f32;
        let mut animating = false;
        for (index, peak) in self.peaks.iter_mut().enumerate() {
            let target = state
                .output_peaks_db
                .get(index)
                .copied()
                .unwrap_or(METER_MIN_DB);
            let reading = peak.update(target, METER_MIN_DB, now);
            animating |= reading.animating;
            let fraction = ((reading.db - METER_MIN_DB) / -METER_MIN_DB).clamp(0.0, 1.0);
            let segment = egui::Rect::from_min_max(
                egui::pos2(rect.left() + segment_width * index as f32, rect.top()),
                egui::pos2(
                    rect.left() + segment_width * (index + 1) as f32,
                    rect.bottom(),
                ),
            )
            .shrink2(egui::vec2(1.0, 0.0));
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    segment.min,
                    egui::vec2(segment.width() * fraction, segment.height()),
                ),
                1.0,
                colors::METER_LEVEL,
            );
        }
        if animating {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BusChannelId, BusChannelState, BusId, PortId};

    fn state(channels: usize) -> BusState {
        BusState {
            id: BusId::from_raw(1),
            name: "Master".to_owned(),
            channels: (0..channels)
                .map(|index| BusChannelState {
                    id: BusChannelId::from_raw(index as u64 + 1),
                    label: format!("Channel {}", index + 1),
                    output_port_id: PortId::from_raw(index as u64 + 1),
                })
                .collect::<Vec<_>>()
                .into(),
            gain_db: -3.0,
            balance: 0.25,
            muted: false,
            output_peaks_db: vec![-12.0; channels].into(),
            control_pending: false,
            control_error: None,
        }
    }

    fn frame(
        context: &egui::Context,
        controls: &mut BusControls,
        state: &BusState,
        events: Vec<egui::Event>,
    ) -> Vec<BusAction> {
        let mut actions = Vec::new();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(140.0, 80.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = controls.show(ui, state),
        );
        output.textures_delta.clear();
        actions
    }

    #[shoop_wasm_test_support::shoop_test]
    fn block_renders_channel_aware_meter_and_stereo_only_balance() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut controls = BusControls::default();
        frame(&context, &mut controls, &state(2), Vec::new());
        assert!(controls.test_rects.block.is_some());
        assert!(controls.test_rects.meter.is_some());
        assert!(controls.test_rects.gain.is_some());
        assert!(controls.test_rects.balance.is_some());
        assert_eq!(controls.peaks.len(), 2);

        frame(&context, &mut controls, &state(3), Vec::new());
        assert!(controls.test_rects.balance.is_none());
        assert_eq!(controls.peaks.len(), 3);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fader_and_dial_emit_changes_and_reconcile_to_authoritative_values() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut controls = BusControls::default();
        let mut state = state(2);
        frame(&context, &mut controls, &state, Vec::new());

        let gain_start = controls.test_rects.gain.unwrap().center();
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(gain_start),
                egui::Event::PointerButton {
                    pos: gain_start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let gain_actions = frame(
            &context,
            &mut controls,
            &state,
            vec![egui::Event::PointerMoved(
                gain_start + egui::vec2(30.0, 0.0),
            )],
        );
        let gain = gain_actions
            .iter()
            .find_map(|action| match action {
                BusAction::GainChanged(value) => Some(*value),
                _ => None,
            })
            .unwrap();
        frame(
            &context,
            &mut controls,
            &state,
            vec![egui::Event::PointerButton {
                pos: gain_start + egui::vec2(30.0, 0.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(controls.gain.is_pending());
        state.gain_db = gain;
        frame(&context, &mut controls, &state, Vec::new());
        assert!(!controls.gain.is_pending());

        let balance_start = controls.test_rects.balance.unwrap().center();
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(balance_start),
                egui::Event::PointerButton {
                    pos: balance_start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let balance_actions = frame(
            &context,
            &mut controls,
            &state,
            vec![egui::Event::PointerMoved(
                balance_start - egui::vec2(0.0, 20.0),
            )],
        );
        assert!(balance_actions
            .iter()
            .any(|action| matches!(action, BusAction::BalanceChanged(value) if *value > 0.25)));
        assert!(controls.balance.is_pending());
        frame(
            &context,
            &mut controls,
            &state,
            vec![egui::Event::PointerButton {
                pos: balance_start - egui::vec2(0.0, 20.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        state.control_error = Some("rejected".to_owned());
        frame(&context, &mut controls, &state, Vec::new());
        assert!(!controls.balance.is_pending());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn mute_button_emits_the_typed_bus_action() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut controls = BusControls::default();
        let state = state(2);
        frame(&context, &mut controls, &state, Vec::new());
        let position = controls.test_rects.mute.unwrap().center();
        frame(
            &context,
            &mut controls,
            &state,
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
        let actions = frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(actions, [BusAction::MuteChanged(true)]);
    }
}
