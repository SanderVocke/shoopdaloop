use crate::{
    colors, AppIntent, ChannelId, ChannelTimelineUpdate, CompositeLoopWidget, LoopDetailsState,
    LoopId, MidiSequenceWidget, TimelineEditTool, WaveformWidget,
};
use egui_material_icons::icons::{ICON_MORE_HORIZ, ICON_SEARCH, ICON_ZOOM_IN, ICON_ZOOM_OUT};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaView {
    pub(crate) start_frame: f64,
    pub(crate) end_frame: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaWidgetResponse {
    pub rect: egui::Rect,
    pub pan_frames: f64,
    pub zoom_delta: f32,
    pub hovered_frame: Option<i64>,
    pub clicked_frame: Option<i64>,
    pub pointer_fraction: f32,
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

const MIN_GRID_LINE_PIXELS: f64 = 20.0;

fn snap_frame_to_grid(frame: i64, origin: i64, interval: u64) -> i64 {
    if interval == 0 {
        return frame;
    }
    let interval = i128::from(interval);
    let delta = i128::from(frame) - i128::from(origin);
    let steps = if delta >= 0 {
        (2 * delta + interval) / (2 * interval)
    } else {
        -((-2 * delta + interval) / (2 * interval))
    };
    (i128::from(origin) + steps * interval).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

fn grid_interval(sync_loop_length: u64, divider: Option<u32>) -> Option<u64> {
    let divider = divider.filter(|divider| *divider > 0)?;
    (sync_loop_length > 0).then(|| (sync_loop_length / u64::from(divider)).max(1))
}

pub(crate) fn paint_timeline_regions(
    painter: &egui::Painter,
    rect: egui::Rect,
    view: MediaView,
    loop_start: i64,
    preplay_samples: u64,
    loop_length: u64,
    data_length: u64,
    sync_loop_length: u64,
    grid_divider: Option<u32>,
    grid_origin: i64,
) {
    let loop_end = loop_start.saturating_add_unsigned(loop_length);
    let data_end = data_length.min(i64::MAX as u64) as i64;
    let paint_range = |start: i64, end: i64, color| {
        let left = view
            .frame_to_x(start as f64, rect)
            .clamp(rect.left(), rect.right());
        let right = view
            .frame_to_x(end as f64, rect)
            .clamp(rect.left(), rect.right());
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
    paint_range(0, data_end, colors::WAVEFORM_UNPLAYED_REGION);
    paint_range(
        loop_start.saturating_sub_unsigned(preplay_samples).max(0),
        loop_start.min(data_end),
        colors::WAVEFORM_PREPLAY_REGION,
    );
    paint_range(
        loop_start.max(0),
        loop_end.min(data_end),
        colors::WAVEFORM_LOOP_REGION,
    );
    for (frame, color) in [
        (
            loop_start.saturating_sub_unsigned(preplay_samples),
            colors::WAVEFORM_PREPLAY_REGION,
        ),
        (loop_start, colors::WAVEFORM_SYNC_MARKER),
        (loop_end, colors::WAVEFORM_SYNC_MARKER),
    ] {
        let x = view.frame_to_x(frame as f64, rect);
        if rect.x_range().contains(x) {
            painter.vline(x, rect.y_range(), egui::Stroke::new(1.0, color));
        }
    }
    if let Some(interval) = grid_interval(sync_loop_length, grid_divider) {
        if grid_divider.is_some_and(|divider| divider > 1) {
            paint_grid(
                painter,
                rect,
                view,
                grid_origin,
                interval,
                colors::WAVEFORM_MINOR_GRID,
            );
        }
        paint_grid(
            painter,
            rect,
            view,
            grid_origin,
            sync_loop_length,
            colors::WAVEFORM_SYNC_MARKER,
        );
    }
}

fn paint_grid(
    painter: &egui::Painter,
    rect: egui::Rect,
    view: MediaView,
    origin: i64,
    interval: u64,
    color: egui::Color32,
) {
    let min_step = MIN_GRID_LINE_PIXELS * view.visible_frames() / f64::from(rect.width().max(1.0));
    let mut step = interval.max(1) as f64;
    while step < min_step {
        step *= 2.0;
    }
    let first = ((view.start_frame - origin as f64) / step).ceil();
    let count = (view.visible_frames() / step).ceil().min(4096.0) as usize + 1;
    for index in 0..count {
        let frame = origin as f64 + (first + index as f64) * step;
        let x = view.frame_to_x(frame, rect);
        if rect.x_range().contains(x) {
            painter.vline(x, rect.y_range(), egui::Stroke::new(1.0, color));
        }
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
}

pub(crate) fn media_response(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    view: MediaView,
    editing: bool,
) -> MediaWidgetResponse {
    let pointer = response.hover_pos();
    let pointer_fraction = pointer
        .map(|pos| ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0))
        .unwrap_or(0.5);
    let (scroll, gesture_zoom) = if response.hovered() {
        ui.ctx().input_mut(|input| {
            let scroll = input.smooth_scroll_delta;
            input.smooth_scroll_delta = egui::Vec2::ZERO;
            (scroll, input.zoom_delta())
        })
    } else {
        (egui::Vec2::ZERO, 1.0)
    };
    let drag_x = if response.dragged_by(egui::PointerButton::Middle)
        || response.dragged_by(egui::PointerButton::Secondary)
    {
        response.drag_delta().x
    } else {
        0.0
    };
    let hovered_frame = editing
        .then_some(pointer)
        .flatten()
        .map(|position| view.x_to_frame(position.x, rect).round() as i64);
    MediaWidgetResponse {
        rect,
        pan_frames: -f64::from(drag_x + scroll.x) * view.visible_frames()
            / f64::from(rect.width().max(1.0)),
        zoom_delta: if gesture_zoom != 1.0 {
            gesture_zoom
        } else {
            (scroll.y * 0.002).exp()
        },
        hovered_frame,
        clicked_frame: if response.clicked_by(egui::PointerButton::Primary) {
            hovered_frame
        } else {
            None
        },
        pointer_fraction,
    }
}

const MIN_FRAMES_PER_PIXEL: f64 = 1.0 / 64.0;
const MAX_FRAMES_PER_PIXEL: f64 = 4_294_967_296.0;

#[derive(Debug)]
struct MediaViewState {
    frames_per_pixel: f64,
    offset: f64,
}

impl Default for MediaViewState {
    fn default() -> Self {
        Self {
            frames_per_pixel: 256.0,
            offset: 0.0,
        }
    }
}

impl MediaViewState {
    fn view(&self, width: f32) -> MediaView {
        MediaView {
            start_frame: self.offset,
            end_frame: self.offset + self.frames_per_pixel * f64::from(width.max(1.0)),
        }
    }
    fn zoom_by(&mut self, factor: f32, width: f32, fraction: f32) {
        let old_visible = self.view(width).visible_frames();
        self.frames_per_pixel = (self.frames_per_pixel / f64::from(factor.max(0.001)))
            .clamp(MIN_FRAMES_PER_PIXEL, MAX_FRAMES_PER_PIXEL);
        self.offset += (old_visible - self.view(width).visible_frames()) * f64::from(fraction);
    }
    fn fit(&mut self, bounds: (f64, f64), width: f32) {
        let total = (bounds.1 - bounds.0).max(1.0);
        self.frames_per_pixel = (total * 1.2 / f64::from(width.max(1.0)))
            .clamp(MIN_FRAMES_PER_PIXEL, MAX_FRAMES_PER_PIXEL);
        self.offset = bounds.0 - total * 0.1;
    }
}

fn media_bounds(details: &LoopDetailsState) -> (f64, f64) {
    let mut start = 0.0_f64;
    let mut end = 1.0_f64;
    for (_, offset, preplay, length, data_length) in lane_timings(details) {
        start = start.min(offset.saturating_sub_unsigned(preplay) as f64);
        end = end
            .max(data_length as f64)
            .max(offset.saturating_add_unsigned(length) as f64);
    }
    (start, end.max(start + 1.0))
}

fn lane_timings(
    details: &LoopDetailsState,
) -> impl Iterator<Item = (ChannelId, i64, u64, u64, u64)> + '_ {
    details
        .channels
        .iter()
        .map(|channel| {
            (
                channel.id,
                channel.start_offset,
                channel.preplay_samples,
                channel.loop_length,
                channel.samples.len() as u64,
            )
        })
        .chain(details.midi_channels.iter().map(|channel| {
            (
                channel.id,
                channel.start_offset,
                channel.preplay_samples,
                channel.loop_length,
                channel.data_length,
            )
        }))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ChannelScope {
    #[default]
    All,
    Single,
}

#[derive(Debug)]
struct TimelineEdit {
    intent: AppIntent,
    markers: Vec<(ChannelId, i64)>,
}

fn resolve_timeline_edit(
    details: &LoopDetailsState,
    tool: TimelineEditTool,
    scope: ChannelScope,
    hovered_channel: ChannelId,
    frame: i64,
) -> Option<TimelineEdit> {
    let (_, current_start, _, _, _) =
        lane_timings(details).find(|(id, ..)| *id == hovered_channel)?;
    let delta = frame.checked_sub(current_start)?;
    if tool == TimelineEditTool::LoopEnd {
        let length = u64::try_from(delta).ok().filter(|length| *length > 0)?;
        let markers = lane_timings(details)
            .map(|(id, start, ..)| Some((id, start.checked_add_unsigned(length)?)))
            .collect::<Option<Vec<_>>>()?;
        return Some(TimelineEdit {
            intent: AppIntent::SetLoopTimeline {
                loop_id: details.loop_id,
                start_offset: None,
                preplay_samples: None,
                loop_length: Some(length),
            },
            markers,
        });
    }
    let mut markers = Vec::new();
    let updates = lane_timings(details)
        .filter(|(id, ..)| scope == ChannelScope::All || *id == hovered_channel)
        .map(|(id, start, ..)| {
            let (start_offset, preplay_samples, marker) = match tool {
                TimelineEditTool::LoopStart => {
                    let offset = start.checked_add(delta)?;
                    (Some(offset), None, offset)
                }
                TimelineEditTool::PreplayStart => {
                    let samples = delta.saturating_neg().max(0) as u64;
                    (None, Some(samples), start.checked_sub_unsigned(samples)?)
                }
                TimelineEditTool::LoopEnd => unreachable!(),
            };
            markers.push((id, marker));
            Some(ChannelTimelineUpdate {
                channel_id: id,
                start_offset,
                preplay_samples,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(TimelineEdit {
        intent: AppIntent::SetLoopChannelTimings {
            loop_id: details.loop_id,
            updates,
        },
        markers,
    })
}

#[derive(Debug)]
pub struct DetailsPane {
    loop_id: LoopId,
    media_view: MediaViewState,
    view_initialized: bool,
    waveforms: Vec<WaveformWidget>,
    midi_sequences: Vec<MidiSequenceWidget>,
    composite: CompositeLoopWidget,
    edit_tool: Option<TimelineEditTool>,
    scope: ChannelScope,
    grid_divider: Option<u32>,
    snap_enabled: bool,
    grid_origins: BTreeMap<ChannelId, i64>,
    fallback_grid_length: Option<u64>,
    lane_heights: BTreeMap<ChannelId, f32>,
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
            view_initialized: false,
            waveforms: Vec::new(),
            midi_sequences: Vec::new(),
            composite: CompositeLoopWidget::default(),
            edit_tool: None,
            scope: ChannelScope::All,
            grid_divider: Some(8),
            snap_enabled: true,
            grid_origins: BTreeMap::new(),
            fallback_grid_length: None,
            lane_heights: BTreeMap::new(),
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
            self.view_initialized = false;
            self.waveforms.clear();
            self.midi_sequences.clear();
            self.grid_origins.clear();
            self.fallback_grid_length = None;
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
        if details.channels.is_empty() && details.midi_channels.is_empty() {
            if !details.loading && !details.midi_loading {
                ui.label("The selected loop has no audio or MIDI data.");
            }
            return Vec::new();
        }
        let bounds = media_bounds(details);
        let width = ui.available_width();
        if !self.view_initialized {
            self.media_view.fit(bounds, width);
            self.view_initialized = !details.loading && !details.midi_loading;
        }
        if self.fallback_grid_length.is_none() {
            self.fallback_grid_length = lane_timings(details)
                .map(|(_, _, _, length, _)| length)
                .find(|length| *length > 0)
                .or_else(|| {
                    lane_timings(details)
                        .map(|(_, _, _, _, length)| length)
                        .max()
                        .filter(|length| *length > 0)
                });
        }
        let grid_length = self.grid_length(details);
        self.toolbar(ui, bounds, width, grid_length, details.sync_loop_length > 0);
        for (id, start, ..) in lane_timings(details) {
            self.grid_origins.entry(id).or_insert(start);
        }
        let view = self.media_view.view(width);
        self.waveforms
            .resize_with(details.channels.len(), WaveformWidget::default);
        self.midi_sequences
            .resize_with(details.midi_channels.len(), MidiSequenceWidget::default);
        let mut responses = Vec::new();
        let edit_intent = egui::ScrollArea::vertical()
            .id_salt("details_media")
            .scroll_source(crate::control_safe_scroll_source())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (channel, waveform) in details.channels.iter().zip(&mut self.waveforms) {
                    let height = *self
                        .lane_heights
                        .entry(channel.id)
                        .or_insert(DEFAULT_AUDIO_LANE_HEIGHT);
                    let response = waveform.show(
                        ui,
                        channel,
                        view,
                        grid_length,
                        self.grid_divider,
                        self.grid_origins[&channel.id],
                        self.edit_tool.is_some(),
                        height,
                    );
                    self.lane_heights
                        .insert(channel.id, resize_lane(ui, response.rect, height));
                    responses.push((channel.id, response));
                }
                for (channel, sequence) in
                    details.midi_channels.iter().zip(&mut self.midi_sequences)
                {
                    let height = *self
                        .lane_heights
                        .entry(channel.id)
                        .or_insert(DEFAULT_MIDI_LANE_HEIGHT);
                    let response = sequence.show(
                        ui,
                        channel,
                        view,
                        grid_length,
                        self.grid_divider,
                        self.grid_origins[&channel.id],
                        self.edit_tool.is_some(),
                        height,
                    );
                    self.lane_heights
                        .insert(channel.id, resize_lane(ui, response.rect, height));
                    responses.push((channel.id, response));
                }
                if let Some((id, response)) = responses
                    .iter()
                    .find(|(_, response)| response.hovered_frame.is_some())
                {
                    let tool = self.edit_tool.expect("hover requires an edit tool");
                    let frame = self.snap_frame(details, *id, response.hovered_frame.unwrap());
                    if let Some(edit) = resolve_timeline_edit(details, tool, self.scope, *id, frame)
                    {
                        for (channel_id, marker) in &edit.markers {
                            if let Some((_, lane)) =
                                responses.iter().find(|(id, _)| id == channel_id)
                            {
                                let x = view.frame_to_x(*marker as f64, lane.rect);
                                if lane.rect.x_range().contains(x) {
                                    ui.painter_at(lane.rect).vline(
                                        x,
                                        lane.rect.y_range(),
                                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                                    );
                                }
                            }
                        }
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if let Some(pos) = ui.ctx().pointer_hover_pos() {
                            let painter = ui.painter_at(response.rect);
                            let text = format!("{} · {}", tool_label(Some(tool)), frame);
                            let galley = painter.layout_no_wrap(
                                text,
                                egui::FontId::proportional(12.0),
                                egui::Color32::WHITE,
                            );
                            let label_pos = egui::pos2(
                                (pos.x + 8.0)
                                    .min(response.rect.right() - galley.size().x - 4.0)
                                    .max(response.rect.left()),
                                response.rect.bottom() - galley.size().y - 3.0,
                            );
                            painter.rect_filled(
                                egui::Rect::from_min_size(label_pos, galley.size()).expand(2.0),
                                2.0,
                                egui::Color32::BLACK,
                            );
                            painter.galley(label_pos, galley, egui::Color32::WHITE);
                        }
                        if response.clicked_frame.is_some() {
                            return Some(edit.intent);
                        }
                    } else {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
                    }
                }
                None
            })
            .inner;
        // Navigation is applied once, after all lanes have shared the same view.
        for (_, response) in &responses {
            self.media_view.offset += response.pan_frames;
            if response.zoom_delta != 1.0 {
                self.media_view
                    .zoom_by(response.zoom_delta, width, response.pointer_fraction);
            }
        }
        edit_intent.into_iter().collect()
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        bounds: (f64, f64),
        width: f32,
        grid_length: u64,
        has_sync: bool,
    ) {
        ui.scope(|ui| {
        let height = ui.spacing().interact_size.y.max(24.0);
        ui.spacing_mut().interact_size = egui::vec2(0.0, height);
        ui.spacing_mut().button_padding = egui::vec2(5.0, 2.0);
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        ui.horizontal_wrapped(|ui| {
            ui.horizontal(|ui| {
                if toolbar_icon(ui, ICON_ZOOM_OUT, height)
                    .on_hover_text("Zoom out")
                    .clicked()
                {
                    self.media_view.zoom_by(0.8, width, 0.5);
                }
                if toolbar_icon(ui, ICON_ZOOM_IN, height)
                    .on_hover_text("Zoom in")
                    .clicked()
                {
                    self.media_view.zoom_by(1.25, width, 0.5);
                }
                if ui
                    .add(egui::Button::new("Fit").min_size(egui::vec2(0.0, height)))
                    .on_hover_text("Show all stored content and the loop range")
                    .clicked()
                {
                    self.media_view.fit(bounds, width);
                }
                let zoom_button = toolbar_icon(ui, ICON_SEARCH, height).on_hover_text("Zoom slider");
                egui::Popup::menu(&zoom_button).show(|ui| {
                    let previous = self.media_view.frames_per_pixel;
                    let mut value = previous;
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut value,
                                MIN_FRAMES_PER_PIXEL..=MAX_FRAMES_PER_PIXEL,
                            )
                            .logarithmic(true)
                            .show_value(false),
                        )
                        .changed()
                    {
                        self.media_view
                            .zoom_by((previous / value) as f32, width, 0.5);
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Tool:");
                egui::ComboBox::from_id_salt("details_tool")
                    .width(96.0)
                    .selected_text(tool_label(self.edit_tool))
                    .show_ui(ui, |ui| {
                        for tool in [
                            None,
                            Some(TimelineEditTool::LoopStart),
                            Some(TimelineEditTool::PreplayStart),
                            Some(TimelineEditTool::LoopEnd),
                        ] {
                            ui.selectable_value(&mut self.edit_tool, tool, tool_label(tool));
                        }
                    });
                ui.label("Channels:");
                let end_tool = self.edit_tool == Some(TimelineEditTool::LoopEnd);
                ui.add_enabled_ui(!end_tool, |ui| {
                    let mut scope = if end_tool {
                        ChannelScope::All
                    } else {
                        self.scope
                    };
                    egui::ComboBox::from_id_salt("details_scope")
                        .width(64.0)
                        .selected_text(if scope == ChannelScope::All {
                            "All"
                        } else {
                            "Single"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut scope, ChannelScope::All, "All");
                            ui.selectable_value(&mut scope, ChannelScope::Single, "Single");
                        });
                    if !end_tool {
                        self.scope = scope;
                    }
                })
                .response
                .on_hover_text(if end_tool {
                    "Loop length is shared by all channels"
                } else {
                    "Edit all channels or only the hovered channel"
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Grid:");
                egui::ComboBox::from_id_salt("details_grid_divider")
                    .width(52.0)
                    .selected_text(grid_divider_label(self.grid_divider))
                    .show_ui(ui, |ui| {
                        for choice in GRID_DIVIDER_CHOICES {
                            ui.selectable_value(
                                &mut self.grid_divider,
                                choice,
                                grid_divider_label(choice),
                            );
                        }
                    }).response.on_hover_text(if has_sync {
                        "Grid divisions use the Sync loop length"
                    } else {
                        "Sync is empty: grid divisions use this loop's initial range or stored content"
                    });
                if ui.add_enabled(grid_interval(grid_length, self.grid_divider).is_some(),
                    egui::Button::new("Snap").selected(self.snap_enabled).min_size(egui::vec2(0.0, height)))
                    .on_hover_text("Snap the preview and click to the grid")
                    .on_disabled_hover_text("Choose a grid division and record or load loop content to enable snapping")
                    .clicked() { self.snap_enabled = !self.snap_enabled; }
                let grid_button = toolbar_icon(ui, ICON_MORE_HORIZ, height)
                    .on_hover_text("Grid reference stays fixed while editing");
                egui::Popup::menu(&grid_button).show(|ui| {
                    if ui.button("Re-anchor grid to current starts").clicked() {
                        self.grid_origins.clear();
                        ui.close();
                    }
                });
            });
        });
        });
    }

    fn grid_length(&self, details: &LoopDetailsState) -> u64 {
        if details.sync_loop_length > 0 {
            details.sync_loop_length
        } else {
            self.fallback_grid_length.unwrap_or(0)
        }
    }

    fn snap_frame(&self, details: &LoopDetailsState, channel_id: ChannelId, frame: i64) -> i64 {
        if !self.snap_enabled {
            return frame;
        }
        let Some(interval) = grid_interval(self.grid_length(details), self.grid_divider) else {
            return frame;
        };
        snap_frame_to_grid(
            frame,
            self.grid_origins.get(&channel_id).copied().unwrap_or(0),
            interval,
        )
    }
}

fn toolbar_icon(
    ui: &mut egui::Ui,
    icon: egui_material_icons::MaterialIcon,
    height: f32,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::Vec2::splat(2.0);
        ui.add_sized(
            egui::Vec2::splat(height),
            egui::Button::new(icon.rich_text().size(height - 8.0))
                .min_size(egui::Vec2::splat(height)),
        )
    })
    .inner
}

pub(crate) fn paint_lane_label(ui: &egui::Ui, rect: egui::Rect, label: &str) {
    ui.painter_at(rect).text(
        rect.left_top() + egui::vec2(6.0, 3.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::proportional(12.0),
        colors::MUTED_FOREGROUND,
    );
}

fn tool_label(tool: Option<TimelineEditTool>) -> &'static str {
    match tool {
        None => "None",
        Some(TimelineEditTool::LoopStart) => "Set start",
        Some(TimelineEditTool::PreplayStart) => "Set preplay",
        Some(TimelineEditTool::LoopEnd) => "Set end",
    }
}

fn grid_divider_label(divider: Option<u32>) -> String {
    divider.map_or_else(|| "None".to_owned(), |divider| format!("1/{divider}"))
}

fn resize_lane(ui: &mut egui::Ui, lane_rect: egui::Rect, height: f32) -> f32 {
    let handle_rect = egui::Rect::from_min_size(
        lane_rect.left_bottom() - egui::vec2(0.0, 6.0),
        egui::vec2(lane_rect.width(), 6.0),
    );
    let response = ui
        .interact(
            handle_rect,
            ui.make_persistent_id(("lane_resize", lane_rect.top().to_bits())),
            egui::Sense::drag(),
        )
        .on_hover_cursor(egui::CursorIcon::ResizeVertical);
    if response.dragged_by(egui::PointerButton::Primary) {
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
        crate::initialize(&context);
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
    fn midi_only_and_mixed_details_create_the_expected_lanes() {
        let context = egui::Context::default();
        crate::initialize(&context);
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

    fn details() -> LoopDetailsState {
        LoopDetailsState {
            loop_id: LoopId::from_raw(3),
            channels: vec![
                WaveformChannelState {
                    id: ChannelId::from_raw(1),
                    start_offset: 20,
                    samples: Arc::from([0.0; 100]),
                    loop_length: 40,
                    ..Default::default()
                },
                WaveformChannelState {
                    id: ChannelId::from_raw(2),
                    start_offset: 30,
                    samples: Arc::from([0.0; 120]),
                    loop_length: 40,
                    ..Default::default()
                },
            ],
            midi_channels: vec![MidiSequenceChannelState {
                id: ChannelId::from_raw(3),
                start_offset: 50,
                data_length: 200,
                loop_length: 40,
                ..Default::default()
            }],
            sync_loop_length: 80,
            ..Default::default()
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn single_and_all_edits_preserve_channel_offsets_and_preview_the_exact_command() {
        let details = details();
        let single = resolve_timeline_edit(
            &details,
            TimelineEditTool::LoopStart,
            ChannelScope::Single,
            ChannelId::from_raw(2),
            42,
        )
        .unwrap();
        assert_eq!(single.markers, [(ChannelId::from_raw(2), 42)]);
        assert_eq!(
            single.intent,
            AppIntent::SetLoopChannelTimings {
                loop_id: details.loop_id,
                updates: vec![ChannelTimelineUpdate {
                    channel_id: ChannelId::from_raw(2),
                    start_offset: Some(42),
                    preplay_samples: None
                }],
            }
        );
        let all = resolve_timeline_edit(
            &details,
            TimelineEditTool::LoopStart,
            ChannelScope::All,
            ChannelId::from_raw(2),
            42,
        )
        .unwrap();
        assert_eq!(
            all.markers,
            [
                (ChannelId::from_raw(1), 32),
                (ChannelId::from_raw(2), 42),
                (ChannelId::from_raw(3), 62)
            ]
        );
        let AppIntent::SetLoopChannelTimings { updates, .. } = all.intent else {
            panic!("expected channel updates")
        };
        assert_eq!(
            updates
                .iter()
                .map(|update| update.start_offset.unwrap())
                .collect::<Vec<_>>(),
            [32, 42, 62]
        );
        let preplay = resolve_timeline_edit(
            &details,
            TimelineEditTool::PreplayStart,
            ChannelScope::All,
            ChannelId::from_raw(3),
            38,
        )
        .unwrap();
        assert_eq!(
            preplay.markers,
            [
                (ChannelId::from_raw(1), 8),
                (ChannelId::from_raw(2), 18),
                (ChannelId::from_raw(3), 38)
            ]
        );
        let AppIntent::SetLoopChannelTimings { updates, .. } = preplay.intent else {
            panic!("expected channel updates")
        };
        assert!(updates
            .iter()
            .all(|update| update.preplay_samples == Some(12)));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn end_is_loop_wide_and_invalid_end_clicks_do_not_collapse_the_loop() {
        let details = details();
        let edit = resolve_timeline_edit(
            &details,
            TimelineEditTool::LoopEnd,
            ChannelScope::Single,
            ChannelId::from_raw(3),
            90,
        )
        .unwrap();
        assert_eq!(
            edit.markers,
            [
                (ChannelId::from_raw(1), 60),
                (ChannelId::from_raw(2), 70),
                (ChannelId::from_raw(3), 90)
            ]
        );
        assert_eq!(
            edit.intent,
            AppIntent::SetLoopTimeline {
                loop_id: details.loop_id,
                start_offset: None,
                preplay_samples: None,
                loop_length: Some(40)
            }
        );
        for frame in [49, 50] {
            assert!(resolve_timeline_edit(
                &details,
                TimelineEditTool::LoopEnd,
                ChannelScope::All,
                ChannelId::from_raw(3),
                frame
            )
            .is_none());
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn storage_bounds_include_empty_midi_duration_and_are_independent_of_events() {
        let mut details = details();
        assert_eq!(media_bounds(&details), (0.0, 200.0));
        details.midi_channels[0].data_length = 300;
        assert_eq!(media_bounds(&details), (0.0, 300.0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn shaded_regions_are_clipped_to_storage_on_both_sides() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut output = context.run_ui(Default::default(), |ui| {
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 60.0));
            paint_timeline_regions(
                ui.painter(),
                rect,
                MediaView {
                    start_frame: -50.0,
                    end_frame: 150.0,
                },
                20,
                10,
                30,
                100,
                0,
                None,
                0,
            );
        });
        output.textures_delta.clear();
        let rectangles = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(rectangles.len(), 3);
        assert_eq!(rectangles[0].fill, colors::WAVEFORM_UNPLAYED_REGION);
        assert_eq!(
            (rectangles[0].rect.left(), rectangles[0].rect.right()),
            (50.0, 150.0)
        );
        assert!(rectangles
            .iter()
            .all(|rect| rect.rect.left() >= 50.0 && rect.rect.right() <= 150.0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn snap_reference_and_view_scale_stay_fixed_after_a_start_edit() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut pane = DetailsPane::default();
        let mut details = details();
        let mut output = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&details));
        });
        output.textures_delta.clear();
        let scale = pane.media_view.frames_per_pixel;
        let offset = pane.media_view.offset;
        assert_eq!(pane.snap_frame(&details, ChannelId::from_raw(1), 35), 40);
        details.channels[0].start_offset = -33;
        details.channels[0].preplay_samples = 200;
        let mut output = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&details));
        });
        output.textures_delta.clear();
        assert_eq!(pane.snap_frame(&details, ChannelId::from_raw(1), 35), 40);
        assert_eq!(pane.media_view.frames_per_pixel, scale);
        assert_eq!(pane.media_view.offset, offset);
        pane.grid_divider = None;
        assert_eq!(pane.snap_frame(&details, ChannelId::from_raw(1), 35), 35);
        assert_eq!(grid_interval(80, None), None);
        assert_eq!(grid_interval(1, Some(16)), Some(1));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_lane_height_survives_subsequent_frames() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut pane = DetailsPane::default();
        let details = details();
        for _ in 0..2 {
            let mut output = context.run_ui(Default::default(), |ui| {
                pane.show(ui, Some(&details));
            });
            output.textures_delta.clear();
            pane.lane_heights.insert(ChannelId::from_raw(3), 180.0);
        }
        let mut output = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&details));
        });
        output.textures_delta.clear();
        assert_eq!(pane.lane_heights[&ChannelId::from_raw(3)], 180.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn zoom_preserves_the_frame_under_the_pointer_and_fit_includes_a_margin() {
        let mut state = MediaViewState {
            frames_per_pixel: 2.0,
            offset: -100.0,
        };
        let before = state.view(200.0).start_frame + state.view(200.0).visible_frames() * 0.75;
        state.zoom_by(2.0, 200.0, 0.75);
        let after = state.view(200.0).start_frame + state.view(200.0).visible_frames() * 0.75;
        assert_eq!(before, after);
        state.fit((-100.0, 300.0), 200.0);
        assert!(state.view(200.0).start_frame < -100.0);
        assert!(state.view(200.0).end_frame > 300.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn normalized_wheel_input_zooms_consumes_scrolling_and_pans_horizontally() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let view = MediaView {
            start_frame: 0.0,
            end_frame: 400.0,
        };
        let mut rect = egui::Rect::NOTHING;
        let mut output = context.run_ui(Default::default(), |ui| {
            let (r, _) =
                ui.allocate_exact_size(egui::vec2(200.0, 60.0), egui::Sense::click_and_drag());
            rect = r;
        });
        output.textures_delta.clear();
        for delta in [egui::vec2(0.0, 40.0), egui::vec2(40.0, 0.0)] {
            let mut result = None;
            let mut output = context.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::PointerMoved(rect.center())],
                    ..Default::default()
                },
                |ui| {
                    ui.ctx()
                        .input_mut(|input| input.smooth_scroll_delta = delta);
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(200.0, 60.0),
                        egui::Sense::click_and_drag(),
                    );
                    result = Some(media_response(ui, rect, &response, view, true));
                    assert_eq!(
                        ui.ctx().input(|input| input.smooth_scroll_delta),
                        egui::Vec2::ZERO
                    );
                },
            );
            output.textures_delta.clear();
            let result = result.unwrap();
            assert!(result.hovered_frame.is_some());
            if delta.y != 0.0 {
                assert!(result.zoom_delta > 1.0);
            } else {
                assert!(result.pan_frames < 0.0);
            }
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn snapped_hover_markers_match_the_command_emitted_on_click() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut pane = DetailsPane {
            edit_tool: Some(TimelineEditTool::LoopStart),
            ..Default::default()
        };
        let details = details();
        let mut output = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&details));
        });
        output.textures_delta.clear();
        let rect = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape)
                    if shape.fill == colors::WAVEFORM_BACKGROUND
                        && shape.rect.height() == DEFAULT_AUDIO_LANE_HEIGHT =>
                {
                    Some(shape.rect)
                }
                _ => None,
            })
            .unwrap();
        let pos = egui::pos2(rect.left() + rect.width() * 0.4, rect.center().y);
        let frame = pane.snap_frame(
            &details,
            details.channels[0].id,
            pane.media_view
                .view(rect.width())
                .x_to_frame(pos.x, rect)
                .round() as i64,
        );
        let expected = resolve_timeline_edit(
            &details,
            TimelineEditTool::LoopStart,
            ChannelScope::All,
            details.channels[0].id,
            frame,
        )
        .unwrap();
        let mut output = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(pos)],
                ..Default::default()
            },
            |ui| {
                assert!(pane.show(ui, Some(&details)).is_empty());
            },
        );
        output.textures_delta.clear();
        let preview_lines = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::LineSegment { points, stroke }
                    if stroke.color == egui::Color32::WHITE
                        && stroke.width == 1.0
                        && points[0].x == points[1].x =>
                {
                    Some(points[0].x)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(preview_lines.len(), expected.markers.len());
        for (x, (_, marker)) in preview_lines.iter().zip(&expected.markers) {
            assert!(
                (*x - pane
                    .media_view
                    .view(rect.width())
                    .frame_to_x(*marker as f64, rect))
                .abs()
                    < 0.01
            );
        }
        let mut emitted = Vec::new();
        for pressed in [true, false] {
            let mut output = context.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| {
                    emitted.extend(pane.show(ui, Some(&details)));
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(emitted, [expected.intent]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn grid_snap_rounds_negative_offsets_and_half_steps() {
        assert_eq!(snap_frame_to_grid(104, 100, 8), 108);
        assert_eq!(snap_frame_to_grid(95, 100, 8), 92);
        assert_eq!(snap_frame_to_grid(-5, 0, 8), -8);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn empty_sync_uses_a_stable_loop_grid_for_paint_hover_and_click() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut pane = DetailsPane {
            edit_tool: Some(TimelineEditTool::LoopStart),
            grid_divider: Some(1),
            ..Default::default()
        };
        let mut details = details();
        details.sync_loop_length = 0;
        let mut output = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&details));
        });
        output.textures_delta.clear();
        assert_eq!(pane.grid_length(&details), 40);
        let rect = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape)
                    if shape.fill == colors::WAVEFORM_BACKGROUND
                        && shape.rect.height() == DEFAULT_AUDIO_LANE_HEIGHT =>
                {
                    Some(shape.rect)
                }
                _ => None,
            })
            .unwrap();
        let view = pane.media_view.view(rect.width());
        let snapped_x = view.frame_to_x(100.0, rect);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == colors::WAVEFORM_SYNC_MARKER
                    && (points[0].x - snapped_x).abs() < 0.01 && points[0].y == rect.top())));
        let position = egui::pos2(view.frame_to_x(96.0, rect), rect.center().y);
        let mut output = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(position)],
                ..Default::default()
            },
            |ui| {
                assert!(pane.show(ui, Some(&details)).is_empty());
            },
        );
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::WHITE
                    && (points[0].x - snapped_x).abs() < 0.01 && points[0].y == rect.top())));
        let mut emitted = Vec::new();
        for pressed in [true, false] {
            let mut output = context.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| {
                    emitted.extend(pane.show(ui, Some(&details)));
                },
            );
            output.textures_delta.clear();
        }
        let [AppIntent::SetLoopChannelTimings { updates, .. }] = emitted.as_slice() else {
            panic!("expected one channel timing edit");
        };
        assert_eq!(updates[0].start_offset, Some(100));
        details.channels[0].start_offset = 100;
        for channel in &mut details.channels {
            channel.loop_length = 200;
        }
        let mut output = context.run_ui(Default::default(), |ui| {
            pane.show(ui, Some(&details));
        });
        output.textures_delta.clear();
        assert_eq!(pane.grid_length(&details), 40);
        assert_eq!(pane.snap_frame(&details, details.channels[0].id, 96), 100);
        details.sync_loop_length = 80;
        assert_eq!(pane.grid_length(&details), 80);
    }
}
