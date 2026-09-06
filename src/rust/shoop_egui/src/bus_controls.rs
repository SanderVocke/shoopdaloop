use crate::{
    builtin_fx_editor::BuiltInFxEditor, colors, dial::paint_dial,
    meter_ballistics::PeakMeterAnimation, optimistic_value::OptimisticValue, BusAction, BusId,
    BusState, FxLifecycle, TrackFxState, TrackProcessorDescriptor, MAX_BUS_GAIN_DB,
    MIN_BUS_GAIN_DB,
};
use egui_material_icons::icons::{
    ICON_DRAG_INDICATOR, ICON_MORE_VERT, ICON_VOLUME_MUTE, ICON_VOLUME_UP,
};

const METER_MIN_DB: f32 = -50.0;
const MIXER_STRIP_WIDTH: f32 = 92.0;
const MIN_METER_WIDTH: f32 = 24.0;
const MIN_METER_LANE_WIDTH: f32 = 3.0;
const BALANCE_SIZE: f32 = 26.0;
const MIN_FADER_HEIGHT: f32 = 88.0;
const MAX_FADER_HEIGHT: f32 = 260.0;
const FADER_FOOTER_HEIGHT: f32 = 76.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BusDragPayload {
    pub bus_id: BusId,
}

#[derive(Debug, Default)]
pub struct BusControls {
    gain: OptimisticValue<f32>,
    gain_dragging: bool,
    balance: OptimisticValue<f32>,
    balance_drag_start: Option<f32>,
    peaks: Vec<PeakMeterAnimation>,
    remove_confirmation_open: bool,
    fx_logs_open: bool,
    builtin_fx_editor: BuiltInFxEditor,
    #[cfg(test)]
    test_rects: TestBusControlRects,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TestBusControlRects {
    block: Option<egui::Rect>,
    name: Option<egui::Rect>,
    drag: Option<egui::Rect>,
    menu: Option<egui::Rect>,
    remove: Option<egui::Rect>,
    confirm_remove: Option<egui::Rect>,
    meter: Option<egui::Rect>,
    mute: Option<egui::Rect>,
    gain: Option<egui::Rect>,
    balance: Option<egui::Rect>,
    fx: Option<egui::Rect>,
}

impl BusControls {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &BusState,
        incoming_routes: usize,
        outgoing_links: usize,
    ) -> Vec<BusAction> {
        self.show_with_processor(ui, state, incoming_routes, outgoing_links, None)
    }

    pub fn show_with_processor(
        &mut self,
        ui: &mut egui::Ui,
        state: &BusState,
        incoming_routes: usize,
        outgoing_links: usize,
        processor: Option<&TrackProcessorDescriptor>,
    ) -> Vec<BusAction> {
        self.show_with_catalog(ui, state, incoming_routes, outgoing_links, processor, &[])
    }

    pub fn show_with_catalog(
        &mut self,
        ui: &mut egui::Ui,
        state: &BusState,
        incoming_routes: usize,
        outgoing_links: usize,
        processor: Option<&TrackProcessorDescriptor>,
        catalog: &[TrackProcessorDescriptor],
    ) -> Vec<BusAction> {
        #[cfg(test)]
        {
            self.test_rects = TestBusControlRects::default();
        }
        let _span = tracing::trace_span!(
            "frontend.egui.bus_controls",
            bus_id = state.id.raw(),
            channel_count = state.channels.len()
        )
        .entered();
        if state.control_error.is_some() {
            self.gain.clear();
            self.balance.clear();
        }
        let mut actions = Vec::new();
        let _response = egui::Frame::new()
            .fill(colors::RAISED_BACKGROUND)
            .stroke(egui::Stroke::new(1.0, egui::Color32::WHITE))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    let meter_width =
                        MIN_METER_WIDTH.max(state.channels.len() as f32 * MIN_METER_LANE_WIDTH);
                    let strip_width = MIXER_STRIP_WIDTH.max(
                        meter_width + ui.spacing().interact_size.x + ui.spacing().item_spacing.x,
                    );
                    ui.set_width(strip_width);
                    let color = if state.control_error.is_some() || state.structural_error.is_some()
                    {
                        colors::ERROR
                    } else {
                        colors::FOREGROUND
                    };
                    let name = ui.add_sized(
                        [ui.available_width(), 20.0],
                        egui::Label::new(egui::RichText::new(&state.name).strong().color(color))
                            .truncate(),
                    );
                    #[cfg(test)]
                    {
                        self.test_rects.name = Some(name.rect);
                    }
                    if let Some(error) = state
                        .structural_error
                        .as_ref()
                        .or(state.control_error.as_ref())
                    {
                        name.on_hover_text(error);
                    }
                    ui.horizontal(|ui| {
                        let (drag_rect, drag) =
                            ui.allocate_exact_size(egui::vec2(18.0, 20.0), egui::Sense::drag());
                        drag.dnd_set_drag_payload(BusDragPayload { bus_id: state.id });
                        #[cfg(test)]
                        {
                            self.test_rects.drag = Some(drag_rect);
                        }
                        ui.painter().text(
                            drag_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            ICON_DRAG_INDICATOR.codepoint,
                            egui::FontId::new(16.0, ICON_DRAG_INDICATOR.font_family()),
                            colors::MUTED_FOREGROUND,
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let _menu =
                                ui.menu_button(ICON_MORE_VERT.rich_text().size(17.0), |ui| {
                                    let remove = ui.button("Delete bus");
                                    #[cfg(test)]
                                    {
                                        self.test_rects.remove = Some(remove.rect);
                                    }
                                    if remove.clicked() {
                                        self.remove_confirmation_open = true;
                                        ui.close();
                                    }
                                });
                            #[cfg(test)]
                            {
                                self.test_rects.menu = Some(_menu.response.rect);
                            }
                        });
                    });
                    self.show_control_row(ui, state, meter_width, processor, catalog, &mut actions);
                });
            })
            .response;
        #[cfg(test)]
        {
            self.test_rects.block = Some(_response.rect);
        }
        self.show_fx_windows(ui.ctx(), state, processor, &mut actions);
        if self.remove_confirmation_open {
            let mut open = true;
            let mut remove = false;
            let mut cancel = false;
            egui::Window::new("Remove bus?")
                .id(egui::Id::new(("remove_bus", state.id.raw())))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label(format!(
                        "Remove '{}' and {} incoming route(s) and {} system link(s)?",
                        state.name, incoming_routes, outgoing_links
                    ));
                    ui.horizontal(|ui| {
                        let remove_button = ui.button("Remove");
                        #[cfg(test)]
                        {
                            self.test_rects.confirm_remove = Some(remove_button.rect);
                        }
                        remove = remove_button.clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            if remove {
                actions.push(BusAction::Remove);
            }
            self.remove_confirmation_open = open && !remove && !cancel;
        }
        actions
    }

    fn show_control_row(
        &mut self,
        ui: &mut egui::Ui,
        state: &BusState,
        meter_width: f32,
        processor: Option<&TrackProcessorDescriptor>,
        catalog: &[TrackProcessorDescriptor],
        actions: &mut Vec<BusAction>,
    ) {
        let stereo = state.stereo();
        let slider_thickness = ui.spacing().interact_size.x;
        let group_width = meter_width + ui.spacing().item_spacing.x + slider_thickness;
        let fader_height =
            (ui.available_height() - FADER_FOOTER_HEIGHT).clamp(MIN_FADER_HEIGHT, MAX_FADER_HEIGHT);
        let mut gain = self.gain.resolve(state.gain_db, self.gain_dragging);
        let fill = if state.muted {
            colors::MUTED_SLIDER_FILL
        } else {
            colors::COLORED_HIGHLIGHT
        };
        let gain_response = ui
            .horizontal(|ui| {
                ui.add_space((ui.available_width() - group_width).max(0.0) / 2.0);
                let (meter_rect, _meter_response) = ui.allocate_exact_size(
                    egui::vec2(meter_width, fader_height),
                    egui::Sense::hover(),
                );
                #[cfg(test)]
                {
                    self.test_rects.meter = Some(_meter_response.rect);
                }
                self.paint_meter(ui, meter_rect, state);

                let slider_width = ui.spacing().slider_width;
                ui.spacing_mut().slider_width = fader_height;
                let response = ui
                    .scope(|ui| {
                        ui.visuals_mut().selection.bg_fill = fill;
                        ui.add(
                            egui::Slider::new(&mut gain, MIN_BUS_GAIN_DB..=MAX_BUS_GAIN_DB)
                                .vertical()
                                .show_value(false)
                                .trailing_fill(true),
                        )
                    })
                    .inner;
                ui.spacing_mut().slider_width = slider_width;
                response
            })
            .inner
            .on_hover_text(format!("Bus gain: {gain:.1} dB"));
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

        ui.vertical_centered(|ui| {
            ui.label(format!("{gain:.1} dB"));
            let (balance_rect, balance_response) = ui.allocate_exact_size(
                egui::vec2(BALANCE_SIZE, BALANCE_SIZE),
                if stereo {
                    egui::Sense::click_and_drag()
                } else {
                    egui::Sense::hover()
                },
            );
            if stereo {
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
                    ui,
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
                ui.painter().text(
                    balance_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "—",
                    egui::FontId::proportional(12.0),
                    colors::MUTED_FOREGROUND,
                );
                #[cfg(test)]
                {
                    self.test_rects.balance = None;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - group_width).max(0.0) / 2.0);
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
            let mute = ui
                .add_sized(
                    [group_width, 24.0],
                    egui::Button::new(icon.rich_text().size(16.0).color(color)),
                )
                .on_hover_text("Mute/unmute bus");
            #[cfg(test)]
            {
                self.test_rects.mute = Some(mute.rect);
            }
            if mute.clicked() {
                actions.push(BusAction::MuteChanged(!state.muted));
            }
        });
        self.show_fx_row(ui, state, processor, catalog, group_width, actions);
    }

    fn show_fx_row(
        &mut self,
        ui: &mut egui::Ui,
        state: &BusState,
        processor: Option<&TrackProcessorDescriptor>,
        catalog: &[TrackProcessorDescriptor],
        group_width: f32,
        actions: &mut Vec<BusAction>,
    ) {
        let Some(fx) = &state.fx else {
            let mut selected = String::from("none");
            egui::ComboBox::from_id_salt(("bus_fx_processor", state.id.raw()))
                .selected_text("No FX")
                .show_ui(ui, |ui| {
                    for entry in catalog.iter().filter(|entry| entry.available) {
                        ui.selectable_value(
                            &mut selected,
                            entry.id.as_str().to_owned(),
                            entry.label.as_str(),
                        );
                    }
                });
            if selected != "none" {
                if let Some(entry) = catalog.iter().find(|entry| entry.id.as_str() == selected) {
                    actions.push(BusAction::FxProcessorChanged(Some(entry.id.clone())));
                }
            }
            return;
        };
        let features = processor.map(|value| value.features).unwrap_or_default();
        let color = bus_fx_color(fx);
        let fx_button = ui
            .add_sized(
                [group_width, 22.0],
                egui::Button::new(egui::RichText::new("FX").color(color)),
            )
            .on_hover_text(bus_fx_hover_text(fx));
        #[cfg(test)]
        {
            self.test_rects.fx = Some(fx_button.rect);
        }
        if fx_button.clicked() {
            actions.push(bus_fx_primary_action(fx));
        }
        fx_button.context_menu(|ui| {
            if ui
                .add_enabled(features.logs, egui::Button::new("Process logs..."))
                .clicked()
            {
                self.fx_logs_open = true;
                ui.close();
            }
            if ui.button("Remove FX").clicked() {
                actions.push(BusAction::FxProcessorChanged(None));
                ui.close();
            }
            for entry in catalog.iter().filter(|entry| {
                entry.available && Some(entry.id.clone()) != Some(fx.processor_type.clone())
            }) {
                if ui.button(entry.label.as_str()).clicked() {
                    actions.push(BusAction::FxProcessorChanged(Some(entry.id.clone())));
                    ui.close();
                }
            }
        });
    }

    fn show_fx_windows(
        &mut self,
        context: &egui::Context,
        state: &BusState,
        processor: Option<&TrackProcessorDescriptor>,
        actions: &mut Vec<BusAction>,
    ) {
        actions.extend(
            self.builtin_fx_editor
                .show_for_bus(context, state, processor),
        );
        if !self.fx_logs_open {
            return;
        }
        let Some(fx) = &state.fx else {
            self.fx_logs_open = false;
            return;
        };
        let mut open = true;
        egui::Window::new(format!("{} — Process logs", state.name))
            .id(egui::Id::new(("bus_fx_logs", state.id.raw())))
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
                if fx.logs.is_empty() {
                    ui.weak("No process logs");
                }
                for log in fx.logs.iter() {
                    ui.label(format!(
                        "generation {}\nstdout:\n{}\nstderr:\n{}",
                        log.generation, log.stdout, log.stderr
                    ));
                }
                if ui.button("Clear").clicked() {
                    actions.push(BusAction::FxClearLogs);
                }
            });
        if !open {
            self.fx_logs_open = false;
        }
    }
}

fn bus_fx_color(fx: &TrackFxState) -> egui::Color32 {
    match fx.lifecycle {
        FxLifecycle::Running if fx.active => egui::Color32::LIGHT_GREEN,
        FxLifecycle::Running => egui::Color32::GRAY,
        FxLifecycle::Starting | FxLifecycle::Degraded | FxLifecycle::Restarting => {
            egui::Color32::YELLOW
        }
        FxLifecycle::Crashed | FxLifecycle::Unavailable => egui::Color32::LIGHT_RED,
        FxLifecycle::Stopped => egui::Color32::GRAY,
    }
}

fn bus_fx_hover_text(fx: &TrackFxState) -> String {
    format!(
        "{}: {:?}{}",
        fx.processor_type,
        fx.lifecycle,
        fx.status_summary
            .as_deref()
            .or(fx.crash_summary.as_deref())
            .map(|summary| format!(" — {summary}"))
            .unwrap_or_default()
    )
}

fn bus_fx_primary_action(fx: &TrackFxState) -> BusAction {
    if matches!(
        fx.lifecycle,
        FxLifecycle::Crashed | FxLifecycle::Unavailable
    ) {
        BusAction::FxToggleOrRecover
    } else {
        BusAction::FxVisibilityChanged(!fx.visible)
    }
}

impl BusControls {
    #[cfg(test)]
    pub(crate) fn block_rect(&self) -> Option<egui::Rect> {
        self.test_rects.block
    }

    #[cfg(test)]
    pub(crate) fn gain_rect(&self) -> Option<egui::Rect> {
        self.test_rects.gain
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
                egui::Rect::from_min_max(
                    egui::pos2(
                        segment.left(),
                        segment.bottom() - segment.height() * fraction,
                    ),
                    segment.right_bottom(),
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

    #[cfg(test)]
    pub(crate) fn fx_rect(&self) -> Option<egui::Rect> {
        self.test_rects.fx
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        BuiltInFxState, BusChannelId, BusChannelState, BusId, PortId, TrackProcessorConstraints,
        TrackProcessorEditorDescriptor, TrackProcessorEditorState, TrackProcessorFeatures,
        TrackProcessorMidiPolicy, TrackProcessorTypeId,
    };

    fn state(channels: usize) -> BusState {
        BusState {
            id: BusId::from_raw(1),
            name: "Master".to_owned(),
            structural_state: crate::StructuralState::Confirmed,
            structural_error: None,
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
            fx: None,
        }
    }

    fn frame(
        context: &egui::Context,
        controls: &mut BusControls,
        state: &BusState,
        events: Vec<egui::Event>,
    ) -> Vec<BusAction> {
        frame_with_processor(context, controls, state, None, events)
    }

    fn frame_with_processor(
        context: &egui::Context,
        controls: &mut BusControls,
        state: &BusState,
        processor: Option<&TrackProcessorDescriptor>,
        events: Vec<egui::Event>,
    ) -> Vec<BusAction> {
        let mut actions = Vec::new();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(240.0, 420.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = controls.show_with_processor(ui, state, 0, 0, processor),
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
        assert!(
            controls.test_rects.gain.unwrap().height() > controls.test_rects.gain.unwrap().width()
        );
        assert!(
            controls.test_rects.meter.unwrap().height()
                > controls.test_rects.meter.unwrap().width()
        );
        assert_eq!(controls.peaks.len(), 2);

        frame(&context, &mut controls, &state(3), Vec::new());
        assert!(controls.test_rects.balance.is_none());
        assert_eq!(controls.peaks.len(), 3);

        frame(&context, &mut controls, &state(64), Vec::new());
        assert!(controls.test_rects.meter.unwrap().width() >= 64.0 * MIN_METER_LANE_WIDTH);
        assert_eq!(controls.peaks.len(), 64);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn strip_keeps_vertical_mixer_layout_inside_a_horizontal_bus_list() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut controls = BusControls::default();
        let state = state(2);
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(480.0, 300.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| {
                    controls.show(ui, &state, 0, 0);
                });
            },
        );
        output.textures_delta.clear();

        let drag = controls.test_rects.drag.unwrap();
        let name = controls.test_rects.name.unwrap();
        let menu = controls.test_rects.menu.unwrap();
        let balance = controls.test_rects.balance.unwrap();
        let meter = controls.test_rects.meter.unwrap();
        let gain = controls.test_rects.gain.unwrap();
        let mute = controls.test_rects.mute.unwrap();
        assert!(drag.top() >= name.bottom());
        assert!(menu.top() >= name.bottom());
        assert!(name.width() > drag.width() + menu.width());
        assert!(meter.top() >= drag.bottom());
        assert!(gain.top() >= drag.bottom());
        assert!(balance.top() >= meter.bottom());
        assert!(mute.top() >= balance.bottom());
        assert!((mute.left() - meter.left()).abs() < 1.0);
        assert!(mute.right() >= gain.right());
        assert!(meter.height() >= MIN_FADER_HEIGHT);
        assert!(controls.test_rects.block.unwrap().width() < 120.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pending_control_change_keeps_the_optimistic_strip_unchanged() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut controls = BusControls::default();
        let mut state = state(2);
        frame(&context, &mut controls, &state, Vec::new());
        let settled_block = controls.test_rects.block.unwrap();
        state.control_pending = true;
        frame(&context, &mut controls, &state, Vec::new());
        assert!(controls.test_rects.menu.is_some());
        assert_eq!(
            controls.test_rects.block.unwrap().size(),
            settled_block.size()
        );
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
                gain_start - egui::vec2(0.0, 30.0),
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
                pos: gain_start - egui::vec2(0.0, 30.0),
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

    #[shoop_wasm_test_support::shoop_test]
    fn drag_payload_and_confirmation_gated_remove_use_stable_bus_identity() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut controls = BusControls::default();
        let state = state(4);
        frame(&context, &mut controls, &state, Vec::new());
        let drag = controls.test_rects.drag.unwrap().center();
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(drag),
                egui::Event::PointerButton {
                    pos: drag,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(drag + egui::vec2(0.0, 12.0)),
            ],
        );
        assert_eq!(
            egui::DragAndDrop::payload::<BusDragPayload>(&context).as_deref(),
            Some(&BusDragPayload { bus_id: state.id })
        );
        egui::DragAndDrop::clear_payload(&context);

        let menu = controls.test_rects.menu.unwrap().center();
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(menu),
                egui::Event::PointerButton {
                    pos: menu,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(menu),
                egui::Event::PointerButton {
                    pos: menu,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&context, &mut controls, &state, Vec::new());
        let remove = controls.test_rects.remove.unwrap().center();
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(remove),
                egui::Event::PointerButton {
                    pos: remove,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(remove),
                egui::Event::PointerButton {
                    pos: remove,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&context, &mut controls, &state, Vec::new());
        let confirm = controls.test_rects.confirm_remove.unwrap().center();
        frame(
            &context,
            &mut controls,
            &state,
            vec![
                egui::Event::PointerMoved(confirm),
                egui::Event::PointerButton {
                    pos: confirm,
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
                egui::Event::PointerMoved(confirm),
                egui::Event::PointerButton {
                    pos: confirm,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(actions, [BusAction::Remove]);
    }

    fn fx_fixture() -> (BusState, TrackProcessorDescriptor) {
        let processor = TrackProcessorDescriptor {
            id: TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
            label: "Built-in FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: TrackProcessorConstraints {
                min_dry_audio_channels: Some(1),
                max_dry_audio_channels: None,
                min_wet_audio_channels: Some(1),
                max_wet_audio_channels: None,
                matching_audio_channels: true,
                midi: TrackProcessorMidiPolicy::Unsupported,
            },
            features: TrackProcessorFeatures {
                state: true,
                embedded_ui: true,
                logs: true,
                recovery: true,
                ..TrackProcessorFeatures::default()
            },
            editor: Some(TrackProcessorEditorDescriptor::BuiltInFx),
        };
        let mut state = state(2);
        state.fx = Some(TrackFxState {
            processor_type: processor.id.clone(),
            active: true,
            visible: false,
            lifecycle: FxLifecycle::Running,
            generation: 1,
            deadline_misses: 0,
            stale_completions: 0,
            status_summary: None,
            crash_summary: None,
            logs: Arc::from([]),
            editor: Some(TrackProcessorEditorState::BuiltInFx(
                BuiltInFxState::default(),
            )),
        });
        (state, processor)
    }

    fn click(
        context: &egui::Context,
        controls: &mut BusControls,
        state: &BusState,
        processor: &TrackProcessorDescriptor,
        position: egui::Pos2,
    ) -> Vec<BusAction> {
        frame_with_processor(
            context,
            controls,
            state,
            Some(processor),
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
        frame_with_processor(
            context,
            controls,
            state,
            Some(processor),
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
    fn fx_button_toggles_visibility_without_midi_ports() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut controls = BusControls::default();
        let (state, processor) = fx_fixture();
        frame_with_processor(
            &context,
            &mut controls,
            &state,
            Some(&processor),
            Vec::new(),
        );
        assert!(controls.fx_rect().is_some());
        let position = controls.fx_rect().unwrap().center();
        let actions = click(&context, &mut controls, &state, &processor, position);
        assert_eq!(actions, [BusAction::FxVisibilityChanged(true)]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fx_picker_hides_synth_processors() {
        let catalog = [
            TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
            TrackProcessorTypeId::new(TrackProcessorTypeId::OXISYNTH),
        ];
        let bus_catalog = catalog
            .iter()
            .filter(|id| id.as_str() != TrackProcessorTypeId::OXISYNTH)
            .collect::<Vec<_>>();
        assert_eq!(bus_catalog.len(), 1);
        assert_eq!(bus_catalog[0].as_str(), TrackProcessorTypeId::BUILTIN_FX);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fx_processor_action_round_trips_through_match() {
        let change = BusAction::FxProcessorChanged(Some(TrackProcessorTypeId::new(
            TrackProcessorTypeId::BUILTIN_FX,
        )));
        assert!(matches!(change, BusAction::FxProcessorChanged(Some(_))));
        let remove = BusAction::FxProcessorChanged(None);
        assert!(matches!(remove, BusAction::FxProcessorChanged(None)));
    }
}
