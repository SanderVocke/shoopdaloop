use egui_cxx_qt::{egui, CanvasInfo, EguiUi};

const UI_TYPE: &str = "loop-widget";

pub fn initialize() {
    egui_cxx_qt::initialize();
    egui_cxx_qt::install_ui_factory_for(UI_TYPE, || Box::<LoopWidgetUi>::default())
        .expect("install the egui loop widget UI factory exactly once");
}

struct LoopWidgetUi {
    playing: bool,
    position: f32,
}

impl Default for LoopWidgetUi {
    fn default() -> Self {
        Self {
            playing: true,
            position: 0.38,
        }
    }
}

impl EguiUi for LoopWidgetUi {
    fn draw(&mut self, root_ui: &mut egui::Ui, _canvas: CanvasInfo) {
        if self.playing {
            let elapsed = root_ui.ctx().input(|input| input.stable_dt);
            self.position = (self.position + elapsed / 6.0).fract();
            root_ui.ctx().request_repaint();
        }

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
                    egui::vec2(rect.width() * self.position, rect.height()),
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
                    format!("Loop prototype  ·  {:02}%", (self.position * 100.0) as u32),
                    egui::FontId::proportional(15.0),
                    egui::Color32::WHITE,
                );

                let button_size = egui::vec2(44.0, 30.0);
                let stop_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.right() - 28.0, rect.center().y),
                    button_size,
                );
                let play_rect = stop_rect.translate(egui::vec2(-50.0, 0.0));

                if ui
                    .put(
                        play_rect,
                        egui::Button::new(
                            egui::RichText::new("▶").color(egui::Color32::from_rgb(80, 220, 100)),
                        ),
                    )
                    .clicked()
                {
                    self.playing = true;
                }
                if ui
                    .put(
                        stop_rect,
                        egui::Button::new(egui::RichText::new("■").color(egui::Color32::WHITE)),
                    )
                    .clicked()
                {
                    self.playing = false;
                }
            });
    }
}
