use crate::{colors, waveform::WaveformPyramid, WaveformChannelState};
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

#[cfg(not(target_arch = "wasm32"))]
struct WaveformPreparationRequest {
    samples: Arc<[f32]>,
    context: egui::Context,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct WaveformPreprocessor {
    request_sender: Option<SyncSender<WaveformPreparationRequest>>,
    result_receiver: Option<Receiver<WaveformPyramid>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for WaveformPreprocessor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WaveformPreprocessor")
            .field("started", &self.request_sender.is_some())
            .finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WaveformPreprocessor {
    fn request(&mut self, samples: Arc<[f32]>, context: egui::Context) -> bool {
        self.ensure_started();
        let request = WaveformPreparationRequest { samples, context };
        match self
            .request_sender
            .as_ref()
            .expect("waveform preprocessor was started")
            .try_send(request)
        {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => false,
            Err(TrySendError::Disconnected(_)) => {
                self.request_sender = None;
                self.result_receiver = None;
                false
            }
        }
    }

    fn try_receive(&self) -> Option<WaveformPyramid> {
        match self.result_receiver.as_ref()?.try_recv() {
            Ok(pyramid) => Some(pyramid),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }

    fn ensure_started(&mut self) {
        if self.request_sender.is_some() {
            return;
        }
        let (request_sender, request_receiver) =
            mpsc::sync_channel::<WaveformPreparationRequest>(1);
        let (result_sender, result_receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("egui-waveform".to_owned())
            .spawn(move || {
                while let Ok(request) = request_receiver.recv() {
                    let pyramid = WaveformPyramid::new(request.samples);
                    if result_sender.send(pyramid).is_err() {
                        break;
                    }
                    request.context.request_repaint();
                }
            })
            .expect("spawn egui waveform preprocessor");
        self.request_sender = Some(request_sender);
        self.result_receiver = Some(result_receiver);
    }
}

#[derive(Debug)]
pub struct WaveformWidget {
    zoom: f32,
    offset: usize,
    pyramid: Option<WaveformPyramid>,
    #[cfg(not(target_arch = "wasm32"))]
    requested_samples: Option<Arc<[f32]>>,
    #[cfg(not(target_arch = "wasm32"))]
    preprocessor: WaveformPreprocessor,
}

impl Default for WaveformWidget {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: 0,
            pyramid: None,
            #[cfg(not(target_arch = "wasm32"))]
            requested_samples: None,
            #[cfg(not(target_arch = "wasm32"))]
            preprocessor: WaveformPreprocessor::default(),
        }
    }
}

impl WaveformWidget {
    pub fn show(&mut self, ui: &mut egui::Ui, channel: &WaveformChannelState) {
        let desired = egui::vec2(ui.available_width(), 72.0);
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, colors::WAVEFORM_BACKGROUND);
        painter.rect_stroke(
            rect,
            0.0,
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
            Self::show_overlay(ui, rect, &channel.label, &mut self.zoom);
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

        let offset = self.offset;
        let sample_to_x = |sample: i64| {
            rect.left() + (sample as f32 - offset as f32) / visible_samples as f32 * rect.width()
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

        self.update_pyramid(ui.ctx(), &channel.samples);
        let Some(pyramid) = self
            .pyramid
            .as_ref()
            .filter(|pyramid| pyramid.matches(&channel.samples))
        else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Preparing waveform…",
                egui::FontId::proportional(12.0),
                colors::MUTED_FOREGROUND,
            );
            Self::show_overlay(ui, rect, &channel.label, &mut self.zoom);
            return;
        };
        let bins = pyramid.bins(
            self.offset,
            visible_samples,
            rect.width().round().max(1.0) as usize,
        );

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
                egui::Stroke::new(1.0, colors::WAVEFORM_LINE),
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
        Self::show_overlay(ui, rect, &channel.label, &mut self.zoom);
    }

    fn show_overlay(ui: &mut egui::Ui, rect: egui::Rect, label: &str, zoom: &mut f32) {
        ui.painter().text(
            rect.left_top() + egui::vec2(6.0, 5.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
        let zoom_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 116.0, rect.top() + 3.0),
            egui::vec2(110.0, 18.0),
        );
        ui.put(
            zoom_rect,
            egui::Slider::new(zoom, 1.0..=64.0)
                .logarithmic(true)
                .show_value(false)
                .text("zoom"),
        )
        .on_hover_text(format!("Waveform zoom: {:.1}×", *zoom));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn update_pyramid(&mut self, context: &egui::Context, samples: &Arc<[f32]>) {
        while let Some(pyramid) = self.preprocessor.try_receive() {
            if self
                .requested_samples
                .as_ref()
                .is_some_and(|requested| pyramid.matches(requested))
            {
                self.requested_samples = None;
            }
            if pyramid.matches(samples) {
                self.pyramid = Some(pyramid);
            }
        }
        if self
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| pyramid.matches(samples))
            || self
                .requested_samples
                .as_ref()
                .is_some_and(|requested| Arc::ptr_eq(requested, samples))
        {
            return;
        }
        if self
            .preprocessor
            .request(Arc::clone(samples), context.clone())
        {
            self.requested_samples = Some(Arc::clone(samples));
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn update_pyramid(&mut self, _context: &egui::Context, samples: &Arc<[f32]>) {
        if !self
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| pyramid.matches(samples))
        {
            self.pyramid = Some(WaveformPyramid::new(Arc::clone(samples)));
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[shoop_wasm_test_support::shoop_test]
    fn asynchronously_prepares_and_caches_samples() {
        let context = egui::Context::default();
        let samples: Arc<[f32]> = Arc::from(
            (0..100_000)
                .map(|index| (index as f32 / 100.0).sin())
                .collect::<Vec<_>>(),
        );
        let mut widget = WaveformWidget::default();

        widget.update_pyramid(&context, &samples);
        assert!(widget.pyramid.is_none());
        assert!(widget
            .requested_samples
            .as_ref()
            .is_some_and(|requested| Arc::ptr_eq(requested, &samples)));

        let deadline = Instant::now() + Duration::from_secs(2);
        while widget.pyramid.is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
            widget.update_pyramid(&context, &samples);
        }

        assert!(widget
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| pyramid.matches(&samples)));
        assert!(widget.requested_samples.is_none());

        widget.update_pyramid(&context, &samples);
        assert!(widget.requested_samples.is_none());
    }
}
