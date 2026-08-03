use std::pin::Pin;
use std::sync::{Arc, RwLock};

use cxx_qt_lib::QString;
use egui_cxx_qt::{
    egui, CanvasHandle, CanvasInfo, CanvasQueueError, CanvasSubclass, CanvasUiFactory, EguiUi,
};

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
        #[cxx_name = "playClicked"]
        fn play_clicked(self: Pin<&mut ShoopEguiLoopWidget>);

        #[qsignal]
        #[cxx_name = "stopClicked"]
        fn stop_clicked(self: Pin<&mut ShoopEguiLoopWidget>);

        #[qinvokable]
        #[cxx_name = "setLoopState"]
        fn set_loop_state(
            self: Pin<&mut ShoopEguiLoopWidget>,
            name: QString,
            position: f32,
            playing: bool,
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
struct LoopState {
    name: String,
    position: f32,
    playing: bool,
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
            name: "Loop".to_owned(),
            position: 0.38,
            playing: false,
        }
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
    fn set_loop_state(mut self: Pin<&mut Self>, name: QString, position: f32, playing: bool) {
        *self.state.write().expect("loop state lock poisoned") = LoopState {
            name: name.to_string(),
            position: position.clamp(0.0, 1.0),
            playing,
        };
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
            })
        })
    }
}

struct LoopWidgetUi {
    state: Arc<RwLock<LoopState>>,
    canvas: CanvasHandle<ffi::ShoopEguiLoopWidget>,
}

impl LoopWidgetUi {
    fn emit_play_clicked(&self) {
        self.queue_signal(|mut canvas| canvas.as_mut().play_clicked());
    }

    fn emit_stop_clicked(&self) {
        self.queue_signal(|mut canvas| canvas.as_mut().stop_clicked());
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
        let LoopState {
            name,
            position,
            playing,
        } = self.state.read().expect("loop state lock poisoned").clone();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(24.0),
            )
            .show(root_ui, |ui| {
                ui.add_space(((ui.available_height() - 44.0) / 2.0).max(0.0));
                let size = egui::vec2(ui.available_width(), 44.0);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let rounding = egui::CornerRadius::same(5);
                let painter = ui.painter();

                painter.rect_filled(rect, rounding, egui::Color32::from_rgb(0, 0, 68));
                let progress_rect = egui::Rect::from_min_size(
                    rect.min,
                    egui::vec2(rect.width() * position, rect.height()),
                );
                painter.rect_filled(progress_rect, rounding, egui::Color32::from_rgb(0, 68, 0));
                painter.rect_stroke(
                    rect,
                    rounding,
                    egui::Stroke::new(2.0, egui::Color32::from_gray(220)),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    egui::pos2(rect.left() + 14.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    format!("{name}  ·  {:02}%", (position * 100.0) as u32),
                    egui::FontId::proportional(15.0),
                    egui::Color32::WHITE,
                );

                let button_size = egui::vec2(44.0, 30.0);
                let stop_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.right() - 28.0, rect.center().y),
                    button_size,
                );
                let play_rect = stop_rect.translate(egui::vec2(-50.0, 0.0));
                let play_color = if playing {
                    egui::Color32::from_rgb(120, 255, 140)
                } else {
                    egui::Color32::from_rgb(80, 220, 100)
                };

                if ui
                    .put(
                        play_rect,
                        egui::Button::new(egui::RichText::new("▶").color(play_color)),
                    )
                    .clicked()
                {
                    self.emit_play_clicked();
                }
                if ui
                    .put(
                        stop_rect,
                        egui::Button::new(egui::RichText::new("■").color(egui::Color32::WHITE)),
                    )
                    .clicked()
                {
                    self.emit_stop_clicked();
                }
            });
    }
}
