use std::pin::Pin;
use std::sync::{Arc, RwLock};

use cxx_qt_lib::QString;
use egui_cxx_qt::{
    egui, CanvasHandle, CanvasInfo, CanvasQueueError, CanvasSubclass, CanvasUiFactory, EguiUi,
};
use shoop_egui::{
    CompositeKind, LoopMode as EguiLoopMode, LoopState, LoopWidget, LoopWidgetAction,
};
use shoop_engine::LoopMode as EngineLoopMode;

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

fn loop_mode(value: i32) -> EguiLoopMode {
    match EngineLoopMode::try_from(value).unwrap_or(EngineLoopMode::Unknown) {
        EngineLoopMode::Stopped => EguiLoopMode::Stopped,
        EngineLoopMode::Playing => EguiLoopMode::Playing,
        EngineLoopMode::Recording => EguiLoopMode::Recording,
        EngineLoopMode::Replacing => EguiLoopMode::Replacing,
        EngineLoopMode::PlayingDryThroughWet => EguiLoopMode::PlayingDryThroughWet,
        EngineLoopMode::RecordingDryIntoWet => EguiLoopMode::RecordingDryIntoWet,
        EngineLoopMode::Unknown => EguiLoopMode::Unknown,
    }
}

fn composite_kind(regular: bool, script: bool) -> CompositeKind {
    if regular {
        CompositeKind::Regular
    } else if script {
        CompositeKind::Script
    } else {
        CompositeKind::None
    }
}

fn selected_composite_kind(value: i32) -> CompositeKind {
    match value {
        1 => CompositeKind::Regular,
        2 => CompositeKind::Script,
        _ => CompositeKind::None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_loop_state(
    state: &mut LoopState,
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
    selected_composite_kind_value: i32,
    show_gain: bool,
    gain: f32,
    play_after_record: bool,
) {
    state.name = name;
    state.position = position.clamp(0.0, 1.0);
    state.mode = loop_mode(mode);
    state.next_mode = loop_mode(next_mode);
    state.next_transition_delay = u32::try_from(next_transition_delay).ok();
    state.empty = empty;
    state.composite_kind = composite_kind(regular_composite, script_composite);
    state.sync = sync;
    state.targeted = targeted;
    state.selected = selected;
    state.selected_composite_kind = selected_composite_kind(selected_composite_kind_value);
    state.show_gain = show_gain;
    state.gain = gain.clamp(0.0, 1.0);
    state.play_after_record = play_after_record;
}

pub(crate) fn apply_peak_state(
    state: &mut LoopState,
    stereo: bool,
    peak_left_db: f32,
    peak_right_db: f32,
    midi_activity: bool,
) {
    state.stereo = stereo;
    state.peak_left_db = peak_left_db;
    state.peak_right_db = peak_right_db;
    state.midi_activity = midi_activity;
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
        apply_loop_state(
            &mut state,
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
        apply_peak_state(
            &mut state,
            stereo,
            peak_left_db,
            peak_right_db,
            midi_activity,
        );
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
                widget: LoopWidget::default(),
            })
        })
    }
}

struct LoopWidgetUi {
    state: Arc<RwLock<LoopState>>,
    canvas: CanvasHandle<ffi::ShoopEguiLoopWidget>,
    icons_initialized: bool,
    widget: LoopWidget,
}

impl LoopWidgetUi {
    fn emit_action(&self, action: LoopWidgetAction) {
        self.queue_signal(move |mut canvas| match action {
            LoopWidgetAction::IconClicked(_) => canvas.as_mut().icon_clicked(),
            LoopWidgetAction::IconDoubleClicked => canvas.as_mut().icon_double_clicked(),
            LoopWidgetAction::PlayClicked => canvas.as_mut().play_clicked(),
            LoopWidgetAction::RecordClicked => canvas.as_mut().record_clicked(),
            LoopWidgetAction::StopClicked => canvas.as_mut().stop_clicked(),
            LoopWidgetAction::GainChanged(value) => canvas.as_mut().gain_changed(value),
        });
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

impl EguiUi for LoopWidgetUi {
    fn draw(&mut self, root_ui: &mut egui::Ui, _canvas: CanvasInfo) {
        if !self.icons_initialized {
            shoop_egui::initialize(root_ui.ctx());
            self.icons_initialized = true;
            root_ui.ctx().request_repaint();
            return;
        }

        let state = self.state.read().expect("loop state lock poisoned").clone();
        let response = egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(0.0),
            )
            .show(root_ui, |ui| {
                let size = egui::vec2(ui.available_width(), ui.available_height());
                self.widget.show(ui, &state, size)
            })
            .inner;
        for action in response.actions {
            self.emit_action(action);
        }
    }
}
