use crate::{
    colors, AppIntent, CompositeDetailsState, CompositeEventDetailsState, CompositeEventId, LoopId,
    LoopMode, TrackId,
};
use egui_material_icons::icons::ICON_LOCK_CLOCK;
use std::collections::{BTreeMap, BTreeSet};

const MIN_CYCLE_WIDTH: f32 = 20.0;
const MAX_CYCLE_WIDTH: f32 = 600.0;
const DEFAULT_CYCLE_WIDTH: f32 = 130.0;
const TRACK_LABEL_WIDTH: f32 = 112.0;
const HEADER_HEIGHT: f32 = 22.0;
const LANE_HEIGHT: f32 = 28.0;
const LANE_GAP: f32 = 2.0;
const TRACK_GAP: f32 = 2.0;
const EVENT_INSET: f32 = 1.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LoopDragPayload {
    pub loop_id: LoopId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompositeEventDragPayload {
    events: Vec<CompositeEventId>,
    grabbed_offset: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CompositeEventKey {
    instance_id: u64,
}

impl From<&CompositeEventDetailsState> for CompositeEventKey {
    fn from(event: &CompositeEventDetailsState) -> Self {
        Self {
            instance_id: event.instance_id,
        }
    }
}

impl From<CompositeEventKey> for CompositeEventId {
    fn from(event: CompositeEventKey) -> Self {
        Self {
            instance_id: event.instance_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoxSelectionMode {
    Replace,
    Add,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BoxSelection {
    origin: egui::Pos2,
    mode: BoxSelectionMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PackedEvent {
    event_index: usize,
    lane: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackedTrack {
    track_id: TrackId,
    lane_count: usize,
    events: Vec<PackedEvent>,
}

fn pack_swimlanes(details: &CompositeDetailsState) -> Vec<PackedTrack> {
    let mut by_track = BTreeMap::<TrackId, Vec<usize>>::new();
    for (index, event) in details.events.iter().enumerate() {
        by_track.entry(event.track_id).or_default().push(index);
    }
    details
        .tracks
        .iter()
        .map(|track| {
            let mut event_indices = by_track.remove(&track.id).unwrap_or_default();
            event_indices.sort_by_key(|index| {
                let event = &details.events[*index];
                (
                    event.start_frame,
                    event.end_frame,
                    event.instance_id,
                    event.loop_id,
                    *index,
                )
            });
            let mut lane_ends = Vec::<u64>::new();
            let mut events = Vec::with_capacity(event_indices.len());
            for event_index in event_indices {
                let event = &details.events[event_index];
                let lane = lane_ends
                    .iter()
                    .position(|end| *end <= event.start_frame)
                    .unwrap_or_else(|| {
                        lane_ends.push(0);
                        lane_ends.len() - 1
                    });
                lane_ends[lane] = event.end_frame;
                events.push(PackedEvent { event_index, lane });
            }
            PackedTrack {
                track_id: track.id,
                lane_count: lane_ends.len().max(1),
                events,
            }
        })
        .collect()
}

fn track_height(lane_count: usize) -> f32 {
    lane_count.max(1) as f32 * LANE_HEIGHT + lane_count.saturating_sub(1) as f32 * LANE_GAP
}

fn event_color(event: &CompositeEventDetailsState) -> egui::Color32 {
    match event.loop_mode {
        LoopMode::Playing => colors::LOOP_PROGRESS_PLAYING,
        LoopMode::PlayingDryThroughWet => colors::LOOP_PROGRESS_PLAYING_DRY,
        LoopMode::Recording | LoopMode::Replacing => colors::LOOP_PROGRESS_RECORDING,
        LoopMode::RecordingDryIntoWet => colors::LOOP_PROGRESS_RECORDING_DRY,
        _ => colors::LOOP_AUDIO_BACKGROUND,
    }
}

const SCRIPT_EVENT_MODES: [(LoopMode, &str); 6] = [
    (LoopMode::Playing, "Play"),
    (LoopMode::Recording, "Record"),
    (LoopMode::Replacing, "Replace"),
    (LoopMode::PlayingDryThroughWet, "Play dry through wet"),
    (LoopMode::RecordingDryIntoWet, "Record dry into wet"),
    (LoopMode::Stopped, "Stop"),
];

fn script_mode_label(mode: Option<&str>) -> &str {
    match mode {
        Some("playing") => "Play",
        Some("recording") => "Record",
        Some("replacing") => "Replace",
        Some("playing_dry_through_wet") => "Play dry/wet",
        Some("recording_dry_into_wet") => "Record dry/wet",
        Some("stopped") => "Stop",
        _ => "Unknown",
    }
}

fn script_mode_key(mode: LoopMode) -> &'static str {
    match mode {
        LoopMode::Playing => "playing",
        LoopMode::Recording => "recording",
        LoopMode::Replacing => "replacing",
        LoopMode::PlayingDryThroughWet => "playing_dry_through_wet",
        LoopMode::RecordingDryIntoWet => "recording_dry_into_wet",
        LoopMode::Stopped => "stopped",
        LoopMode::Unknown => "unknown",
    }
}

#[derive(Debug)]
pub struct CompositeLoopWidget {
    loop_id: LoopId,
    cycle_width: f32,
    selected_events: BTreeSet<CompositeEventKey>,
    box_selection: Option<BoxSelection>,
    force_length_cycles: u32,
    #[cfg(test)]
    rendered_events: Vec<(String, egui::Rect, usize)>,
    #[cfg(test)]
    rendered_selected_event_count: usize,
    #[cfg(test)]
    rendered_track_heights: Vec<f32>,
    #[cfg(test)]
    content_size: egui::Vec2,
    #[cfg(test)]
    drop_rect: Option<egui::Rect>,
    #[cfg(test)]
    highlighted_iteration: Option<u64>,
    #[cfg(test)]
    highlighted_rect: Option<egui::Rect>,
    #[cfg(test)]
    timeline_rect: Option<egui::Rect>,
    #[cfg(test)]
    timeline_left: Option<f32>,
    #[cfg(test)]
    playhead_x: Option<f32>,
    #[cfg(test)]
    box_selection_rect: Option<egui::Rect>,
    #[cfg(test)]
    delete_menu_rect: Option<egui::Rect>,
    #[cfg(test)]
    force_length_menu_rect: Option<egui::Rect>,
    #[cfg(test)]
    natural_length_menu_rect: Option<egui::Rect>,
    #[cfg(test)]
    regular_kind_rect: Option<egui::Rect>,
    #[cfg(test)]
    script_kind_rect: Option<egui::Rect>,
    #[cfg(test)]
    mode_menu_rects: Vec<(LoopMode, egui::Rect)>,
}

impl Default for CompositeLoopWidget {
    fn default() -> Self {
        Self {
            loop_id: LoopId::INVALID,
            cycle_width: DEFAULT_CYCLE_WIDTH,
            selected_events: BTreeSet::new(),
            box_selection: None,
            force_length_cycles: 1,
            #[cfg(test)]
            rendered_events: Vec::new(),
            #[cfg(test)]
            rendered_selected_event_count: 0,
            #[cfg(test)]
            rendered_track_heights: Vec::new(),
            #[cfg(test)]
            content_size: egui::Vec2::ZERO,
            #[cfg(test)]
            drop_rect: None,
            #[cfg(test)]
            highlighted_iteration: None,
            #[cfg(test)]
            highlighted_rect: None,
            #[cfg(test)]
            timeline_rect: None,
            #[cfg(test)]
            timeline_left: None,
            #[cfg(test)]
            playhead_x: None,
            #[cfg(test)]
            box_selection_rect: None,
            #[cfg(test)]
            delete_menu_rect: None,
            #[cfg(test)]
            force_length_menu_rect: None,
            #[cfg(test)]
            natural_length_menu_rect: None,
            #[cfg(test)]
            regular_kind_rect: None,
            #[cfg(test)]
            script_kind_rect: None,
            #[cfg(test)]
            mode_menu_rects: Vec::new(),
        }
    }
}

impl CompositeLoopWidget {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        loop_id: LoopId,
        details: &CompositeDetailsState,
    ) -> Vec<AppIntent> {
        if self.loop_id != loop_id {
            self.loop_id = loop_id;
            self.cycle_width = DEFAULT_CYCLE_WIDTH;
            self.selected_events.clear();
            self.box_selection = None;
        }
        let available_events = details
            .events
            .iter()
            .map(CompositeEventKey::from)
            .collect::<BTreeSet<_>>();
        self.selected_events
            .retain(|event| available_events.contains(event));
        #[cfg(test)]
        {
            self.rendered_events.clear();
            self.rendered_selected_event_count = 0;
            self.rendered_track_heights.clear();
            self.content_size = egui::Vec2::ZERO;
            self.drop_rect = None;
            self.highlighted_iteration = None;
            self.highlighted_rect = None;
            self.timeline_rect = None;
            self.timeline_left = None;
            self.playhead_x = None;
            self.box_selection_rect = None;
            self.delete_menu_rect = None;
            self.force_length_menu_rect = None;
            self.natural_length_menu_rect = None;
            self.regular_kind_rect = None;
            self.script_kind_rect = None;
            self.mode_menu_rects.clear();
        }

        let mut intents = Vec::new();
        let fit_width = (ui.available_width() - TRACK_LABEL_WIDTH).max(1.0);
        ui.horizontal(|ui| {
            ui.label("Type");
            let mut kind = details.kind;
            let regular = ui.selectable_value(&mut kind, crate::CompositeKind::Regular, "Regular");
            let script = ui.selectable_value(&mut kind, crate::CompositeKind::Script, "Script");
            #[cfg(test)]
            {
                self.regular_kind_rect = Some(regular.rect);
                self.script_kind_rect = Some(script.rect);
            }
            #[cfg(not(test))]
            let _ = (regular, script);
            if kind != details.kind {
                intents.push(AppIntent::SetCompositeKind {
                    target_loop_id: loop_id,
                    kind,
                });
            }
            ui.separator();
            ui.label("Zoom");
            if ui.small_button("−").clicked() {
                self.cycle_width =
                    (self.cycle_width - 29.0).clamp(MIN_CYCLE_WIDTH, MAX_CYCLE_WIDTH);
            }
            ui.add(
                egui::Slider::new(&mut self.cycle_width, MIN_CYCLE_WIDTH..=MAX_CYCLE_WIDTH)
                    .show_value(false),
            )
            .on_hover_text(format!(
                "Timeline zoom: {:.0} points/cycle",
                self.cycle_width
            ));
            if ui.small_button("+").clicked() {
                self.cycle_width =
                    (self.cycle_width + 29.0).clamp(MIN_CYCLE_WIDTH, MAX_CYCLE_WIDTH);
            }
            if ui.small_button("Fit").clicked() {
                let cycles = timeline_cycles(details);
                self.cycle_width =
                    (fit_width / cycles as f32).clamp(MIN_CYCLE_WIDTH, MAX_CYCLE_WIDTH);
            }
        });

        let mut drop_iteration = None;
        let (_drop_zone, dropped) = ui.dnd_drop_zone::<LoopDragPayload, _>(
            egui::Frame::new().inner_margin(egui::Margin::same(3)),
            |ui| {
                drop_iteration = self.show_timeline(ui, details, &mut intents);
            },
        );
        #[cfg(test)]
        {
            self.drop_rect = Some(_drop_zone.response.rect);
        }
        if let Some((payload, start_iteration)) = dropped
            .filter(|payload| payload.loop_id != loop_id)
            .zip(drop_iteration)
        {
            intents.push(AppIntent::ComposeLoopAt {
                target_loop_id: loop_id,
                source_loop_id: payload.loop_id,
                start_iteration,
            });
        }
        intents
    }

    fn show_timeline(
        &mut self,
        ui: &mut egui::Ui,
        details: &CompositeDetailsState,
        intents: &mut Vec<AppIntent>,
    ) -> Option<u64> {
        let packed = pack_swimlanes(details);
        let timeline_cycles = timeline_cycles(details);
        let visible_timeline_width = (ui.available_width() - TRACK_LABEL_WIDTH).max(1.0);
        let visible_cycles = (visible_timeline_width / self.cycle_width).ceil() as u64;
        let displayed_cycles = timeline_cycles.saturating_add(1).max(visible_cycles).max(1);
        let timeline_width = displayed_cycles as f32 * self.cycle_width;
        let content_width = (TRACK_LABEL_WIDTH + timeline_width)
            .max(ui.available_width())
            .max(TRACK_LABEL_WIDTH + 1.0);
        let rows_height = (packed
            .iter()
            .map(|track| track_height(track.lane_count))
            .sum::<f32>()
            + packed.len().saturating_sub(1) as f32 * TRACK_GAP)
            .max(if details.events.is_empty() { 56.0 } else { 0.0 });
        let content_height = HEADER_HEIGHT + TRACK_GAP + rows_height;
        #[cfg(test)]
        {
            self.content_size = egui::vec2(content_width, content_height);
        }

        egui::ScrollArea::both()
            .id_salt(("composite_timeline", self.loop_id))
            .auto_shrink([false, false])
            .scroll_source(crate::control_safe_scroll_source())
            .show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(content_width, content_height),
                    egui::Sense::hover(),
                );
                let clip_rect = ui.clip_rect();
                let painter = ui.painter_at(rect);
                let timeline_left = rect.left() + TRACK_LABEL_WIDTH;
                let visible_timeline_left = (clip_rect.left().max(rect.left()) + TRACK_LABEL_WIDTH)
                    .max(timeline_left)
                    .min(clip_rect.right());
                let timeline_clip_rect = egui::Rect::from_min_max(
                    egui::pos2(visible_timeline_left, clip_rect.top()),
                    clip_rect.max,
                );
                #[cfg(test)]
                {
                    self.timeline_rect = Some(rect);
                    self.timeline_left = Some(timeline_left);
                }
                let cycle_width = self.cycle_width;
                let frame_to_x = |frame: u64| {
                    let cycle_length = details.cycle_length_frames.max(1) as f64;
                    timeline_left + (frame as f64 / cycle_length * f64::from(cycle_width)) as f32
                };

                painter.rect_filled(rect, 0.0, colors::WAVEFORM_BACKGROUND);
                self.paint_grid(&painter, clip_rect, rect, timeline_left, displayed_cycles);

                if details.events.is_empty() {
                    painter.text(
                        egui::pos2(
                            timeline_left + timeline_width * 0.5,
                            rect.top() + HEADER_HEIGHT + TRACK_GAP + rows_height * 0.5,
                        ),
                        egui::Align2::CENTER_CENTER,
                        "The composite schedule is empty. Drag a loop here to add it.",
                        egui::FontId::proportional(12.0),
                        colors::MUTED_FOREGROUND,
                    );
                }

                let mut event_rects = Vec::with_capacity(details.events.len());
                let mut clicked_event = None;
                let mut context_event = None;
                let mut delete_requested = false;
                let mut length_request = None;
                let mut mode_request = None;
                let mut row_top = rect.top() + HEADER_HEIGHT + TRACK_GAP;
                for (track_state, track_layout) in details.tracks.iter().zip(&packed) {
                    let height = track_height(track_layout.lane_count);
                    #[cfg(test)]
                    self.rendered_track_heights.push(height);
                    let row_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.left(), row_top),
                        egui::vec2(content_width, height),
                    );
                    painter.rect_filled(
                        row_rect,
                        0.0,
                        if track_state.id.raw() % 2 == 0 {
                            egui::Color32::from_gray(36)
                        } else {
                            egui::Color32::from_gray(42)
                        },
                    );
                    for packed_event in &track_layout.events {
                        let event = &details.events[packed_event.event_index];
                        let top = row_top + packed_event.lane as f32 * (LANE_HEIGHT + LANE_GAP);
                        let left = frame_to_x(event.start_frame);
                        let right = frame_to_x(event.end_frame).max(left + 1.0);
                        let event_left = left + EVENT_INSET;
                        let event_right = (right - EVENT_INSET).max(event_left + 1.0);
                        let event_rect = egui::Rect::from_min_max(
                            egui::pos2(event_left, top + EVENT_INSET),
                            egui::pos2(event_right, top + LANE_HEIGHT - EVENT_INSET),
                        );
                        let event_key = CompositeEventKey::from(event);
                        event_rects.push((event_key, event_rect));
                        if event_rect.intersects(clip_rect) {
                            let selected = self.selected_events.contains(&event_key);
                            painter.rect_filled(event_rect, 2.0, event_color(event));
                            painter.rect_stroke(
                                event_rect,
                                2.0,
                                egui::Stroke::new(
                                    if selected { 2.0 } else { 1.0 },
                                    if selected {
                                        colors::LOOP_SELECTED_EDGE
                                    } else {
                                        colors::MUTED_FOREGROUND
                                    },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            if event_rect.intersects(timeline_clip_rect) {
                                let response = ui.interact(
                                    event_rect.intersect(timeline_clip_rect),
                                    ui.id().with(("composite_event", event_key.instance_id)),
                                    egui::Sense::click_and_drag(),
                                );
                                if response.clicked() {
                                    clicked_event = Some(event_key);
                                }
                                if response.secondary_clicked() {
                                    context_event = Some(event_key);
                                    self.force_length_cycles =
                                        event.forced_n_cycles.unwrap_or_else(|| {
                                            let cycle_length = details.cycle_length_frames.max(1);
                                            event
                                                .end_frame
                                                .saturating_sub(event.start_frame)
                                                .div_ceil(cycle_length)
                                                .max(1)
                                                .try_into()
                                                .unwrap_or(u32::MAX)
                                        });
                                }
                                let event_drag_started = response.drag_started()
                                    && ui.input(|input| {
                                        input
                                            .pointer
                                            .press_origin()
                                            .is_some_and(|origin| event_rect.contains(origin))
                                    });
                                if event_drag_started {
                                    if !self.selected_events.contains(&event_key) {
                                        self.selected_events.clear();
                                        self.selected_events.insert(event_key);
                                    }
                                    response.dnd_set_drag_payload(CompositeEventDragPayload {
                                        events: self
                                            .selected_events
                                            .iter()
                                            .copied()
                                            .map(CompositeEventId::from)
                                            .collect(),
                                        grabbed_offset: event.start_frame.saturating_sub(
                                            details
                                                .events
                                                .iter()
                                                .filter(|candidate| {
                                                    self.selected_events.contains(
                                                        &CompositeEventKey::from(*candidate),
                                                    )
                                                })
                                                .map(|candidate| candidate.start_frame)
                                                .min()
                                                .unwrap_or(event.start_frame),
                                        ) / details.cycle_length_frames.max(1),
                                    });
                                }
                                response.context_menu(|ui| {
                                    if details.kind == crate::CompositeKind::Script {
                                        ui.label("Mode");
                                        for (mode, label) in SCRIPT_EVENT_MODES {
                                            let selected = event.mode.as_deref()
                                                == Some(script_mode_key(mode));
                                            let response = ui.selectable_label(selected, label);
                                            #[cfg(test)]
                                            self.mode_menu_rects.push((mode, response.rect));
                                            if response.clicked() {
                                                mode_request = Some((event_key, mode));
                                                ui.close();
                                            }
                                        }
                                        ui.separator();
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label("Length");
                                        ui.add(
                                            egui::DragValue::new(&mut self.force_length_cycles)
                                                .range(1..=u32::MAX)
                                                .suffix(" cycles"),
                                        );
                                    });
                                    let force = ui.button("Force instance length");
                                    #[cfg(test)]
                                    {
                                        self.force_length_menu_rect = Some(force.rect);
                                    }
                                    if force.clicked() {
                                        length_request = Some((
                                            event_key,
                                            Some(self.force_length_cycles.max(1)),
                                        ));
                                        ui.close();
                                    }
                                    if event.forced_n_cycles.is_some() {
                                        let natural = ui.button("Use natural instance length");
                                        #[cfg(test)]
                                        {
                                            self.natural_length_menu_rect = Some(natural.rect);
                                        }
                                        if natural.clicked() {
                                            length_request = Some((event_key, None));
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    let delete = ui.button("Delete");
                                    #[cfg(test)]
                                    {
                                        self.delete_menu_rect = Some(delete.rect);
                                    }
                                    if delete.clicked() {
                                        delete_requested = true;
                                        ui.close();
                                    }
                                });
                            }
                            #[cfg(test)]
                            if selected {
                                self.rendered_selected_event_count += 1;
                            }
                            if event_rect.width() >= 10.0 {
                                let label_left = if event.forced_n_cycles.is_some() {
                                    let icon_rect = egui::Rect::from_min_size(
                                        event_rect.left_center() + egui::vec2(4.0, -7.0),
                                        egui::vec2(14.0, 14.0),
                                    );
                                    painter.text(
                                        icon_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        ICON_LOCK_CLOCK.codepoint,
                                        egui::FontId::new(
                                            13.0,
                                            egui::FontFamily::Name(
                                                egui_material_icons::FONT_FAMILY.into(),
                                            ),
                                        ),
                                        colors::FOREGROUND,
                                    );
                                    icon_rect.right() + 2.0
                                } else {
                                    event_rect.left()
                                };
                                let label_rect = egui::Rect::from_min_max(
                                    egui::pos2(label_left, event_rect.top()),
                                    event_rect.right_bottom(),
                                );
                                let label = if details.kind == crate::CompositeKind::Script {
                                    format!(
                                        "{} · {}",
                                        event.loop_name,
                                        script_mode_label(event.mode.as_deref())
                                    )
                                } else {
                                    event.loop_name.clone()
                                };
                                painter.with_clip_rect(event_rect.shrink(3.0)).text(
                                    label_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    label,
                                    egui::FontId::proportional(12.0),
                                    colors::FOREGROUND,
                                );
                            }
                        }
                        #[cfg(test)]
                        self.rendered_events.push((
                            event.loop_name.clone(),
                            event_rect,
                            packed_event.lane,
                        ));
                    }
                    let sticky_left = clip_rect.left().max(rect.left());
                    let label_rect = egui::Rect::from_min_size(
                        egui::pos2(sticky_left, row_top),
                        egui::vec2(TRACK_LABEL_WIDTH, height),
                    );
                    painter.rect_filled(label_rect, 0.0, colors::CONTROL_BACKGROUND);
                    painter.text(
                        label_rect.left_center() + egui::vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &track_state.name,
                        egui::FontId::proportional(12.0),
                        colors::FOREGROUND,
                    );
                    painter.vline(
                        label_rect.right(),
                        label_rect.y_range(),
                        egui::Stroke::new(1.0, colors::MUTED_FOREGROUND),
                    );
                    row_top += height + TRACK_GAP;
                }

                if let Some(event) = clicked_event {
                    if ui.input(|input| input.modifiers.ctrl) {
                        if !self.selected_events.remove(&event) {
                            self.selected_events.insert(event);
                        }
                    } else {
                        self.selected_events.clear();
                        self.selected_events.insert(event);
                    }
                    ui.ctx().request_repaint();
                }
                if let Some(event) = context_event {
                    if !self.selected_events.contains(&event) {
                        self.selected_events.clear();
                        self.selected_events.insert(event);
                    }
                    ui.ctx().request_repaint();
                }
                if let Some((event, n_cycles)) = length_request {
                    intents.push(AppIntent::SetCompositeLoopCycles {
                        target_loop_id: self.loop_id,
                        event: event.into(),
                        n_cycles,
                    });
                }
                if let Some((event, mode)) = mode_request {
                    intents.push(AppIntent::SetCompositeEventMode {
                        target_loop_id: self.loop_id,
                        event: event.into(),
                        mode,
                    });
                }
                if !self.selected_events.is_empty()
                    && (delete_requested || ui.input(|input| input.key_pressed(egui::Key::Delete)))
                {
                    intents.push(AppIntent::DeleteCompositeEvents {
                        target_loop_id: self.loop_id,
                        events: self
                            .selected_events
                            .iter()
                            .copied()
                            .map(CompositeEventId::from)
                            .collect(),
                    });
                    self.selected_events.clear();
                    ui.ctx().request_repaint();
                }

                let loop_payload = egui::DragAndDrop::payload::<LoopDragPayload>(ui.ctx());
                let event_payload =
                    egui::DragAndDrop::payload::<CompositeEventDragPayload>(ui.ctx());
                self.update_box_selection(
                    ui,
                    &painter,
                    timeline_clip_rect,
                    rect,
                    visible_timeline_left,
                    &event_rects,
                    loop_payload.is_some() || event_payload.is_some(),
                );
                if let Some(played_frame) = details.played_frame {
                    let x = frame_to_x(played_frame);
                    if timeline_clip_rect.x_range().contains(x) {
                        painter.vline(
                            x,
                            rect.y_range(),
                            egui::Stroke::new(2.0, colors::WAVEFORM_PLAYHEAD),
                        );
                        #[cfg(test)]
                        {
                            self.playhead_x = Some(x);
                        }
                    }
                }
                let pointer = ui.ctx().pointer_hover_pos();
                let valid_payload = loop_payload
                    .is_some_and(|payload| payload.loop_id != self.loop_id)
                    || event_payload.is_some();
                let hovered_iteration = pointer
                    .filter(|pointer| {
                        valid_payload
                            && rect.contains(*pointer)
                            && clip_rect.contains(*pointer)
                            && pointer.x >= timeline_left
                    })
                    .map(|pointer| ((pointer.x - timeline_left) / self.cycle_width).floor() as u64)
                    .filter(|iteration| *iteration < displayed_cycles);
                if let Some(iteration) = hovered_iteration {
                    let column_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            timeline_left + iteration as f32 * self.cycle_width,
                            rect.top(),
                        ),
                        egui::vec2(self.cycle_width, content_height),
                    )
                    .intersect(clip_rect);
                    painter.rect_stroke(
                        column_rect,
                        0.0,
                        egui::Stroke::new(2.0, colors::LOOP_TARGET_EDGE),
                        egui::StrokeKind::Inside,
                    );
                    #[cfg(test)]
                    {
                        self.highlighted_iteration = Some(iteration);
                        self.highlighted_rect = Some(column_rect);
                    }
                }
                if ui.input(|input| input.pointer.any_released()) {
                    if let Some(payload) = event_payload.zip(hovered_iteration) {
                        intents.push(AppIntent::RelocateCompositeEvents {
                            target_loop_id: self.loop_id,
                            events: payload.0.events.clone(),
                            start_iteration: payload.1.saturating_sub(payload.0.grabbed_offset),
                            duplicate: ui.input(|input| input.modifiers.ctrl),
                        });
                        if !ui.input(|input| input.modifiers.ctrl) {
                            self.selected_events.clear();
                        }
                        egui::DragAndDrop::clear_payload(ui.ctx());
                    }
                }
                hovered_iteration
            })
            .inner
    }

    #[cfg(test)]
    pub(crate) fn shown_loop_id(&self) -> LoopId {
        self.loop_id
    }

    #[cfg(test)]
    pub(crate) fn rendered_event_count(&self) -> usize {
        self.rendered_events.len()
    }

    fn update_box_selection(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        timeline_clip_rect: egui::Rect,
        content_rect: egui::Rect,
        visible_timeline_left: f32,
        event_rects: &[(CompositeEventKey, egui::Rect)],
        loop_drag_active: bool,
    ) {
        let (pressed, released, down, pointer, press_origin, modifiers) = ui.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
                input.pointer.press_origin(),
                input.modifiers,
            )
        });
        if pressed && !loop_drag_active {
            if let Some(origin) = press_origin.filter(|origin| {
                content_rect.contains(*origin)
                    && timeline_clip_rect.contains(*origin)
                    && origin.x >= visible_timeline_left
                    && event_rects.iter().all(|(_, rect)| !rect.contains(*origin))
            }) {
                self.box_selection = Some(BoxSelection {
                    origin,
                    mode: if modifiers.alt {
                        BoxSelectionMode::Remove
                    } else if modifiers.ctrl {
                        BoxSelectionMode::Add
                    } else {
                        BoxSelectionMode::Replace
                    },
                });
            }
        }

        let Some(selection) = self.box_selection else {
            return;
        };
        if !down && !released {
            self.box_selection = None;
            return;
        }
        let current = pointer.unwrap_or(selection.origin);
        let current = egui::pos2(
            current
                .x
                .clamp(visible_timeline_left, timeline_clip_rect.right()),
            current.y.clamp(
                content_rect.top().max(timeline_clip_rect.top()),
                timeline_clip_rect.bottom(),
            ),
        );
        let selection_rect = egui::Rect::from_two_pos(selection.origin, current);
        if down {
            painter.rect_filled(
                selection_rect,
                0.0,
                colors::COLORED_HIGHLIGHT.gamma_multiply(0.2),
            );
            painter.rect_stroke(
                selection_rect,
                0.0,
                egui::Stroke::new(1.0, colors::COLORED_HIGHLIGHT),
                egui::StrokeKind::Inside,
            );
            #[cfg(test)]
            {
                self.box_selection_rect = Some(selection_rect);
            }
        }
        if released {
            let enclosed = event_rects
                .iter()
                .filter_map(|(event, rect)| selection_rect.contains_rect(*rect).then_some(*event))
                .collect::<BTreeSet<_>>();
            match selection.mode {
                BoxSelectionMode::Replace => self.selected_events = enclosed,
                BoxSelectionMode::Add => self.selected_events.extend(enclosed),
                BoxSelectionMode::Remove => {
                    self.selected_events
                        .retain(|event| !enclosed.contains(event));
                }
            }
            self.box_selection = None;
            ui.ctx().request_repaint();
        }
    }

    fn paint_grid(
        &self,
        painter: &egui::Painter,
        clip_rect: egui::Rect,
        rect: egui::Rect,
        timeline_left: f32,
        timeline_cycles: u64,
    ) {
        let first =
            (((clip_rect.left() - timeline_left) / self.cycle_width).floor() as i64).max(0) as u64;
        let last =
            (((clip_rect.right() - timeline_left) / self.cycle_width).ceil() as i64).max(0) as u64;
        for cycle in first..=last.min(timeline_cycles) {
            let x = timeline_left + cycle as f32 * self.cycle_width;
            painter.vline(
                x,
                rect.top()..=rect.bottom(),
                egui::Stroke::new(1.0, egui::Color32::from_gray(64)),
            );
            if cycle < timeline_cycles {
                painter.text(
                    egui::pos2(x + self.cycle_width * 0.5, rect.top() + HEADER_HEIGHT * 0.5),
                    egui::Align2::CENTER_CENTER,
                    (cycle + 1).to_string(),
                    egui::FontId::proportional(11.0),
                    colors::MUTED_FOREGROUND,
                );
            }
        }
        let sticky_left = clip_rect.left().max(rect.left());
        let label_rect = egui::Rect::from_min_size(
            egui::pos2(sticky_left, rect.top()),
            egui::vec2(TRACK_LABEL_WIDTH, HEADER_HEIGHT),
        );
        painter.rect_filled(label_rect, 0.0, colors::CONTROL_BACKGROUND);
        painter.text(
            label_rect.left_center() + egui::vec2(6.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "Track / cycle",
            egui::FontId::proportional(11.0),
            colors::MUTED_FOREGROUND,
        );
    }
}

fn timeline_cycles(details: &CompositeDetailsState) -> u64 {
    let cycle_length = details.cycle_length_frames.max(1);
    details
        .timeline_length_frames
        .max(1)
        .div_ceil(cycle_length)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompositeKind, CompositeTrackDetailsState};

    fn event(
        loop_id: u64,
        track_id: u64,
        start_frame: u64,
        end_frame: u64,
    ) -> CompositeEventDetailsState {
        CompositeEventDetailsState {
            loop_id: LoopId::from_raw(loop_id),
            loop_name: format!("Loop {loop_id}"),
            track_id: TrackId::from_raw(track_id),
            start_frame,
            end_frame,
            ..Default::default()
        }
    }

    fn details(events: Vec<CompositeEventDetailsState>) -> CompositeDetailsState {
        CompositeDetailsState {
            kind: CompositeKind::Regular,
            cycle_length_frames: 100,
            timeline_length_frames: events
                .iter()
                .map(|event| event.end_frame)
                .max()
                .unwrap_or(0),
            played_frame: None,
            tracks: vec![
                CompositeTrackDetailsState {
                    id: TrackId::from_raw(1),
                    name: "One".to_owned(),
                },
                CompositeTrackDetailsState {
                    id: TrackId::from_raw(2),
                    name: "Two".to_owned(),
                },
            ],
            events,
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn overlap_containment_equal_starts_and_touching_edges_pack_deterministically() {
        let state = details(vec![
            event(4, 1, 20, 40),
            event(1, 1, 0, 100),
            event(3, 1, 100, 120),
            event(2, 1, 0, 50),
            event(5, 2, 10, 20),
        ]);
        let first = pack_swimlanes(&state);
        let second = pack_swimlanes(&state);
        assert_eq!(first, second);
        assert_eq!(first[0].lane_count, 3);
        assert_eq!(first[1].lane_count, 1);
        let lanes = first[0]
            .events
            .iter()
            .map(|packed| (state.events[packed.event_index].loop_id.raw(), packed.lane))
            .collect::<Vec<_>>();
        assert_eq!(lanes, [(2, 0), (1, 1), (4, 2), (3, 0)]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn independent_duplicate_spans_get_separate_lanes_and_empty_tracks_keep_one() {
        let state = details(vec![event(1, 1, 0, 10), event(1, 1, 0, 10)]);
        let packed = pack_swimlanes(&state);
        assert_eq!(packed[0].lane_count, 2);
        assert_eq!(packed[1].lane_count, 1);
        assert_eq!(track_height(2), LANE_HEIGHT * 2.0 + LANE_GAP);
    }

    fn keyed_event(
        key: u32,
        loop_id: u64,
        track_id: u64,
        start_frame: u64,
        end_frame: u64,
    ) -> CompositeEventDetailsState {
        CompositeEventDetailsState {
            instance_id: u64::from(key) + 1,
            ..event(loop_id, track_id, start_frame, end_frame)
        }
    }

    fn widget_frame(
        context: &egui::Context,
        widget: &mut CompositeLoopWidget,
        loop_id: LoopId,
        state: &CompositeDetailsState,
        mut events: Vec<egui::Event>,
    ) -> Vec<AppIntent> {
        let modifiers = events.iter().rev().find_map(|event| match event {
            egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
            _ => None,
        });
        if let Some(modifiers) = modifiers {
            events.insert(0, egui::Event::ModifiersChanged(modifiers));
        }
        let mut intents = Vec::new();
        let mut ignored_output_0 = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500.0, 220.0),
                )),
                events,
                ..Default::default()
            },
            |ui| intents = widget.show(ui, loop_id, state),
        );
        ignored_output_0.textures_delta.clear();
        intents
    }

    fn click(
        context: &egui::Context,
        widget: &mut CompositeLoopWidget,
        loop_id: LoopId,
        state: &CompositeDetailsState,
        position: egui::Pos2,
        modifiers: egui::Modifiers,
    ) {
        let _ = widget_frame(
            context,
            widget,
            loop_id,
            state,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
            ],
        );
        let _ = widget_frame(
            context,
            widget,
            loop_id,
            state,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
    }

    fn box_select(
        context: &egui::Context,
        widget: &mut CompositeLoopWidget,
        loop_id: LoopId,
        state: &CompositeDetailsState,
        rect: egui::Rect,
        modifiers: egui::Modifiers,
    ) {
        let _ = widget_frame(
            context,
            widget,
            loop_id,
            state,
            vec![
                egui::Event::PointerMoved(rect.min),
                egui::Event::PointerButton {
                    pos: rect.min,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
            ],
        );
        let _ = widget_frame(
            context,
            widget,
            loop_id,
            state,
            vec![egui::Event::PointerMoved(rect.max)],
        );
        assert_eq!(widget.box_selection_rect, Some(rect));
        let _ = widget_frame(
            context,
            widget,
            loop_id,
            state,
            vec![egui::Event::PointerButton {
                pos: rect.max,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
        assert!(widget.box_selection.is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn clicks_replace_or_toggle_block_selection_and_selected_blocks_are_highlighted() {
        let context = egui::Context::default();
        let loop_id = LoopId::from_raw(8);
        let state = details(vec![
            keyed_event(0, 1, 1, 20, 50),
            keyed_event(1, 2, 1, 60, 90),
            keyed_event(2, 3, 2, 20, 50),
        ]);
        let mut widget = CompositeLoopWidget::default();
        let _ = widget_frame(&context, &mut widget, loop_id, &state, Vec::new());
        let event_rects = widget
            .rendered_events
            .iter()
            .map(|event| event.1)
            .collect::<Vec<_>>();

        click(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[0].center(),
            egui::Modifiers::NONE,
        );
        assert_eq!(
            widget.selected_events,
            BTreeSet::from([CompositeEventKey { instance_id: 1 }])
        );

        click(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[1].center(),
            egui::Modifiers::CTRL,
        );
        assert_eq!(widget.selected_events.len(), 2);
        click(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[0].center(),
            egui::Modifiers::CTRL,
        );
        assert_eq!(
            widget.selected_events,
            BTreeSet::from([CompositeEventKey { instance_id: 2 }])
        );

        click(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[2].center(),
            egui::Modifiers::NONE,
        );
        let _ = widget_frame(&context, &mut widget, loop_id, &state, Vec::new());
        assert_eq!(widget.selected_events.len(), 1);
        assert_eq!(widget.rendered_selected_event_count, 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn box_selection_replaces_adds_removes_and_requires_full_containment() {
        let context = egui::Context::default();
        let loop_id = LoopId::from_raw(8);
        let state = details(vec![
            keyed_event(0, 1, 1, 20, 50),
            keyed_event(1, 2, 1, 60, 90),
            keyed_event(2, 3, 2, 20, 50),
        ]);
        let mut widget = CompositeLoopWidget::default();
        let _ = widget_frame(&context, &mut widget, loop_id, &state, Vec::new());
        let event_rects = widget
            .rendered_events
            .iter()
            .map(|event| event.1)
            .collect::<Vec<_>>();

        box_select(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[0].union(event_rects[1]).expand(0.5),
            egui::Modifiers::NONE,
        );
        assert_eq!(widget.selected_events.len(), 2);

        box_select(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[2].expand(0.5),
            egui::Modifiers::CTRL,
        );
        assert_eq!(widget.selected_events.len(), 3);

        box_select(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[0].expand(0.5),
            egui::Modifiers::ALT,
        );
        assert_eq!(widget.selected_events.len(), 2);
        assert!(!widget
            .selected_events
            .contains(&CompositeEventKey { instance_id: 1 }));

        let partial = egui::Rect::from_min_max(
            event_rects[1].min - egui::vec2(0.5, 0.5),
            egui::pos2(event_rects[1].center().x, event_rects[1].bottom() + 0.5),
        );
        box_select(
            &context,
            &mut widget,
            loop_id,
            &state,
            partial,
            egui::Modifiers::NONE,
        );
        assert!(widget.selected_events.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn delete_key_emits_selected_event_ids_and_clears_the_editor_selection() {
        let context = egui::Context::default();
        let loop_id = LoopId::from_raw(8);
        let state = details(vec![
            keyed_event(0, 1, 1, 20, 50),
            keyed_event(1, 2, 1, 60, 90),
        ]);
        let mut widget = CompositeLoopWidget::default();
        let _ = widget_frame(&context, &mut widget, loop_id, &state, Vec::new());
        let event_rects = widget
            .rendered_events
            .iter()
            .map(|event| event.1)
            .collect::<Vec<_>>();
        click(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[0].center(),
            egui::Modifiers::NONE,
        );
        click(
            &context,
            &mut widget,
            loop_id,
            &state,
            event_rects[1].center(),
            egui::Modifiers::CTRL,
        );

        let intents = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &state,
            vec![egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            intents,
            [AppIntent::DeleteCompositeEvents {
                target_loop_id: loop_id,
                events: vec![
                    CompositeEventId { instance_id: 1 },
                    CompositeEventId { instance_id: 2 },
                ],
            }]
        );
        assert!(widget.selected_events.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn event_context_menu_selects_the_block_and_emits_delete() {
        let context = egui::Context::default();
        let loop_id = LoopId::from_raw(8);
        let state = details(vec![keyed_event(3, 1, 1, 20, 50)]);
        let mut widget = CompositeLoopWidget::default();
        let _ = widget_frame(&context, &mut widget, loop_id, &state, Vec::new());
        let event_center = widget.rendered_events[0].1.center();

        let _ = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &state,
            vec![
                egui::Event::PointerMoved(event_center),
                egui::Event::PointerButton {
                    pos: event_center,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let _ = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &state,
            vec![egui::Event::PointerButton {
                pos: event_center,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(widget.selected_events.len(), 1);
        let _ = widget_frame(&context, &mut widget, loop_id, &state, Vec::new());
        let delete_center = widget.delete_menu_rect.unwrap().center();
        let _ = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &state,
            vec![
                egui::Event::PointerMoved(delete_center),
                egui::Event::PointerButton {
                    pos: delete_center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(widget
            .delete_menu_rect
            .is_some_and(|rect| rect.contains(delete_center)));
        let intents = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &state,
            vec![
                egui::Event::PointerMoved(delete_center),
                egui::Event::PointerButton {
                    pos: delete_center,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(
            intents,
            [AppIntent::DeleteCompositeEvents {
                target_loop_id: loop_id,
                events: vec![CompositeEventId { instance_id: 4 }],
            }]
        );
        assert!(widget.selected_events.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn event_context_menu_forces_and_restores_natural_length() {
        let context = egui::Context::default();
        crate::fonts::initialize(&context);
        let loop_id = LoopId::from_raw(8);
        let mut forced = keyed_event(3, 1, 1, 20, 220);
        forced.forced_n_cycles = Some(2);
        let state = details(vec![forced]);
        let mut widget = CompositeLoopWidget::default();
        let _ = widget_frame(&context, &mut widget, loop_id, &state, Vec::new());
        let event_center = widget.rendered_events[0].1.center();

        for pressed in [true, false] {
            let _ = widget_frame(
                &context,
                &mut widget,
                loop_id,
                &state,
                vec![
                    egui::Event::PointerMoved(event_center),
                    egui::Event::PointerButton {
                        pos: event_center,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let _ = widget_frame(&context, &mut widget, loop_id, &state, Vec::new());
        let natural_center = widget.natural_length_menu_rect.unwrap().center();
        let _ = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &state,
            vec![
                egui::Event::PointerMoved(natural_center),
                egui::Event::PointerButton {
                    pos: natural_center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let intents = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &state,
            vec![egui::Event::PointerButton {
                pos: natural_center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            intents,
            [AppIntent::SetCompositeLoopCycles {
                target_loop_id: loop_id,
                event: CompositeEventId { instance_id: 4 },
                n_cycles: None,
            }]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn timeline_accepts_typed_loop_drops_and_ignores_self_or_outside_drops() {
        let context = egui::Context::default();
        let mut widget = CompositeLoopWidget::default();
        let target = LoopId::from_raw(8);
        let source = LoopId::from_raw(9);
        let state = details(Vec::new());
        let _ = widget_frame(&context, &mut widget, target, &state, Vec::new());
        let drop_center = egui::pos2(
            widget.timeline_left.unwrap() + DEFAULT_CYCLE_WIDTH * 1.5,
            widget.timeline_rect.unwrap().center().y,
        );

        egui::DragAndDrop::set_payload(&context, LoopDragPayload { loop_id: source });
        let _ = widget_frame(
            &context,
            &mut widget,
            target,
            &state,
            vec![
                egui::Event::PointerMoved(drop_center),
                egui::Event::PointerButton {
                    pos: drop_center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(widget.highlighted_iteration, Some(1));
        assert!(widget.highlighted_rect.unwrap().contains(drop_center));
        let intents = widget_frame(
            &context,
            &mut widget,
            target,
            &state,
            vec![egui::Event::PointerButton {
                pos: drop_center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            intents,
            [AppIntent::ComposeLoopAt {
                target_loop_id: target,
                source_loop_id: source,
                start_iteration: 1,
            }]
        );

        for (payload, position) in [
            (LoopDragPayload { loop_id: target }, drop_center),
            (
                LoopDragPayload { loop_id: source },
                egui::pos2(490.0, 210.0),
            ),
        ] {
            egui::DragAndDrop::set_payload(&context, payload);
            let _ = widget_frame(
                &context,
                &mut widget,
                target,
                &state,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            let intents = widget_frame(
                &context,
                &mut widget,
                target,
                &state,
                vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(intents.is_empty());
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn selected_composite_events_drop_as_a_group_and_ctrl_duplicates() {
        let context = egui::Context::default();
        let target = LoopId::from_raw(8);
        let state = details(vec![
            keyed_event(0, 1, 1, 0, 100),
            keyed_event(1, 2, 1, 200, 300),
        ]);
        let mut widget = CompositeLoopWidget::default();
        let _ = widget_frame(&context, &mut widget, target, &state, Vec::new());
        let events = vec![
            CompositeEventId { instance_id: 1 },
            CompositeEventId { instance_id: 2 },
        ];
        let drop_position = egui::pos2(
            widget.timeline_left.unwrap() + DEFAULT_CYCLE_WIDTH * 2.5,
            widget.timeline_rect.unwrap().center().y,
        );

        for (modifiers, duplicate) in [
            (egui::Modifiers::NONE, false),
            (egui::Modifiers::CTRL, true),
        ] {
            widget.selected_events = events
                .iter()
                .map(|event| CompositeEventKey {
                    instance_id: event.instance_id,
                })
                .collect();
            egui::DragAndDrop::set_payload(
                &context,
                CompositeEventDragPayload {
                    events: events.clone(),
                    grabbed_offset: 1,
                },
            );
            let _ = widget_frame(
                &context,
                &mut widget,
                target,
                &state,
                vec![
                    egui::Event::PointerMoved(drop_position),
                    egui::Event::PointerButton {
                        pos: drop_position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    },
                ],
            );
            assert_eq!(widget.highlighted_iteration, Some(2));
            let intents = widget_frame(
                &context,
                &mut widget,
                target,
                &state,
                vec![egui::Event::PointerButton {
                    pos: drop_position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                }],
            );
            assert_eq!(
                intents,
                [AppIntent::RelocateCompositeEvents {
                    target_loop_id: target,
                    events: events.clone(),
                    start_iteration: 1,
                    duplicate,
                }]
            );
            assert_eq!(widget.selected_events.is_empty(), !duplicate);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn empty_composite_paints_an_explicit_schedule_message() {
        let context = egui::Context::default();
        let mut widget = CompositeLoopWidget::default();
        let mut output = context.run_ui(Default::default(), |ui| {
            widget.show(ui, LoopId::from_raw(8), &details(Vec::new()));
        });
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Text(text) => text.galley.job.text.contains("schedule is empty"),
            _ => false,
        }));
        assert!(widget.rendered_events.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn forced_length_events_paint_a_lock_clock_icon() {
        let context = egui::Context::default();
        crate::fonts::initialize(&context);
        let mut forced = event(1, 1, 0, 100);
        forced.forced_n_cycles = Some(1);
        let mut output = context.run_ui(Default::default(), |ui| {
            CompositeLoopWidget::default().show(
                ui,
                LoopId::from_raw(8),
                &details(vec![forced.clone()]),
            );
        });
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Text(text) => text.galley.job.text == ICON_LOCK_CLOCK.codepoint,
            _ => false,
        }));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn current_position_is_painted_at_its_timeline_frame() {
        let context = egui::Context::default();
        let mut widget = CompositeLoopWidget::default();
        let mut state = details(vec![event(1, 1, 0, 300)]);
        state.played_frame = Some(150);

        widget_frame(
            &context,
            &mut widget,
            LoopId::from_raw(8),
            &state,
            Vec::new(),
        );

        assert_eq!(
            widget.playhead_x,
            Some(widget.timeline_left.unwrap() + DEFAULT_CYCLE_WIDTH * 1.5)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn widget_paints_named_events_grows_rows_and_has_bounded_zoomed_overflow() {
        let context = egui::Context::default();
        let state = details(vec![event(1, 1, 0, 300), event(2, 1, 100, 200)]);
        let mut widget = CompositeLoopWidget::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 180.0),
                )),
                ..Default::default()
            },
            |ui| {
                widget.show(ui, LoopId::from_raw(9), &state);
            },
        );
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert_eq!(
            widget
                .rendered_events
                .iter()
                .map(|event| event.0.as_str())
                .collect::<Vec<_>>(),
            ["Loop 1", "Loop 2"]
        );
        assert_eq!(widget.rendered_track_heights[0], track_height(2));
        assert!(widget.content_size.x > 360.0);
        let initial_width = widget.rendered_events[0].1.width();
        widget.cycle_width = DEFAULT_CYCLE_WIDTH * 2.0;
        let mut ignored_output_1 = context.run_ui(Default::default(), |ui| {
            widget.show(ui, LoopId::from_raw(9), &state);
        });
        ignored_output_1.textures_delta.clear();
        assert!(widget.rendered_events[0].1.width() > initial_width * 1.9);
        widget.cycle_width = MAX_CYCLE_WIDTH + 100.0;
        let mut ignored_output_2 = context.run_ui(Default::default(), |ui| {
            widget.show(ui, LoopId::from_raw(9), &state);
        });
        ignored_output_2.textures_delta.clear();
        assert!(widget.cycle_width <= MAX_CYCLE_WIDTH);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn narrow_and_wide_panes_report_horizontal_and_vertical_overflow() {
        let tracks = (1..=8)
            .map(|id| CompositeTrackDetailsState {
                id: TrackId::from_raw(id),
                name: format!("Track {id}"),
            })
            .collect::<Vec<_>>();
        let events = (1..=8)
            .flat_map(|track| {
                [
                    event(track * 2, track, 0, 1_000),
                    event(track * 2 + 1, track, 100, 900),
                ]
            })
            .collect::<Vec<_>>();
        let state = CompositeDetailsState {
            kind: CompositeKind::Regular,
            cycle_length_frames: 100,
            timeline_length_frames: 1_000,
            played_frame: None,
            tracks,
            events,
        };
        for size in [egui::vec2(360.0, 150.0), egui::vec2(900.0, 300.0)] {
            let context = egui::Context::default();
            let mut widget = CompositeLoopWidget::default();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    widget.show(ui, LoopId::from_raw(10), &state);
                },
            );
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty());
            assert_eq!(widget.rendered_events.len(), 16);
            assert!(widget.content_size.x > size.x);
            assert!(widget.content_size.y > size.y);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn type_toggle_emits_kind_change_and_script_events_show_their_mode() {
        let context = egui::Context::default();
        let loop_id = LoopId::from_raw(8);
        let mut widget = CompositeLoopWidget::default();
        let regular = details(vec![event(1, 1, 0, 100)]);
        let _ = widget_frame(&context, &mut widget, loop_id, &regular, Vec::new());
        let script_center = widget.script_kind_rect.unwrap().center();
        let _ = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &regular,
            vec![
                egui::Event::PointerMoved(script_center),
                egui::Event::PointerButton {
                    pos: script_center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let intents = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &regular,
            vec![egui::Event::PointerButton {
                pos: script_center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            intents,
            [AppIntent::SetCompositeKind {
                target_loop_id: loop_id,
                kind: CompositeKind::Script,
            }]
        );

        let mut script = regular;
        script.kind = CompositeKind::Script;
        script.events[0].mode = Some("recording".to_owned());
        let mut output = context.run_ui(Default::default(), |ui| {
            widget.show(ui, loop_id, &script);
        });
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Text(text) => text.galley.job.text.contains("Loop 1 · Record"),
            _ => false,
        }));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn script_event_context_menu_emits_mode_change_for_one_instance() {
        let context = egui::Context::default();
        let loop_id = LoopId::from_raw(8);
        let mut script = details(vec![keyed_event(3, 1, 1, 20, 100)]);
        script.kind = CompositeKind::Script;
        script.events[0].mode = Some("playing".to_owned());
        let mut widget = CompositeLoopWidget::default();
        let _ = widget_frame(&context, &mut widget, loop_id, &script, Vec::new());
        let event_center = widget.rendered_events[0].1.center();
        for pressed in [true, false] {
            let _ = widget_frame(
                &context,
                &mut widget,
                loop_id,
                &script,
                vec![
                    egui::Event::PointerMoved(event_center),
                    egui::Event::PointerButton {
                        pos: event_center,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let _ = widget_frame(&context, &mut widget, loop_id, &script, Vec::new());
        let record_center = widget
            .mode_menu_rects
            .iter()
            .find(|(mode, _)| *mode == LoopMode::Recording)
            .unwrap()
            .1
            .center();
        let _ = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &script,
            vec![
                egui::Event::PointerMoved(record_center),
                egui::Event::PointerButton {
                    pos: record_center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let intents = widget_frame(
            &context,
            &mut widget,
            loop_id,
            &script,
            vec![egui::Event::PointerButton {
                pos: record_center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            intents,
            [AppIntent::SetCompositeEventMode {
                target_loop_id: loop_id,
                event: CompositeEventId { instance_id: 4 },
                mode: LoopMode::Recording,
            }]
        );
    }
}
