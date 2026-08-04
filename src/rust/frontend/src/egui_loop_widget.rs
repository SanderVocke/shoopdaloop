use std::pin::Pin;
use std::sync::{Arc, RwLock};

use cxx_qt_lib::QString;
use egui_cxx_qt::{
    egui, CanvasHandle, CanvasInfo, CanvasQueueError, CanvasSubclass, CanvasUiFactory, EguiUi,
};
use egui_material_icons::icons::{
    ICON_BORDER_CLEAR, ICON_EDIT_NOTE, ICON_FIBER_MANUAL_RECORD, ICON_HELP, ICON_PLAY_ARROW,
    ICON_STAR, ICON_STOP, ICON_TIMER, ICON_VIEW_LIST,
};
use egui_material_icons::MaterialIcon;
use shoop_engine::LoopMode;

pub fn initialize() {
    egui_cxx_qt::initialize();
}

#[egui_cxx_qt::canvas_bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        type ShoopEguiLoopWidget = super::ShoopEguiLoopWidgetRust;

        #[qsignal]
        #[cxx_name = "iconClicked"]
        fn icon_clicked(self: Pin<&mut ShoopEguiLoopWidget>);

        #[qsignal]
        #[cxx_name = "iconDoubleClicked"]
        fn icon_double_clicked(self: Pin<&mut ShoopEguiLoopWidget>);

        #[qsignal]
        #[cxx_name = "playClicked"]
        fn play_clicked(self: Pin<&mut ShoopEguiLoopWidget>);

        #[qsignal]
        #[cxx_name = "recordClicked"]
        fn record_clicked(self: Pin<&mut ShoopEguiLoopWidget>);

        #[qsignal]
        #[cxx_name = "stopClicked"]
        fn stop_clicked(self: Pin<&mut ShoopEguiLoopWidget>);

        #[qsignal]
        #[cxx_name = "gainChanged"]
        fn gain_changed(self: Pin<&mut ShoopEguiLoopWidget>, value: f32);

        #[qinvokable]
        #[cxx_name = "setLoopState"]
        fn set_loop_state(
            self: Pin<&mut ShoopEguiLoopWidget>,
            name: QString,
            position: f32,
            mode: i32,
            next_mode: i32,
            next_transition_delay: i32,
            empty: bool,
            regular_composite: bool,
            script_composite: bool,
            sync: bool,
            targeted: bool,
            selected: bool,
            selected_composite_kind: i32,
            show_gain: bool,
            gain: f32,
            play_after_record: bool,
        );

        #[qinvokable]
        #[cxx_name = "setPeakState"]
        fn set_peak_state(
            self: Pin<&mut ShoopEguiLoopWidget>,
            stereo: bool,
            peak_left_db: f32,
            peak_right_db: f32,
            midi_activity: bool,
        );
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_shoop_egui_loop_widget"]
        unsafe fn register_qml_type(
            inference_example: *mut ShoopEguiLoopWidget,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut module_name = module_name.to_owned();
    let mut type_name = type_name.to_owned();
    unsafe {
        ffi::register_qml_type_shoop_egui_loop_widget(
            std::ptr::null_mut(),
            &mut module_name,
            1,
            0,
            &mut type_name,
        );
    }
}

#[derive(Clone)]
pub(crate) struct LoopState {
    name: String,
    position: f32,
    mode: i32,
    next_mode: i32,
    next_transition_delay: i32,
    empty: bool,
    regular_composite: bool,
    script_composite: bool,
    sync: bool,
    targeted: bool,
    selected: bool,
    selected_composite_kind: i32,
    show_gain: bool,
    gain: f32,
    play_after_record: bool,
    stereo: bool,
    peak_left_db: f32,
    peak_right_db: f32,
    midi_activity: bool,
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
            name: "Loop".to_owned(),
            position: 0.0,
            mode: LoopMode::Unknown as i32,
            next_mode: LoopMode::Unknown as i32,
            next_transition_delay: -1,
            empty: true,
            regular_composite: false,
            script_composite: false,
            sync: false,
            targeted: false,
            selected: false,
            selected_composite_kind: 0,
            show_gain: false,
            gain: 0.6,
            play_after_record: true,
            stereo: false,
            peak_left_db: -200.0,
            peak_right_db: -200.0,
            midi_activity: false,
        }
    }
}

impl LoopState {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_loop_state(
        &mut self,
        name: String,
        position: f32,
        mode: i32,
        next_mode: i32,
        next_transition_delay: i32,
        empty: bool,
        regular_composite: bool,
        script_composite: bool,
        sync: bool,
        targeted: bool,
        selected: bool,
        selected_composite_kind: i32,
        show_gain: bool,
        gain: f32,
        play_after_record: bool,
    ) {
        self.name = name;
        self.position = position.clamp(0.0, 1.0);
        self.mode = mode;
        self.next_mode = next_mode;
        self.next_transition_delay = next_transition_delay;
        self.empty = empty;
        self.regular_composite = regular_composite;
        self.script_composite = script_composite;
        self.sync = sync;
        self.targeted = targeted;
        self.selected = selected;
        self.selected_composite_kind = selected_composite_kind;
        self.show_gain = show_gain;
        self.gain = gain.clamp(0.0, 1.0);
        self.play_after_record = play_after_record;
    }

    pub(crate) fn update_peak_state(
        &mut self,
        stereo: bool,
        peak_left_db: f32,
        peak_right_db: f32,
        midi_activity: bool,
    ) {
        self.stereo = stereo;
        self.peak_left_db = peak_left_db;
        self.peak_right_db = peak_right_db;
        self.midi_activity = midi_activity;
    }
}

pub struct ShoopEguiLoopWidgetRust {
    state: Arc<RwLock<LoopState>>,
}

impl Default for ShoopEguiLoopWidgetRust {
    fn default() -> Self {
        Self {
            state: Arc::new(RwLock::new(LoopState::default())),
        }
    }
}

impl ffi::ShoopEguiLoopWidget {
    #[allow(clippy::too_many_arguments)]
    fn set_loop_state(
        mut self: Pin<&mut Self>,
        name: QString,
        position: f32,
        mode: i32,
        next_mode: i32,
        next_transition_delay: i32,
        empty: bool,
        regular_composite: bool,
        script_composite: bool,
        sync: bool,
        targeted: bool,
        selected: bool,
        selected_composite_kind: i32,
        show_gain: bool,
        gain: f32,
        play_after_record: bool,
    ) {
        let mut state = self.state.write().expect("loop state lock poisoned");
        state.update_loop_state(
            name.to_string(),
            position,
            mode,
            next_mode,
            next_transition_delay,
            empty,
            regular_composite,
            script_composite,
            sync,
            targeted,
            selected,
            selected_composite_kind,
            show_gain,
            gain,
            play_after_record,
        );
        drop(state);
        self.as_mut().request_repaint();
    }

    fn set_peak_state(
        mut self: Pin<&mut Self>,
        stereo: bool,
        peak_left_db: f32,
        peak_right_db: f32,
        midi_activity: bool,
    ) {
        let mut state = self.state.write().expect("loop state lock poisoned");
        state.update_peak_state(stereo, peak_left_db, peak_right_db, midi_activity);
        drop(state);
        self.as_mut().request_repaint();
    }
}

impl CanvasSubclass for ffi::ShoopEguiLoopWidget {
    fn ui_factory(self: Pin<&mut Self>, canvas: CanvasHandle<Self>) -> CanvasUiFactory {
        let state = Arc::clone(&self.state);
        CanvasUiFactory::new(move || {
            Box::new(LoopWidgetUi {
                state: Arc::clone(&state),
                canvas: canvas.clone(),
                icons_initialized: false,
                gain_drag_start: None,
            })
        })
    }
}

struct LoopWidgetUi {
    state: Arc<RwLock<LoopState>>,
    canvas: CanvasHandle<ffi::ShoopEguiLoopWidget>,
    icons_initialized: bool,
    gain_drag_start: Option<f32>,
}

impl LoopWidgetUi {
    fn emit_icon_clicked(&self) {
        self.queue_signal(|mut canvas| canvas.as_mut().icon_clicked());
    }

    fn emit_icon_double_clicked(&self) {
        self.queue_signal(|mut canvas| canvas.as_mut().icon_double_clicked());
    }

    fn emit_play_clicked(&self) {
        self.queue_signal(|mut canvas| canvas.as_mut().play_clicked());
    }

    fn emit_record_clicked(&self) {
        self.queue_signal(|mut canvas| canvas.as_mut().record_clicked());
    }

    fn emit_stop_clicked(&self) {
        self.queue_signal(|mut canvas| canvas.as_mut().stop_clicked());
    }

    fn emit_gain_changed(&self, value: f32) {
        self.queue_signal(move |mut canvas| canvas.as_mut().gain_changed(value));
    }

    fn queue_signal(
        &self,
        signal: impl FnOnce(Pin<&mut ffi::ShoopEguiLoopWidget>) + Send + 'static,
    ) {
        match self.canvas.queue(signal) {
            Ok(()) | Err(CanvasQueueError::ObjectDestroyed) => {}
            Err(error) => eprintln!("failed to emit egui loop widget signal: {error}"),
        }
    }
}

pub(crate) trait LoopWidgetActionSink {
    fn emit_icon_clicked(&mut self);
    fn emit_icon_double_clicked(&mut self);
    fn emit_play_clicked(&mut self);
    fn emit_record_clicked(&mut self);
    fn emit_stop_clicked(&mut self);
    fn emit_gain_changed(&mut self, value: f32);
    fn gain_drag_start(&mut self) -> &mut Option<f32>;
}

impl LoopWidgetActionSink for LoopWidgetUi {
    fn emit_icon_clicked(&mut self) {
        LoopWidgetUi::emit_icon_clicked(self);
    }

    fn emit_icon_double_clicked(&mut self) {
        LoopWidgetUi::emit_icon_double_clicked(self);
    }

    fn emit_play_clicked(&mut self) {
        LoopWidgetUi::emit_play_clicked(self);
    }

    fn emit_record_clicked(&mut self) {
        LoopWidgetUi::emit_record_clicked(self);
    }

    fn emit_stop_clicked(&mut self) {
        LoopWidgetUi::emit_stop_clicked(self);
    }

    fn emit_gain_changed(&mut self, value: f32) {
        LoopWidgetUi::emit_gain_changed(self, value);
    }

    fn gain_drag_start(&mut self) -> &mut Option<f32> {
        &mut self.gain_drag_start
    }
}

fn loop_mode(value: i32) -> LoopMode {
    LoopMode::try_from(value).unwrap_or(LoopMode::Unknown)
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
        return (ICON_BORDER_CLEAR, egui::Color32::GRAY, false);
    }

    match mode {
        LoopMode::Playing => (
            ICON_PLAY_ARROW,
            if script_composite {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_rgb(0, 170, 0)
            },
            false,
        ),
        LoopMode::PlayingDryThroughWet => {
            (ICON_PLAY_ARROW, egui::Color32::from_rgb(255, 165, 0), true)
        }
        LoopMode::Recording => (ICON_FIBER_MANUAL_RECORD, egui::Color32::RED, false),
        LoopMode::RecordingDryIntoWet => (
            ICON_FIBER_MANUAL_RECORD,
            egui::Color32::from_rgb(255, 165, 0),
            true,
        ),
        LoopMode::Stopped if regular_composite => {
            (ICON_VIEW_LIST, egui::Color32::from_rgb(30, 30, 30), false)
        }
        LoopMode::Stopped if script_composite => {
            (ICON_EDIT_NOTE, egui::Color32::from_rgb(30, 30, 30), false)
        }
        LoopMode::Stopped => (ICON_STOP, egui::Color32::GRAY, false),
        _ => (ICON_HELP, egui::Color32::GRAY, false),
    }
}

fn peak_fraction(db: f32, minimum_db: f32) -> f32 {
    ((db - minimum_db) / -minimum_db).clamp(0.0, 1.0)
}

fn generated_loop_name(name: &str) -> bool {
    name.strip_prefix('(')
        .and_then(|name| name.strip_suffix(')'))
        .is_some_and(|name| {
            !name.is_empty() && name.chars().all(|character| character.is_ascii_digit())
        })
}

pub(crate) fn draw_loop_widget(
    sink: &mut impl LoopWidgetActionSink,
    ui: &mut egui::Ui,
    state: &LoopState,
    size: egui::Vec2,
) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let rounding = egui::CornerRadius::same(2);
    let background = if state.regular_composite {
        egui::Color32::from_rgb(255, 192, 203)
    } else if state.script_composite {
        egui::Color32::from_rgb(119, 170, 119)
    } else if !state.empty {
        egui::Color32::from_rgb(0, 0, 68)
    } else {
        egui::Color32::from_rgb(30, 30, 30)
    };
    ui.painter().rect_filled(rect, rounding, background);

    if state.position > 0.0 {
        let progress_color = match loop_mode(state.mode) {
            LoopMode::Playing => egui::Color32::from_rgb(0, 68, 0),
            LoopMode::PlayingDryThroughWet => egui::Color32::from_rgb(51, 51, 0),
            LoopMode::Recording => egui::Color32::from_rgb(102, 0, 0),
            LoopMode::RecordingDryIntoWet => egui::Color32::from_rgb(102, 51, 0),
            _ => egui::Color32::from_rgb(68, 68, 68),
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
            .rect_filled(midi_rect, 0.0, egui::Color32::CYAN);
    }

    let meter_color = egui::Color32::from_rgb(0, 188, 212);
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
        let meter_width = (rect.width() - 4.0).max(0.0) * peak_fraction(state.peak_left_db, -30.0);
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
        egui::Color32::from_rgb(255, 165, 0)
    } else if state.selected {
        egui::Color32::YELLOW
    } else if state.selected_composite_kind == 1 {
        egui::Color32::from_rgb(255, 192, 203)
    } else if state.selected_composite_kind == 2 {
        egui::Color32::from_rgb(119, 170, 119)
    } else if state.empty {
        egui::Color32::GRAY
    } else {
        egui::Color32::from_gray(221)
    };
    ui.painter().rect_stroke(
        rect,
        rounding,
        egui::Stroke::new(2.0, border_color),
        egui::StrokeKind::Inside,
    );

    let icon_rect = egui::Rect::from_min_size(rect.min, egui::vec2(24.0, 24.0));
    let has_transition = state.next_transition_delay >= 0 && state.mode != state.next_mode;
    if has_transition {
        if state.next_transition_delay > 0 {
            ui.painter().text(
                icon_rect.center(),
                egui::Align2::CENTER_CENTER,
                (state.next_transition_delay + 1).to_string(),
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        } else {
            paint_icon(
                ui.painter(),
                icon_rect.center(),
                ICON_TIMER,
                20.0,
                egui::Color32::WHITE,
            );
        }
        let (next_icon, next_color, next_fx) =
            icon_for_state(loop_mode(state.next_mode), false, false, false);
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
                egui::Color32::WHITE,
            );
        }
    } else {
        let (icon, color, fx) = icon_for_state(
            loop_mode(state.mode),
            state.empty,
            state.regular_composite,
            state.script_composite,
        );
        paint_icon(ui.painter(), icon_rect.center(), icon, 21.0, color);
        if fx {
            ui.painter().text(
                icon_rect.right_bottom() - egui::vec2(1.0, 1.0),
                egui::Align2::RIGHT_BOTTOM,
                "FX",
                egui::FontId::proportional(7.0),
                egui::Color32::WHITE,
            );
        }
    }

    if state.sync {
        paint_icon(
            ui.painter(),
            egui::pos2(rect.left() + 6.0, rect.top() + 6.0),
            ICON_STAR,
            10.0,
            egui::Color32::YELLOW,
        );
    }

    let icon_click_rect = if response.hovered() {
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
        sink.emit_icon_double_clicked();
    } else if icon_response.clicked() {
        sink.emit_icon_clicked();
    }

    let dial_rect = if state.show_gain {
        Some(egui::Rect::from_center_size(
            egui::pos2(rect.right() - 14.0, rect.center().y),
            egui::vec2(18.0, 18.0),
        ))
    } else {
        None
    };

    if response.hovered() {
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
        let stop_rect = if state.script_composite {
            record_rect
        } else {
            record_rect.translate(egui::vec2(button_width + gap, 0.0))
        };
        let icon_size = button_width.min(button_height) * 0.9;

        if ui
            .put(
                play_rect,
                egui::Button::new(ICON_PLAY_ARROW.rich_text().size(icon_size).color(
                    if state.script_composite {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(0, 128, 0)
                    },
                )),
            )
            .clicked()
        {
            sink.emit_play_clicked();
        }
        if !state.script_composite {
            let record_response = ui.put(record_rect, egui::Button::new(""));
            paint_icon(
                ui.painter(),
                record_rect.center(),
                ICON_FIBER_MANUAL_RECORD,
                icon_size,
                egui::Color32::RED,
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
                    egui::Color32::from_rgb(0, 128, 0),
                );
            }
            if record_response.clicked() {
                sink.emit_record_clicked();
            }
        }
        if ui
            .put(
                stop_rect,
                egui::Button::new(
                    ICON_STOP
                        .rich_text()
                        .size(icon_size)
                        .color(egui::Color32::WHITE),
                ),
            )
            .clicked()
        {
            sink.emit_stop_clicked();
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
                egui::Color32::GRAY
            } else {
                egui::Color32::WHITE
            },
        );
    }

    if let Some(dial_rect) = dial_rect {
        let dial_response = ui.interact(
            dial_rect,
            ui.id().with("loop_gain"),
            egui::Sense::click_and_drag(),
        );
        if dial_response.drag_started() {
            *sink.gain_drag_start() = Some(state.gain);
        }
        let mut gain = state.gain;
        if dial_response.dragged() {
            let start = sink.gain_drag_start().unwrap_or(state.gain);
            gain = (start - dial_response.drag_delta().y / 100.0).clamp(0.0, 1.0);
        }
        if dial_response.double_clicked() {
            gain = 0.6;
        }
        if (gain - state.gain).abs() > f32::EPSILON {
            sink.emit_gain_changed(gain);
        }
        if dial_response.drag_stopped() {
            *sink.gain_drag_start() = None;
        }

        let visuals = ui.style().interact(&dial_response);
        ui.painter().circle_filled(
            dial_rect.center(),
            dial_rect.width() / 2.0,
            egui::Color32::from_rgb(34, 34, 34),
        );
        ui.painter().circle_stroke(
            dial_rect.center(),
            dial_rect.width() / 2.0,
            egui::Stroke::new(1.0, visuals.fg_stroke.color),
        );
        let angle = -2.35 + gain * 4.7;
        let radius = dial_rect.width() * 0.32;
        let indicator = egui::pos2(
            dial_rect.center().x + angle.sin() * radius,
            dial_rect.center().y - angle.cos() * radius,
        );
        ui.painter().line_segment(
            [dial_rect.center(), indicator],
            egui::Stroke::new(1.5, egui::Color32::WHITE),
        );
        ui.painter().text(
            dial_rect.center(),
            egui::Align2::CENTER_CENTER,
            "V",
            egui::FontId::proportional(7.0),
            egui::Color32::from_gray(180),
        );
    }
}

impl EguiUi for LoopWidgetUi {
    fn draw(&mut self, root_ui: &mut egui::Ui, _canvas: CanvasInfo) {
        if !self.icons_initialized {
            egui_material_icons::initialize(root_ui.ctx());
            self.icons_initialized = true;
        }

        let state = self.state.read().expect("loop state lock poisoned").clone();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(0.0),
            )
            .show(root_ui, |ui| {
                let size = egui::vec2(ui.available_width(), ui.available_height());
                draw_loop_widget(self, ui, &state, size);
            });
    }
}
