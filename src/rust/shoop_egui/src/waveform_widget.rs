use crate::{
    colors,
    details_pane::{paint_timeline_regions, MediaWidgetResponse},
    waveform::WaveformPyramid,
    MediaView, WaveformChannelState,
};
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
    pyramid: Option<WaveformPyramid>,
    #[cfg(not(target_arch = "wasm32"))]
    requested_samples: Option<Arc<[f32]>>,
    #[cfg(not(target_arch = "wasm32"))]
    preprocessor: WaveformPreprocessor,
}

impl Default for WaveformWidget {
    fn default() -> Self {
        Self {
            pyramid: None,
            #[cfg(not(target_arch = "wasm32"))]
            requested_samples: None,
            #[cfg(not(target_arch = "wasm32"))]
            preprocessor: WaveformPreprocessor::default(),
        }
    }
}

impl WaveformWidget {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        channel: &WaveformChannelState,
        view: MediaView,
        sync_loop_length: u64,
        editing: bool,
    ) -> MediaWidgetResponse {
        let desired = egui::vec2(ui.available_width(), 72.0);
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
        let pan_frames = if response.dragged() && !editing {
            let mut panned_view = view;
            panned_view.pan(response.drag_delta().x, rect.width());
            panned_view.start_frame - view.start_frame
        } else {
            0.0
        };
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
        paint_timeline_regions(
            &painter,
            rect,
            view,
            channel.start_offset,
            channel.preplay_samples,
            channel.loop_length,
            sync_loop_length,
        );
        let clicked_frame = editing
            .then(|| response.interact_pointer_pos())
            .flatten()
            .filter(|_| response.clicked())
            .map(|position| view.x_to_frame(position.x, rect).round() as i64);
        let result = MediaWidgetResponse {
            pan_frames,
            clicked_frame,
        };
        if channel.samples.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No audio data",
                egui::FontId::proportional(12.0),
                colors::MUTED_FOREGROUND,
            );
            Self::show_label(ui, rect, &channel.label);
            return result;
        }

        let sample_to_x = |sample: i64| view.frame_to_x(sample as f64, rect);
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
            Self::show_label(ui, rect, &channel.label);
            return result;
        };
        let sample_start = view
            .start_frame
            .floor()
            .max(0.0)
            .min(channel.samples.len() as f64) as usize;
        let sample_end = view
            .end_frame
            .ceil()
            .max(0.0)
            .min(channel.samples.len() as f64) as usize;
        let sample_count = sample_end.saturating_sub(sample_start);
        let bins = pyramid.bins(
            sample_start,
            sample_count,
            rect.width().round().max(1.0) as usize,
        );

        let center = rect.center().y;
        let half_height = rect.height() * 0.5 - 2.0;
        let denominator = bins.len().saturating_sub(1).max(1) as f64;
        for (index, bin) in bins.iter().enumerate() {
            let sample = sample_start as f64
                + index as f64 / denominator * sample_count.saturating_sub(1) as f64;
            let x = view.frame_to_x(sample, rect);
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
        Self::show_label(ui, rect, &channel.label);
        result
    }

    fn show_label(ui: &mut egui::Ui, rect: egui::Rect, label: &str) {
        let label_rect = egui::Rect::from_min_max(
            rect.left_top() + egui::vec2(6.0, 3.0),
            rect.right_top() + egui::vec2(-6.0, 22.0),
        );
        ui.place(label_rect, egui::Label::new(label).truncate());
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
    fn overlay_does_not_move_layout_cursor_into_the_lane() {
        let context = egui::Context::default();
        let mut widget = WaveformWidget::default();
        let mut output = context.run_ui(Default::default(), |ui| {
            let top = ui.next_widget_position().y;
            widget.show(
                ui,
                &WaveformChannelState {
                    label: "Direct 1".to_owned(),
                    ..Default::default()
                },
                MediaView {
                    timeline_start: 0.0,
                    timeline_end: 16.0,
                    start_frame: 0.0,
                    end_frame: 16.0,
                },
                4,
                false,
            );
            assert!(ui.next_widget_position().y >= top + 72.0);
        });
        output.textures_delta.clear();
    }

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
