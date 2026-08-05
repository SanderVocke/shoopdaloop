use crate::{AppAction, AppState, GlobalControls, TracksWidget};

const LOGO_BYTES: &[u8] = include_bytes!("../../../../resources/logo-small.png");

#[derive(Default)]
pub struct AppWidget {
    tracks: TracksWidget,
    global_controls: GlobalControls,
    logo: Option<egui::TextureHandle>,
}

impl AppWidget {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &AppState) -> Vec<AppAction> {
        self.ensure_logo(ui.ctx());
        let mut actions = Vec::new();

        egui::Panel::top("global_controls")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(egui::Margin::symmetric(6, 4)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("global_controls_scroll")
                    .show(ui, |ui| {
                        actions.extend(
                            self.global_controls
                                .show(ui, &state.global_controls)
                                .into_iter()
                                .map(AppAction::Global),
                        );
                    });
            });

        egui::Panel::right("logo_and_status")
            .resizable(false)
            .exact_size(180.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(42, 42, 42))
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ui, |ui| self.show_logo_and_status(ui, state));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(8.0),
            )
            .show(ui, |ui| {
                let response = self.tracks.show(ui, &state.tracks);
                actions.extend(response.loop_actions.into_iter().map(AppAction::Loop));
                actions.extend(response.track_actions.into_iter().map(AppAction::Track));
            });

        actions
    }

    fn ensure_logo(&mut self, context: &egui::Context) {
        if self.logo.is_some() {
            return;
        }
        let Ok(image) = image::load_from_memory(LOGO_BYTES) else {
            return;
        };
        let rgba = image.into_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        self.logo = Some(context.load_texture("shoopdaloop-logo", color_image, Default::default()));
    }

    fn show_logo_and_status(&self, ui: &mut egui::Ui, state: &AppState) {
        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.logo {
                let size = logo.size_vec2();
                let width = ui.available_width().min(145.0);
                let height = width * size.y / size.x;
                ui.add(egui::Image::new((logo.id(), egui::vec2(width, height))));
            } else {
                ui.heading("ShoopDaLoop");
            }
            if !state.status.version.is_empty() {
                ui.label(format!("ShoopDaLoop v{}", state.status.version));
            }
        });

        ui.add_space(12.0);
        ui.label("DSP");
        ui.add(
            egui::ProgressBar::new((state.status.dsp_load_percent / 100.0).clamp(0.0, 1.0))
                .text(format!("{:.1}%", state.status.dsp_load_percent)),
        );
        ui.label(format!("xruns: {}", state.status.xruns));
        ui.separator();
        ui.label(format!("latency: {} frames", state.status.buffer_size));
        match state.status.latency_ms() {
            Some(latency) => {
                ui.label(format!("{latency:.2} ms"));
            }
            None => {
                ui.label("-- ms");
            }
        }
    }
}
