use crate::{
    colors, AppIntent, CompositeDetailsState, CompositeEventDetailsState, LoopId, LoopMode, TrackId,
};
use std::collections::BTreeMap;

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
                    event.playlist_index,
                    event.section_index,
                    event.parallel_index,
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

#[derive(Debug)]
pub struct CompositeLoopWidget {
    loop_id: LoopId,
    cycle_width: f32,
    #[cfg(test)]
    rendered_events: Vec<(String, egui::Rect, usize)>,
    #[cfg(test)]
    rendered_track_heights: Vec<f32>,
    #[cfg(test)]
    content_size: egui::Vec2,
    #[cfg(test)]
    drop_rect: Option<egui::Rect>,
}

impl Default for CompositeLoopWidget {
    fn default() -> Self {
        Self {
            loop_id: LoopId::INVALID,
            cycle_width: DEFAULT_CYCLE_WIDTH,
            #[cfg(test)]
            rendered_events: Vec::new(),
            #[cfg(test)]
            rendered_track_heights: Vec::new(),
            #[cfg(test)]
            content_size: egui::Vec2::ZERO,
            #[cfg(test)]
            drop_rect: None,
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
        }
        #[cfg(test)]
        {
            self.rendered_events.clear();
            self.rendered_track_heights.clear();
            self.content_size = egui::Vec2::ZERO;
            self.drop_rect = None;
        }

        let fit_width = (ui.available_width() - TRACK_LABEL_WIDTH).max(1.0);
        ui.horizontal(|ui| {
            ui.label(match details.kind {
                crate::CompositeKind::Regular => "Regular composition",
                crate::CompositeKind::Script => "Script composition",
                crate::CompositeKind::None => "Composition",
            });
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

        let (_drop_zone, dropped) = ui.dnd_drop_zone::<LoopDragPayload, _>(
            egui::Frame::new().inner_margin(egui::Margin::same(3)),
            |ui| {
                if details.events.is_empty() {
                    ui.add_sized(
                        [ui.available_width().max(160.0), 56.0],
                        egui::Label::new(
                            "The composite schedule is empty. Drag a loop here to add it.",
                        ),
                    );
                } else {
                    self.show_timeline(ui, details);
                }
            },
        );
        #[cfg(test)]
        {
            self.drop_rect = Some(_drop_zone.response.rect);
        }
        dropped
            .filter(|payload| payload.loop_id != loop_id)
            .map(|payload| {
                vec![AppIntent::ComposeLoopSerial {
                    target_loop_id: loop_id,
                    source_loop_id: payload.loop_id,
                }]
            })
            .unwrap_or_default()
    }

    fn show_timeline(&mut self, ui: &mut egui::Ui, details: &CompositeDetailsState) {
        let packed = pack_swimlanes(details);
        let timeline_cycles = timeline_cycles(details);
        let timeline_width = timeline_cycles as f32 * self.cycle_width;
        let content_width = (TRACK_LABEL_WIDTH + timeline_width)
            .max(ui.available_width())
            .max(TRACK_LABEL_WIDTH + 1.0);
        let rows_height = packed
            .iter()
            .map(|track| track_height(track.lane_count))
            .sum::<f32>()
            + packed.len().saturating_sub(1) as f32 * TRACK_GAP;
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
                let frame_to_x = |frame: u64| {
                    let cycle_length = details.cycle_length_frames.max(1) as f64;
                    timeline_left
                        + (frame as f64 / cycle_length * f64::from(self.cycle_width)) as f32
                };

                painter.rect_filled(rect, 0.0, colors::WAVEFORM_BACKGROUND);
                self.paint_grid(&painter, clip_rect, rect, timeline_left, timeline_cycles);

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
                        if event_rect.intersects(clip_rect) {
                            painter.rect_filled(event_rect, 2.0, event_color(event));
                            painter.rect_stroke(
                                event_rect,
                                2.0,
                                egui::Stroke::new(1.0, colors::MUTED_FOREGROUND),
                                egui::StrokeKind::Inside,
                            );
                            if event_rect.width() >= 10.0 {
                                painter.with_clip_rect(event_rect.shrink(3.0)).text(
                                    event_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &event.loop_name,
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
            });
    }

    #[cfg(test)]
    pub(crate) fn shown_loop_id(&self) -> LoopId {
        self.loop_id
    }

    #[cfg(test)]
    pub(crate) fn rendered_event_count(&self) -> usize {
        self.rendered_events.len()
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

    #[test]
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

    #[test]
    fn independent_duplicate_spans_get_separate_lanes_and_empty_tracks_keep_one() {
        let state = details(vec![event(1, 1, 0, 10), event(1, 1, 0, 10)]);
        let packed = pack_swimlanes(&state);
        assert_eq!(packed[0].lane_count, 2);
        assert_eq!(packed[1].lane_count, 1);
        assert_eq!(track_height(2), LANE_HEIGHT * 2.0 + LANE_GAP);
    }

    fn widget_frame(
        context: &egui::Context,
        widget: &mut CompositeLoopWidget,
        loop_id: LoopId,
        state: &CompositeDetailsState,
        events: Vec<egui::Event>,
    ) -> Vec<AppIntent> {
        let mut intents = Vec::new();
        let _ = context.run_ui(
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
        intents
    }

    #[test]
    fn timeline_accepts_typed_loop_drops_and_ignores_self_or_outside_drops() {
        let context = egui::Context::default();
        let mut widget = CompositeLoopWidget::default();
        let target = LoopId::from_raw(8);
        let source = LoopId::from_raw(9);
        let state = details(Vec::new());
        let _ = widget_frame(&context, &mut widget, target, &state, Vec::new());
        let drop_center = widget.drop_rect.unwrap().center();

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
            [AppIntent::ComposeLoopSerial {
                target_loop_id: target,
                source_loop_id: source,
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

    #[test]
    fn empty_composite_paints_an_explicit_schedule_message() {
        let context = egui::Context::default();
        let mut widget = CompositeLoopWidget::default();
        let output = context.run_ui(Default::default(), |ui| {
            widget.show(ui, LoopId::from_raw(8), &details(Vec::new()));
        });
        assert!(output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Text(text) => text.galley.job.text.contains("schedule is empty"),
            _ => false,
        }));
        assert!(widget.rendered_events.is_empty());
    }

    #[test]
    fn widget_paints_named_events_grows_rows_and_has_bounded_zoomed_overflow() {
        let context = egui::Context::default();
        let state = details(vec![event(1, 1, 0, 300), event(2, 1, 100, 200)]);
        let mut widget = CompositeLoopWidget::default();
        let output = context.run_ui(
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
        let _ = context.run_ui(Default::default(), |ui| {
            widget.show(ui, LoopId::from_raw(9), &state);
        });
        assert!(widget.rendered_events[0].1.width() > initial_width * 1.9);
        widget.cycle_width = MAX_CYCLE_WIDTH + 100.0;
        let _ = context.run_ui(Default::default(), |ui| {
            widget.show(ui, LoopId::from_raw(9), &state);
        });
        assert!(widget.cycle_width <= MAX_CYCLE_WIDTH);
    }

    #[test]
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
            tracks,
            events,
        };
        for size in [egui::vec2(360.0, 150.0), egui::vec2(900.0, 300.0)] {
            let context = egui::Context::default();
            let mut widget = CompositeLoopWidget::default();
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    widget.show(ui, LoopId::from_raw(10), &state);
                },
            );
            assert!(!output.shapes.is_empty());
            assert_eq!(widget.rendered_events.len(), 16);
            assert!(widget.content_size.x > size.x);
            assert!(widget.content_size.y > size.y);
        }
    }
}
