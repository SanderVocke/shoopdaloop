use crate::{colors, MidiNote, PianoAction};
use egui::{pos2, vec2, Pos2, Rect, Stroke, StrokeKind, Vec2};

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

#[derive(Debug, Default)]
pub struct PianoPane {
    held_note: Option<MidiNote>,
    scroll_initialized: bool,
    #[cfg(test)]
    keyboard_rect: Option<Rect>,
    #[cfg(test)]
    indicator_centers: Vec<f32>,
}

impl PianoPane {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        enabled: bool,
        destination_centers: &[f32],
    ) -> Vec<PianoAction> {
        let mut actions = Vec::new();
        #[cfg(test)]
        self.indicator_centers.clear();
        if !enabled {
            if let Some(action) = self.release_all() {
                actions.push(action);
            }
        }

        let (indicator_rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 16.0), egui::Sense::hover());
        let indicator_clip = indicator_rect.intersect(ui.clip_rect());
        let painter = ui.painter().with_clip_rect(indicator_clip);
        for center_x in destination_centers {
            if indicator_clip.x_range().contains(*center_x) {
                #[cfg(test)]
                self.indicator_centers.push(*center_x);
                let tip = egui::pos2(*center_x, indicator_rect.top() + 1.0);
                painter.line_segment(
                    [tip + egui::vec2(0.0, 12.0), tip + egui::vec2(0.0, 4.0)],
                    Stroke::new(2.0, colors::MIDI_ACTIVITY),
                );
                painter.add(egui::Shape::convex_polygon(
                    vec![tip, tip + egui::vec2(-4.0, 5.0), tip + egui::vec2(4.0, 5.0)],
                    colors::MIDI_ACTIVITY,
                    Stroke::NONE,
                ));
            }
        }

        let viewport_width = ui.available_width();
        let initial_offset = PianoLayout::default().centered_offset(MIDDLE_C, viewport_width);
        let mut scroll = egui::ScrollArea::horizontal()
            .id_salt("piano_keyboard_scroll")
            .scroll_source(crate::control_safe_scroll_source());
        if !self.scroll_initialized {
            scroll = scroll.horizontal_scroll_offset(initial_offset);
        }
        scroll.show(ui, |ui| {
            let size = PianoLayout::default().size();
            let sense = if enabled {
                egui::Sense::click_and_drag()
            } else {
                egui::Sense::hover()
            };
            let (rect, response) = ui.allocate_exact_size(size, sense);
            let layout = PianoLayout::new(rect.min);
            #[cfg(test)]
            {
                self.keyboard_rect = Some(rect);
            }
            if enabled {
                self.handle_pointer(ui.ctx(), &response, layout, &mut actions);
            }
            self.paint(ui, layout, enabled);
        });
        self.scroll_initialized = true;
        actions
    }

    pub fn release_all(&mut self) -> Option<PianoAction> {
        self.held_note.take().map(|_| PianoAction::ReleaseAll)
    }

    fn handle_pointer(
        &mut self,
        context: &egui::Context,
        response: &egui::Response,
        layout: PianoLayout,
        actions: &mut Vec<PianoAction>,
    ) {
        let (position, pressed, released, down, cancelled) = context.input(|input| {
            (
                input.pointer.interact_pos(),
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::PointerGone | egui::Event::WindowFocused(false)
                    )
                }),
            )
        });
        if cancelled {
            if let Some(action) = self.release_all() {
                actions.push(action);
            }
            return;
        }
        if released {
            if let Some(note) = self.held_note.take() {
                actions.push(PianoAction::Release(note));
            }
            return;
        }
        let pointed_note = position
            .filter(|position| response.rect.contains(*position))
            .and_then(|position| layout.note_at(position))
            .and_then(MidiNote::new);
        if pressed && response.hovered() {
            if let Some(note) = pointed_note {
                if self.held_note != Some(note) {
                    self.held_note = Some(note);
                    actions.push(PianoAction::Press(note));
                }
            }
        } else if down && self.held_note != pointed_note {
            if let Some(note) = self.held_note.take() {
                actions.push(PianoAction::Release(note));
            }
            if let Some(note) = pointed_note {
                self.held_note = Some(note);
                actions.push(PianoAction::Press(note));
            }
        }
    }

    fn paint(&self, ui: &egui::Ui, layout: PianoLayout, enabled: bool) {
        let painter = ui.painter();
        for note in (0..MIDI_NOTE_COUNT).filter(|note| !is_black(*note)) {
            let rect = layout.key_rect(note).unwrap();
            let held = self.held_note.is_some_and(|held| held.value() == note);
            painter.rect(
                rect,
                0.0,
                if held {
                    colors::COLORED_HIGHLIGHT
                } else if enabled {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_gray(105)
                },
                Stroke::new(
                    1.0,
                    if enabled {
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::from_gray(65)
                    },
                ),
                StrokeKind::Inside,
            );
            if let Some(label) = c_label(note) {
                painter.text(
                    rect.center_bottom() - vec2(0.0, 5.0),
                    egui::Align2::CENTER_BOTTOM,
                    label,
                    egui::FontId::proportional(9.0),
                    if enabled {
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::from_gray(60)
                    },
                );
            }
        }
        for note in (0..MIDI_NOTE_COUNT).filter(|note| is_black(*note)) {
            let rect = layout.key_rect(note).unwrap();
            let held = self.held_note.is_some_and(|held| held.value() == note);
            painter.rect(
                rect,
                1.0,
                if held {
                    colors::COLORED_HIGHLIGHT
                } else if enabled {
                    egui::Color32::BLACK
                } else {
                    egui::Color32::from_gray(45)
                },
                Stroke::new(
                    1.0,
                    if enabled {
                        egui::Color32::from_gray(80)
                    } else {
                        egui::Color32::from_gray(65)
                    },
                ),
                StrokeKind::Inside,
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn keyboard_rect(&self) -> Option<Rect> {
        self.keyboard_rect
    }

    #[cfg(test)]
    pub(crate) fn indicator_centers(&self) -> &[f32] {
        &self.indicator_centers
    }

    #[cfg(test)]
    pub(crate) fn hold_for_test(&mut self, note: MidiNote) {
        self.held_note = Some(note);
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

    #[shoop_wasm_test_support::shoop_test]
    fn geometry_covers_the_complete_midi_range() {
        let layout = PianoLayout::default();
        assert_eq!(layout.key_rect(0).unwrap().min, Pos2::ZERO);
        assert!(layout.key_rect(127).unwrap().max.x <= layout.size().x);
        assert_eq!(layout.key_rect(128), None);
        assert_eq!(layout.size().x, 75.0 * WHITE_KEY_WIDTH);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn every_c_uses_scientific_pitch_notation() {
        let labels: Vec<_> = (0..MIDI_NOTE_COUNT).filter_map(c_label).collect();
        assert_eq!(labels.first().map(String::as_str), Some("C-1"));
        assert_eq!(labels.get(5).map(String::as_str), Some("C4"));
        assert_eq!(labels.last().map(String::as_str), Some("C9"));
        assert_eq!(c_label(60).as_deref(), Some("C4"));
        assert_eq!(c_label(61), None);
        assert_eq!(c_label(128), None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn black_keys_take_hit_test_precedence() {
        let layout = PianoLayout::default();
        let c_sharp = layout.key_rect(1).unwrap();
        assert_eq!(layout.note_at(c_sharp.center()), Some(1));
        assert_eq!(
            layout.note_at(pos2(WHITE_KEY_WIDTH - 1.0, BLACK_KEY_HEIGHT + 1.0)),
            Some(0)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn middle_c_centering_is_clamped_and_deterministic() {
        let layout = PianoLayout::default();
        let offset = layout.centered_offset(MIDDLE_C, 900.0);
        let middle = layout.key_rect(MIDDLE_C).unwrap().center().x;
        assert_eq!(offset, middle - 450.0);
        assert_eq!(layout.centered_offset(0, 900.0), 0.0);
        assert_eq!(layout.centered_offset(127, 900.0), layout.size().x - 900.0);
    }

    fn frame(
        context: &egui::Context,
        pane: &mut PianoPane,
        events: Vec<egui::Event>,
    ) -> Vec<PianoAction> {
        let mut actions = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 200.0))),
                events,
                ..Default::default()
            },
            |ui| actions = pane.show(ui, true, &[]),
        );
        actions
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pointer_press_release_and_focus_loss_have_paired_lifecycle() {
        let context = egui::Context::default();
        let mut pane = PianoPane::default();
        frame(&context, &mut pane, Vec::new());
        let keyboard = pane.keyboard_rect().unwrap();
        let middle_c = PianoLayout::new(keyboard.min)
            .key_rect(MIDDLE_C)
            .unwrap()
            .center();
        assert!((middle_c.x - 450.0).abs() < WHITE_KEY_WIDTH);
        let initial_min = keyboard.min.x;
        frame(&context, &mut pane, Vec::new());
        assert_eq!(pane.keyboard_rect().unwrap().min.x, initial_min);
        assert_eq!(
            frame(
                &context,
                &mut pane,
                vec![
                    egui::Event::PointerMoved(middle_c),
                    egui::Event::PointerButton {
                        pos: middle_c,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            ),
            vec![PianoAction::Press(MidiNote::new(MIDDLE_C).unwrap())]
        );
        assert!(frame(&context, &mut pane, Vec::new()).is_empty());
        assert_eq!(
            frame(&context, &mut pane, vec![egui::Event::WindowFocused(false)],),
            vec![PianoAction::ReleaseAll]
        );
        assert!(frame(&context, &mut pane, vec![egui::Event::PointerGone]).is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn piano_paints_with_horizontal_overflow_at_supported_sizes() {
        for size in [vec2(360.0, 200.0), vec2(900.0, 600.0)] {
            let context = egui::Context::default();
            let mut pane = PianoPane::default();
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    pane.show(ui, false, &[]);
                },
            );
            assert!(output.shapes.len() > usize::from(MIDI_NOTE_COUNT));
            assert!(pane.keyboard_rect().unwrap().width() > size.x);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabled_keyboard_ignores_input_and_active_destinations_draw_indicators() {
        let context = egui::Context::default();
        let mut pane = PianoPane::default();
        let mut actions = Vec::new();
        let render = |pane: &mut PianoPane,
                      enabled: bool,
                      centers: &[f32],
                      events: Vec<egui::Event>,
                      actions: &mut Vec<PianoAction>| {
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 200.0))),
                    events,
                    ..Default::default()
                },
                |ui| *actions = pane.show(ui, enabled, centers),
            );
        };

        render(&mut pane, false, &[], Vec::new(), &mut actions);
        let note = PianoLayout::new(pane.keyboard_rect().unwrap().min)
            .key_rect(MIDDLE_C)
            .unwrap()
            .center();
        render(
            &mut pane,
            false,
            &[],
            vec![
                egui::Event::PointerMoved(note),
                egui::Event::PointerButton {
                    pos: note,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut actions,
        );
        assert!(actions.is_empty());
        render(
            &mut pane,
            false,
            &[],
            vec![egui::Event::PointerButton {
                pos: note,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut actions,
        );

        render(&mut pane, true, &[120.0, 360.0], Vec::new(), &mut actions);
        assert_eq!(pane.indicator_centers(), &[120.0, 360.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pointer_release_outside_still_releases_the_held_note() {
        let context = egui::Context::default();
        let mut pane = PianoPane::default();
        frame(&context, &mut pane, Vec::new());
        let keyboard = pane.keyboard_rect().unwrap();
        let note = PianoLayout::new(keyboard.min)
            .key_rect(MIDDLE_C)
            .unwrap()
            .center();
        frame(
            &context,
            &mut pane,
            vec![
                egui::Event::PointerMoved(note),
                egui::Event::PointerButton {
                    pos: note,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let outside = pos2(899.0, 199.0);
        assert_eq!(
            frame(
                &context,
                &mut pane,
                vec![
                    egui::Event::PointerMoved(outside),
                    egui::Event::PointerButton {
                        pos: outside,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            ),
            vec![PianoAction::Release(MidiNote::new(MIDDLE_C).unwrap())]
        );
    }
}
