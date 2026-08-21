use crate::{
    colors, AppIntent, CompositeLoopWidget, LoopDetailsState, LoopId, MidiSequenceWidget,
    TimelineEditTool, WaveformWidget,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaView {
    pub(crate) timeline_start: f64,
    pub(crate) timeline_end: f64,
    pub(crate) start_frame: f64,
    pub(crate) end_frame: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MediaWidgetResponse {
    pub pan_frames: f64,
    pub clicked_frame: Option<i64>,
}

pub(crate) fn paint_timeline_regions(
    painter: &egui::Painter,
    rect: egui::Rect,
    view: MediaView,
    loop_start: i64,
    preplay_samples: u64,
    loop_length: u64,
    sync_loop_length: u64,
) {
    let frame_to_x = |frame: i64| view.frame_to_x(frame as f64, rect);
    let loop_end = loop_start.saturating_add_unsigned(loop_length);
    let paint_range = |start: i64, end: i64, color| {
        let left = frame_to_x(start).clamp(rect.left(), rect.right());
        let right = frame_to_x(end).clamp(rect.left(), rect.right());
        if right > left {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(left, rect.top()),
                    egui::pos2(right, rect.bottom()),
                ),
                0.0,
                color,
            );
        }
    };
    paint_range(
        loop_start.saturating_sub_unsigned(preplay_samples),
        loop_start,
        colors::WAVEFORM_PREPLAY_REGION,
    );
    paint_range(loop_start, loop_end, colors::WAVEFORM_LOOP_REGION);

    if sync_loop_length == 0 {
        return;
    }
    let visible_start = view.start_frame.floor() as i64;
    let delta = visible_start.saturating_sub(loop_start).max(0) as u64;
    let mut marker = loop_start.saturating_add_unsigned(
        delta
            .div_ceil(sync_loop_length)
            .saturating_mul(sync_loop_length),
    );
    while marker <= loop_end {
        let x = frame_to_x(marker);
        if rect.x_range().contains(x) {
            painter.vline(
                x,
                rect.y_range(),
                egui::Stroke::new(1.0, colors::WAVEFORM_SYNC_MARKER),
            );
        }
        let next = marker.saturating_add_unsigned(sync_loop_length);
        if next == marker {
            break;
        }
        marker = next;
    }
}

impl MediaView {
    pub(crate) fn visible_frames(self) -> f64 {
        (self.end_frame - self.start_frame).max(1.0)
    }

    pub(crate) fn frame_to_x(self, frame: f64, rect: egui::Rect) -> f32 {
        rect.left() + ((frame - self.start_frame) / self.visible_frames()) as f32 * rect.width()
    }

    pub(crate) fn x_to_frame(self, x: f32, rect: egui::Rect) -> f64 {
        self.start_frame
            + f64::from(x - rect.left()) / f64::from(rect.width().max(1.0)) * self.visible_frames()
    }

    pub(crate) fn pan(&mut self, drag_delta_x: f32, width: f32) {
        let visible_frames = self.visible_frames();
        let max_start = (self.timeline_end - visible_frames).max(self.timeline_start);
        self.start_frame = (self.start_frame
            - f64::from(drag_delta_x) * visible_frames / f64::from(width.max(1.0)))
        .clamp(self.timeline_start, max_start);
        self.end_frame = self.start_frame + visible_frames;
    }
}

#[derive(Debug)]
struct MediaViewState {
    zoom: f32,
    offset: f64,
}

impl Default for MediaViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: 0.0,
        }
    }
}

impl MediaViewState {
    fn view(&mut self, (timeline_start, timeline_end): (f64, f64)) -> MediaView {
        let total_frames = (timeline_end - timeline_start).max(1.0);
        let visible_frames = (total_frames / f64::from(self.zoom)).max(1.0);
        let max_offset = (timeline_end - visible_frames).max(timeline_start);
        self.offset = self.offset.clamp(timeline_start, max_offset);
        MediaView {
            timeline_start,
            timeline_end,
            start_frame: self.offset,
            end_frame: self.offset + visible_frames,
        }
    }
}

fn media_bounds(details: &LoopDetailsState) -> (f64, f64) {
    let mut start = 0.0_f64;
    let mut end = 1.0_f64;
    for channel in &details.channels {
        start = start.min(channel.start_offset as f64);
        start = start.min(
            channel
                .start_offset
                .saturating_sub_unsigned(channel.preplay_samples) as f64,
        );
        end = end.max(channel.samples.len() as f64).max(
            channel
                .start_offset
                .saturating_add_unsigned(channel.loop_length) as f64,
        );
    }
    for channel in &details.midi_channels {
        start = start.min(channel.start_offset as f64);
        start = start.min(
            channel
                .start_offset
                .saturating_sub_unsigned(channel.preplay_samples) as f64,
        );
        end = end.max(
            channel
                .start_offset
                .saturating_add_unsigned(channel.loop_length) as f64,
        );
        if let Some(event_end) = channel.events.iter().map(|event| event.frame).max() {
            end = end.max(f64::from(event_end));
        }
    }
    (start, end.max(start + 1.0))
}

fn timeline_edit_intent(
    details: &LoopDetailsState,
    tool: TimelineEditTool,
    frame: i64,
) -> AppIntent {
    let current_start = details
        .channels
        .first()
        .map(|channel| channel.start_offset)
        .or_else(|| {
            details
                .midi_channels
                .first()
                .map(|channel| channel.start_offset)
        })
        .unwrap_or(0);
    let (start_offset, preplay_samples, loop_length) = match tool {
        TimelineEditTool::LoopStart => (Some(frame), None, None),
        TimelineEditTool::PreplayStart => (
            None,
            Some(u64::try_from(current_start.saturating_sub(frame)).unwrap_or(0)),
            None,
        ),
        TimelineEditTool::LoopEnd => (
            None,
            None,
            Some(u64::try_from(frame.saturating_sub(current_start)).unwrap_or(0)),
        ),
    };
    AppIntent::SetLoopTimeline {
        loop_id: details.loop_id,
        start_offset,
        preplay_samples,
        loop_length,
    }
}

#[derive(Debug, Default)]
pub struct DetailsPane {
    loop_id: LoopId,
    media_view: MediaViewState,
    waveforms: Vec<WaveformWidget>,
    midi_sequences: Vec<MidiSequenceWidget>,
    composite: CompositeLoopWidget,
    edit_tool: Option<TimelineEditTool>,
}

impl DetailsPane {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        details: Option<&LoopDetailsState>,
    ) -> Vec<AppIntent> {
        let Some(details) = details else {
            ui.label("Make a selection to show additional details here.");
            return Vec::new();
        };

        if self.loop_id != details.loop_id {
            self.loop_id = details.loop_id;
            self.media_view = MediaViewState::default();
            self.waveforms.clear();
            self.midi_sequences.clear();
        }

        ui.heading(&details.title);
        if let Some(composite) = &details.composite {
            return self.composite.show(ui, details.loop_id, composite);
        }
        if details.loading {
            ui.label("Audio waveform data is loading.");
        }
        if details.midi_loading {
            ui.label("MIDI data is loading.");
        }
        if !details.loading
            && !details.midi_loading
            && details.channels.is_empty()
            && details.midi_channels.is_empty()
        {
            ui.label("The selected loop has no audio or MIDI data.");
            return Vec::new();
        }

        ui.add(
            egui::Slider::new(&mut self.media_view.zoom, 1.0..=64.0)
                .logarithmic(true)
                .show_value(false)
                .text("zoom"),
        )
        .on_hover_text(format!("Media zoom: {:.1}×", self.media_view.zoom));

        ui.horizontal(|ui| {
            ui.label("Click tool:");
            for (tool, label) in [
                (TimelineEditTool::LoopStart, "Loop start"),
                (TimelineEditTool::PreplayStart, "Preplay start"),
                (TimelineEditTool::LoopEnd, "Loop end"),
            ] {
                if ui
                    .selectable_label(self.edit_tool == Some(tool), label)
                    .clicked()
                {
                    self.edit_tool = (self.edit_tool != Some(tool)).then_some(tool);
                }
            }
        });

        let media_view = self.media_view.view(media_bounds(details));
        let mut pan_frames = 0.0;
        let mut clicked_frame = None;
        self.waveforms
            .resize_with(details.channels.len(), WaveformWidget::default);
        self.midi_sequences
            .resize_with(details.midi_channels.len(), MidiSequenceWidget::default);
        egui::ScrollArea::vertical()
            .id_salt("details_media")
            .scroll_source(crate::control_safe_scroll_source())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.vertical(|ui| {
                    for (channel, waveform) in details.channels.iter().zip(&mut self.waveforms) {
                        let response = waveform.show(
                            ui,
                            channel,
                            media_view,
                            details.sync_loop_length,
                            self.edit_tool.is_some(),
                        );
                        pan_frames += response.pan_frames;
                        clicked_frame = clicked_frame.or(response.clicked_frame);
                    }
                    for (channel, sequence) in
                        details.midi_channels.iter().zip(&mut self.midi_sequences)
                    {
                        let response = sequence.show(
                            ui,
                            channel,
                            media_view,
                            details.sync_loop_length,
                            self.edit_tool.is_some(),
                        );
                        pan_frames += response.pan_frames;
                        clicked_frame = clicked_frame.or(response.clicked_frame);
                    }
                });
            });
        self.media_view.offset = media_view.start_frame + pan_frames;
        let Some((tool, frame)) = self.edit_tool.zip(clicked_frame) else {
            return Vec::new();
        };
        vec![timeline_edit_intent(details, tool, frame)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CompositeDetailsState, CompositeEventDetailsState, CompositeKind,
        CompositeTrackDetailsState, MidiEventState, MidiSequenceChannelState, TrackId,
        WaveformChannelState,
    };
    use std::sync::Arc;

    #[shoop_wasm_test_support::shoop_test]
    fn composite_details_take_precedence_over_primitive_empty_state() {
        let context = egui::Context::default();
        let mut pane = DetailsPane::default();
        let loop_id = LoopId::from_raw(7);
        let details = LoopDetailsState {
            loop_id,
            title: "Arrangement".to_owned(),
            composite: Some(CompositeDetailsState {
                kind: CompositeKind::Script,
                cycle_length_frames: 100,
                timeline_length_frames: 200,
                played_frame: None,
                tracks: vec![CompositeTrackDetailsState {
                    id: TrackId::from_raw(2),
                    name: "Rhythm".to_owned(),
                }],
                events: vec![CompositeEventDetailsState {
                    loop_id: LoopId::from_raw(9),
                    loop_name: "Beat".to_owned(),
                    track_id: TrackId::from_raw(2),
                    start_frame: 0,
                    end_frame: 200,
                    ..Default::default()
                }],
            }),
            ..Default::default()
        };
        let mut ignored_output_0 = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&details));
        });
        ignored_output_0.textures_delta.clear();
        assert_eq!(pane.composite.shown_loop_id(), loop_id);
        assert_eq!(pane.composite.rendered_event_count(), 1);
        assert!(pane.waveforms.is_empty());
        assert!(pane.midi_sequences.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn mixed_media_uses_one_bounded_frame_view() {
        let details = LoopDetailsState {
            channels: vec![WaveformChannelState {
                samples: Arc::from([0.0; 100]),
                loop_length: 80,
                ..Default::default()
            }],
            midi_channels: vec![MidiSequenceChannelState {
                events: Arc::from([MidiEventState {
                    frame: 220,
                    data: Arc::from([0x90, 60, 100]),
                }]),
                start_offset: -20,
                preplay_samples: 10,
                loop_length: 150,
                ..Default::default()
            }],
            ..Default::default()
        };
        let bounds = media_bounds(&details);
        assert_eq!(bounds, (-30.0, 220.0));

        let mut state = MediaViewState {
            zoom: 2.0,
            offset: -30.0,
        };
        let mut view = state.view(bounds);
        assert_eq!((view.start_frame, view.end_frame), (-30.0, 95.0));
        view.pan(-1_000.0, 100.0);
        assert_eq!((view.start_frame, view.end_frame), (95.0, 220.0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_only_and_mixed_details_create_the_expected_lanes() {
        let context = egui::Context::default();
        let mut pane = DetailsPane::default();
        let midi = MidiSequenceChannelState {
            label: "MIDI 1".to_owned(),
            events: Arc::from([
                MidiEventState {
                    frame: 1,
                    data: Arc::from([0x90, 60, 100]),
                },
                MidiEventState {
                    frame: 8,
                    data: Arc::from([0x80, 60, 0]),
                },
            ]),
            loop_length: 16,
            ..Default::default()
        };
        let midi_only = LoopDetailsState {
            loop_id: LoopId::from_raw(1),
            title: "MIDI only".to_owned(),
            midi_channels: vec![midi.clone()],
            sync_loop_length: 4,
            ..Default::default()
        };
        let mut ignored_output_1 = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&midi_only));
        });
        ignored_output_1.textures_delta.clear();
        assert!(pane.waveforms.is_empty());
        assert_eq!(pane.midi_sequences.len(), 1);

        let mixed = LoopDetailsState {
            loop_id: LoopId::from_raw(2),
            title: "Mixed".to_owned(),
            channels: vec![WaveformChannelState {
                samples: Arc::from([0.25, -0.25]),
                ..Default::default()
            }],
            midi_channels: vec![midi],
            sync_loop_length: 4,
            ..Default::default()
        };
        let mut ignored_output_2 = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&mixed));
        });
        ignored_output_2.textures_delta.clear();
        assert_eq!(pane.waveforms.len(), 1);
        assert_eq!(pane.midi_sequences.len(), 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn timeline_tools_create_bounded_loop_wide_edits() {
        let details = LoopDetailsState {
            loop_id: LoopId::from_raw(9),
            channels: vec![WaveformChannelState {
                start_offset: 20,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(
            timeline_edit_intent(&details, TimelineEditTool::LoopStart, -5),
            AppIntent::SetLoopTimeline {
                loop_id: details.loop_id,
                start_offset: Some(-5),
                preplay_samples: None,
                loop_length: None,
            }
        );
        assert_eq!(
            timeline_edit_intent(&details, TimelineEditTool::PreplayStart, 8),
            AppIntent::SetLoopTimeline {
                loop_id: details.loop_id,
                start_offset: None,
                preplay_samples: Some(12),
                loop_length: None,
            }
        );
        assert_eq!(
            timeline_edit_intent(&details, TimelineEditTool::LoopEnd, 52),
            AppIntent::SetLoopTimeline {
                loop_id: details.loop_id,
                start_offset: None,
                preplay_samples: None,
                loop_length: Some(32),
            }
        );
        assert_eq!(
            timeline_edit_intent(&details, TimelineEditTool::LoopEnd, 10),
            AppIntent::SetLoopTimeline {
                loop_id: details.loop_id,
                start_offset: None,
                preplay_samples: None,
                loop_length: Some(0),
            }
        );
    }
}
