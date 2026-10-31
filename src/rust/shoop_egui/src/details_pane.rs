use crate::{
    colors, AppIntent, CompositeLoopWidget, LoopDetailsState, LoopId, MidiSequenceWidget,
    TimelineEditTool, WaveformWidget,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaView {
    pub(crate) start_frame: f64,
    pub(crate) end_frame: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaWidgetResponse {
    pub rect: egui::Rect,
    pub pan_frames: f64,
    pub clicked_frame: Option<i64>,
}

pub const GRID_DIVIDER_CHOICES: [Option<u32>; 8] = [
    None,
    Some(1),
    Some(2),
    Some(3),
    Some(4),
    Some(6),
    Some(8),
    Some(16),
];

const MIN_GRID_LINE_PIXELS: f32 = 20.0;

fn snap_frame_to_grid(frame: i64, start_offset: i64, interval: u64) -> i64 {
    if interval == 0 {
        return frame;
    }
    let interval = i128::from(interval);
    let delta = i128::from(frame) - i128::from(start_offset);
    let round_div = |d: i128| -> i128 {
        if d >= 0 {
            (2 * d + interval) / (2 * interval)
        } else {
            -((-2 * d + interval) / (2 * interval))
        }
    };
    let steps = round_div(delta);
    start_offset
        .saturating_add((steps * interval).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64)
}

fn grid_interval(sync_loop_length: u64, divider: Option<u32>) -> Option<u64> {
    if sync_loop_length == 0 {
        return None;
    }
    match divider {
        None => Some(sync_loop_length),
        Some(1) => Some(sync_loop_length),
        Some(d) if d > 1 => Some(sync_loop_length / u64::from(d)),
        Some(_) => Some(sync_loop_length),
    }
}

pub(crate) fn paint_timeline_regions(
    painter: &egui::Painter,
    rect: egui::Rect,
    view: MediaView,
    loop_start: i64,
    preplay_samples: u64,
    loop_length: u64,
    sync_loop_length: u64,
    grid_divider: Option<u32>,
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
        i64::MIN / 4,
        loop_start.saturating_sub_unsigned(preplay_samples),
        colors::WAVEFORM_UNPLAYED_REGION,
    );
    paint_range(
        loop_start.saturating_sub_unsigned(preplay_samples),
        loop_start,
        colors::WAVEFORM_PREPLAY_REGION,
    );
    paint_range(loop_start, loop_end, colors::WAVEFORM_LOOP_REGION);

    let Some(interval) = grid_interval(sync_loop_length, grid_divider) else {
        return;
    };
    let visible_start = view.start_frame.floor() as i64;
    let visible_end = view.end_frame.ceil() as i64;
    let frames_per_pixel = view.visible_frames() / f64::from(rect.width().max(1.0));
    let min_step = (MIN_GRID_LINE_PIXELS as f64 * frames_per_pixel).ceil() as u64;
    let mut step = interval.max(1);
    while step < min_step && step <= u64::MAX / 2 {
        step *= 2;
    }
    let is_minor = grid_divider.is_some_and(|d| d > 1) && step == interval;
    let grid_color = if is_minor {
        colors::WAVEFORM_MINOR_GRID
    } else {
        colors::WAVEFORM_SYNC_MARKER
    };
    let limit = loop_end.max(visible_end);
    let step_i = step.min(i64::MAX as u64) as i64;
    let first_index = visible_start.saturating_sub(loop_start).div_euclid(step_i);
    let last_index = limit.saturating_sub(loop_start).div_euclid(step_i);
    let max_lines = ((visible_end - visible_start) as u64 / step)
        .saturating_add(2)
        .min(4096);
    let mut index = first_index;
    let mut drawn = 0_u64;
    while index <= last_index && drawn < max_lines {
        let marker = loop_start.saturating_add(index.saturating_mul(step_i));
        let x = frame_to_x(marker);
        if rect.x_range().contains(x) {
            painter.vline(x, rect.y_range(), egui::Stroke::new(1.0, grid_color));
            drawn += 1;
        }
        index += 1;
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
        self.start_frame -= f64::from(drag_delta_x) * visible_frames / f64::from(width.max(1.0));
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

const MIN_ZOOM: f32 = 1.0 / 64.0;
const MAX_ZOOM: f32 = 64.0;

impl MediaViewState {
    fn view(&mut self, (timeline_start, timeline_end): (f64, f64)) -> MediaView {
        let total_frames = (timeline_end - timeline_start).max(1.0);
        let visible_frames = (total_frames / f64::from(self.zoom)).max(1.0);
        MediaView {
            start_frame: self.offset,
            end_frame: self.offset + visible_frames,
        }
    }

    fn set_zoom(&mut self, new_zoom: f32, bounds: (f64, f64)) {
        let old_visible = (bounds.1 - bounds.0).max(1.0) / f64::from(self.zoom);
        let new_visible = (bounds.1 - bounds.0).max(1.0) / f64::from(new_zoom);
        let center = self.offset + old_visible / 2.0;
        self.zoom = new_zoom;
        self.offset = center - new_visible / 2.0;
    }

    fn fit(&mut self, bounds: (f64, f64)) {
        let margin_factor = 1.2;
        let total = (bounds.1 - bounds.0).max(1.0);
        self.zoom = (1.0_f64 / margin_factor).clamp(MIN_ZOOM as f64, MAX_ZOOM as f64) as f32;
        self.offset = bounds.0 - (total * (margin_factor - 1.0)) / 2.0;
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

#[derive(Debug)]
pub struct DetailsPane {
    loop_id: LoopId,
    media_view: MediaViewState,
    waveforms: Vec<WaveformWidget>,
    midi_sequences: Vec<MidiSequenceWidget>,
    composite: CompositeLoopWidget,
    edit_tool: Option<TimelineEditTool>,
    grid_divider: Option<u32>,
    snap_enabled: bool,
    lane_heights: Vec<f32>,
}

const MIN_LANE_HEIGHT: f32 = 20.0;
const MAX_LANE_HEIGHT: f32 = 500.0;
const DEFAULT_AUDIO_LANE_HEIGHT: f32 = 72.0;
const DEFAULT_MIDI_LANE_HEIGHT: f32 = 96.0;

impl Default for DetailsPane {
    fn default() -> Self {
        Self {
            loop_id: LoopId::INVALID,
            media_view: MediaViewState::default(),
            waveforms: Vec::new(),
            midi_sequences: Vec::new(),
            composite: CompositeLoopWidget::default(),
            edit_tool: None,
            grid_divider: Some(8),
            snap_enabled: true,
            lane_heights: Vec::new(),
        }
    }
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
            self.lane_heights.clear();
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

        let bounds = media_bounds(details);
        ui.horizontal(|ui| {
            ui.label("Zoom:");
            if ui.small_button("\u{2212}").clicked() {
                self.media_view.set_zoom(
                    (self.media_view.zoom / 1.25).clamp(MIN_ZOOM, MAX_ZOOM),
                    bounds,
                );
            }
            let mut zoom = self.media_view.zoom;
            if ui
                .add(
                    egui::Slider::new(&mut zoom, MIN_ZOOM..=MAX_ZOOM)
                        .logarithmic(true)
                        .show_value(false)
                        .text("zoom"),
                )
                .on_hover_text(format!("Media zoom: {:.2}×", zoom))
                .changed()
            {
                self.media_view.set_zoom(zoom, bounds);
            }
            if ui.small_button("+").clicked() {
                self.media_view.set_zoom(
                    (self.media_view.zoom * 1.25).clamp(MIN_ZOOM, MAX_ZOOM),
                    bounds,
                );
            }
            if ui.small_button("Fit").clicked() {
                self.media_view.fit(bounds);
            }
        });

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
            ui.separator();
            ui.label("Grid:");
            egui::ComboBox::from_id_salt("details_grid_divider")
                .selected_text(grid_divider_label(self.grid_divider))
                .show_ui(ui, |ui| {
                    for choice in GRID_DIVIDER_CHOICES {
                        ui.selectable_value(
                            &mut self.grid_divider,
                            choice,
                            grid_divider_label(choice),
                        );
                    }
                });
            if ui.selectable_label(self.snap_enabled, "snap").clicked() {
                self.snap_enabled = !self.snap_enabled;
            }
        });

        let media_view = self.media_view.view(bounds);
        let mut pan_frames = 0.0;
        let mut clicked_frame = None;
        self.waveforms
            .resize_with(details.channels.len(), WaveformWidget::default);
        self.midi_sequences
            .resize_with(details.midi_channels.len(), MidiSequenceWidget::default);
        let lane_count = details.channels.len() + details.midi_channels.len();
        self.lane_heights
            .resize(lane_count, DEFAULT_AUDIO_LANE_HEIGHT);
        for index in details.channels.len()..lane_count {
            self.lane_heights[index] = DEFAULT_MIDI_LANE_HEIGHT;
        }
        egui::ScrollArea::vertical()
            .id_salt("details_media")
            .scroll_source(crate::control_safe_scroll_source())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.vertical(|ui| {
                    let mut lane_index = 0;
                    for (channel, waveform) in details.channels.iter().zip(&mut self.waveforms) {
                        let height = self.lane_heights[lane_index];
                        let response = waveform.show(
                            ui,
                            channel,
                            media_view,
                            details.sync_loop_length,
                            self.grid_divider,
                            self.edit_tool.is_some(),
                            height,
                        );
                        pan_frames += response.pan_frames;
                        clicked_frame = clicked_frame.or(response.clicked_frame);
                        self.lane_heights[lane_index] = resize_lane(ui, response.rect, height);
                        lane_index += 1;
                    }
                    for (channel, sequence) in
                        details.midi_channels.iter().zip(&mut self.midi_sequences)
                    {
                        let height = self.lane_heights[lane_index];
                        let response = sequence.show(
                            ui,
                            channel,
                            media_view,
                            details.sync_loop_length,
                            self.grid_divider,
                            self.edit_tool.is_some(),
                            height,
                        );
                        pan_frames += response.pan_frames;
                        clicked_frame = clicked_frame.or(response.clicked_frame);
                        self.lane_heights[lane_index] = resize_lane(ui, response.rect, height);
                        lane_index += 1;
                    }
                });
            });
        self.media_view.offset = media_view.start_frame + pan_frames;
        let Some((tool, frame)) = self.edit_tool.zip(clicked_frame) else {
            return Vec::new();
        };
        vec![timeline_edit_intent(
            details,
            tool,
            self.snap_frame(details, frame),
        )]
    }

    fn snap_frame(&self, details: &LoopDetailsState, frame: i64) -> i64 {
        if !self.snap_enabled {
            return frame;
        }
        let Some(interval) = grid_interval(details.sync_loop_length, self.grid_divider) else {
            return frame;
        };
        let start_offset = details
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
        snap_frame_to_grid(frame, start_offset, interval)
    }
}

fn grid_divider_label(divider: Option<u32>) -> String {
    match divider {
        None => "None".to_owned(),
        Some(1) => "1/1".to_owned(),
        Some(d) => format!("1/{d}"),
    }
}

fn resize_lane(ui: &mut egui::Ui, lane_rect: egui::Rect, height: f32) -> f32 {
    let handle_height = 6.0;
    let handle_rect = egui::Rect::from_min_size(
        lane_rect.left_bottom() - egui::vec2(0.0, handle_height),
        egui::vec2(lane_rect.width(), handle_height),
    );
    let response = ui.allocate_rect(handle_rect, egui::Sense::drag());
    if response.dragged() {
        (height + response.drag_delta().y).clamp(MIN_LANE_HEIGHT, MAX_LANE_HEIGHT)
    } else {
        height
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
        assert_eq!((view.start_frame, view.end_frame), (1220.0, 1345.0));
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

    #[shoop_wasm_test_support::shoop_test]
    fn snap_rounds_to_nearest_grid_step_from_loop_start() {
        assert_eq!(snap_frame_to_grid(103, 100, 8), 100);
        assert_eq!(snap_frame_to_grid(105, 100, 8), 108);
        assert_eq!(snap_frame_to_grid(104, 100, 8), 108);
        assert_eq!(snap_frame_to_grid(97, 100, 8), 100);
        assert_eq!(snap_frame_to_grid(95, 100, 8), 92);
        assert_eq!(snap_frame_to_grid(-3, 0, 8), 0);
        assert_eq!(snap_frame_to_grid(-5, 0, 8), -8);
        assert_eq!(snap_frame_to_grid(5, 0, 0), 5);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn grid_interval_follows_sync_length_and_divider() {
        assert_eq!(grid_interval(0, Some(8)), None);
        assert_eq!(grid_interval(4800, None), Some(4800));
        assert_eq!(grid_interval(4800, Some(1)), Some(4800));
        assert_eq!(grid_interval(4800, Some(8)), Some(600));
        assert_eq!(grid_interval(4800, Some(16)), Some(300));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pane_snaps_clicks_only_when_enabled_and_grid_available() {
        let details = LoopDetailsState {
            loop_id: LoopId::from_raw(3),
            channels: vec![WaveformChannelState {
                start_offset: 100,
                ..Default::default()
            }],
            sync_loop_length: 4800,
            ..Default::default()
        };
        let mut pane = DetailsPane::default();
        pane.grid_divider = Some(8);
        pane.snap_enabled = true;
        assert_eq!(pane.snap_frame(&details, 695), 700);
        pane.snap_enabled = false;
        assert_eq!(pane.snap_frame(&details, 103), 103);
        pane.snap_enabled = true;
        pane.grid_divider = None;
        assert_eq!(pane.snap_frame(&details, 2500), 4900);
        let no_sync = LoopDetailsState {
            sync_loop_length: 0,
            ..details.clone()
        };
        assert_eq!(pane.snap_frame(&no_sync, 103), 103);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fit_zoom_shows_all_media_with_margin() {
        let mut state = MediaViewState {
            zoom: 1.0,
            offset: 0.0,
        };
        let bounds: (f64, f64) = (0.0, 40.0);
        state.fit(bounds);
        let view = state.view(bounds);
        assert!(view.start_frame <= bounds.0);
        assert!(view.end_frame >= bounds.1);
        let visible = view.end_frame - view.start_frame;
        assert!((visible - 48.0).abs() < 1.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn zoom_anchors_the_view_center_and_is_not_boundary_clamped() {
        let bounds: (f64, f64) = (0.0, 400.0);
        let mut state = MediaViewState {
            zoom: 2.0,
            offset: 100.0,
        };
        state.set_zoom(4.0, bounds);
        let view = state.view(bounds);
        assert_eq!(view.start_frame, 150.0);
        assert_eq!(view.end_frame, 250.0);
        state.set_zoom(0.5, bounds);
        let view = state.view(bounds);
        assert_eq!(view.start_frame, -200.0);
        assert_eq!(view.end_frame, 600.0);
    }
}
