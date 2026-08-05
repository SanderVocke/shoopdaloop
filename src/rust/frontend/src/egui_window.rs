use std::pin::Pin;
use std::sync::{Arc, RwLock};

use cxx_qt_lib::{QString, QVariant};
use cxx_qt_lib_shoop::qvariant_helpers::qvariant_to_qsharedpointer_qvector_qvariant;
use egui_cxx_qt::{
    egui, CanvasHandle, CanvasInfo, CanvasQueueError, CanvasSubclass, CanvasUiFactory, EguiUi,
};
use shoop_egui::{
    AppAction, AppState, AppWidget, ChannelId, DefaultRecordingAction, GlobalControlAction,
    LoopDetailsState, LoopId, LoopState, LoopWidgetAction, TrackId, TrackState, TrackWidgetAction,
    WaveformChannelState,
};

use crate::egui_loop_widget::{apply_loop_state, apply_peak_state};

#[egui_cxx_qt::canvas_bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qvariant.h");
        type QString = cxx_qt_lib::QString;
        type QVariant = cxx_qt_lib::QVariant;
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

        #[qsignal]
        #[cxx_name = "globalStopAll"]
        fn global_stop_all(self: Pin<&mut ShoopEguiWindow>);

        #[qsignal]
        #[cxx_name = "globalDeselectAll"]
        fn global_deselect_all(self: Pin<&mut ShoopEguiWindow>);

        #[qsignal]
        #[cxx_name = "globalClearRecordings"]
        fn global_clear_recordings(self: Pin<&mut ShoopEguiWindow>, include_sync: bool);

        #[qsignal]
        #[cxx_name = "globalClearAll"]
        fn global_clear_all(self: Pin<&mut ShoopEguiWindow>, include_sync: bool);

        #[qsignal]
        #[cxx_name = "defaultRecordingActionChanged"]
        fn default_recording_action_changed(self: Pin<&mut ShoopEguiWindow>, value: i32);

        #[qsignal]
        #[cxx_name = "playAfterRecordChanged"]
        fn play_after_record_changed(self: Pin<&mut ShoopEguiWindow>, value: bool);

        #[qsignal]
        #[cxx_name = "syncChanged"]
        fn sync_changed(self: Pin<&mut ShoopEguiWindow>, value: bool);

        #[qsignal]
        #[cxx_name = "soloChanged"]
        fn solo_changed(self: Pin<&mut ShoopEguiWindow>, value: bool);

        #[qsignal]
        #[cxx_name = "applyNCyclesChanged"]
        fn apply_n_cycles_changed(self: Pin<&mut ShoopEguiWindow>, value: i32);

        #[qinvokable]
        #[cxx_name = "setGlobalState"]
        fn set_global_state(
            self: Pin<&mut ShoopEguiWindow>,
            version: QString,
            dsp_load_percent: f32,
            xruns: i32,
            buffer_size: i32,
            sample_rate: i32,
            default_recording_action: i32,
            play_after_record: bool,
            sync: bool,
            solo: bool,
            apply_n_cycles: i32,
        );

        #[qinvokable]
        #[cxx_name = "setDetailsState"]
        fn set_details_state(
            self: Pin<&mut ShoopEguiWindow>,
            generation: i32,
            has_selection: bool,
            title: QString,
            loading: bool,
            channel_count: i32,
        );

        #[qinvokable]
        #[cxx_name = "setDetailsChannelState"]
        fn set_details_channel_state(
            self: Pin<&mut ShoopEguiWindow>,
            generation: i32,
            channel_index: i32,
            id: QString,
            start_offset: i32,
            loop_length: i32,
            played_sample: i32,
        );

        #[qinvokable]
        #[cxx_name = "setDetailsDataTarget"]
        fn set_details_data_target(
            self: Pin<&mut ShoopEguiWindow>,
            generation: i32,
            channel_index: i32,
        );

        #[qinvokable]
        #[cxx_name = "setDetailsChannelData"]
        fn set_details_channel_data(self: Pin<&mut ShoopEguiWindow>, input_data: QVariant);

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
    state: Arc<RwLock<AppState>>,
    details_data_target: Arc<RwLock<Option<(u64, usize)>>>,
}

impl Default for ShoopEguiWindowRust {
    fn default() -> Self {
        Self {
            state: Arc::new(RwLock::new(AppState::default())),
            details_data_target: Arc::new(RwLock::new(None)),
        }
    }
}

fn legacy_track_id(track_index: usize) -> TrackId {
    TrackId::from_raw(track_index as u64 + 1)
}

fn legacy_loop_id(track_index: usize, loop_index: usize) -> LoopId {
    LoopId::from_raw(((track_index as u64 + 1) << 32) | loop_index as u64 + 1)
}

fn indexed_loop_mut(
    state: &mut AppState,
    track_index: i32,
    loop_index: i32,
) -> Option<&mut LoopState> {
    let track_index = usize::try_from(track_index).ok()?;
    let loop_index = usize::try_from(loop_index).ok()?;
    state.tracks.get_mut(track_index)?.loops.get_mut(loop_index)
}

impl ffi::ShoopEguiWindow {
    #[allow(clippy::too_many_arguments)]
    fn set_global_state(
        mut self: Pin<&mut Self>,
        version: QString,
        dsp_load_percent: f32,
        xruns: i32,
        buffer_size: i32,
        sample_rate: i32,
        default_recording_action: i32,
        play_after_record: bool,
        sync: bool,
        solo: bool,
        apply_n_cycles: i32,
    ) {
        let mut state = self.state.write().expect("egui window state lock poisoned");
        state.status.version = version.to_string();
        state.status.dsp_load_percent = dsp_load_percent.clamp(0.0, 100.0);
        state.status.xruns = u32::try_from(xruns).unwrap_or(0);
        state.status.buffer_size = u32::try_from(buffer_size).unwrap_or(0);
        state.status.sample_rate = u32::try_from(sample_rate).unwrap_or(0);
        state.global_controls.default_recording_action = if default_recording_action == 1 {
            DefaultRecordingAction::Grab
        } else {
            DefaultRecordingAction::Record
        };
        state.global_controls.play_after_record = play_after_record;
        state.global_controls.sync = sync;
        state.global_controls.solo = solo;
        state.global_controls.apply_n_cycles = u32::try_from(apply_n_cycles).unwrap_or(0);
        drop(state);
        self.as_mut().request_repaint();
    }

    fn set_details_state(
        mut self: Pin<&mut Self>,
        generation: i32,
        has_selection: bool,
        title: QString,
        loading: bool,
        channel_count: i32,
    ) {
        let (Ok(generation), Ok(channel_count)) =
            (u64::try_from(generation), usize::try_from(channel_count))
        else {
            return;
        };
        let mut state = self.state.write().expect("egui window state lock poisoned");
        if has_selection {
            let details = state.details.get_or_insert_with(LoopDetailsState::default);
            if details.generation != generation {
                *details = LoopDetailsState {
                    generation,
                    ..Default::default()
                };
            }
            details.title = title.to_string();
            details.loading = loading;
            details
                .channels
                .resize_with(channel_count, WaveformChannelState::default);
        } else {
            state.details = None;
        }
        drop(state);
        self.as_mut().request_repaint();
    }

    fn set_details_channel_state(
        mut self: Pin<&mut Self>,
        generation: i32,
        channel_index: i32,
        id: QString,
        start_offset: i32,
        loop_length: i32,
        played_sample: i32,
    ) {
        let (Ok(generation), Ok(channel_index)) =
            (u64::try_from(generation), usize::try_from(channel_index))
        else {
            return;
        };
        let mut state = self.state.write().expect("egui window state lock poisoned");
        let Some(channel) = state
            .details
            .as_mut()
            .filter(|details| details.generation == generation)
            .and_then(|details| details.channels.get_mut(channel_index))
        else {
            return;
        };
        channel.id = ChannelId::from_raw(channel_index as u64 + 1);
        channel.label = id.to_string();
        channel.start_offset = i64::from(start_offset);
        channel.loop_length = u64::try_from(loop_length).unwrap_or(0);
        channel.played_sample = (played_sample >= 0).then_some(i64::from(played_sample));
        drop(state);
        self.as_mut().request_repaint();
    }

    fn set_details_data_target(self: Pin<&mut Self>, generation: i32, channel_index: i32) {
        let target = u64::try_from(generation)
            .ok()
            .zip(usize::try_from(channel_index).ok());
        *self
            .details_data_target
            .write()
            .expect("egui details target lock poisoned") = target;
    }

    fn set_details_channel_data(mut self: Pin<&mut Self>, input_data: QVariant) {
        let target = *self
            .details_data_target
            .read()
            .expect("egui details target lock poisoned");
        let Some((generation, channel_index)) = target else {
            return;
        };
        let samples = (|| {
            let shared = qvariant_to_qsharedpointer_qvector_qvariant(&input_data).ok()?;
            let vector = unsafe { shared.data().ok()?.as_ref()? };
            Some(
                vector
                    .iter()
                    .map(|value| value.value::<f32>().unwrap_or(0.0))
                    .collect::<Vec<_>>(),
            )
        })();
        let Some(samples) = samples else {
            eprintln!("failed to convert egui waveform channel data");
            return;
        };

        let mut state = self.state.write().expect("egui window state lock poisoned");
        let Some(channel) = state
            .details
            .as_mut()
            .filter(|details| details.generation == generation)
            .and_then(|details| details.channels.get_mut(channel_index))
        else {
            return;
        };
        channel.samples = Arc::from(samples);
        drop(state);
        self.as_mut().request_repaint();
    }

    fn set_track(mut self: Pin<&mut Self>, track_index: i32, name: QString, loop_count: i32) {
        let (Ok(track_index), Ok(loop_count)) =
            (usize::try_from(track_index), usize::try_from(loop_count))
        else {
            return;
        };
        let mut state = self.state.write().expect("egui window state lock poisoned");
        state
            .tracks
            .resize_with(track_index + 1, TrackState::default);
        state.tracks[track_index].id = legacy_track_id(track_index);
        state.tracks[track_index].name = name.to_string();
        state.tracks[track_index]
            .loops
            .resize_with(loop_count, LoopState::default);
        for (loop_index, loop_state) in state.tracks[track_index].loops.iter_mut().enumerate() {
            loop_state.id = legacy_loop_id(track_index, loop_index);
        }
        drop(state);
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
        let mut state = self.state.write().expect("egui window state lock poisoned");
        let Some(track) = state.tracks.get_mut(track_index) else {
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
        drop(state);
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
        let mut app_state = self.state.write().expect("egui window state lock poisoned");
        let Some(loop_state) = indexed_loop_mut(&mut app_state, track_index, loop_index) else {
            return;
        };
        loop_state.id = legacy_loop_id(
            usize::try_from(track_index).unwrap_or_default(),
            usize::try_from(loop_index).unwrap_or_default(),
        );
        apply_loop_state(
            loop_state,
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
        if sync {
            if let Some(track) = usize::try_from(track_index)
                .ok()
                .and_then(|index| app_state.tracks.get_mut(index))
            {
                track.is_sync = true;
            }
        }
        drop(app_state);
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
        let mut app_state = self.state.write().expect("egui window state lock poisoned");
        let Some(loop_state) = indexed_loop_mut(&mut app_state, track_index, loop_index) else {
            return;
        };
        apply_peak_state(
            loop_state,
            stereo,
            peak_left_db,
            peak_right_db,
            midi_activity,
        );
        drop(app_state);
        self.as_mut().request_repaint();
    }
}

impl CanvasSubclass for ffi::ShoopEguiWindow {
    fn ui_factory(self: Pin<&mut Self>, canvas: CanvasHandle<Self>) -> CanvasUiFactory {
        let state = Arc::clone(&self.state);
        CanvasUiFactory::new(move || {
            Box::new(EguiWindowUi {
                state: Arc::clone(&state),
                canvas: canvas.clone(),
                icons_initialized: false,
                widget: AppWidget::default(),
            })
        })
    }
}

struct EguiWindowUi {
    state: Arc<RwLock<AppState>>,
    canvas: CanvasHandle<ffi::ShoopEguiWindow>,
    icons_initialized: bool,
    widget: AppWidget,
}

impl EguiWindowUi {
    fn loop_indices(&self, track_id: TrackId, loop_id: LoopId) -> Option<(i32, i32)> {
        let state = self.state.read().ok()?;
        let track_index = state.tracks.iter().position(|track| track.id == track_id)?;
        let loop_index = state.tracks[track_index]
            .loops
            .iter()
            .position(|loop_state| loop_state.id == loop_id)?;
        Some((
            i32::try_from(track_index).ok()?,
            i32::try_from(loop_index).ok()?,
        ))
    }

    fn track_index(&self, track_id: TrackId) -> Option<i32> {
        let state = self.state.read().ok()?;
        i32::try_from(state.tracks.iter().position(|track| track.id == track_id)?).ok()
    }

    fn emit_action(&self, track_id: TrackId, loop_id: LoopId, action: LoopWidgetAction) {
        let Some((track_index, loop_index)) = self.loop_indices(track_id, loop_id) else {
            return;
        };
        self.queue_signal(move |mut canvas| match action {
            LoopWidgetAction::IconClicked(_) => {
                canvas.as_mut().icon_clicked(track_index, loop_index)
            }
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

    fn emit_track_action(&self, track_id: TrackId, action: TrackWidgetAction) {
        let Some(track_index) = self.track_index(track_id) else {
            return;
        };
        self.queue_signal(move |mut canvas| match action {
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

    fn emit_global_action(&self, action: GlobalControlAction) {
        self.queue_signal(move |mut canvas| match action {
            GlobalControlAction::StopAll => canvas.as_mut().global_stop_all(),
            GlobalControlAction::DeselectAll => canvas.as_mut().global_deselect_all(),
            GlobalControlAction::ClearRecordings { include_sync } => {
                canvas.as_mut().global_clear_recordings(include_sync)
            }
            GlobalControlAction::ClearAll { include_sync } => {
                canvas.as_mut().global_clear_all(include_sync)
            }
            GlobalControlAction::SetDefaultRecordingAction(value) => {
                let value = match value {
                    DefaultRecordingAction::Record => 0,
                    DefaultRecordingAction::Grab => 1,
                };
                canvas.as_mut().default_recording_action_changed(value);
            }
            GlobalControlAction::SetPlayAfterRecord(value) => {
                canvas.as_mut().play_after_record_changed(value)
            }
            GlobalControlAction::SetSync(value) => canvas.as_mut().sync_changed(value),
            GlobalControlAction::SetSolo(value) => canvas.as_mut().solo_changed(value),
            GlobalControlAction::SetApplyNCycles(value) => {
                if let Ok(value) = i32::try_from(value) {
                    canvas.as_mut().apply_n_cycles_changed(value);
                }
            }
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

        let state = self
            .state
            .read()
            .expect("egui window state lock poisoned")
            .clone();
        for action in self.widget.show(root_ui, &state) {
            match action {
                AppAction::Loop {
                    track_id,
                    loop_id,
                    action,
                } => self.emit_action(track_id, loop_id, action),
                AppAction::Track { track_id, action } => self.emit_track_action(track_id, action),
                AppAction::Global(action) => self.emit_global_action(action),
                AppAction::AddTrack(_) | AppAction::AddLoop { .. } => {}
            }
        }
    }
}
