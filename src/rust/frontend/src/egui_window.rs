use std::pin::Pin;
use std::sync::{Arc, RwLock};

use cxx_qt_lib::QString;
use egui_cxx_qt::{
    egui, CanvasHandle, CanvasInfo, CanvasQueueError, CanvasSubclass, CanvasUiFactory, EguiUi,
};

use crate::egui_loop_widget::{draw_loop_widget, LoopState, LoopWidgetActionSink};

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

        #[qinvokable]
        #[cxx_name = "setTrack"]
        fn set_track(
            self: Pin<&mut ShoopEguiWindow>,
            track_index: i32,
            name: QString,
            loop_count: i32,
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

#[derive(Clone, Default)]
struct TrackState {
    name: String,
    loops: Vec<LoopState>,
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
        state.update_peak_state(stereo, peak_left_db, peak_right_db, midi_activity);
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
                gain_drag_starts: Vec::new(),
            })
        })
    }
}

struct EguiWindowUi {
    tracks: Arc<RwLock<Vec<TrackState>>>,
    canvas: CanvasHandle<ffi::ShoopEguiWindow>,
    icons_initialized: bool,
    gain_drag_starts: Vec<Vec<Option<f32>>>,
}

struct WindowLoopActionSink<'a> {
    canvas: CanvasHandle<ffi::ShoopEguiWindow>,
    track_index: i32,
    loop_index: i32,
    gain_drag_start: &'a mut Option<f32>,
}

impl WindowLoopActionSink<'_> {
    fn queue_signal(&self, signal: impl FnOnce(Pin<&mut ffi::ShoopEguiWindow>) + Send + 'static) {
        match self.canvas.queue(signal) {
            Ok(()) | Err(CanvasQueueError::ObjectDestroyed) => {}
            Err(error) => eprintln!("failed to emit egui window signal: {error}"),
        }
    }
}

impl LoopWidgetActionSink for WindowLoopActionSink<'_> {
    fn emit_icon_clicked(&mut self) {
        let (track_index, loop_index) = (self.track_index, self.loop_index);
        self.queue_signal(move |mut canvas| canvas.as_mut().icon_clicked(track_index, loop_index));
    }

    fn emit_icon_double_clicked(&mut self) {
        let (track_index, loop_index) = (self.track_index, self.loop_index);
        self.queue_signal(move |mut canvas| {
            canvas.as_mut().icon_double_clicked(track_index, loop_index)
        });
    }

    fn emit_play_clicked(&mut self) {
        let (track_index, loop_index) = (self.track_index, self.loop_index);
        self.queue_signal(move |mut canvas| canvas.as_mut().play_clicked(track_index, loop_index));
    }

    fn emit_record_clicked(&mut self) {
        let (track_index, loop_index) = (self.track_index, self.loop_index);
        self.queue_signal(move |mut canvas| {
            canvas.as_mut().record_clicked(track_index, loop_index)
        });
    }

    fn emit_stop_clicked(&mut self) {
        let (track_index, loop_index) = (self.track_index, self.loop_index);
        self.queue_signal(move |mut canvas| canvas.as_mut().stop_clicked(track_index, loop_index));
    }

    fn emit_gain_changed(&mut self, value: f32) {
        let (track_index, loop_index) = (self.track_index, self.loop_index);
        self.queue_signal(move |mut canvas| {
            canvas.as_mut().gain_changed(track_index, loop_index, value)
        });
    }

    fn gain_drag_start(&mut self) -> &mut Option<f32> {
        self.gain_drag_start
    }
}

impl EguiUi for EguiWindowUi {
    fn draw(&mut self, root_ui: &mut egui::Ui, _canvas: CanvasInfo) {
        if !self.icons_initialized {
            egui_material_icons::initialize(root_ui.ctx());
            self.icons_initialized = true;
        }

        let tracks = self
            .tracks
            .read()
            .expect("egui window state lock poisoned")
            .clone();
        self.gain_drag_starts.resize_with(tracks.len(), Vec::new);
        for (track, drag_starts) in tracks.iter().zip(&mut self.gain_drag_starts) {
            drag_starts.resize(track.loops.len(), None);
        }

        let canvas = self.canvas.clone();
        let gain_drag_starts = &mut self.gain_drag_starts;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(8.0),
            )
            .show(root_ui, |ui| {
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        for (track_index, track) in tracks.iter().enumerate() {
                            ui.push_id(track_index, |ui| {
                                ui.group(|ui| {
                                    ui.set_width(180.0);
                                    ui.label(egui::RichText::new(&track.name).strong());
                                    for (loop_index, state) in track.loops.iter().enumerate() {
                                        ui.push_id(loop_index, |ui| {
                                            let mut sink = WindowLoopActionSink {
                                                canvas: canvas.clone(),
                                                track_index: track_index as i32,
                                                loop_index: loop_index as i32,
                                                gain_drag_start: &mut gain_drag_starts[track_index]
                                                    [loop_index],
                                            };
                                            draw_loop_widget(
                                                &mut sink,
                                                ui,
                                                state,
                                                egui::vec2(ui.available_width(), 26.0),
                                            );
                                        });
                                    }
                                });
                            });
                        }
                    });
                });
            });
    }
}
