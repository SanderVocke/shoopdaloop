use egui::{pos2, vec2, Pos2, Rect, Vec2};

pub const MIDI_NOTE_COUNT: u8 = 128;
pub const MIDDLE_C: u8 = 60;
pub const WHITE_KEY_WIDTH: f32 = 24.0;
pub const WHITE_KEY_HEIGHT: f32 = 112.0;
pub const BLACK_KEY_WIDTH: f32 = 15.0;
pub const BLACK_KEY_HEIGHT: f32 = 70.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PianoLayout {
    origin: Pos2,
}

impl Default for PianoLayout {
    fn default() -> Self {
        Self::new(Pos2::ZERO)
    }
}

impl PianoLayout {
    pub const fn new(origin: Pos2) -> Self {
        Self { origin }
    }

    pub fn size(self) -> Vec2 {
        vec2(white_key_count() as f32 * WHITE_KEY_WIDTH, WHITE_KEY_HEIGHT)
    }

    pub fn key_rect(self, note: u8) -> Option<Rect> {
        if note >= MIDI_NOTE_COUNT {
            return None;
        }
        let white_before = (0..note).filter(|candidate| !is_black(*candidate)).count() as f32;
        if is_black(note) {
            let boundary = self.origin.x + white_before * WHITE_KEY_WIDTH;
            Some(Rect::from_min_size(
                pos2(boundary - BLACK_KEY_WIDTH * 0.5, self.origin.y),
                vec2(BLACK_KEY_WIDTH, BLACK_KEY_HEIGHT),
            ))
        } else {
            Some(Rect::from_min_size(
                pos2(
                    self.origin.x + white_before * WHITE_KEY_WIDTH,
                    self.origin.y,
                ),
                vec2(WHITE_KEY_WIDTH, WHITE_KEY_HEIGHT),
            ))
        }
    }

    pub fn note_at(self, position: Pos2) -> Option<u8> {
        if !Rect::from_min_size(self.origin, self.size()).contains(position) {
            return None;
        }
        (0..MIDI_NOTE_COUNT)
            .filter(|note| is_black(*note))
            .find(|note| {
                self.key_rect(*note)
                    .is_some_and(|rect| rect.contains(position))
            })
            .or_else(|| {
                (0..MIDI_NOTE_COUNT)
                    .filter(|note| !is_black(*note))
                    .find(|note| {
                        self.key_rect(*note)
                            .is_some_and(|rect| rect.contains(position))
                    })
            })
    }

    pub fn centered_offset(self, note: u8, viewport_width: f32) -> f32 {
        let center = self
            .key_rect(note)
            .map(|rect| rect.center().x - self.origin.x)
            .unwrap_or(0.0);
        (center - viewport_width * 0.5)
            .max(0.0)
            .min((self.size().x - viewport_width).max(0.0))
    }
}

pub const fn is_black(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}

pub fn c_label(note: u8) -> Option<String> {
    (note < MIDI_NOTE_COUNT && note % 12 == 0).then(|| format!("C{}", i16::from(note / 12) - 1))
}

fn white_key_count() -> usize {
    (0..MIDI_NOTE_COUNT).filter(|note| !is_black(*note)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_covers_the_complete_midi_range() {
        let layout = PianoLayout::default();
        assert_eq!(layout.key_rect(0).unwrap().min, Pos2::ZERO);
        assert!(layout.key_rect(127).unwrap().max.x <= layout.size().x);
        assert_eq!(layout.key_rect(128), None);
        assert_eq!(layout.size().x, 75.0 * WHITE_KEY_WIDTH);
    }

    #[test]
    fn every_c_uses_scientific_pitch_notation() {
        let labels: Vec<_> = (0..MIDI_NOTE_COUNT).filter_map(c_label).collect();
        assert_eq!(labels.first().map(String::as_str), Some("C-1"));
        assert_eq!(labels.get(5).map(String::as_str), Some("C4"));
        assert_eq!(labels.last().map(String::as_str), Some("C9"));
        assert_eq!(c_label(60).as_deref(), Some("C4"));
        assert_eq!(c_label(61), None);
        assert_eq!(c_label(128), None);
    }

    #[test]
    fn black_keys_take_hit_test_precedence() {
        let layout = PianoLayout::default();
        let c_sharp = layout.key_rect(1).unwrap();
        assert_eq!(layout.note_at(c_sharp.center()), Some(1));
        assert_eq!(
            layout.note_at(pos2(WHITE_KEY_WIDTH - 1.0, BLACK_KEY_HEIGHT + 1.0)),
            Some(0)
        );
    }

    #[test]
    fn hit_testing_finds_endpoint_keys_and_rejects_outside() {
        let layout = PianoLayout::new(pos2(5.0, 7.0));
        assert_eq!(
            layout.note_at(layout.key_rect(0).unwrap().center_bottom() - vec2(0.0, 1.0)),
            Some(0)
        );
        assert_eq!(
            layout.note_at(layout.key_rect(127).unwrap().center_bottom() - vec2(0.0, 1.0)),
            Some(127)
        );
        assert_eq!(layout.note_at(pos2(4.0, 7.0)), None);
    }

    #[test]
    fn middle_c_centering_is_clamped_and_deterministic() {
        let layout = PianoLayout::default();
        let offset = layout.centered_offset(MIDDLE_C, 900.0);
        let middle = layout.key_rect(MIDDLE_C).unwrap().center().x;
        assert_eq!(offset, middle - 450.0);
        assert_eq!(layout.centered_offset(0, 900.0), 0.0);
        assert_eq!(layout.centered_offset(127, 900.0), layout.size().x - 900.0);
    }
}
