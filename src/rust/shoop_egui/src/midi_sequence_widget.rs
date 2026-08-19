use crate::{colors, MidiEventState, MidiSequenceChannelState};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MidiNoteSpan {
    start: u32,
    end: u32,
    note: u8,
    channel: u8,
    velocity: u8,
}

fn note_spans(events: &[MidiEventState], timeline_end: u32) -> Vec<MidiNoteSpan> {
    let mut active = BTreeMap::<(u8, u8), VecDeque<(u32, u8)>>::new();
    let mut notes = Vec::new();
    for event in events {
        let data = event.data.as_ref();
        if data.len() < 3 {
            continue;
        }
        let channel = data[0] & 0x0f;
        let note = data[1];
        if note > 127 {
            continue;
        }
        match data[0] & 0xf0 {
            0x90 if data[2] > 0 => {
                active
                    .entry((channel, note))
                    .or_default()
                    .push_back((event.frame, data[2]));
            }
            0x80 | 0x90 => {
                if let Some(starts) = active.get_mut(&(channel, note)) {
                    if let Some((start, velocity)) = starts.pop_front() {
                        notes.push(MidiNoteSpan {
                            start,
                            end: event.frame.max(start.saturating_add(1)),
                            note,
                            channel,
                            velocity,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    for ((channel, note), starts) in active {
        for (start, velocity) in starts {
            notes.push(MidiNoteSpan {
                start,
                end: timeline_end.max(start.saturating_add(1)),
                note,
                channel,
                velocity,
            });
        }
    }
    notes.sort_by_key(|note| (note.start, note.channel, note.note, note.end));
    notes
}

#[derive(Debug)]
pub struct MidiSequenceWidget {
    zoom: f32,
    offset: f64,
    source: Option<Arc<[MidiEventState]>>,
    parsed_timeline_end: u32,
    notes: Vec<MidiNoteSpan>,
}

impl Default for MidiSequenceWidget {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: 0.0,
            source: None,
            parsed_timeline_end: 0,
            notes: Vec::new(),
        }
    }
}

impl MidiSequenceWidget {
    pub fn show(&mut self, ui: &mut egui::Ui, channel: &MidiSequenceChannelState) {
        self.update_notes(channel);
        let desired = egui::vec2(ui.available_width(), 96.0);
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, colors::WAVEFORM_BACKGROUND);
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, colors::MUTED_FOREGROUND),
            egui::StrokeKind::Inside,
        );
        let event_end = self.notes.iter().map(|note| note.end).max().unwrap_or(0);
        let timeline_start = channel.start_offset.min(0) as f64;
        let timeline_end = channel
            .start_offset
            .saturating_add_unsigned(channel.loop_length)
            .max(i64::from(event_end))
            .max(1) as f64;
        let total_frames = (timeline_end - timeline_start).max(1.0);
        let visible_frames = (total_frames / f64::from(self.zoom)).max(1.0);
        let max_offset = (timeline_end - visible_frames).max(timeline_start);
        if response.dragged() {
            let frames_per_point = visible_frames / f64::from(rect.width().max(1.0));
            self.offset -= f64::from(response.drag_delta().x) * frames_per_point;
        }
        self.offset = self.offset.clamp(timeline_start, max_offset);
        let frame_to_x = |frame: f64| {
            rect.left() + ((frame - self.offset) / visible_frames) as f32 * rect.width()
        };

        let loop_left = frame_to_x(channel.start_offset as f64).clamp(rect.left(), rect.right());
        let loop_right = frame_to_x(
            channel
                .start_offset
                .saturating_add_unsigned(channel.loop_length) as f64,
        )
        .clamp(rect.left(), rect.right());
        if loop_right > loop_left {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(loop_left, rect.top()),
                    egui::pos2(loop_right, rect.bottom()),
                ),
                0.0,
                colors::WAVEFORM_LOOP_REGION,
            );
        }

        if self.notes.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No MIDI notes",
                egui::FontId::proportional(12.0),
                colors::MUTED_FOREGROUND,
            );
        } else {
            for note in &self.notes {
                let left = frame_to_x(f64::from(note.start)).clamp(rect.left(), rect.right());
                let right = frame_to_x(f64::from(note.end)).clamp(rect.left(), rect.right());
                if right <= rect.left() || left >= rect.right() {
                    continue;
                }
                let row_height = rect.height() / 128.0;
                let top = rect.top() + f32::from(127 - note.note) * row_height;
                let note_rect = egui::Rect::from_min_max(
                    egui::pos2(left, top),
                    egui::pos2(
                        (right.max(left + 1.0)).min(rect.right()),
                        top + row_height.max(2.0),
                    ),
                );
                let intensity = 96_u8.saturating_add(note.velocity / 2);
                painter.rect_filled(
                    note_rect,
                    1.0,
                    egui::Color32::from_rgb(0, intensity, intensity),
                );
            }
        }

        if let Some(played_sample) = channel.played_sample {
            let x = frame_to_x(played_sample as f64);
            if rect.x_range().contains(x) {
                painter.vline(
                    x,
                    rect.y_range(),
                    egui::Stroke::new(2.0, colors::WAVEFORM_PLAYHEAD),
                );
            }
        }
        let label_rect = egui::Rect::from_min_max(
            rect.left_top() + egui::vec2(6.0, 3.0),
            egui::pos2(rect.right() - 122.0, rect.top() + 22.0),
        );
        ui.put(label_rect, egui::Label::new(&channel.label).truncate());
        let zoom_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 116.0, rect.top() + 3.0),
            egui::vec2(110.0, 18.0),
        );
        ui.put(
            zoom_rect,
            egui::Slider::new(&mut self.zoom, 1.0..=64.0)
                .logarithmic(true)
                .show_value(false)
                .text("zoom"),
        )
        .on_hover_text(format!("MIDI zoom: {:.1}×", self.zoom));
    }

    fn update_notes(&mut self, channel: &MidiSequenceChannelState) {
        let timeline_end = channel
            .start_offset
            .saturating_add_unsigned(channel.loop_length)
            .clamp(0, i64::from(u32::MAX)) as u32;
        if self
            .source
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, &channel.events))
            && self.parsed_timeline_end == timeline_end
        {
            return;
        }
        self.notes = note_spans(&channel.events, timeline_end);
        self.source = Some(Arc::clone(&channel.events));
        self.parsed_timeline_end = timeline_end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(frame: u32, data: &[u8]) -> MidiEventState {
        MidiEventState {
            frame,
            data: Arc::from(data),
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pairs_notes_by_channel_and_treats_zero_velocity_as_off() {
        let events = [
            event(1, &[0x90, 60, 100]),
            event(2, &[0x91, 60, 90]),
            event(3, &[0x90, 60, 0]),
            event(5, &[0x81, 60, 64]),
        ];
        assert_eq!(
            note_spans(&events, 8),
            [
                MidiNoteSpan {
                    start: 1,
                    end: 3,
                    note: 60,
                    channel: 0,
                    velocity: 100,
                },
                MidiNoteSpan {
                    start: 2,
                    end: 5,
                    note: 60,
                    channel: 1,
                    velocity: 90,
                },
            ]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn overlapping_and_unmatched_messages_are_deterministic() {
        let events = [
            event(1, &[0x90, 64, 10]),
            event(2, &[0x90, 64, 20]),
            event(3, &[0x80, 64, 0]),
            event(4, &[0x80, 70, 0]),
            event(5, &[0x90]),
            event(6, &[0x90, 200, 100]),
        ];
        assert_eq!(
            note_spans(&events, 9),
            [
                MidiNoteSpan {
                    start: 1,
                    end: 3,
                    note: 64,
                    channel: 0,
                    velocity: 10,
                },
                MidiNoteSpan {
                    start: 2,
                    end: 9,
                    note: 64,
                    channel: 0,
                    velocity: 20,
                },
            ]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn widget_paints_empty_and_populated_sequences() {
        let context = egui::Context::default();
        let mut widget = MidiSequenceWidget::default();
        for events in [
            Arc::from([]),
            Arc::from([event(1, &[0x90, 60, 100]), event(10, &[0x80, 60, 0])]),
        ] {
            let output = context.run_ui(Default::default(), |ui| {
                widget.show(
                    ui,
                    &MidiSequenceChannelState {
                        label: "Dry MIDI 1".to_owned(),
                        events: Arc::clone(&events),
                        loop_length: 16,
                        ..Default::default()
                    },
                );
            });
            assert!(!output.shapes.is_empty());
        }
    }
}
