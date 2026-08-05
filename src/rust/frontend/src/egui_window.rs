use std::pin::Pin;
use std::sync::{Arc, RwLock};

use cxx_qt_lib::QString;
use egui_cxx_qt::{
    egui, CanvasHandle, CanvasInfo, CanvasQueueError, CanvasSubclass, CanvasUiFactory, EguiUi,
};
use shoop_egui::{
    IndexedLoopAction, IndexedTrackAction, LoopState, LoopWidgetAction, TrackState,
    TrackWidgetAction, TracksWidget,
};

use crate::egui_loop_widget::{apply_loop_state, apply_peak_state};

#[egui_cxx_qt::canvas_bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        type ShoopEguiWindow = super::ShoopEguiWindowRust;

        #[qsignal]
        #[cxx_name = "iconClicked"]
        fn icon_clicked(self: Pin<&mut ShoopEguiWindow>, track_index: i32, loop_index: i32);

        #[qsignal]
        #[cxx_name = "iconDoubleClicked"]
        fn icon_double_clicked(self: Pin<&mut ShoopEguiWindow>, track_index: i32, loop_index: i32);

        #[qsignal]
        #[cxx_name = "playClicked"]
        fn play_clicked(self: Pin<&mut ShoopEguiWindow>, track_index: i32, loop_index: i32);

        #[qsignal]
        #[cxx_name = "recordClicked"]
        fn record_clicked(self: Pin<&mut ShoopEguiWindow>, track_index: i32, loop_index: i32);

        #[qsignal]
        #[cxx_name = "stopClicked"]
        fn stop_clicked(self: Pin<&mut ShoopEguiWindow>, track_index: i32, loop_index: i32);

        #[qsignal]
        #[cxx_name = "gainChanged"]
        fn gain_changed(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            loop_index: i32,
            value: f32,
        );

        #[qsignal]
        #[cxx_name = "trackNameChanged"]
        fn track_name_changed(self: Pin<&mut ShoopEguiWindow>, track_index: i32, name: QString);

        #[qsignal]
        #[cxx_name = "trackOutputGainChanged"]
        fn track_output_gain_changed(self: Pin<&mut ShoopEguiWindow>, track_index: i32, value: f32);

        #[qsignal]
        #[cxx_name = "trackOutputBalanceChanged"]
        fn track_output_balance_changed(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            value: f32,
        );

        #[qsignal]
        #[cxx_name = "trackOutputMuteChanged"]
        fn track_output_mute_changed(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            value: bool,
        );

        #[qsignal]
        #[cxx_name = "trackInputGainChanged"]
        fn track_input_gain_changed(self: Pin<&mut ShoopEguiWindow>, track_index: i32, value: f32);

        #[qsignal]
        #[cxx_name = "trackInputBalanceChanged"]
        fn track_input_balance_changed(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            value: f32,
        );

        #[qsignal]
        #[cxx_name = "trackInputMonitoringChanged"]
        fn track_input_monitoring_changed(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            value: bool,
        );

        #[qinvokable]
        #[cxx_name = "setTrack"]
        fn set_track(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            name: QString,
            loop_count: i32,
        );

        #[qinvokable]
        #[cxx_name = "setTrackControlState"]
        fn set_track_control_state(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            has_output: bool,
            has_output_audio: bool,
            output_stereo: bool,
            output_gain_db: f32,
            output_balance: f32,
            output_muted: bool,
            output_peak_left_db: f32,
            output_peak_right_db: f32,
            output_midi_activity: bool,
            has_input: bool,
            has_input_audio: bool,
            input_stereo: bool,
            input_gain_db: f32,
            input_balance: f32,
            input_monitoring: bool,
            input_peak_left_db: f32,
            input_peak_right_db: f32,
            input_midi_activity: bool,
        );

        #[qinvokable]
        #[cxx_name = "setLoopState"]
        fn set_loop_state(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            loop_index: i32,
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
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            loop_index: i32,
            stereo: bool,
            peak_left_db: f32,
            peak_right_db: f32,
            midi_activity: bool,
        );
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/register_qml_type.h");
        #[rust_name = "register_qml_type_shoop_egui_window"]
        unsafe fn register_qml_type(
            inference_example: *mut ShoopEguiWindow,
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
        ffi::register_qml_type_shoop_egui_window(
            std::ptr::null_mut(),
            &mut module_name,
            1,
            0,
            &mut type_name,
        );
    }
}

pub struct ShoopEguiWindowRust {
    tracks: Arc<RwLock<Vec<TrackState>>>,
}

impl Default for ShoopEguiWindowRust {
    fn default() -> Self {
        Self {
            tracks: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

fn indexed_loop_mut(
    tracks: &mut [TrackState],
    track_index: i32,
    loop_index: i32,
) -> Option<&mut LoopState> {
    let track_index = usize::try_from(track_index).ok()?;
    let loop_index = usize::try_from(loop_index).ok()?;
    tracks.get_mut(track_index)?.loops.get_mut(loop_index)
}

impl ffi::ShoopEguiWindow {
    fn set_track(mut self: Pin<&mut Self>, track_index: i32, name: QString, loop_count: i32) {
        let (Ok(track_index), Ok(loop_count)) =
            (usize::try_from(track_index), usize::try_from(loop_count))
        else {
            return;
        };
        let mut tracks = self
            .tracks
            .write()
            .expect("egui window state lock poisoned");
        tracks.resize_with(track_index + 1, TrackState::default);
        tracks[track_index].name = name.to_string();
        tracks[track_index]
            .loops
            .resize_with(loop_count, LoopState::default);
        drop(tracks);
        self.as_mut().request_repaint();
    }

    #[allow(clippy::too_many_arguments)]
    fn set_track_control_state(
        mut self: Pin<&mut Self>,
        track_index: i32,
        has_output: bool,
        has_output_audio: bool,
        output_stereo: bool,
        output_gain_db: f32,
        output_balance: f32,
        output_muted: bool,
        output_peak_left_db: f32,
        output_peak_right_db: f32,
        output_midi_activity: bool,
        has_input: bool,
        has_input_audio: bool,
        input_stereo: bool,
        input_gain_db: f32,
        input_balance: f32,
        input_monitoring: bool,
        input_peak_left_db: f32,
        input_peak_right_db: f32,
        input_midi_activity: bool,
    ) {
        let Ok(track_index) = usize::try_from(track_index) else {
            return;
        };
        let mut tracks = self
            .tracks
            .write()
            .expect("egui window state lock poisoned");
        let Some(track) = tracks.get_mut(track_index) else {
            return;
        };
        track.controls.has_output = has_output;
        track.controls.has_output_audio = has_output_audio;
        track.controls.output_stereo = output_stereo;
        track.controls.output_gain_db = output_gain_db;
        track.controls.output_balance = output_balance;
        track.controls.output_muted = output_muted;
        track.controls.output_peak_left_db = output_peak_left_db;
        track.controls.output_peak_right_db = output_peak_right_db;
        track.controls.output_midi_activity = output_midi_activity;
        track.controls.has_input = has_input;
        track.controls.has_input_audio = has_input_audio;
        track.controls.input_stereo = input_stereo;
        track.controls.input_gain_db = input_gain_db;
        track.controls.input_balance = input_balance;
        track.controls.input_monitoring = input_monitoring;
        track.controls.input_peak_left_db = input_peak_left_db;
        track.controls.input_peak_right_db = input_peak_right_db;
        track.controls.input_midi_activity = input_midi_activity;
        track.controls.clamp();
        drop(tracks);
        self.as_mut().request_repaint();
    }

    #[allow(clippy::too_many_arguments)]
    fn set_loop_state(
        mut self: Pin<&mut Self>,
        track_index: i32,
        loop_index: i32,
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
        let mut tracks = self
            .tracks
            .write()
            .expect("egui window state lock poisoned");
        let Some(state) = indexed_loop_mut(&mut tracks, track_index, loop_index) else {
            return;
        };
        apply_loop_state(
            state,
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
        drop(tracks);
        self.as_mut().request_repaint();
    }

    fn set_peak_state(
        mut self: Pin<&mut Self>,
        track_index: i32,
        loop_index: i32,
        stereo: bool,
        peak_left_db: f32,
        peak_right_db: f32,
        midi_activity: bool,
    ) {
        let mut tracks = self
            .tracks
            .write()
            .expect("egui window state lock poisoned");
        let Some(state) = indexed_loop_mut(&mut tracks, track_index, loop_index) else {
            return;
        };
        apply_peak_state(state, stereo, peak_left_db, peak_right_db, midi_activity);
        drop(tracks);
        self.as_mut().request_repaint();
    }
}

impl CanvasSubclass for ffi::ShoopEguiWindow {
    fn ui_factory(self: Pin<&mut Self>, canvas: CanvasHandle<Self>) -> CanvasUiFactory {
        let tracks = Arc::clone(&self.tracks);
        CanvasUiFactory::new(move || {
            Box::new(EguiWindowUi {
                tracks: Arc::clone(&tracks),
                canvas: canvas.clone(),
                icons_initialized: false,
                widget: TracksWidget::default(),
            })
        })
    }
}

struct EguiWindowUi {
    tracks: Arc<RwLock<Vec<TrackState>>>,
    canvas: CanvasHandle<ffi::ShoopEguiWindow>,
    icons_initialized: bool,
    widget: TracksWidget,
}

impl EguiWindowUi {
    fn emit_action(&self, indexed: IndexedLoopAction) {
        let Ok(track_index) = i32::try_from(indexed.track_index) else {
            return;
        };
        let Ok(loop_index) = i32::try_from(indexed.loop_index) else {
            return;
        };
        self.queue_signal(move |mut canvas| match indexed.action {
            LoopWidgetAction::IconClicked => canvas.as_mut().icon_clicked(track_index, loop_index),
            LoopWidgetAction::IconDoubleClicked => {
                canvas.as_mut().icon_double_clicked(track_index, loop_index)
            }
            LoopWidgetAction::PlayClicked => canvas.as_mut().play_clicked(track_index, loop_index),
            LoopWidgetAction::RecordClicked => {
                canvas.as_mut().record_clicked(track_index, loop_index)
            }
            LoopWidgetAction::StopClicked => canvas.as_mut().stop_clicked(track_index, loop_index),
            LoopWidgetAction::GainChanged(value) => {
                canvas.as_mut().gain_changed(track_index, loop_index, value)
            }
        });
    }

    fn emit_track_action(&self, indexed: IndexedTrackAction) {
        let Ok(track_index) = i32::try_from(indexed.track_index) else {
            return;
        };
        self.queue_signal(move |mut canvas| match indexed.action {
            TrackWidgetAction::NameChanged(name) => canvas
                .as_mut()
                .track_name_changed(track_index, QString::from(&name)),
            TrackWidgetAction::OutputGainChanged(value) => canvas
                .as_mut()
                .track_output_gain_changed(track_index, value),
            TrackWidgetAction::OutputBalanceChanged(value) => canvas
                .as_mut()
                .track_output_balance_changed(track_index, value),
            TrackWidgetAction::OutputMuteChanged(value) => canvas
                .as_mut()
                .track_output_mute_changed(track_index, value),
            TrackWidgetAction::InputGainChanged(value) => {
                canvas.as_mut().track_input_gain_changed(track_index, value)
            }
            TrackWidgetAction::InputBalanceChanged(value) => canvas
                .as_mut()
                .track_input_balance_changed(track_index, value),
            TrackWidgetAction::InputMonitoringChanged(value) => canvas
                .as_mut()
                .track_input_monitoring_changed(track_index, value),
        });
    }

    fn queue_signal(&self, signal: impl FnOnce(Pin<&mut ffi::ShoopEguiWindow>) + Send + 'static) {
        match self.canvas.queue(signal) {
            Ok(()) | Err(CanvasQueueError::ObjectDestroyed) => {}
            Err(error) => eprintln!("failed to emit egui window signal: {error}"),
        }
    }
}

impl EguiUi for EguiWindowUi {
    fn draw(&mut self, root_ui: &mut egui::Ui, _canvas: CanvasInfo) {
        if !self.icons_initialized {
            shoop_egui::initialize(root_ui.ctx());
            self.icons_initialized = true;
            root_ui.ctx().request_repaint();
            return;
        }

        let tracks = self
            .tracks
            .read()
            .expect("egui window state lock poisoned")
            .clone();
        let response = egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(8.0),
            )
            .show(root_ui, |ui| self.widget.show(ui, &tracks))
            .inner;
        for action in response.loop_actions {
            self.emit_action(action);
        }
        for action in response.track_actions {
            self.emit_track_action(action);
        }
    }
}
