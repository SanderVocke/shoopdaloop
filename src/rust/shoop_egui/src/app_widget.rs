use crate::{AppAction, AppState, DetailsPane, GlobalControls, TracksWidget};

const LOGO_BYTES: &[u8] = include_bytes!("../../../../resources/logo-small.png");

pub struct AppWidget {
    tracks: TracksWidget,
    global_controls: GlobalControls,
    details: DetailsPane,
    details_open: bool,
    logo: Option<egui::TextureHandle>,
}

impl Default for AppWidget {
    fn default() -> Self {
        Self {
            tracks: TracksWidget::default(),
            global_controls: GlobalControls,
            details: DetailsPane::default(),
            details_open: true,
            logo: None,
        }
    }
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

        egui::Panel::bottom("details_toggle")
            .resizable(false)
            .exact_size(24.0)
            .show(ui, |ui| {
                if ui.selectable_label(self.details_open, "details").clicked() {
                    self.details_open = !self.details_open;
                }
            });

        if self.details_open {
            egui::Panel::bottom("details")
                .resizable(true)
                .default_size(200.0)
                .min_size(70.0)
                .max_size(400.0)
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(85, 85, 85))
                        .inner_margin(egui::Margin::same(6)),
                )
                .show(ui, |ui| self.details.show(ui, state.details.as_ref()));
        }

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{LoopDetailsState, TrackState, WaveformChannelState};

    #[test]
    fn complete_application_state_produces_paint_commands() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut widget = AppWidget::default();
        let state = AppState {
            tracks: vec![TrackState {
                name: "Track".to_owned(),
                ..Default::default()
            }],
            details: Some(LoopDetailsState {
                title: "Loop".to_owned(),
                channels: vec![WaveformChannelState {
                    id: "audio".to_owned(),
                    samples: Arc::from([-0.5, 0.25, 0.75, -0.1]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let mut uploaded_logo = false;
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    widget.show(ui, &state);
                },
            );

            assert!(output.shapes.len() > 10);
            uploaded_logo |= !output.textures_delta.set.is_empty();
        }
        assert!(uploaded_logo);
    }
}
