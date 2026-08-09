use crate::{colors, waveform_bins, WaveformChannelState};

#[derive(Debug)]
pub struct WaveformWidget {
    zoom: f32,
    offset: usize,
}

impl Default for WaveformWidget {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: 0,
        }
    }
}

impl WaveformWidget {
    pub fn show(&mut self, ui: &mut egui::Ui, channel: &WaveformChannelState) {
        ui.horizontal(|ui| {
            ui.label(&channel.label);
            ui.add(
                egui::Slider::new(&mut self.zoom, 1.0..=64.0)
                    .logarithmic(true)
                    .show_value(false)
                    .text("zoom"),
            )
            .on_hover_text(format!("Waveform zoom: {:.1}×", self.zoom));
        });

        let desired = egui::vec2(ui.available_width(), 72.0);
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, colors::WAVEFORM_BACKGROUND);
        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0, colors::MUTED_FOREGROUND),
            egui::StrokeKind::Inside,
        );
        painter.hline(
            rect.x_range(),
            rect.center().y,
            egui::Stroke::new(1.0, colors::WAVEFORM_ZERO_LINE),
        );

        if channel.samples.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No audio data",
                egui::FontId::proportional(12.0),
                colors::MUTED_FOREGROUND,
            );
            return;
        }

        let visible_samples = ((channel.samples.len() as f32 / self.zoom).round() as usize)
            .clamp(1, channel.samples.len());
        let max_offset = channel.samples.len().saturating_sub(visible_samples);
        if response.dragged() {
            let samples_per_point = visible_samples as f32 / rect.width().max(1.0);
            let delta = (-response.drag_delta().x * samples_per_point).round() as isize;
            self.offset = self.offset.saturating_add_signed(delta).min(max_offset);
        } else {
            self.offset = self.offset.min(max_offset);
        }
        let end = self.offset + visible_samples;
        let samples = &channel.samples[self.offset..end];
        let bins = waveform_bins(samples, rect.width().round().max(1.0) as usize);

        let sample_to_x = |sample: i64| {
            rect.left()
                + (sample as f32 - self.offset as f32) / visible_samples as f32 * rect.width()
        };
        let loop_start = channel.start_offset;
        let loop_end = loop_start.saturating_add_unsigned(channel.loop_length);
        let left = sample_to_x(loop_start).clamp(rect.left(), rect.right());
        let right = sample_to_x(loop_end).clamp(rect.left(), rect.right());
        if right > left {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(left, rect.top()),
                    egui::pos2(right, rect.bottom()),
                ),
                0.0,
                colors::WAVEFORM_LOOP_REGION,
            );
        }

        let center = rect.center().y;
        let half_height = rect.height() * 0.5 - 2.0;
        let denominator = bins.len().saturating_sub(1).max(1) as f32;
        for (index, bin) in bins.iter().enumerate() {
            let x = rect.left() + index as f32 / denominator * rect.width();
            let top = center - bin.max.clamp(-1.0, 1.0) * half_height;
            let bottom = center - bin.min.clamp(-1.0, 1.0) * half_height;
            painter.vline(
                x,
                top.min(bottom)..=top.max(bottom),
                egui::Stroke::new(1.0, colors::AUDIO_ACTIVITY),
            );
        }

        if let Some(played_sample) = channel.played_sample {
            let x = sample_to_x(played_sample);
            if rect.x_range().contains(x) {
                painter.vline(
                    x,
                    rect.y_range(),
                    egui::Stroke::new(2.0, colors::WAVEFORM_PLAYHEAD),
                );
            }
        }
    }
}
